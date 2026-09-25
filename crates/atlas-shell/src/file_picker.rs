//! Native file and folder pickers, owned by and modal to the app window.
//!
//! Every app window has one [`DialogGate`]. Every [`FilePicker`] for that
//! window — the app's own and the AI panel's — is built from it
//! ([`DialogGate::picker`]), and the app wires the window exactly once: the
//! owner HWND from `eframe::Frame` in `update` ([`DialogGate::set_owner`]),
//! the input gate in `raw_input_hook` ([`DialogGate::gate_input`]), and one
//! [`DialogGate::close_all`] when a drop lands. [`DialogGate::any_open`]
//! answers restart/close blocking.
//!
//! **One dialog at a time per app.** A second window of the same app (the
//! File Atlas viewport Slate hosts in a linked session) gets
//! [`DialogGate::other_window`]: its own owner and gate, one shared scope. A
//! picker asked to open while any dialog in the scope is up opens nothing and
//! flashes the open dialog instead. The scope is passed explicitly (not a
//! process-wide static) so parallel tests stay isolated and standalone apps,
//! being separate processes, are unaffected.
//!
//! The dialog runs on its own thread so the frame loop keeps painting
//! (Art. II), owned by the window's HWND: it opens above the undecorated
//! window and stays there (`Show(NULL)` often opened it behind, which looked
//! like a freeze). A gate never told its HWND (a hosted viewport, whose
//! `eframe::App::update` never runs) uses the UI thread's active window at
//! open — the window the user just clicked.
//!
//! While a dialog is up its window swallows its own input; a click, key
//! press, or close request brings the dialog forward and flashes it — the
//! owned-modal cue Windows gives. The owner is deliberately kept *enabled*:
//! `IFileDialog::Show` disables it, and a disabled window cannot be relied on
//! to take the gesture users actually make, dragging a file out of the dialog
//! onto the canvas. The app accepts that drop as an ordinary external drop
//! and calls `close_all`, which cancels the dialog the way its Cancel button
//! does; the dialog's own result is discarded.
//!
//! A window whose gate is never called (again, the hosted viewport: its
//! host's `raw_input_hook` does not see it) is left disabled, so the native
//! modal supplies modality there. The cost is drag-to-dismiss in that window
//! only.

use crossbeam_channel::{Receiver, TryRecvError};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

/// Frame cadence while a dialog is up: re-enable the owner the dialog just
/// disabled, and deliver the result promptly.
const POLL_INTERVAL: Duration = Duration::from_millis(33);
/// A cancel posted before the dialog window exists is repeated at this rate.
const CANCEL_RETRY: Duration = Duration::from_millis(150);

/// The window a dialog is modal to, as an integer so it can ride the picker
/// thread (`RawWindowHandle` is not `Send`).
#[derive(Clone, Copy, Debug)]
pub struct DialogOwner {
    #[cfg(windows)]
    hwnd: std::num::NonZeroIsize,
}

impl DialogOwner {
    pub fn from_window(handle: &impl raw_window_handle::HasWindowHandle) -> Option<Self> {
        #[cfg(windows)]
        {
            match handle.window_handle().ok()?.as_raw() {
                raw_window_handle::RawWindowHandle::Win32(win32) => Some(Self { hwnd: win32.hwnd }),
                _ => None,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = handle;
            None
        }
    }

    #[cfg(test)]
    fn fake() -> Self {
        Self {
            #[cfg(windows)]
            hwnd: std::num::NonZeroIsize::new(1).unwrap(),
        }
    }
}

#[cfg(windows)]
impl raw_window_handle::HasWindowHandle for DialogOwner {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::Win32WindowHandle::new(self.hwnd);
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::Win32(
                handle,
            ))
        })
    }
}

