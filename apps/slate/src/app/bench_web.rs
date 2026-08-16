//! Frame-time bench for a board full of web portals.
//!
//! The reference corpus is the climate/weather dashboard: 58 remote pages in an
//! 8-column grid, every one of them animated. `#[ignore]`d, like the Atlas load
//! benchmarks — this is the evidence behind the web-portal section of
//! `docs/performance.md`, not a pass/fail gate.
//!
//! Run it:
//!
//! ```powershell
//! cargo test -p slate --release --lib bench_web -- --ignored --nocapture
//! ```
//!
//! What it measures is Slate's own per-frame cost: the pump, the admission
//! sort, texture uploads, and the paint of every card. The pixel backend is a
//! fake that hands out real-sized (1280×720) images, because the costs that
//! actually bit — a full-frame `ColorImage` clone per readback and a fresh GPU
//! texture per upload — are on our side of the [`WebHost`] seam, not inside
//! WebView2.

use super::board_web::{self, WebHost, WebInput, WebRequest};
use super::board_web_stills::StillCache;
use super::*;
use eframe::egui::{Pos2, Rect as ERect, Vec2 as EVec2};
use slate_doc::{NodeId, ViewKind};
use std::time::{Duration, Instant};

/// Page size a live portal lays out at, matching `viewport.width_css` = 1280
/// with a 16:9 frame — the size the real capture path reads back.
const PAGE_W: usize = 1280;
const PAGE_H: usize = 720;

/// The reference board: 58 portals, 8 columns, 480×270 cards, 24 px gutters.
const PORTALS: usize = 58;
const COLS: usize = 8;
const CARD_W: f32 = 480.0;
const CARD_H: f32 = 270.0;
const GAP: f32 = 24.0;

/// A host that behaves like the Windows one in the ways that cost: every
/// admitted portal has a fresh full-size frame waiting on every call.
#[derive(Default, Clone)]
struct BenchHost(std::rc::Rc<std::cell::RefCell<BenchLog>>);

#[derive(Default)]
struct BenchLog {
    admitted: std::collections::HashSet<NodeId>,
    frames_taken: usize,
    posters_taken: usize,
    /// Every request for pixels, answered or not.
    take_calls: usize,
    /// Pages that have not painted, and so answer nothing.
    starving: bool,
    /// Pages handing back the flat frame a webview produces while it navigates,
    /// reloads, or has just been resized.
    blanking: bool,
    /// Webviews actually created and torn down. On Windows each one is a browser
    /// process and a page load, so this is the churn that a pan must not cause.
    opens: usize,
    closes: usize,
}

impl BenchHost {
    fn frames_taken(&self) -> usize {
        self.0.borrow().frames_taken
    }
    fn posters_taken(&self) -> usize {
        self.0.borrow().posters_taken
    }
    fn reset_counts(&self) {
        let mut log = self.0.borrow_mut();
        log.frames_taken = 0;
        log.posters_taken = 0;
        log.take_calls = 0;
        log.opens = 0;
        log.closes = 0;
    }
    /// Webviews created and destroyed since the last reset.
    fn churn(&self) -> (usize, usize) {
        let log = self.0.borrow();
        (log.opens, log.closes)
    }
    /// Requests for pixels, including the ones answered with nothing.
    fn take_calls(&self) -> usize {
        self.0.borrow().take_calls
    }
    /// Make every page behave like one that has not painted yet.
    fn starve(&self, on: bool) {
        self.0.borrow_mut().starving = on;
    }
    /// Make every page hand back the flat frame of a navigation or a reload.
    fn blank(&self, on: bool) {
        self.0.borrow_mut().blanking = on;
    }
}

/// The frame a webview produces before its page has painted: one flat colour,
/// full size, indistinguishable from a real capture except by its content.
fn blank_image() -> std::sync::Arc<egui::ColorImage> {
    std::sync::Arc::new(egui::ColorImage::new(
        [PAGE_W, PAGE_H],
        egui::Color32::BLACK,
    ))
}

