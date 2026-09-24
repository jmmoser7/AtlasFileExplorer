//! Session activity log — what the frame loop did, written so a later debug
//! session can read it instead of reconstructing a hitch from memory.
//!
//! Always-on in real runs (opt out with `ATLAS_SESSION_LOG=0`). The happy
//! path is a fixed ring and a span stack of `&'static str` — no per-frame
//! allocation, no disk I/O. Disk sees stalls, slow named spans, marks, and
//! infrequent lifecycle counts.
//!
//! Files land in `data_dir()/session-log/`:
//! - `<app>.jsonl` — append-only events
//! - `<app>-latest.json` — small snapshot agents should read first
//!
//! Call [`SessionLog::attach`] at the start of each UI frame so [`span`] /
//! [`count`] / [`event`] / [`mark`] can record without plumbing a handle
//! through every hot function. UI-thread only.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Frames retained — a couple of seconds at 60 Hz, the window a hitch lives in.
const WINDOW: usize = 180;
/// Constitution Art. II.1: 60 Hz is the floor.
const BUDGET_MS: f32 = 16.67;
/// App time that counts as a stall (two missed frames). "Not Responding" is
/// worse; this is the line a hitch has already crossed.
const STALL_APP_MS: f32 = 33.0;
/// Delivered interval that counts as a stall even if our app time was small
/// (GPU / vsync / driver).
const STALL_DELIVERED_MS: f32 = 50.0;
/// Named work that ate half a frame is worth a line even without a stall.
const SLOW_SPAN_MS: f32 = 8.0;
/// Nested paint (canvas → board → portal) stays inside this. A dropped
/// enter is silent, so the cap has to cover the deepest real stack.
const STACK: usize = 16;
/// Every named span in one frame. Overflow drops the name from stall lines,
/// so this is sized for a full Slate frame plus the board's own spans.
const COMPLETED: usize = 64;
const PENDING_CAP: usize = 64;
const STALL_CAP: usize = 16;
const MARK_CAP: usize = 8;
const JSONL_ROTATE: u64 = 2_000_000;
const FLUSH_EVERY: Duration = Duration::from_millis(500);
const LATEST_EVERY: Duration = Duration::from_secs(1);

thread_local! {
    static CURRENT: std::cell::RefCell<Option<Arc<Mutex<Inner>>>> = const { std::cell::RefCell::new(None) };
}

/// Cheap counters the apps refresh once per frame. All `&'static` / `Copy`
/// so a snapshot cannot allocate on the frame loop.
#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub entries: u32,
    pub nodes: u32,
    pub selected: u32,
    pub thumbs_pending: u32,
    pub extra: u32,
    pub scan_active: bool,
    pub view: &'static str,
    pub tool: &'static str,
}

/// One recorded stall, for the Advanced readout and `*-latest.json`.
#[derive(Clone, Debug)]
pub struct Stall {
    pub unix_ms: u64,
    pub app_ms: f32,
    pub delivered_ms: f32,
    pub spans: String,
    pub view: &'static str,
    pub tool: &'static str,
    pub entries: u32,
    pub nodes: u32,
}

/// Why this frame ran, for stall classification.
///
/// A long gap between frames is a stall only when the user did something or
/// the previous pass asked to paint again immediately (animation). An idle
/// timer (`request_repaint_after`) that fires on schedule is not a stall.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameWake {
    pub had_input: bool,
    /// Previous pass called `request_repaint` (immediate), including egui
    /// animations.
    pub eager: bool,
}

/// One-shot phase clock from process start through the first frame.
///
/// Call [`note_process_start`] at the top of `main`. Each [`Startup::phase`]
/// records the milliseconds since the previous mark (the first mark includes
/// everything since process start).
pub struct Startup {
    last: Instant,
    phases: Vec<(&'static str, f32)>,
}

impl Startup {
    pub fn begin() -> Self {
        Self {
            last: process_start(),
            phases: Vec::new(),
        }
    }