#[cfg(windows)]
impl raw_window_handle::HasDisplayHandle for DialogOwner {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe {
            raw_window_handle::DisplayHandle::borrow_raw(
                raw_window_handle::RawDisplayHandle::Windows(
                    raw_window_handle::WindowsDisplayHandle::new(),
                ),
            )
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    File,
    Files,
    Folder,
    Folders,
    Save,
}

/// What to ask the user for. Every mode yields `Option<Vec<PathBuf>>`
/// (`None` = cancelled); single-path modes yield at most one path — see
/// [`first`].
#[derive(Clone, Debug)]
pub struct PickRequest {
    mode: Mode,
    title: Option<String>,
    filters: Vec<(String, Vec<String>)>,
    directory: Option<PathBuf>,
    file_name: Option<String>,
}

impl PickRequest {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            title: None,
            filters: Vec::new(),
            directory: None,
            file_name: None,
        }
    }
    pub fn file() -> Self {
        Self::new(Mode::File)
    }
    pub fn files() -> Self {
        Self::new(Mode::Files)
    }
    pub fn folder() -> Self {
        Self::new(Mode::Folder)
    }
    pub fn folders() -> Self {
        Self::new(Mode::Folders)
    }
    pub fn save() -> Self {
        Self::new(Mode::Save)
    }
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    pub fn filter(mut self, name: impl Into<String>, extensions: &[impl ToString]) -> Self {
        let extensions = extensions.iter().map(ToString::to_string).collect();
        self.filters.push((name.into(), extensions));
        self
    }
    pub fn directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self
    }
    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    /// Blocks the calling (picker) thread until the dialog closes.
    fn run(self, owner: Option<DialogOwner>) -> Option<Vec<PathBuf>> {
        let mut dialog = rfd::FileDialog::new();
        if let Some(title) = self.title {
            dialog = dialog.set_title(title);
        }
        for (name, extensions) in &self.filters {
            dialog = dialog.add_filter(name, extensions);
        }
        if let Some(dir) = self.directory {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = self.file_name {
            dialog = dialog.set_file_name(name);
        }
        #[cfg(windows)]
        if let Some(owner) = &owner {
            dialog = dialog.set_parent(owner);
        }
        #[cfg(not(windows))]
        let _ = owner;
        match self.mode {
            Mode::File => dialog.pick_file().map(|p| vec![p]),
            Mode::Files => dialog.pick_files(),
            Mode::Folder => dialog.pick_folder().map(|p| vec![p]),
            Mode::Folders => dialog.pick_folders(),
            Mode::Save => dialog.save_file().map(|p| vec![p]),
        }
    }
}

/// The single path of a `file` / `folder` / `save` pick.
pub fn first(paths: Option<Vec<PathBuf>>) -> Option<PathBuf> {
    paths.and_then(|p| p.into_iter().next())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    /// A dialog is on screen and its pick will be delivered.
    Open,
    /// The dialog was told to cancel; whatever it reports is discarded.
    Closing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Deliver,
    Discard,
}

/// The picker's lifecycle, free of threads and windows.
#[derive(Debug, Default)]
pub struct PickerState {
    phase: Phase,
    generation: u64,
}

impl PickerState {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// A dialog is on screen (open or still closing): the window stays gated.
    pub fn is_open(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Start a session. `None` while one is already up — one dialog at a time.
    pub fn begin(&mut self) -> Option<u64> {
        if self.phase != Phase::Idle {
            return None;
        }
        self.generation += 1;
        self.phase = Phase::Open;
        Some(self.generation)
    }

    /// Dismiss the dialog (a drop landed on the window, or the app needs it
    /// gone). `true` means a cancel must be sent to the dialog now.
    pub fn close(&mut self) -> bool {
        if self.phase != Phase::Open {
            return false;
        }
        self.phase = Phase::Closing;
        true
    }

    /// The dialog thread for `generation` reported (a pick, a cancel, or
    /// exit). Only a pick from a dialog that was not dismissed is delivered.
    pub fn finished(&mut self, generation: u64) -> Outcome {
        if generation != self.generation || self.phase == Phase::Idle {
            return Outcome::Discard;
        }
        let outcome = match self.phase {
            Phase::Open => Outcome::Deliver,
            _ => Outcome::Discard,
        };
        self.phase = Phase::Idle;
        outcome
    }
}

/// Window ids only need to be distinct within a process.
static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);

/// The app's one-dialog slot, shared by all of its windows.
#[derive(Default)]
struct Scope {
    active: Mutex<Weak<DialogCore>>,
}

/// A window's dialog handle. Clone freely; clones are the same window.
#[derive(Clone)]
pub struct DialogGate {
    scope: Arc<Scope>,
    window: u64,
    owner: Arc<Mutex<Option<DialogOwner>>>,
}

impl Default for DialogGate {
    fn default() -> Self {
        Self::new()
    }
}

impl DialogGate {
    /// The main window of a new app scope.
    pub fn new() -> Self {
        Self::in_scope(Arc::default())
    }