/// A fresh full-size readback, as the D3D path produces one: the allocation and
/// the BGRA conversion are real costs on the Windows side of the seam, so the
/// bench pays them too.
/// Patterned, not flat, and deliberately so: a page that has actually painted has
/// more than one colour in it, and a flat capture is what a webview hands back
/// *before* its page has drawn. Faking one here is how the bench managed to
/// report a healthy board while the real one showed black cards.
fn page_image() -> std::sync::Arc<egui::ColorImage> {
    let mut pixels = Vec::with_capacity(PAGE_W * PAGE_H);
    for y in 0..PAGE_H {
        for x in 0..PAGE_W {
            let lit = (x / 64 + y / 64) % 2 == 0;
            pixels.push(if lit {
                egui::Color32::from_rgb(28, 84, 150)
            } else {
                egui::Color32::from_rgb(18, 54, 96)
            });
        }
    }
    std::sync::Arc::new(egui::ColorImage {
        size: [PAGE_W, PAGE_H],
        pixels,
    })
}

impl WebHost for BenchHost {
    fn available(&self) -> bool {
        true
    }
    fn admit(&mut self, id: NodeId, _req: &WebRequest) {
        let mut log = self.0.borrow_mut();
        if log.admitted.insert(id) {
            log.opens += 1;
        }
    }
    fn evict(&mut self, id: NodeId) {
        let mut log = self.0.borrow_mut();
        if log.admitted.remove(&id) {
            log.closes += 1;
        }
    }
    fn take_frame(&mut self, id: NodeId) -> Option<std::sync::Arc<egui::ColorImage>> {
        let mut log = self.0.borrow_mut();
        log.take_calls += 1;
        if !log.admitted.contains(&id) || log.starving {
            return None;
        }
        log.frames_taken += 1;
        Some(if log.blanking {
            blank_image()
        } else {
            page_image()
        })
    }
    fn capture_poster(&mut self, _id: NodeId) -> Option<std::sync::Arc<egui::ColorImage>> {
        self.0.borrow_mut().posters_taken += 1;
        Some(page_image())
    }
    fn last_frame(&self, id: NodeId) -> Option<std::sync::Arc<egui::ColorImage>> {
        let log = self.0.borrow();
        (log.admitted.contains(&id) && !log.starving).then(page_image)
    }
    fn send_input(&mut self, _id: NodeId, _input: WebInput) {}
    fn cursor(&self, _id: NodeId) -> Option<egui::CursorIcon> {
        None
    }
    fn load_error(&self, _id: NodeId) -> Option<String> {
        None
    }
}

struct Bench {
    ctx: egui::Context,
    app: SlateApp,
    host: BenchHost,
    ids: Vec<NodeId>,
}

impl Bench {
    fn new() -> Bench {
        let ctx = egui::Context::default();
        let mut app = SlateApp::with_ctx(&ctx, None);
        app.kits = kits::KitState::builtin_only();
        app.leave_home();
        app.ensure_work_tab();
        app.doc_mut().view.active_view = ViewKind::Board;

        let host = BenchHost::default();
        app.web.set_host(Box::new(host.clone()));

        // The same grid the staged climate proposal builds, and the same trust
        // the human grants once on open.
        let mut nodes = Vec::with_capacity(PORTALS);
        for i in 0..PORTALS {
            let col = i % COLS;
            let row = i / COLS;
            let rect = slate_doc::scene::WorldRect::new(
                40.0 + col as f32 * (CARD_W + GAP),
                120.0 + row as f32 * (CARD_H + GAP),
                CARD_W,
                CARD_H,
            );
            let portal = slate_doc::scene::PortalNode::bound_web(
                format!("site-{i}"),
                format!("https://site-{i}.example/dashboard"),
            );
            nodes.push(
                app.doc_mut()
                    .scene
                    .build_node(rect, slate_doc::scene::NodeKind::Portal(portal)),
            );
        }
        let ids = app.add_nodes(nodes);
        for i in 0..PORTALS {
            app.web.grant_consent(format!("https://site-{i}.example"));
        }

        let mut bench = Bench {
            ctx,
            app,
            host,
            ids,
        };
        // One frame establishes `canvas_rect`, which `fit_board` needs.
        bench.frame();
        bench
    }