    pub fn phase(&mut self, name: &'static str) {
        let now = Instant::now();
        let ms = now.saturating_duration_since(self.last).as_secs_f32() * 1000.0;
        self.phases.push((name, ms));
        self.last = now;
    }

    pub fn total_ms(&self) -> f32 {
        self.phases.iter().map(|(_, ms)| *ms).sum()
    }

    fn detail(&self) -> String {
        let mut s = String::new();
        for (name, ms) in &self.phases {
            if !s.is_empty() {
                s.push(',');
            }
            s.push_str(name);
            s.push(':');
            s.push_str(&format!("{ms:.1}"));
        }
        s
    }
}

static PROCESS_START: OnceLock<Instant> = OnceLock::new();

/// Capture process start. The first call wins; later calls are ignored.
pub fn note_process_start() {
    let _ = PROCESS_START.set(Instant::now());
}

pub fn process_start() -> Instant {
    *PROCESS_START.get_or_init(Instant::now)
}

/// App time always counts. A long delivered interval counts only when this
/// frame had input or the previous pass requested an immediate repaint.
pub fn is_stall(app_ms: f32, delivered_ms: f32, wake: FrameWake) -> bool {
    app_ms >= STALL_APP_MS || (delivered_ms >= STALL_DELIVERED_MS && (wake.had_input || wake.eager))
}

/// Frame-time ring + activity writer shared by File Atlas and Slate.
pub struct SessionLog {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    app: &'static str,
    persist: bool,
    log_path: PathBuf,
    latest_path: PathBuf,
    pid: u32,
    started_unix: u64,
    delivered: [f32; WINDOW],
    app_ms: [f32; WINDOW],
    next: usize,
    filled: usize,
    stack: [(&'static str, Instant); STACK],
    stack_len: usize,
    completed: [(&'static str, f32); COMPLETED],
    completed_len: usize,
    pending: Vec<Line>,
    stalls: Vec<Stall>,
    marks: Vec<String>,
    snap: Snapshot,
    last_flush: Instant,
    last_latest: Instant,
    last_stall_app_ms: Option<f32>,
    startup: Option<StartupSnap>,
    repaint_causes: String,
    last_repaint_note: Instant,
}

struct StartupSnap {
    ms: f32,
    phases: Vec<(&'static str, f32)>,
}

#[derive(Clone, Serialize)]
struct Line {
    t: u64,
    app: &'static str,
    kind: &'static str,
    name: &'static str,
    ms: f32,
    delivered_ms: f32,
    n: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    spans: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    entries: u32,
    nodes: u32,
    thumbs: u32,
    extra: u32,
    scan: bool,
    view: &'static str,
    tool: &'static str,
}

#[derive(Serialize)]
struct LatestFile<'a> {
    app: &'static str,
    pid: u32,
    started_unix: u64,
    persist: bool,
    log_path: &'a Path,
    latest_path: &'a Path,
    frame: FrameSummary,
    now: NowSummary,
    last_stalls: &'a [StallFile],
    marks: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    startup: Option<StartupFile<'a>>,
    #[serde(skip_serializing_if = "str::is_empty")]
    repaint_causes: &'a str,
}

#[derive(Serialize)]
struct StartupFile<'a> {
    ms: f32,
    phases: &'a [PhaseFile<'a>],
}

#[derive(Serialize)]
struct PhaseFile<'a> {
    name: &'a str,
    ms: f32,
}

#[derive(Serialize)]
struct FrameSummary {
    n: usize,
    app_mean_ms: f32,
    app_max_ms: f32,
    p95_ms: f32,
    max_ms: f32,
    dropped: usize,
}

#[derive(Serialize)]
struct NowSummary {
    entries: u32,
    nodes: u32,
    selected: u32,
    thumbs_pending: u32,
    extra: u32,
    scan_active: bool,
    view: &'static str,
    tool: &'static str,
    last_stall_app_ms: Option<f32>,
}

#[derive(Serialize)]
struct StallFile {
    unix_ms: u64,
    app_ms: f32,
    delivered_ms: f32,
    spans: String,
    view: &'static str,
    tool: &'static str,
    entries: u32,
    nodes: u32,
}

impl SessionLog {
    /// Persist under `data_dir()/session-log`, unless `ATLAS_SESSION_LOG=0`.
    pub fn new(app: &'static str) -> Self {
        if env_off() {
            Self::memory(app)
        } else {
            Self::persist(app)
        }
    }