    fn in_scope(scope: Arc<Scope>) -> Self {
        Self {
            scope,
            window: NEXT_WINDOW.fetch_add(1, Ordering::Relaxed),
            owner: Arc::default(),
        }
    }

    /// Another window of the same app: its own owner and gate, and the same
    /// one-dialog rule.
    pub fn other_window(&self) -> Self {
        Self::in_scope(self.scope.clone())
    }

    /// The window's HWND. Apps call this from `eframe::App::update` with the
    /// `Frame`; a gate that never hears it falls back to the active window.
    pub fn set_owner(&self, owner: Option<DialogOwner>) {
        *self.owner.lock().unwrap() = owner;
    }

    /// A dialog slot for this window.
    pub fn picker<M: Send + 'static>(&self) -> FilePicker<M> {
        FilePicker {
            gate: self.clone(),
            state: PickerState::default(),
            session: None,
        }
    }

    /// A dialog is up in any window of this app.
    pub fn any_open(&self) -> bool {
        self.active().is_some()
    }

    /// Cancel the open dialog, whichever slot raised it. Call when an external
    /// drop replaced the pick.
    pub fn close_all(&self) {
        if let Some(core) = self.active() {
            core.request_close();
        }
    }

    /// From `eframe::App::raw_input_hook`: while this window's dialog is up,
    /// the window ignores input and an attempt to act brings the dialog
    /// forward. Returns `true` when it gated.
    pub fn gate_input(&self, raw: &mut egui::RawInput) -> bool {
        let Some(core) = self.active() else {
            return false;
        };
        if core.window != self.window {
            return false;
        }
        core.gated.store(true, Ordering::Release);
        if core.attention_due(swallow_input(raw)) {
            win::attention(core.thread());
        }
        true
    }

    fn active(&self) -> Option<Arc<DialogCore>> {
        self.scope.active.lock().unwrap().upgrade()
    }

    /// Claim the app's dialog slot. `None` (after flashing the open dialog)
    /// while one is up. `fallback` resolves an unknown owner to the calling
    /// thread's active window, which is only meaningful on the UI thread.
    fn begin(&self, fallback: bool) -> Option<Arc<DialogCore>> {
        let mut active = self.scope.active.lock().unwrap();
        if let Some(open) = active.upgrade() {
            win::attention(open.thread());
            return None;
        }
        let owner = *self.owner.lock().unwrap();
        let core = Arc::new(DialogCore {
            window: self.window,
            owner: owner.or_else(|| if fallback { win::active_window() } else { None }),
            thread: AtomicU32::new(0),
            close_requested: AtomicBool::new(false),
            gated: AtomicBool::new(false),
            last_cancel: Mutex::new(None),
        });
        *active = Arc::downgrade(&core);
        Some(core)
    }
}

/// One dialog on screen. Alive while its slot is waiting or its thread runs.
struct DialogCore {
    window: u64,
    owner: Option<DialogOwner>,
    /// Picker thread id (0 until it starts, and for adopted channels).
    thread: AtomicU32,
    close_requested: AtomicBool,
    /// The owning window's gate ran since open: that window supplies
    /// modality, so the owner may be re-enabled for drops.
    gated: AtomicBool,
    last_cancel: Mutex<Option<Instant>>,
}

impl DialogCore {
    fn thread(&self) -> u32 {
        self.thread.load(Ordering::Acquire)
    }

    /// The user tried to act in the owning window: bring the dialog forward,
    /// unless it is already on its way out.
    fn attention_due(&self, attempted: bool) -> bool {
        attempted && !self.close_requested.load(Ordering::Acquire)
    }

    fn keeps_owner_enabled(&self) -> bool {
        self.owner.is_some()
            && self.gated.load(Ordering::Acquire)
            && !self.close_requested.load(Ordering::Acquire)
    }

    fn request_close(&self) {
        if !self.close_requested.swap(true, Ordering::AcqRel) {
            self.cancel_now();
        }
    }

    /// A cancel posted before the dialog window exists finds nothing; retry.
    fn cancel_if_due(&self) {
        let due = self
            .last_cancel
            .lock()
            .unwrap()
            .is_none_or(|at| at.elapsed() >= CANCEL_RETRY);
        if due {
            self.cancel_now();
        }
    }

