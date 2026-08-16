//! Driving a real board of real pages, and writing down what it does.
//!
//! `bench_web.rs` measures Slate's own arithmetic against a fake host, which is
//! the right way to hold a frame-time budget honest and is useless for the
//! things a reader actually complains about — cards blinking, pages going black,
//! the pool churning while they pan. Every one of those lives on the other side
//! of the [`WebHost`](super::board_web::WebHost) seam, where a fake host reports
//! a perfectly healthy board.
//!
//! So this is the same app, headless, with the **real** WebView2 backend, the
//! real `.slate`, and the real network: an egui context runs frames at 60 Hz
//! while the Win32 message loop is pumped so WebView2's callbacks land, a script
//! moves the camera the way a reader does, and every portal's state is sampled
//! on every frame. What comes out is the evidence — how long a card waits for
//! its first picture, how often a page flips between states, and how many
//! browsers were opened and closed to achieve it.
//!
//! ```powershell
//! cargo test -p slate --release --lib probe_web -- --ignored --nocapture
//! $env:SLATE_PROBE_BOARD = "C:\path\to\other.slate"   # a different corpus
//! $env:SLATE_WEB_TRACE = "1"                          # every transition, live
//! ```
//!
//! It needs a desktop session, the Evergreen runtime, and the network, so it is
//! `#[ignore]`d like every other benchmark in this repo.

use super::board_web::WebState;
use super::board_web_win::probe;
use super::*;
use eframe::egui::{Pos2, Rect as ERect, Vec2 as EVec2};
use slate_doc::{NodeId, ViewKind};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The board driven when nothing else is named.
const DEFAULT_BOARD: &str = "presentations/climate-weather-dashboard.slate";

fn board_path() -> std::path::PathBuf {
    std::env::var("SLATE_PROBE_BOARD")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(DEFAULT_BOARD)
        })
}

/// What one portal did over the run.
#[derive(Default, Clone)]
struct Track {
    /// Last state word seen, so a change can be counted.
    state: Option<String>,
    /// State changes. **This is the oscillation number**: a page that settles
    /// answers two or three; a page caught in a loop answers dozens.
    flips: usize,
    /// Flips between exactly `live` and `budgeted`, which is the specific
    /// complaint — a page that keeps being told it may run and then may not.
    live_flips: usize,
    /// How long after the run started this card first had a picture.
    first_still: Option<Duration>,
    /// Times the card's picture was replaced.
    updates: usize,
    last_update: Option<Instant>,
}

struct Operator {
    ctx: egui::Context,
    app: SlateApp,
    ids: Vec<NodeId>,
    tracks: HashMap<NodeId, Track>,
    started: Instant,
    cost: Cost,
}

/// Where a phase's frame time went. Three places can charge the frame loop, and
/// they want completely different fixes, so they are counted apart.
#[derive(Default, Clone)]
struct Cost {
    frames: u32,
    /// Windows message dispatch — WebView2 callbacks and compositor work. Rises
    /// with the number of browsers *alive*, whatever they are doing.
    pump: f32,
    /// Creating and destroying browsers. Rises with pool churn.
    host: f32,
    /// Reading page frames back off the GPU and uploading them. Rises with the
    /// number of pages rendering and their capture rate.
    pixel: f32,
    worst_host: f32,
    /// The single longest frame of the phase, broken down. A mean cannot find a
    /// hitch, and a hitch is the thing the reader sees — so the worst frame is
    /// kept whole, with whatever the three known costs fail to explain left in
    /// `rest` (egui's own layout, tessellation, and texture work).
    worst: f32,
    worst_parts: (f32, f32, f32),
}

impl Cost {
    fn per_frame(&self) -> (f32, f32, f32) {
        let n = self.frames.max(1) as f32;
        (self.pump / n, self.host / n, self.pixel / n)
    }

    /// What the worst frame was spent on: pump, host, pixels, everything else.
    fn worst_split(&self) -> (f32, f32, f32, f32) {
        let (pump, host, pixel) = self.worst_parts;
        (
            pump,
            host,
            pixel,
            (self.worst - pump - host - pixel).max(0.0),
        )
    }
}