    pub fn persist(app: &'static str) -> Self {
        let dir = crate::index::data_dir().join("session-log");
        Self::persist_at(app, dir)
    }

    pub fn persist_at(app: &'static str, dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self::from_inner(Inner::new(app, true, dir.join(format!("{app}.jsonl")), dir))
    }

    /// In-memory only — app test harnesses use this so `cargo test` does not
    /// write into the real LocalAppData folder.
    pub fn memory(app: &'static str) -> Self {
        Self::from_inner(Inner::new(app, false, PathBuf::new(), PathBuf::new()))
    }

    fn from_inner(inner: Inner) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// Install this log as the thread-local current recorder for a frame.
    pub fn attach(&self) -> AttachGuard {
        let prev = CURRENT.with(|c| c.replace(Some(Arc::clone(&self.inner))));
        AttachGuard { prev }
    }

    pub fn set_snapshot(&self, snap: Snapshot) {
        if let Ok(mut g) = self.inner.lock() {
            g.snap = snap;
        }
    }

    pub fn end_frame(&self, app_time: Duration, delivered: f32, wake: FrameWake) {
        if let Ok(mut g) = self.inner.lock() {
            g.end_frame(app_time, delivered, wake, None);
        }
    }

    /// Same as [`Self::end_frame`], plus a repaint-cause summary.
    ///
    /// `repaint` is recorded on a stall immediately, and otherwise at most
    /// once every two seconds, so the idle-repaint hunt has a named source
    /// without a line per frame.
    pub fn end_frame_with(
        &self,
        app_time: Duration,
        delivered: f32,
        wake: FrameWake,
        repaint: Option<&str>,
    ) {
        if let Ok(mut g) = self.inner.lock() {
            g.end_frame(app_time, delivered, wake, repaint);
        }
    }

    /// Write the one startup record (jsonl + latest). Safe to call once.
    pub fn record_startup(&self, startup: &Startup) {
        if let Ok(mut g) = self.inner.lock() {
            g.record_startup(startup);
        }
    }

    pub fn mark(&self, label: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.mark(label);
        }
    }

    pub fn persist_enabled(&self) -> bool {
        self.inner.lock().map(|g| g.persist).unwrap_or(false)
    }

    pub fn log_path(&self) -> Option<PathBuf> {
        self.inner.lock().ok().and_then(|g| {
            if g.log_path.as_os_str().is_empty() {
                None
            } else {
                Some(g.log_path.clone())
            }
        })
    }

    pub fn latest_path(&self) -> Option<PathBuf> {
        self.inner.lock().ok().and_then(|g| {
            if g.latest_path.as_os_str().is_empty() {
                None
            } else {
                Some(g.latest_path.clone())
            }
        })
    }

    pub fn last_stall_app_ms(&self) -> Option<f32> {
        self.inner.lock().ok().and_then(|g| g.last_stall_app_ms)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().map(|g| g.filled == 0).unwrap_or(true)
    }

    pub fn app_mean_ms(&self) -> f32 {
        self.inner.lock().map(|g| g.app_mean_ms()).unwrap_or(0.0)
    }

    pub fn app_max_ms(&self) -> f32 {
        self.inner.lock().map(|g| g.app_max_ms()).unwrap_or(0.0)
    }

    pub fn p95_ms(&self) -> f32 {
        self.inner.lock().map(|g| g.p95_ms()).unwrap_or(0.0)
    }