    fn cancel_now(&self) {
        win::cancel(self.thread());
        *self.last_cancel.lock().unwrap() = Some(Instant::now());
    }
}

struct Session<M> {
    generation: u64,
    rx: Receiver<M>,
    core: Arc<DialogCore>,
}

/// One dialog slot of a window, built by [`DialogGate::picker`]. `M` is the
/// caller's result message, built on the picker thread from the paths by the
/// `map` passed to [`FilePicker::open`].
pub struct FilePicker<M> {
    gate: DialogGate,
    state: PickerState,
    session: Option<Session<M>>,
}

impl<M: Send + 'static> FilePicker<M> {
    /// This slot's dialog is up.
    pub fn is_open(&self) -> bool {
        self.state.is_open()
    }

    /// Show a dialog. Returns `false` (and shows nothing) while any dialog of
    /// this app is open.
    pub fn open(
        &mut self,
        request: PickRequest,
        map: impl FnOnce(Option<Vec<PathBuf>>) -> M + Send + 'static,
    ) -> bool {
        let Some(core) = self.gate.begin(true) else {
            return false;
        };
        let Some(generation) = self.state.begin() else {
            return false;
        };
        let (tx, rx) = crossbeam_channel::bounded(1);
        {
            let core = core.clone();
            std::thread::spawn(move || {
                core.thread
                    .store(win::current_thread_id(), Ordering::Release);
                let owner = core.owner;
                let picked = map(request.run(owner));
                // The dialog is gone: free the slot before the result lands.
                drop(core);
                let _ = tx.send(picked);
            });
        }
        self.session = Some(Session {
            generation,
            rx,
            core,
        });
        true
    }

    /// Treat `rx` as the result of an open dialog without showing one — for
    /// tests, which must never raise a native window. Same admission rule as
    /// [`FilePicker::open`].
    pub fn adopt(&mut self, rx: Receiver<M>) -> bool {
        let Some(core) = self.gate.begin(false) else {
            return false;
        };
        let Some(generation) = self.state.begin() else {
            return false;
        };
        self.session = Some(Session {
            generation,
            rx,
            core,
        });
        true
    }

    /// Call every frame. Returns the pick once, unless the dialog was closed.
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<M> {
        let session = self.session.as_ref()?;
        if session.core.close_requested.load(Ordering::Acquire) {
            self.state.close();
        }
        match session.rx.try_recv() {
            Ok(msg) => {
                let generation = session.generation;
                self.session = None;
                (self.state.finished(generation) == Outcome::Deliver).then_some(msg)
            }
            Err(TryRecvError::Disconnected) => {
                let generation = session.generation;
                self.session = None;
                self.state.finished(generation);
                None
            }
            Err(TryRecvError::Empty) => {
                let core = &session.core;
                if self.state.phase() == Phase::Closing {
                    core.cancel_if_due();
                } else if core.keeps_owner_enabled() {
                    if let Some(owner) = core.owner {
                        win::keep_enabled(owner);
                    }
                }
                ctx.request_repaint_after(POLL_INTERVAL);
                None
            }
        }
    }

    /// Cancel this slot's dialog as if the user pressed Cancel; its result is
    /// discarded. Apps reacting to a drop use [`DialogGate::close_all`].
    pub fn close(&mut self) {
        if let Some(session) = &self.session {
            session.core.request_close();
            self.state.close();
        }
    }
}

impl<M> Drop for FilePicker<M> {
    /// A slot dropped mid-dialog (a hosted window closing) cancels it rather
    /// than leave an orphan holding the app's dialog slot.
    fn drop(&mut self) {
        if let Some(session) = &self.session {
            session.core.request_close();
        }
    }
}

/// Strip input from `raw`, keeping focus changes, screenshots, file drags,
/// and releases (so nothing pressed before the dialog stays held). Returns
/// `true` when the user tried to act: a press, a key, or a close request.
pub fn swallow_input(raw: &mut egui::RawInput) -> bool {
    let mut attempted = false;
    raw.events.retain(|event| match event {
        egui::Event::WindowFocused(_) | egui::Event::Screenshot { .. } => true,
        egui::Event::PointerButton { pressed, .. } | egui::Event::Key { pressed, .. }
            if !*pressed =>
        {
            true
        }
        egui::Event::PointerButton { .. } => {
            attempted = true;
            false
        }
        egui::Event::Key { repeat, .. } => {
            attempted |= !*repeat;
            false
        }
        _ => false,
    });
    raw.events.push(egui::Event::PointerGone);
    if let Some(viewport) = raw.viewports.get_mut(&raw.viewport_id) {
        let before = viewport.events.len();
        viewport
            .events
            .retain(|e| !matches!(e, egui::ViewportEvent::Close));
        attempted |= viewport.events.len() != before;
    }
    attempted
}