impl Operator {
    fn open() -> Option<Operator> {
        // A cache that outlives the run, so the second time this probe drives
        // the board it is the reader's second session — cards restored from
        // disk, no warming, no browsers spent on pictures already taken.
        if std::env::var_os("SLATE_STILL_CACHE").is_none() {
            std::env::set_var(
                "SLATE_STILL_CACHE",
                std::env::temp_dir().join("slate-probe-stills"),
            );
        }
        let path = board_path();
        if !path.exists() {
            eprintln!("no board at {}", path.display());
            return None;
        }
        let ctx = egui::Context::default();
        let mut app = SlateApp::with_ctx(&ctx, Some(path.clone()));
        app.doc_mut().view.active_view = ViewKind::Board;

        // A profile folder that survives between runs, so the second run starts
        // with a warm HTTP cache — which is also the reader's second session.
        let host = probe::host("operator")?;
        app.web.set_host(Box::new(host));

        // The human's one click on open. It persists, so the second run of this
        // probe — like the reader's second session — is not asked again.
        app.web_allow_all_origins_on_board();

        let ids: Vec<NodeId> = app
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| {
                matches!(&n.kind, slate_doc::scene::NodeKind::Portal(p)
                    if p.kind == slate_doc::scene::PortalKind::Web)
            })
            .map(|n| n.id)
            .collect();
        let mut op = Operator {
            ctx,
            app,
            ids,
            tracks: HashMap::new(),
            started: Instant::now(),
            cost: Cost::default(),
        };
        op.frame();
        op.started = Instant::now();
        Some(op)
    }

    /// One frame, timed the way the reader experiences it.
    ///
    /// The message pump is inside the measurement on purpose. WebView2's
    /// completions and the compositor arrive through this thread's queue, and in
    /// the real app that queue is winit's — the same thread, between the same
    /// frames. Timing only `Context::run` would hide every cost the browser
    /// charges us, which is most of them.
    fn frame(&mut self) -> Duration {
        let start = Instant::now();
        probe::pump();
        let pumped = start.elapsed();
        let input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1920.0, 1080.0))),
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let _ = ctx.run(input, |c| app.update_app(c));
        let spent = start.elapsed();
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        let (pump, host, pixel) = (ms(pumped), self.app.web.host_ms(), self.app.web.pixel_ms());
        self.cost.pump += pump;
        self.cost.host += host;
        self.cost.pixel += pixel;
        self.cost.worst_host = self.cost.worst_host.max(host);
        if ms(spent) > self.cost.worst {
            self.cost.worst = ms(spent);
            self.cost.worst_parts = (pump, host, pixel);
        }
        self.cost.frames += 1;
        self.observe();
        spent
    }

    /// Sample every portal. Cheap enough to do every frame, which matters: an
    /// oscillation at 4 Hz is invisible to a sampler that runs once a second.
    fn observe(&mut self) {
        for id in &self.ids {
            let state = self.app.web.state(*id);
            let word = state.label();
            let has_still = self.app.web.has_still(*id);
            let poster_at = self.app.web.poster_at(*id);
            let track = self.tracks.entry(*id).or_default();
            if track.state.as_deref() != Some(word.as_str()) {
                let was_live_or_budgeted =
                    matches!(track.state.as_deref(), Some("live") | Some("budgeted"));
                if was_live_or_budgeted && matches!(state, WebState::Live | WebState::Budgeted) {
                    track.live_flips += 1;
                }
                if track.state.is_some() {
                    track.flips += 1;
                }
                track.state = Some(word);
            }
            if has_still && track.first_still.is_none() {
                track.first_still = Some(self.started.elapsed());
            }
            if poster_at != track.last_update {
                track.last_update = poster_at;
                track.updates += 1;
            }
        }
    }

    /// Frames paced like the real loop. Every budget in this feature is
    /// expressed in time, so a loop that runs flat out measures nothing.
    fn run(&mut self, secs: f32, mut before: impl FnMut(&mut Operator, usize)) -> Stats {
        const TARGET: Duration = Duration::from_micros(16_667);
        let end = Instant::now() + Duration::from_secs_f32(secs);
        let mut samples = Vec::new();
        let mut i = 0usize;
        while Instant::now() < end {
            before(self, i);
            let spent = self.frame();
            samples.push(spent);
            if let Some(rest) = TARGET.checked_sub(spent) {
                std::thread::sleep(rest);
            }
            i += 1;
        }
        Stats::new(samples)
    }

    fn still(&mut self, secs: f32) -> Stats {
        self.run(secs, |_, _| {})
    }

    fn set_zoom(&mut self, z: f32) {
        self.app.tab_mut().cam.z = z;
    }

    fn stills(&self) -> usize {
        self.ids
            .iter()
            .filter(|id| self.app.web.has_still(**id))
            .count()
    }

    fn churn(&self) -> usize {
        self.app.web.churn_in(Duration::from_secs(30))
    }
}