    pub fn max_ms(&self) -> f32 {
        self.inner.lock().map(|g| g.max_ms()).unwrap_or(0.0)
    }

    pub fn dropped(&self) -> usize {
        self.inner.lock().map(|g| g.dropped()).unwrap_or(0)
    }

    pub fn frames(&self) -> usize {
        self.inner.lock().map(|g| g.filled).unwrap_or(0)
    }

    pub fn last_stalls(&self) -> Vec<Stall> {
        self.inner
            .lock()
            .map(|g| g.stalls.clone())
            .unwrap_or_default()
    }

    /// Open the log folder in the platform file manager.
    pub fn reveal(&self) {
        let Some(path) = self.log_path() else {
            return;
        };
        let dir = path.parent().unwrap_or(&path);
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer.exe").arg(dir).spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }
}

/// Restores the previous thread-local log when the frame ends.
pub struct AttachGuard {
    prev: Option<Arc<Mutex<Inner>>>,
}

impl Drop for AttachGuard {
    fn drop(&mut self) {
        CURRENT.with(|c| {
            *c.borrow_mut() = self.prev.take();
        });
    }
}

/// Begin a named span. Drop to record its duration.
pub fn span(name: &'static str) -> SpanGuard {
    with_current(|g| g.enter(name));
    SpanGuard {
        name,
        start: Instant::now(),
    }
}

pub struct SpanGuard {
    name: &'static str,
    start: Instant,
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_secs_f32() * 1000.0;
        with_current(|g| g.exit(self.name, ms));
    }
}

/// Infrequent lifecycle count (scan batch size, tessellation misses, …).
pub fn count(name: &'static str, n: u32) {
    with_current(|g| g.push_count(name, n));
}

/// Infrequent named event with a short detail (root path, command id).
pub fn event(name: &'static str, detail: &str) {
    with_current(|g| g.push_event(name, detail));
}

/// User bookmark: "this is the hitch I just felt".
pub fn mark(label: &str) {
    with_current(|g| g.mark(label));
}

fn with_current(f: impl FnOnce(&mut Inner)) {
    CURRENT.with(|c| {
        if let Some(arc) = c.borrow().as_ref() {
            if let Ok(mut g) = arc.lock() {
                f(&mut g);
            }
        }
    });
}

fn env_off() -> bool {
    matches!(
        std::env::var("ATLAS_SESSION_LOG").as_deref(),
        Ok("0") | Ok("off") | Ok("false") | Ok("OFF")
    )
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Inner {
    fn new(app: &'static str, persist: bool, log_path: PathBuf, dir: PathBuf) -> Self {
        let latest_path = if persist {
            dir.join(format!("{app}-latest.json"))
        } else {
            PathBuf::new()
        };
        let now = Instant::now();
        Inner {
            app,
            persist,
            log_path,
            latest_path,
            pid: std::process::id(),
            started_unix: now_unix_ms() / 1000,
            delivered: [0.0; WINDOW],
            app_ms: [0.0; WINDOW],
            next: 0,
            filled: 0,
            stack: [("", Instant::now()); STACK],
            stack_len: 0,
            completed: [("", 0.0); COMPLETED],
            completed_len: 0,
            pending: Vec::new(),
            stalls: Vec::new(),
            marks: Vec::new(),
            snap: Snapshot::default(),
            last_flush: now,
            last_latest: now.checked_sub(LATEST_EVERY).unwrap_or(now),
            last_stall_app_ms: None,
            startup: None,
            repaint_causes: String::new(),
            last_repaint_note: now.checked_sub(Duration::from_secs(2)).unwrap_or(now),
        }
    }

    fn record_startup(&mut self, startup: &Startup) {
        let ms = startup.total_ms();
        self.startup = Some(StartupSnap {
            ms,
            phases: startup.phases.clone(),
        });
        let mut line = self.base_line();
        line.kind = "startup";
        line.name = "startup";
        line.ms = ms;
        line.detail = Some(startup.detail());
        self.queue(line);
        self.flush(true);
    }

    fn enter(&mut self, name: &'static str) {
        if self.stack_len < STACK {
            self.stack[self.stack_len] = (name, Instant::now());
            self.stack_len += 1;
        }
    }

    fn exit(&mut self, name: &'static str, ms: f32) {
        if self.stack_len > 0 {
            self.stack_len -= 1;
        }
        if self.completed_len < COMPLETED {
            self.completed[self.completed_len] = (name, ms);
            self.completed_len += 1;
        }
    }

    fn push_count(&mut self, name: &'static str, n: u32) {
        self.queue(Line {
            kind: "count",
            name,
            n,
            ..self.base_line()
        });
    }

    fn push_event(&mut self, name: &'static str, detail: &str) {
        let mut line = self.base_line();
        line.kind = "event";
        line.name = name;
        if !detail.is_empty() {
            line.detail = Some(detail.to_string());
        }
        self.queue(line);
    }

    fn mark(&mut self, label: &str) {
        let text = if label.is_empty() {
            "mark".to_string()
        } else {
            label.to_string()
        };
        if self.marks.len() >= MARK_CAP {
            self.marks.remove(0);
        }
        self.marks.push(text.clone());
        let mut line = self.base_line();
        line.kind = "mark";
        line.name = "mark";
        line.detail = Some(text);
        line.spans = Some(self.stack_names());
        self.queue(line);
        self.flush(true);
    }

    fn end_frame(
        &mut self,
        app_time: Duration,
        delivered: f32,
        wake: FrameWake,
        repaint: Option<&str>,
    ) {
        let app_ms = app_time.as_secs_f32() * 1000.0;
        let delivered_ms = delivered * 1000.0;
        self.delivered[self.next] = delivered_ms;
        self.app_ms[self.next] = app_ms;
        self.next = (self.next + 1) % WINDOW;
        self.filled = (self.filled + 1).min(WINDOW);

        let stall = is_stall(app_ms, delivered_ms, wake);
        if let Some(causes) = repaint.filter(|s| !s.is_empty()) {
            self.note_repaint(causes, stall);
        }
        let slow = (0..self.completed_len).any(|i| self.completed[i].1 >= SLOW_SPAN_MS);

        if stall {
            let spans = self.completed_names();
            self.last_stall_app_ms = Some(app_ms);
            if self.stalls.len() >= STALL_CAP {
                self.stalls.remove(0);
            }
            self.stalls.push(Stall {
                unix_ms: now_unix_ms(),
                app_ms,
                delivered_ms,
                spans: spans.clone(),
                view: self.snap.view,
                tool: self.snap.tool,
                entries: self.snap.entries,
                nodes: self.snap.nodes,
            });
            let mut line = self.base_line();
            line.kind = "stall";
            line.name = "frame";
            line.ms = app_ms;
            line.delivered_ms = delivered_ms;
            line.spans = Some(spans);
            if !self.repaint_causes.is_empty() {
                line.detail = Some(self.repaint_causes.clone());
            }
            self.queue(line);
        } else if slow {
            for i in 0..self.completed_len {
                let (name, ms) = self.completed[i];
                if ms < SLOW_SPAN_MS {
                    continue;
                }
                let mut line = self.base_line();
                line.kind = "span";
                line.name = name;
                line.ms = ms;
                self.queue(line);
            }
        }

        self.completed_len = 0;
        self.stack_len = 0;

        if self.pending.is_empty() {
            if self.persist && self.last_latest.elapsed() >= LATEST_EVERY {
                self.write_latest();
            }
            return;
        }
        let due = stall || self.last_flush.elapsed() >= FLUSH_EVERY;
        if due {
            self.flush(stall);
        }
    }

    fn note_repaint(&mut self, summary: &str, force: bool) {
        self.repaint_causes = summary.to_string();
        let due = self.last_repaint_note.elapsed() >= Duration::from_secs(2);
        if !force && !due {
            return;
        }
        let mut line = self.base_line();
        line.kind = "repaint";
        line.name = "causes";
        line.detail = Some(self.repaint_causes.clone());
        self.queue(line);
        self.last_repaint_note = Instant::now();
    }

    fn base_line(&self) -> Line {
        Line {
            t: now_unix_ms(),
            app: self.app,
            kind: "event",
            name: "",
            ms: 0.0,
            delivered_ms: 0.0,
            n: 0,
            spans: None,
            detail: None,
            entries: self.snap.entries,
            nodes: self.snap.nodes,
            thumbs: self.snap.thumbs_pending,
            extra: self.snap.extra,
            scan: self.snap.scan_active,
            view: self.snap.view,
            tool: self.snap.tool,
        }
    }

    fn queue(&mut self, line: Line) {
        if self.pending.len() >= PENDING_CAP {
            self.pending.remove(0);
        }
        self.pending.push(line);
    }

    fn stack_names(&self) -> String {
        let mut s = String::new();
        for i in 0..self.stack_len {
            if !s.is_empty() {
                s.push('>');
            }
            s.push_str(self.stack[i].0);
        }
        s
    }

    fn completed_names(&self) -> String {
        let mut s = String::new();
        for i in 0..self.completed_len {
            if !s.is_empty() {
                s.push(',');
            }
            s.push_str(self.completed[i].0);
            s.push(':');
            s.push_str(&format!("{:.1}", self.completed[i].1));
        }
        if s.is_empty() {
            self.stack_names()
        } else {
            s
        }
    }

    fn flush(&mut self, write_latest: bool) {
        if self.persist && !self.pending.is_empty() {
            self.append_jsonl();
        }
        self.pending.clear();
        self.last_flush = Instant::now();
        if write_latest || self.last_latest.elapsed() >= LATEST_EVERY {
            self.write_latest();
        }
    }

    fn append_jsonl(&mut self) {
        if self.log_path.as_os_str().is_empty() {
            return;
        }
        if let Ok(meta) = std::fs::metadata(&self.log_path) {
            if meta.len() > JSONL_ROTATE {
                let prev = self
                    .log_path
                    .with_file_name(format!("{}-prev.jsonl", self.app));
                let _ = std::fs::rename(&self.log_path, prev);
            }
        }
        let mut buf = String::new();
        for line in &self.pending {
            if let Ok(json) = serde_json::to_string(line) {
                buf.push_str(&json);
                buf.push('\n');
            }
        }
        if buf.is_empty() {
            return;
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            let _ = f.write_all(buf.as_bytes());
        }
    }

    fn write_latest(&mut self) {
        if !self.persist || self.latest_path.as_os_str().is_empty() {
            return;
        }
        let stalls: Vec<StallFile> = self
            .stalls
            .iter()
            .rev()
            .take(8)
            .map(|s| StallFile {
                unix_ms: s.unix_ms,
                app_ms: s.app_ms,
                delivered_ms: s.delivered_ms,
                spans: s.spans.clone(),
                view: s.view,
                tool: s.tool,
                entries: s.entries,
                nodes: s.nodes,
            })
            .collect();
        let startup_phases: Vec<PhaseFile> = self
            .startup
            .as_ref()
            .map(|s| {
                s.phases
                    .iter()
                    .map(|(name, ms)| PhaseFile { name, ms: *ms })
                    .collect()
            })
            .unwrap_or_default();
        let startup = self.startup.as_ref().map(|s| StartupFile {
            ms: s.ms,
            phases: &startup_phases,
        });
        let file = LatestFile {
            app: self.app,
            pid: self.pid,
            started_unix: self.started_unix,
            persist: self.persist,
            log_path: &self.log_path,
            latest_path: &self.latest_path,
            frame: FrameSummary {
                n: self.filled,
                app_mean_ms: self.app_mean_ms(),
                app_max_ms: self.app_max_ms(),
                p95_ms: self.p95_ms(),
                max_ms: self.max_ms(),
                dropped: self.dropped(),
            },
            now: NowSummary {
                entries: self.snap.entries,
                nodes: self.snap.nodes,
                selected: self.snap.selected,
                thumbs_pending: self.snap.thumbs_pending,
                extra: self.snap.extra,
                scan_active: self.snap.scan_active,
                view: self.snap.view,
                tool: self.snap.tool,
                last_stall_app_ms: self.last_stall_app_ms,
            },
            last_stalls: &stalls,
            marks: &self.marks,
            startup,
            repaint_causes: &self.repaint_causes,
        };
        if let Ok(json) = serde_json::to_string_pretty(&file) {
            let tmp = self.latest_path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &self.latest_path);
            }
        }
        self.last_latest = Instant::now();
    }