#[cfg(windows)]
mod win {
    use super::DialogOwner;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        EnableWindow, GetActiveWindow, IsWindowEnabled,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, FlashWindowEx, GetClassNameW, IsWindowVisible, PostMessageW,
        SetForegroundWindow, FLASHWINFO, FLASHW_CAPTION, IDCANCEL, WM_COMMAND,
    };

    /// Caption flashes and their interval: a short burst, like the system's
    /// cue for a click on a window behind a modal dialog.
    const FLASH_COUNT: u32 = 6;
    const FLASH_INTERVAL_MS: u32 = 60;

    pub(super) fn current_thread_id() -> u32 {
        unsafe { GetCurrentThreadId() }
    }

    fn hwnd(owner: DialogOwner) -> HWND {
        HWND(owner.hwnd.get() as *mut _)
    }

    /// The calling thread's active window: on the UI thread, the app window
    /// the user just acted in.
    pub(super) fn active_window() -> Option<DialogOwner> {
        let active = unsafe { GetActiveWindow() };
        std::num::NonZeroIsize::new(active.0 as isize).map(|hwnd| DialogOwner { hwnd })
    }

    /// The picker thread's visible dialog windows (`#32770`; the Common Item
    /// Dialog frame and any message box it raised), innermost first: a
    /// nested box disables the dialog beneath it.
    pub(super) fn dialogs(thread: u32) -> Vec<HWND> {
        unsafe extern "system" fn collect(hwnd: HWND, found: LPARAM) -> BOOL {
            let mut class = [0u16; 16];
            let len = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
            if class[..len].iter().copied().eq("#32770".encode_utf16())
                && unsafe { IsWindowVisible(hwnd) }.as_bool()
            {
                unsafe { (*(found.0 as *mut Vec<HWND>)).push(hwnd) };
            }
            true.into()
        }
        let mut found = Vec::new();
        if thread != 0 {
            unsafe {
                let _ = EnumThreadWindows(
                    thread,
                    Some(collect),
                    LPARAM(&mut found as *mut Vec<HWND> as isize),
                );
            }
        }
        found.sort_by_key(|&h| !unsafe { IsWindowEnabled(h) }.as_bool());
        found
    }

    /// Post Cancel to the dialog. `PostMessage` is thread-safe and IDCANCEL
    /// runs the dialog's own cancel path, so `Show` returns cancelled.
    pub(super) fn cancel(thread: u32) {
        for dialog in dialogs(thread) {
            unsafe {
                let _ = PostMessageW(
                    Some(dialog),
                    WM_COMMAND,
                    WPARAM(IDCANCEL.0 as usize),
                    LPARAM(0),
                );
            }
        }
    }

    pub(super) fn attention(thread: u32) {
        let Some(&dialog) = dialogs(thread).first() else {
            return;
        };
        let flash = FLASHWINFO {
            cbSize: size_of::<FLASHWINFO>() as u32,
            hwnd: dialog,
            dwFlags: FLASHW_CAPTION,
            uCount: FLASH_COUNT,
            dwTimeout: FLASH_INTERVAL_MS,
        };
        unsafe {
            let _ = SetForegroundWindow(dialog);
            let _ = FlashWindowEx(&flash);
        }
    }

    /// `IFileDialog::Show` disables its owner. Undo that so an external drop
    /// can still land; `gate_input` supplies the modality instead.
    pub(super) fn keep_enabled(owner: DialogOwner) {
        let hwnd = hwnd(owner);
        unsafe {
            if !IsWindowEnabled(hwnd).as_bool() {
                let _ = EnableWindow(hwnd, true);
            }
        }
    }
}

#[cfg(not(windows))]
mod win {
    use super::DialogOwner;