struct Stats {
    frames: usize,
    mean: f32,
    p95: f32,
    max: f32,
    dropped: usize,
    /// Which frames ran long. A hitch every second and a hitch at random are
    /// different bugs, and the mean cannot tell them apart.
    spikes: Vec<usize>,
}

impl Stats {
    fn new(mut samples: Vec<Duration>) -> Stats {
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        let frames = samples.len().max(1);
        let mean = samples.iter().map(|d| ms(*d)).sum::<f32>() / frames as f32;
        let dropped = samples.iter().filter(|d| ms(**d) > 16.67).count();
        let spikes = samples
            .iter()
            .enumerate()
            .filter(|(_, d)| ms(**d) > 16.67)
            .map(|(i, _)| i)
            .collect();
        samples.sort();
        let at = |q: f32| ms(samples[((frames as f32 * q) as usize).min(samples.len().max(1) - 1)]);
        Stats {
            frames,
            mean,
            p95: at(0.95),
            max: samples.last().map(|d| ms(*d)).unwrap_or(0.0),
            dropped,
            spikes,
        }
    }
}

/// One scripted tour of a board full of pages, with the numbers written down.
///
/// The phases are the gestures the reader described: sitting at the zoom the
/// board opens at, leaning in, panning, nudging the zoom around the point where
/// pages go live, and finally typing into one.
#[test]
#[ignore]
fn operate_a_wall_of_pages() {
    let Some(mut op) = Operator::open() else {
        eprintln!("no WebView2 runtime, GPU device, or compositor here — nothing to operate");
        return;
    };
    let portals = op.ids.len();
    println!("\n{portals} web portals from {}\n", board_path().display());

    let report = |op: &mut Operator, label: &str, stats: Stats, before_churn: usize| {
        let (pump, host, pixel) = op.cost.per_frame();
        let (wp, wh, wx, wr) = op.cost.worst_split();
        println!(
            "{label:<24} mean {:>6.2}  p95 {:>6.2}  max {:>7.2}  dropped {:>3}/{:<4}  \
             │ pump {pump:>5.2}  host {host:>5.2} (worst {:>6.2})  pixels {pixel:>5.2}  \
             │ live {}  browsers {}  stills {}/{}  pool changes {}\n\
             {:<24} worst frame: pump {wp:>6.2}  host {wh:>6.2}  pixels {wx:>6.2}  rest {wr:>6.2}\
             {}",
            stats.mean,
            stats.p95,
            stats.max,
            stats.dropped,
            stats.frames,
            op.cost.worst_host,
            op.app.web.live_count(),
            op.app.web.host_views(),
            op.stills(),
            op.ids.len(),
            op.churn().saturating_sub(before_churn),
            "",
            if stats.spikes.is_empty() {
                String::new()
            } else {
                let gaps: Vec<usize> = stats
                    .spikes
                    .windows(2)
                    .map(|w| w[1] - w[0])
                    .take(10)
                    .collect();
                format!(
                    "   long frames at {:?} (gaps {gaps:?})",
                    &stats.spikes[..stats.spikes.len().min(10)]
                )
            },
        );
        op.cost = Cost::default();
    };

    op.app.fit_board();
    let c = op.churn();
    let s = op.still(12.0);
    report(&mut op, "as the board opens", s, c);

    op.set_zoom(0.5);
    let c = op.churn();
    let s = op.still(8.0);
    report(&mut op, "mid zoom (0.5)", s, c);

    // The reader leans in: cards cross the height at which pages may run.
    op.set_zoom(0.8);
    let c = op.churn();
    let s = op.still(15.0);
    report(&mut op, "leaning in (0.8)", s, c);

    // A continuous pan. Membership is frozen throughout, so this should cost
    // nothing at all beyond painting.
    let c = op.churn();
    let s = op.run(8.0, |op, i| {
        let d = if i % 120 < 60 { 6.0 } else { -6.0 };
        op.app.tab_mut().cam.offset += EVec2::new(d, 0.0);
    });
    report(&mut op, "panning", s, c);

    // Stop-start zoom across the live threshold — the gesture that made pages
    // blink, because each pause is long enough for the pool to be re-decided.
    let c = op.churn();
    let mut z = 0.55f32;
    let mut up = true;
    let s = op.run(10.0, move |op, i| {
        if i % 20 > 0 {
            return;
        }
        z += if up { 0.02 } else { -0.02 };
        if z >= 0.7 {
            up = false;
        }
        if z <= 0.55 {
            up = true;
        }
        op.set_zoom(z);
    });
    report(&mut op, "nudging the zoom", s, c);

    // Into one page.
    op.set_zoom(1.0);
    if let Some(id) = op.ids.first().copied() {
        op.app.web_focus(id);
    }
    let c = op.churn();
    let s = op.still(8.0);
    report(&mut op, "one page focused", s, c);

    // Full screen, and back. The camera flies for a fifth of a second, so
    // membership is frozen across the gesture itself and the interesting moment
    // is where it lands: one page filling the canvas should be live, and the
    // board the reader comes back to should still have its pictures rather than
    // a wall of cards re-earning them.
    if let Some(id) = op.ids.first().copied() {
        let c = op.churn();
        op.app.web_maximize(id);
        let s = op.still(8.0);
        report(&mut op, "full screen on one page", s, c);

        let c = op.churn();
        op.app.web_restore();
        let s = op.still(8.0);
        report(&mut op, "back from full screen", s, c);
    }

    // ---- what each card lived through ----
    let mut tracks: Vec<(NodeId, Track)> =
        op.tracks.iter().map(|(id, t)| (*id, t.clone())).collect();
    tracks.sort_by_key(|(_, t)| std::cmp::Reverse(t.flips));

    let with_still = tracks
        .iter()
        .filter(|(_, t)| t.first_still.is_some())
        .count();
    let mut waits: Vec<f32> = tracks
        .iter()
        .filter_map(|(_, t)| t.first_still.map(|d| d.as_secs_f32()))
        .collect();
    waits.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("\ncards that ever showed a picture: {with_still}/{portals}");
    if !waits.is_empty() {
        let at = |q: f32| waits[((waits.len() as f32 * q) as usize).min(waits.len() - 1)];
        println!(
            "time to first picture: median {:.1}s  p90 {:.1}s  worst {:.1}s",
            at(0.5),
            at(0.9),
            waits[waits.len() - 1]
        );
    }
    println!("\nbusiest cards (state changes over the whole run):");
    for (id, t) in tracks.iter().take(8) {
        println!(
            "  node {:<6} {:>3} changes ({:>3} live/budgeted), {:>4} picture updates, now {}",
            id.0,
            t.flips,
            t.live_flips,
            t.updates,
            t.state.clone().unwrap_or_default()
        );
    }
    let worst_live = tracks.iter().map(|(_, t)| t.live_flips).max().unwrap_or(0);
    println!(
        "\nworst live/budgeted oscillation: {worst_live} flips; total pool changes {}\n",
        op.churn()
    );
}