    fn samples(&self) -> &[f32] {
        &self.delivered[..self.filled]
    }

    fn app_mean_ms(&self) -> f32 {
        if self.filled == 0 {
            return 0.0;
        }
        self.app_ms[..self.filled].iter().sum::<f32>() / self.filled as f32
    }

    fn app_max_ms(&self) -> f32 {
        self.app_ms[..self.filled]
            .iter()
            .copied()
            .fold(0.0, f32::max)
    }

    fn p95_ms(&self) -> f32 {
        self.percentile(0.95)
    }

    fn max_ms(&self) -> f32 {
        self.samples().iter().copied().fold(0.0, f32::max)
    }

    fn dropped(&self) -> usize {
        self.samples().iter().filter(|ms| **ms > BUDGET_MS).count()
    }

    fn percentile(&self, q: f32) -> f32 {
        let s = self.samples();
        if s.is_empty() {
            return 0.0;
        }
        let mut buf = [0.0f32; WINDOW];
        buf[..s.len()].copy_from_slice(s);
        let slice = &mut buf[..s.len()];
        slice.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        slice[((slice.len() as f32 * q) as usize).min(slice.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: f32) -> Duration {
        Duration::from_secs_f32(v / 1000.0)
    }

    const GOOD: f32 = 0.0160;
    const BAD: f32 = 0.050;

    #[test]
    fn an_empty_window_reports_nothing_rather_than_dividing_by_zero() {
        let s = SessionLog::memory("test");
        assert!(s.is_empty());
        assert_eq!(s.p95_ms(), 0.0);
        assert_eq!(s.app_mean_ms(), 0.0);
        assert_eq!(s.dropped(), 0);
    }

    #[test]
    fn the_tail_survives_a_sea_of_good_frames() {
        let s = SessionLog::memory("test");
        for _ in 0..100 {
            s.end_frame(ms(2.0), GOOD, FrameWake::default());
        }
        for _ in 0..10 {
            s.end_frame(ms(40.0), BAD, FrameWake::default());
        }
        assert_eq!(s.dropped(), 10);
        assert!(s.max_ms() > 45.0, "max was {}", s.max_ms());
        assert!(s.p95_ms() > 16.67, "p95 was {}", s.p95_ms());
        assert!(s.app_max_ms() > 39.0);
        assert!(!s.last_stalls().is_empty());
    }

    #[test]
    fn the_window_forgets_old_frames() {
        let s = SessionLog::memory("test");
        for _ in 0..WINDOW {
            s.end_frame(ms(40.0), BAD, FrameWake::default());
        }
        assert_eq!(s.dropped(), WINDOW);
        for _ in 0..WINDOW {
            s.end_frame(ms(2.0), GOOD, FrameWake::default());
        }
        assert_eq!(s.dropped(), 0, "a recovered board stops reporting stutter");
        assert_eq!(s.frames(), WINDOW);
    }

    #[test]
    fn a_slow_span_is_what_a_stall_names() {
        let s = SessionLog::memory("test");
        let _attach = s.attach();
        {
            let _g = span("atlas.tree.rebuild");
            std::thread::sleep(Duration::from_millis(12));
        }
        s.set_snapshot(Snapshot {
            entries: 8421,
            view: "tree",
            tool: "view",
            scan_active: true,
            ..Snapshot::default()
        });
        s.end_frame(ms(40.0), BAD, FrameWake::default());
        let stalls = s.last_stalls();
        assert_eq!(stalls.len(), 1);
        assert!(
            stalls[0].spans.contains("atlas.tree.rebuild"),
            "spans were {}",
            stalls[0].spans
        );
        assert_eq!(stalls[0].entries, 8421);
    }

    #[test]
    fn persist_writes_jsonl_and_latest() {
        let dir = std::env::temp_dir().join(format!(
            "atlas_session_log_{}_{}",
            std::process::id(),
            now_unix_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let s = SessionLog::persist_at("test-app", dir.clone());
        let _attach = s.attach();
        s.set_snapshot(Snapshot {
            entries: 12,
            view: "tree",
            ..Snapshot::default()
        });
        mark("after opening the share");
        s.end_frame(ms(80.0), 0.090, FrameWake::default());
        let jsonl = std::fs::read_to_string(dir.join("test-app.jsonl")).unwrap();
        assert!(jsonl.contains("\"kind\":\"mark\""));
        assert!(jsonl.contains("after opening the share"));
        assert!(jsonl.contains("\"kind\":\"stall\""));
        let latest = std::fs::read_to_string(dir.join("test-app-latest.json")).unwrap();
        assert!(latest.contains("test-app"));
        assert!(latest.contains("last_stalls"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_idle_repaint_gap_is_not_a_stall() {
        let s = SessionLog::memory("test");
        s.end_frame(ms(2.0), 0.234, FrameWake::default());
        assert!(s.last_stalls().is_empty());
    }

    #[test]
    fn a_delivered_gap_with_input_is_a_stall() {
        let s = SessionLog::memory("test");
        s.end_frame(
            ms(2.0),
            0.234,
            FrameWake {
                had_input: true,
                eager: false,
            },
        );
        assert_eq!(s.last_stalls().len(), 1);
        assert!(s.last_stalls()[0].app_ms < 33.0);
    }

    #[test]
    fn a_delivered_gap_after_an_eager_repaint_is_a_stall() {
        let s = SessionLog::memory("test");
        s.end_frame(
            ms(2.0),
            0.234,
            FrameWake {
                had_input: false,
                eager: true,
            },
        );
        assert_eq!(s.last_stalls().len(), 1);
    }

    #[test]
    fn app_time_is_a_stall_even_when_the_frame_was_idle() {
        let s = SessionLog::memory("test");
        s.end_frame(ms(40.0), GOOD, FrameWake::default());
        assert_eq!(s.last_stalls().len(), 1);
    }

    #[test]
    fn startup_is_one_record_in_the_log_and_the_snapshot() {
        let dir = std::env::temp_dir().join(format!(
            "atlas_session_startup_{}_{}",
            std::process::id(),
            now_unix_ms()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let s = SessionLog::persist_at("test-app", dir.clone());
        let mut boot = Startup::begin();
        boot.phase("fonts");
        boot.phase("first_frame");
        s.record_startup(&boot);
        let jsonl = std::fs::read_to_string(dir.join("test-app.jsonl")).unwrap();
        assert!(jsonl.contains("\"kind\":\"startup\""));
        assert!(jsonl.contains("fonts:"));
        let latest = std::fs::read_to_string(dir.join("test-app-latest.json")).unwrap();
        assert!(latest.contains("\"startup\""));
        assert!(latest.contains("first_frame"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spans_without_attach_are_silent() {
        // A test that never attached must not panic, and must not record.
        let _g = span("nobody");
        count("nobody", 3);
        event("nobody", "x");
        mark("x");
    }
}