    pub(super) fn current_thread_id() -> u32 {
        0
    }
    pub(super) fn active_window() -> Option<DialogOwner> {
        None
    }
    pub(super) fn cancel(_thread: u32) {}
    pub(super) fn attention(_thread: u32) {}
    pub(super) fn keep_enabled(_owner: DialogOwner) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: egui::pos2(10.0, 10.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn key(pressed: bool, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key: egui::Key::O,
            physical_key: None,
            pressed,
            repeat,
            modifiers: egui::Modifiers::COMMAND,
        }
    }

    fn clicked() -> egui::RawInput {
        egui::RawInput {
            events: vec![press(true)],
            ..Default::default()
        }
    }

    #[test]
    fn open_then_pick_delivers_once() {
        let mut s = PickerState::default();
        let generation = s.begin().unwrap();
        assert!(s.is_open());
        assert_eq!(s.begin(), None, "one dialog at a time");
        assert_eq!(s.finished(generation), Outcome::Deliver);
        assert!(!s.is_open());
        assert_eq!(s.finished(generation), Outcome::Discard);
    }

    #[test]
    fn open_then_drop_closes_and_discards_the_dialog_result() {
        let mut s = PickerState::default();
        let generation = s.begin().unwrap();
        assert!(s.close(), "a drop must cancel the dialog");
        assert_eq!(s.phase(), Phase::Closing);
        assert!(s.is_open(), "gated until the dialog is really gone");
        assert!(!s.close(), "cancel is sent once, then retried by poll");
        assert_eq!(s.finished(generation), Outcome::Discard);
        assert!(!s.is_open());
    }

    #[test]
    fn open_then_input_requests_attention() {
        let gate = DialogGate::new();
        let mut picker = gate.picker::<u32>();
        let (_tx, rx) = crossbeam_channel::unbounded();
        picker.adopt(rx);
        let core = picker.session.as_ref().unwrap().core.clone();
        assert!(!core.attention_due(false), "hover and wheel do not flash");
        assert!(core.attention_due(true));
        gate.close_all();
        assert!(
            !core.attention_due(true),
            "no flash for a dialog on its way out"
        );
    }

    #[test]
    fn a_late_result_from_an_earlier_session_is_discarded() {
        let mut s = PickerState::default();
        let old = s.begin().unwrap();
        s.close();
        assert_eq!(s.finished(old), Outcome::Discard);
        let new = s.begin().unwrap();
        assert_eq!(s.finished(old), Outcome::Discard);
        assert!(s.is_open());
        assert_eq!(s.finished(new), Outcome::Deliver);
    }