    fn frame(&mut self) -> Duration {
        let input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1920.0, 1080.0))),
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let start = Instant::now();
        let _ = ctx.run(input, |c| app.update_app(c));
        start.elapsed()
    }

    fn set_zoom(&mut self, z: f32) {
        self.app.tab_mut().cam.z = z;
    }

    /// How many cards have a still to show — the "never blank" promise, counted.
    fn stills(&self) -> usize {
        self.ids
            .iter()
            .filter(|id| self.app.web.has_still(**id))
            .count()
    }

    /// Centre the camera on one card, at a zoom that makes it the clear
    /// subject — the "reader leans into one page" case.
    fn focus_card(&mut self, index: usize, z: f32) {
        let col = index % COLS;
        let row = index / COLS;
        let cam = &mut self.app.tab_mut().cam;
        cam.z = z;
        cam.offset = EVec2::new(
            40.0 + col as f32 * (CARD_W + GAP) + CARD_W / 2.0,
            120.0 + row as f32 * (CARD_H + GAP) + CARD_H / 2.0,
        );
    }

    /// Frames paced like a real 60 Hz loop, for at least `secs` and at least
    /// `min_frames` frames.
    ///
    /// Pacing is not cosmetic: every budget in this feature is expressed per
    /// unit of *time* (5 fps idle, 30 fps focused, 120 ms camera settle), so a
    /// loop that runs 120 frames in a tenth of a second measures a board that
    /// never reaches its own capture deadlines and reports a flattering lie.
    ///
    /// The frame floor is the other half of that. A pool decision takes two or
    /// three frames to converge, because on-screen size is recorded while
    /// painting and read by the *next* pump — and a machine running the rest of
    /// the suite alongside this can spend an entire wall-clock window inside one
    /// frame. Time alone is a flaky clock; frames alone is a lying one.
    fn run_paced(
        &mut self,
        secs: f32,
        min_frames: usize,
        mut before: impl FnMut(&mut Bench, usize),
    ) -> Stats {
        const TARGET: Duration = Duration::from_micros(16_667);
        let end = Instant::now() + Duration::from_secs_f32(secs);
        let mut samples = Vec::new();
        let mut i = 0usize;
        while Instant::now() < end || i < min_frames {
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

    /// Real-time pacing only — what the perf regimes measure.
    fn run(&mut self, secs: f32) -> Stats {
        self.run_paced(secs, 0, |_, _| {})
    }

    /// Enough frames *and* enough time for the pool to reach a steady state,
    /// whatever else the machine is doing. Behavioural assertions use this.
    fn settle(&mut self, secs: f32) {
        self.run_paced(secs, 12, |_, _| {});
    }

    /// Still frames with a floor high enough for a decision to finish taking
    /// effect.
    ///
    /// Opening and closing browsers is rationed per frame (`OPENS_PER_FRAME`,
    /// `CLOSES_PER_FRAME`), so a wholesale turnover of the pool needs a dozen
    /// frames however long the clock says to wait — and on a machine running the
    /// rest of the suite alongside this one, half a second can be a handful of
    /// frames. Anything asserting on the *end* of a transition waits in frames.
    fn settle_frames(&mut self, secs: f32, frames: usize) {
        self.run_paced(secs, frames, |_, _| {});
    }

    /// Frames while the camera is moving, which is when jitter is felt.
    fn run_panning(&mut self, secs: f32) -> Stats {
        self.run_paced(secs, 0, |b, i| {
            b.app.tab_mut().cam.offset += EVec2::new(6.0, if i % 2 == 0 { 2.0 } else { -2.0 });
        })
    }

    /// A reader nudging the zoom around the point where pages become live: move,
    /// rest, move, rest. A *continuous* pan freezes pool membership outright, so
    /// it is this stop-start shape — every pause long enough for the pool to be
    /// re-decided — that produced pages blinking several times a second.
    fn run_stepped_zoom(&mut self, secs: f32, lo: f32, hi: f32) -> Stats {
        const HOLD: usize = 20;
        let mut z = lo;
        let mut up = true;
        self.run_paced(secs, HOLD * 6, move |b, i| {
            if i % HOLD > 0 {
                return;
            }
            z += if up { 0.02 } else { -0.02 };
            if z >= hi {
                up = false;
            }
            if z <= lo {
                up = true;
            }
            b.set_zoom(z);
        })
    }
}

/// Frame-time distribution. The tail is the point: a mean under budget with a
/// 40 ms p99 is what "it lags" actually feels like.
struct Stats {
    frames: usize,
    mean: f32,
    p50: f32,
    p95: f32,
    p99: f32,
    max: f32,
    /// Frames that blew the 16.7 ms budget — the ones the reader sees drop.
    dropped: usize,
}

impl Stats {
    fn new(mut samples: Vec<Duration>) -> Stats {
        assert!(!samples.is_empty());
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        let frames = samples.len();
        let mean = samples.iter().map(|d| ms(*d)).sum::<f32>() / frames as f32;
        let dropped = samples.iter().filter(|d| ms(**d) > 16.67).count();
        samples.sort();
        let at = |q: f32| ms(samples[((frames as f32 * q) as usize).min(frames - 1)]);
        Stats {
            frames,
            mean,
            p50: at(0.50),
            p95: at(0.95),
            p99: at(0.99),
            max: ms(*samples.last().unwrap()),
            dropped,
        }
    }

    fn report(&self, label: &str) {
        println!(
            "{label:<24} mean {:>6.2}  p50 {:>6.2}  p95 {:>6.2}  p99 {:>6.2}  max {:>7.2}  \
             dropped {}/{}",
            self.mean, self.p50, self.p95, self.p99, self.max, self.dropped, self.frames
        );
    }
}

/// Seconds of real time spent measuring each camera regime.
const REGIME_SECS: f32 = 3.0;
/// Settling time before a regime is measured, so admission churn and the first
/// texture uploads do not colour the steady state.
const SETTLE_SECS: f32 = 1.0;

/// The headline bench: one board, five camera regimes, frame time in each.
#[test]
#[ignore]
fn web_board_frame_time() {
    let mut b = Bench::new();
    println!(
        "\n{PORTALS} web portals, {PAGE_W}x{PAGE_H} pages, pool {}, {}\n",
        board_web::LIVE_POOL,
        if board_web::bench_legacy() {
            "LEGACY (no budgets)"
        } else {
            "budgeted"
        }
    );

    let regime = |b: &mut Bench, label: &str, panning: bool| {
        b.run(SETTLE_SECS);
        b.host.reset_counts();
        let stats = if panning {
            b.run_panning(REGIME_SECS)
        } else {
            b.run(REGIME_SECS)
        };
        stats.report(label);
        println!(
            "  open {}  live {}  frames read {}  posters {}  stills {}/{PORTALS}",
            b.app.web.open_count(),
            b.app.web.live_count(),
            b.host.frames_taken(),
            b.host.posters_taken(),
            b.stills()
        );
    };

    b.app.fit_board();
    regime(&mut b, "zoomed out (fit)", false);

    b.set_zoom(0.5);
    regime(&mut b, "mid zoom (0.5)", false);

    b.focus_card(20, 1.0);
    regime(&mut b, "leaning into one card", false);

    b.app.fit_board();
    b.set_zoom(0.5);
    regime(&mut b, "panning at 0.5", true);

    // Double-click into a page: the one portal that must be fully interactive.
    let id = b.ids[20];
    b.focus_card(20, 1.0);
    b.app.web_focus(id);
    regime(&mut b, "focused page", false);
    println!();
}

/// The promise a reader actually notices: a board sitting at a zoom where
/// nothing is pool-eligible still fills its cards in, and does it by borrowing
/// one spare slot at a time rather than opening fifty-eight browsers.
#[test]
fn a_zoomed_out_board_fills_in_stills_without_running_anything() {
    let mut b = Bench::new();
    b.app.fit_board();
    b.settle(0.8);
    assert_eq!(b.app.web.live_count(), 0, "fit zoom runs no pages");
    assert!(
        b.app.web.open_count() <= board_web::WARM_SLOTS,
        "warming opened {} views, budget is {}",
        b.app.web.open_count(),
        board_web::WARM_SLOTS
    );
    let before = b.stills();
    b.settle(2.0);
    assert!(
        b.stills() > before,
        "a still board keeps filling in stills: {before} -> {}",
        b.stills()
    );
}

/// More eligible cards than slots must not mean cards that never load.
///
/// Reported as "it started out good and then gave up": zoomed out the board
/// filled itself in, and at a reading zoom — where far more cards clear the live
/// threshold than the pool can hold — it stopped, leaving blank cards that said
/// `Budgeted` and only ever loaded if the reader zoomed into each one by hand.
/// Warming spent the pool's *spare* slots, and a saturated pool has none, so the
/// cards that lost admission lost it permanently. `Budgeted` also promises a
/// last frame (D30) that a card which never ran does not have.
#[test]
fn a_saturated_pool_still_fills_the_cards_that_lost_it() {
    let mut b = Bench::new();
    b.focus_card(20, 0.85);
    b.settle_frames(1.5, 40);
    assert!(
        b.app.web.live_count() > 0,
        "this only means something with the pool in use"
    );

    let before = b.stills();
    b.settle_frames(3.0, 120);
    assert!(
        b.stills() > before,
        "a saturated pool stopped filling cards in: {before} -> {} stills",
        b.stills()
    );
    // The slot warming uses is one admission gave up, never a seventh browser.
    assert!(
        b.app.web.open_count() <= board_web::LIVE_POOL,
        "warming took the pool past its ceiling: {} webviews open",
        b.app.web.open_count()
    );
}

/// Panning does not close and reopen browsers.
///
/// This is the bug the reader reported as flicker, and it is what re-deciding the
/// pool from scratch every frame produces: on-screen area drifts continuously
/// while the camera moves, so the ranking's tail changed constantly, and each
/// change on Windows means tearing down a browser process and loading its page
/// again — which is also why pages on a moving board never finished loading.
#[test]
fn a_pan_neither_opens_nor_closes_a_single_page() {
    let mut b = Bench::new();
    b.focus_card(20, 0.85);
    b.settle(1.0);
    let live = b.app.web.live_count();
    assert!(
        live > 0,
        "something has to be running for this to mean much"
    );

    b.host.reset_counts();
    b.run_panning(1.5);
    let (opens, closes) = b.host.churn();
    assert_eq!(
        (opens, closes),
        (0, 0),
        "a pan opened {opens} and closed {closes} webviews"
    );
    assert_eq!(
        b.app.web.live_count(),
        live,
        "the pool changed size during a pan"
    );
}

/// Nudging the zoom across the threshold where pages become live does not open
/// and close browsers.
///
/// The threshold has to be a band. A board parked near `LIVE_MIN_PX` puts every
/// card within a pixel of the decision, so a single line tested exactly meant a
/// reader adjusting the zoom flipped the whole board between `live` and
/// `budgeted` repeatedly — a browser torn down and rebuilt each time, which is
/// what the flicker was made of. Entry costs the full height; staying costs
/// `LIVE_KEEP_PX`.
#[test]
fn nudging_the_zoom_across_the_live_threshold_does_not_thrash_the_pool() {
    let mut b = Bench::new();
    // Cards are 270 world units tall, so pages become eligible around z = 0.59
    // and an incumbent holds down to about z = 0.41.
    b.focus_card(20, 0.55);
    b.settle(0.5);
    b.host.reset_counts();
    b.run_stepped_zoom(3.0, 0.55, 0.67);
    let (opens, _) = b.host.churn();
    // Live pages only. At this zoom the board is also warming itself cards, and
    // a warm that yields its slot to a page going live is the pool working
    // rather than thrashing — counting host closes wholesale conflated the two.
    let closes = b.app.web.pool_closes_in(Duration::from_secs(30));
    assert!(
        closes == 0,
        "a zoom nudge tore down {closes} live page(s) (and opened {opens})"
    );
    assert!(
        opens <= board_web::LIVE_POOL,
        "a zoom nudge opened {opens} browsers for a pool of {}",
        board_web::LIVE_POOL
    );
}

/// A board opening from cold fills its pool as fast as browsers may start —
/// not as fast as pool *membership* is allowed to change.
///
/// Those are different budgets and conflating them was expensive. Filling a
/// vacancy takes nothing away from anybody, so it is not the sort of change
/// `POOL_CHANGE_MIN_SECS` exists to damp; but a cold board's wanted set names
/// six pages at once, and the moment one of them displaced an incumbent the
/// whole change — vacancies included — waited for the damping window. The pool
/// then grew by one page per window, and a reader leaning into a wall of pages
/// watched five of the six stay stills for several seconds.
#[test]
fn a_cold_board_fills_its_pool_as_fast_as_browsers_may_start() {
    let mut b = Bench::new();
    b.focus_card(4, 1.0);
    // Exactly the browser-pacing budget, which is the only rate that should
    // govern this. Anything gated on the membership window needs far longer.
    b.settle_frames(
        board_web::OPEN_MIN_GAP_SECS * board_web::LIVE_POOL as f32 + 0.4,
        60,
    );
    assert_eq!(
        b.app.web.live_count(),
        board_web::LIVE_POOL - board_web::WARM_RESERVE,
        "the pool filled at the rate browsers are allowed to start"
    );
}

/// However many pages come due together, a frame reads back only its budget.
///
/// A readback is paid whether or not it yields a picture, and on the reference
/// board six live pages share one capture cadence — so they fall due on the same
/// frame and used to be read one after another on it. Measured against a real
/// browser that was a single frame spending 362 ms moving pixels.
#[test]
fn a_frame_never_reads_back_more_pages_than_its_budget() {
    let mut b = Bench::new();
    b.focus_card(4, 1.0);
    // A full pool is this test's premise rather than its subject, so it is
    // waited for rather than timed. Live pages are what fall due together, so
    // this counts those — the pool less the slot warming keeps while cards on
    // screen are still blank.
    let want_live = board_web::LIVE_POOL - board_web::WARM_RESERVE;
    let deadline = Instant::now() + Duration::from_secs(10);
    while b.app.web.live_count() < want_live && Instant::now() < deadline {
        b.settle_frames(0.2, 8);
    }
    assert_eq!(b.app.web.live_count(), want_live, "the pool filled");
    // Longer than the idle period, so every live page is overdue at once —
    // which is the state the budget exists for.
    std::thread::sleep(Duration::from_secs_f32(2.0 / board_web::IDLE_FPS));
    b.host.reset_counts();
    b.frame();
    let reads = b.host.take_calls();
    assert!(
        reads <= board_web::READS_PER_FRAME,
        "one frame read {reads} pages against a budget of {}",
        board_web::READS_PER_FRAME
    );
}

/// A page that loses its slot keeps its browser for a moment, so a slot lost and
/// taken back costs nothing at all.
#[test]
fn a_demoted_page_keeps_its_browser_long_enough_to_be_taken_back() {
    let mut b = Bench::new();
    b.focus_card(4, 1.0);
    // A browser every `OPEN_MIN_GAP_SECS`, and one per frame, so a poolful needs
    // both that many gaps and that many frames — a machine busy with the rest of
    // the suite can otherwise spend the whole window inside a handful of frames.
    b.settle_frames(
        board_web::OPEN_MIN_GAP_SECS * board_web::LIVE_POOL as f32 + 0.5,
        60,
    );
    assert_eq!(
        b.app.web.live_count(),
        board_web::LIVE_POOL - board_web::WARM_RESERVE,
        "the pool filled"
    );

    b.host.reset_counts();
    // Jump to a different part of the board: a different six win the pool — but
    // not before the pool is allowed to change at all, which is the whole point
    // of `POOL_CHANGE_MIN_SECS` and is measured from the last open of the fill.
    b.focus_card(48, 1.0);
    b.settle_frames(board_web::POOL_CHANGE_MIN_SECS + 0.5, 40);
    let (_, closes) = b.host.churn();
    // A wholesale turnover is more than the grace can hold, and should be: the
    // cap is what stops the grace becoming a second pool. What it must not do is
    // tear down everything at the first sign of a change.
    assert!(
        closes <= board_web::LIVE_POOL - board_web::LINGER_MAX,
        "{closes} browsers were torn down at once; the grace absorbed none of it"
    );
    assert!(
        b.app.web.lingering() > 0,
        "nothing was kept warm through the change"
    );
    assert!(
        b.app.web.host_views() <= board_web::LIVE_POOL + board_web::LINGER_MAX,
        "{} browsers open; the grace period became a second pool",
        b.app.web.host_views()
    );

    // Left alone, the grace runs out and the browsers do go.
    b.settle(board_web::EVICT_LINGER_SECS + 0.6);
    assert_eq!(b.app.web.lingering(), 0, "the grace period never expired");
    assert!(b.host.churn().1 > 0, "nothing was ever torn down");
}

/// A flat frame never replaces a card that is already showing its page.
///
/// A webview hands back one flat frame at several ordinary moments — mid
/// navigation, just after a resize recreates the capture pool, throughout a
/// reload — and the blank grace is measured from when the *browser* opened, not
/// from when the page went blank. Accepting one over a good still is what turned
/// working cards black at random.
#[test]
fn a_flat_frame_never_replaces_a_card_that_already_has_a_page() {
    let mut b = Bench::new();
    b.focus_card(20, 1.0);
    b.settle(1.0);
    let id = b
        .ids
        .iter()
        .copied()
        .find(|id| b.app.web.is_live(*id))
        .expect("a page is running");
    let stamp = b.app.web.poster_at(id).expect("it captured something");

    b.host.blank(true);
    b.settle(1.2);
    assert_eq!(
        b.app.web.poster_at(id),
        Some(stamp),
        "a flat frame overwrote a card that was showing its page"
    );
}

/// A page seen once is on its card the next time the board opens, without
/// running anything at all.
///
/// This is the answer to the measurement that started the work: driving the real
/// 58-portal board for a full minute left most cards blank, because the only way
/// to fill one was to run its page — a browser and several seconds of network
/// each, on a pool of six. Almost none of it needed doing twice. With the stills
/// kept on disk the same board came back with a median time-to-first-picture of
/// 0.1 s instead of 22.5 s.
///
/// The second session here is *starved*: no page may produce a pixel, so any
/// card that has a picture got it from disk and nowhere else.
#[test]
fn a_page_seen_once_is_on_the_card_the_next_time_the_board_opens() {
    let dir = std::env::temp_dir().join(format!("slate-bench-stills-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let seen: Vec<NodeId> = {
        let mut b = Bench::new();
        b.app.web.set_stills(StillCache::new(dir.clone()));
        b.focus_card(4, 1.0);
        b.settle_frames(1.5, 60);
        let seen: Vec<NodeId> = b
            .ids
            .iter()
            .copied()
            .filter(|id| b.app.web.has_still(*id))
            .collect();
        assert!(
            !seen.is_empty(),
            "the first session captured nothing to keep"
        );
        // The cache writes on its own thread, and writes beside-then-rename, so
        // wait for finished entries rather than for files — a `.tmp` counted as
        // an entry is a card that has not actually been kept yet.
        let finished = || {
            std::fs::read_dir(&dir)
                .map(|d| {
                    d.filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().is_some_and(|x| x == "jpg"))
                        .count()
                })
                .unwrap_or(0)
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while finished() < seen.len() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(finished(), seen.len(), "not every captured card was kept");
        seen
    };

    let mut b = Bench::new();
    b.app.web.set_stills(StillCache::new(dir.clone()));
    b.host.starve(true);
    b.focus_card(4, 1.0);
    // Reading the cards is real disk work on another thread, and this bench's
    // clock is simulated — sixty frames pass here in no wall time at all. So
    // wait for the reads exactly as the first session waited for the writes,
    // pumping frames throughout because restored pictures become textures on
    // the same per-frame budget as any other upload.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        b.settle_frames(0.1, 4);
        if seen.iter().all(|id| b.app.web.has_still(*id)) || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let restored = seen.iter().filter(|id| b.app.web.has_still(**id)).count();
    let blank: Vec<String> = seen
        .iter()
        .filter(|id| !b.app.web.has_still(**id))
        .map(|id| {
            let i = b.ids.iter().position(|x| x == id).unwrap();
            let key = StillCache::key(&format!("https://site-{i}.example/dashboard"));
            format!(
                "site-{i}(on disk: {})",
                dir.join(format!("{key}.jpg")).exists()
            )
        })
        .collect();
    assert!(
        blank.is_empty(),
        "{restored} of {} cards came back ({} read from disk); {blank:?} opened blank \
         on a board that had already seen their pages",
        seen.len(),
        b.app.web.restored_count()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A page that has not painted yet is asked at its budget, not at the board's
/// frame rate.
///
/// This is the case a success-keyed cadence gets exactly backwards: a page still
/// loading answers `None` to everything, so "ask again when the last frame is
/// 200 ms old" never fires and the poll runs at 60 Hz — on the real host, an
/// asynchronous readback issued every frame for a page with nothing to give.
#[test]
fn a_page_with_nothing_to_give_is_still_only_asked_at_its_budget() {
    let mut b = Bench::new();
    b.host.starve(true);
    b.focus_card(20, 1.0);
    b.settle(0.3);

    b.host.reset_counts();
    let stats = b.run_paced(1.0, 30, |_, _| {});
    let open = b.app.web.open_count();
    assert!(open > 0, "something was open to ask");
    // Idle pages get IDLE_FPS, the focused one FOCUS_FPS; nothing is focused
    // here, so the ceiling is one ask per page per idle period, plus slack for
    // the boundary frames.
    let ceiling = (open as f32 * board_web::IDLE_FPS * 1.5).ceil() as usize + open;
    assert!(
        b.host.take_calls() <= ceiling,
        "{} asks over {:.2}s across {open} open page(s); budget allows about {ceiling}",
        b.host.take_calls(),
        stats.frames as f32 / 60.0,
    );
}

/// What the "never blank" promise costs in texture memory, and the halving that
/// makes it affordable.
///
/// This is the one number that scales with board size rather than with the pool,
/// so it is the one worth pinning: a still per portal at full capture size would
/// be 3.7 MB each, and the reference board has fifty-eight of them.
#[test]
fn a_still_costs_a_quarter_of_the_page_it_came_from() {
    let full = PAGE_W * PAGE_H * 4;
    let mut b = Bench::new();
    b.app.fit_board();
    // Long enough for the camera to go quiet and one warm attempt to land: a
    // warm is a browser launch, so it deliberately waits for a still board.
    b.settle(1.4);
    assert_eq!(b.app.web.live_count(), 0, "nothing is pool-eligible at fit");
    let warmed = b.stills();
    assert!(warmed > 0, "warming produced a still to measure");
    let (opens, closes) = b.host.churn();
    assert!(
        b.app.web.poster_bytes() <= warmed * full / 4,
        "{warmed} warmed stills took {} bytes; a quarter of full size each is {}. \
         The pool opened {opens} and closed {closes} browsers getting there — a still \
         is only shrunk if the page was never live, so churn here means full-size \
         textures kept by cards that briefly ran.",
        b.app.web.poster_bytes(),
        warmed * full / 4
    );

    // A page that is actually running keeps its whole capture. Shrinking those
    // too was measured and rejected: it cost more on the frame loop than the
    // upload it saved (see `capture_need_w`).
    b.focus_card(20, 1.0);
    b.settle_frames(
        board_web::OPEN_MIN_GAP_SECS * board_web::LIVE_POOL as f32 + 0.5,
        60,
    );
    let live: Vec<NodeId> = b
        .ids
        .iter()
        .copied()
        .filter(|id| b.app.web.is_live(*id))
        .collect();
    assert_eq!(
        live.len(),
        board_web::LIVE_POOL - board_web::WARM_RESERVE,
        "the pool filled, less the slot warming keeps while cards are blank"
    );
    for id in &live {
        assert_eq!(
            b.app.web.poster_size(*id).map(|s| s[0]),
            Some(PAGE_W),
            "a live page keeps its full capture"
        );
    }
}