    #[test]
    fn swallow_keeps_releases_focus_and_drops() {
        let mut raw = egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(egui::pos2(1.0, 1.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, 1.0),
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Text("o".into()),
                press(false),
                key(false, false),
                egui::Event::WindowFocused(true),
            ],
            dropped_files: vec![egui::DroppedFile {
                path: Some("C:/a.png".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(!swallow_input(&mut raw), "moves and wheel are not attempts");
        assert_eq!(
            raw.events,
            vec![
                press(false),
                key(false, false),
                egui::Event::WindowFocused(true),
                egui::Event::PointerGone,
            ]
        );
        assert_eq!(raw.dropped_files.len(), 1, "drops pass through the gate");
    }

    #[test]
    fn swallow_reports_presses_keys_and_close_as_attempts() {
        let mut raw = clicked();
        assert!(swallow_input(&mut raw));
        assert_eq!(raw.events, vec![egui::Event::PointerGone]);

        raw.events = vec![key(true, true)];
        assert!(!swallow_input(&mut raw), "held-key repeats do not re-flash");
        raw.events = vec![key(true, false)];
        assert!(swallow_input(&mut raw));

        raw.events.clear();
        raw.viewports
            .entry(raw.viewport_id)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        assert!(swallow_input(&mut raw));
        assert!(raw.viewports[&raw.viewport_id].events.is_empty());
    }

    #[test]
    fn picker_delivers_and_gates_through_adopted_channel() {
        let ctx = egui::Context::default();
        let gate = DialogGate::new();
        let mut picker = gate.picker::<u32>();
        assert!(!gate.gate_input(&mut clicked()));

        let (tx, rx) = crossbeam_channel::unbounded();
        assert!(picker.adopt(rx));
        assert!(picker.is_open() && gate.any_open());
        let mut raw = clicked();
        assert!(gate.gate_input(&mut raw));
        assert_eq!(raw.events, vec![egui::Event::PointerGone]);
        assert_eq!(picker.poll(&ctx), None);
        tx.send(7).unwrap();
        assert_eq!(picker.poll(&ctx), Some(7));
        assert!(!picker.is_open() && !gate.any_open());
    }

    #[test]
    fn the_gate_refuses_a_second_dialog_across_slots_and_windows() {
        let ctx = egui::Context::default();
        let gate = DialogGate::new();
        let hosted = gate.other_window();
        let mut app = gate.picker::<u32>();
        let mut ai = gate.picker::<u32>();
        let mut hosted_app = hosted.picker::<u32>();

        let (tx, rx) = crossbeam_channel::unbounded();
        assert!(app.adopt(rx));
        assert!(!ai.adopt(crossbeam_channel::unbounded().1), "same window");
        assert!(
            !hosted_app.adopt(crossbeam_channel::unbounded().1),
            "other window, same app"
        );
        assert!(!app.adopt(crossbeam_channel::unbounded().1), "same slot");
        assert!(hosted.any_open(), "every window sees the open dialog");

        tx.send(1).unwrap();
        assert_eq!(app.poll(&ctx), Some(1));
        assert!(ai.adopt(crossbeam_channel::unbounded().1), "slot freed");
        assert!(!DialogGate::new().any_open(), "another app is unaffected");
    }

    #[test]
    fn close_all_closes_whichever_slot_is_open() {
        let ctx = egui::Context::default();
        let gate = DialogGate::new();
        let hosted = gate.other_window();
        let mut app = gate.picker::<u32>();
        let mut ai = gate.picker::<u32>();

        let (tx, rx) = crossbeam_channel::unbounded();
        app.adopt(rx);
        gate.close_all();
        tx.send(1).unwrap();
        assert_eq!(
            app.poll(&ctx),
            None,
            "a dismissed dialog's pick is discarded"
        );
        assert!(!gate.any_open());

        let (tx, rx) = crossbeam_channel::unbounded();
        ai.adopt(rx);
        hosted.close_all();
        assert!(ai.is_open(), "gated until the dialog reports");
        tx.send(2).unwrap();
        assert_eq!(ai.poll(&ctx), None);
        assert!(!gate.any_open());
    }

    #[test]
    fn slot_close_discards_the_dialog_result() {
        let ctx = egui::Context::default();
        let gate = DialogGate::new();
        let mut picker = gate.picker::<u32>();
        let (tx, rx) = crossbeam_channel::unbounded();
        picker.adopt(rx);
        picker.close();
        assert!(picker.is_open());
        tx.send(7).unwrap();
        assert_eq!(picker.poll(&ctx), None);
        assert!(!picker.is_open());
    }

    #[test]
    fn only_the_owning_window_gates_and_re_enables_its_owner() {
        let gate = DialogGate::new();
        let hosted = gate.other_window();
        gate.set_owner(Some(DialogOwner::fake()));
        let mut picker = gate.picker::<u32>();
        let (_tx, rx) = crossbeam_channel::unbounded();
        picker.adopt(rx);
        let core = picker.session.as_ref().unwrap().core.clone();
        assert!(
            !core.keeps_owner_enabled(),
            "no gate since open (a hosted viewport): leave the native modal alone"
        );

        let mut raw = clicked();
        assert!(!hosted.gate_input(&mut raw), "another window's dialog");
        assert_eq!(raw.events, vec![press(true)]);
        assert!(!core.keeps_owner_enabled());

        assert!(gate.gate_input(&mut clicked()));
        assert!(core.keeps_owner_enabled(), "the gate supplies modality now");
        picker.close();
        assert!(
            !core.keeps_owner_enabled(),
            "a closing dialog re-enables it"
        );
    }

    #[test]
    fn a_vanished_dialog_thread_or_dropped_slot_frees_the_app() {
        let ctx = egui::Context::default();
        let gate = DialogGate::new();
        let mut picker = gate.picker::<u32>();
        let (tx, rx) = crossbeam_channel::unbounded();
        picker.adopt(rx);
        drop(tx);
        assert_eq!(picker.poll(&ctx), None);
        assert!(!picker.is_open() && !gate.any_open());

        let (_tx, rx) = crossbeam_channel::unbounded();
        picker.adopt(rx);
        drop(picker);
        assert!(!gate.any_open());
    }

    #[cfg(windows)]
    mod real {
        //! Raise real dialogs. Ignored: they put windows on the desktop for a
        //! moment.
        use super::*;
        use windows::core::w;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Input::KeyboardAndMouse::{IsWindowEnabled, SetActiveWindow};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetWindow, IsWindowVisible,
            PeekMessageW, TranslateMessage, GW_OWNER, MSG, PM_REMOVE, WINDOW_EX_STYLE,
            WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };

        /// A dialog thread's EnableWindow is a cross-thread send to this one.
        fn pump() {
            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        fn wait(what: &str, done: &mut dyn FnMut() -> bool) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done() {
                assert!(Instant::now() < deadline, "timed out: {what}");
                pump();
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn window() -> HWND {
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!("atlas-shell owner"),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    80,
                    80,
                    480,
                    320,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .unwrap()
        }

        fn owner_of(hwnd: HWND) -> DialogOwner {
            DialogOwner {
                hwnd: std::num::NonZeroIsize::new(hwnd.0 as isize).unwrap(),
            }
        }

        fn enabled(hwnd: HWND) -> bool {
            unsafe { IsWindowEnabled(hwnd) }.as_bool()
        }

        fn dialog_of<M: Send + 'static>(picker: &FilePicker<M>) -> HWND {
            let core = picker.session.as_ref().unwrap().core.clone();
            let mut dialog = None;
            wait("dialog", &mut || {
                dialog = win::dialogs(core.thread()).first().copied();
                dialog.is_some()
            });
            dialog.unwrap()
        }

        fn close_and_wait<M: Send + 'static>(gate: &DialogGate, picker: &mut FilePicker<M>) {
            let ctx = egui::Context::default();
            gate.close_all();
            wait("cancel", &mut || {
                assert!(picker.poll(&ctx).is_none());
                !picker.is_open()
            });
            assert!(!gate.any_open());
        }

        #[test]
        #[ignore]
        fn real_dialog_is_cancelled_by_close_all() {
            let gate = DialogGate::new();
            let mut picker = gate.picker::<Option<Vec<PathBuf>>>();
            assert!(picker.open(PickRequest::file().title("atlas-shell test"), |p| p));
            dialog_of(&picker);
            close_and_wait(&gate, &mut picker);
        }

        /// The root window: owned, disabled by `Show`, left disabled until
        /// its gate runs (the hosted-viewport path), then re-enabled while
        /// the dialog stays up.
        #[test]
        #[ignore]
        fn real_owned_dialog_is_re_enabled_only_once_gated() {
            let ctx = egui::Context::default();
            let hwnd = window();
            let gate = DialogGate::new();
            gate.set_owner(Some(owner_of(hwnd)));
            let mut picker = gate.picker::<Option<Vec<PathBuf>>>();
            assert!(picker.open(PickRequest::folder().title("atlas-shell owned test"), |p| p));
            let dialog = dialog_of(&picker);
            assert_eq!(unsafe { GetWindow(dialog, GW_OWNER) }.ok(), Some(hwnd));
            wait("Show disables its owner", &mut || !enabled(hwnd));

            for _ in 0..5 {
                assert_eq!(picker.poll(&ctx), None);
                pump();
            }
            assert!(!enabled(hwnd), "ungated window keeps the native modal");

            assert!(gate.gate_input(&mut clicked()), "attention path runs");
            assert_eq!(picker.poll(&ctx), None);
            pump();
            assert!(enabled(hwnd), "gated window is re-enabled for drops");
            assert!(
                unsafe { IsWindowVisible(dialog) }.as_bool(),
                "dialog stays up"
            );

            close_and_wait(&gate, &mut picker);
            assert!(enabled(hwnd));
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }

        /// A gate never told its HWND (a hosted viewport) owns the dialog by
        /// the thread's active window.
        #[test]
        #[ignore]
        fn real_unowned_gate_uses_the_active_window() {
            let hwnd = window();
            unsafe {
                let _ = SetActiveWindow(hwnd);
            }
            pump();
            let hosted = DialogGate::new().other_window();
            let mut picker = hosted.picker::<Option<Vec<PathBuf>>>();
            assert!(picker.open(PickRequest::file().title("atlas-shell hosted test"), |p| p));
            let dialog = dialog_of(&picker);
            assert_eq!(unsafe { GetWindow(dialog, GW_OWNER) }.ok(), Some(hwnd));
            close_and_wait(&hosted, &mut picker);
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    }
}
