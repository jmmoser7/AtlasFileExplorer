//! Native file and folder pickers, owned by and modal to the app window.
//!
//! Both apps open every system file dialog through [`FilePicker`]. The dialog
//! runs on its own thread so the frame loop keeps painting (Art. II), with the
//! app's HWND as owner: it opens above the undecorated window and stays there
//! (`Show(NULL)` often opened it behind, which looked like a freeze).
//!
//! While a dialog is up the window swallows its own input
//! ([`FilePicker::gate_input`]); a click, key press, or close request brings
//! the dialog forward and flashes it — the owned-modal cue Windows gives.
//!
//! The owner is deliberately kept *enabled*. `IFileDialog::Show` disables its
//! owner, and a disabled window cannot be relied on to take the gesture users
//! actually make: dragging a file out of the dialog onto the canvas. The app
//! accepts that drop as an ordinary external drop and calls
//! [`FilePicker::close`], which cancels the dialog the way its Cancel button
//! does; the dialog's own result is discarded.

use crossbeam_channel::{Receiver, TryRecvError};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
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

    /// The user tried to act in the window. `true` asks for the dialog to be
    /// brought forward and flashed.
    pub fn input_attempt(&self) -> bool {
        self.phase == Phase::Open
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

struct Session<M> {
    generation: u64,
    rx: Receiver<M>,
    owner: Option<DialogOwner>,
    /// Picker thread id (0 until it starts, and for adopted channels).
    thread: Arc<AtomicU32>,
    last_cancel: Option<Instant>,
}

/// One modal dialog slot. `M` is the app's own result message, built on the
/// picker thread from the paths by the `map` passed to [`FilePicker::open`].
pub struct FilePicker<M> {
    owner: Option<DialogOwner>,
    state: PickerState,
    session: Option<Session<M>>,
}

impl<M> Default for FilePicker<M> {
    fn default() -> Self {
        Self {
            owner: None,
            state: PickerState::default(),
            session: None,
        }
    }
}

impl<M: Send + 'static> FilePicker<M> {
    pub fn set_owner(&mut self, owner: Option<DialogOwner>) {
        self.owner = owner;
    }

    pub fn is_open(&self) -> bool {
        self.state.is_open()
    }

    /// Show a dialog. Returns `false` (and shows nothing) while one is open.
    pub fn open(
        &mut self,
        request: PickRequest,
        map: impl FnOnce(Option<Vec<PathBuf>>) -> M + Send + 'static,
    ) -> bool {
        let Some(generation) = self.state.begin() else {
            return false;
        };
        let (tx, rx) = crossbeam_channel::bounded(1);
        let thread = Arc::new(AtomicU32::new(0));
        let owner = self.owner;
        {
            let thread = thread.clone();
            std::thread::spawn(move || {
                thread.store(win::current_thread_id(), Ordering::Release);
                let _ = tx.send(map(request.run(owner)));
            });
        }
        self.session = Some(Session {
            generation,
            rx,
            owner,
            thread,
            last_cancel: None,
        });
        true
    }

    /// Treat `rx` as the result of an open dialog without showing one — for
    /// tests, which must never raise a native window.
    pub fn adopt(&mut self, rx: Receiver<M>) {
        let Some(generation) = self.state.begin() else {
            return;
        };
        self.session = Some(Session {
            generation,
            rx,
            owner: None,
            thread: Arc::new(AtomicU32::new(0)),
            last_cancel: None,
        });
    }

    /// Call every frame. Returns the pick once, unless the dialog was closed.
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<M> {
        let session = self.session.as_mut()?;
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
                if self.state.phase() == Phase::Closing {
                    if session
                        .last_cancel
                        .is_none_or(|at| at.elapsed() >= CANCEL_RETRY)
                    {
                        win::cancel(session.thread.load(Ordering::Acquire));
                        session.last_cancel = Some(Instant::now());
                    }
                } else if let Some(owner) = session.owner {
                    win::keep_enabled(owner);
                }
                ctx.request_repaint_after(POLL_INTERVAL);
                None
            }
        }
    }

    /// Cancel the open dialog as if the user pressed Cancel. Its result is
    /// discarded; call when an external drop replaced the pick.
    pub fn close(&mut self) {
        if !self.state.close() {
            return;
        }
        if let Some(session) = &mut self.session {
            win::cancel(session.thread.load(Ordering::Acquire));
            session.last_cancel = Some(Instant::now());
        }
    }

    /// From `eframe::App::raw_input_hook`: while a dialog is up, the window
    /// ignores input and an attempt to act brings the dialog forward.
    /// Returns `true` when it gated.
    pub fn gate_input(&self, raw: &mut egui::RawInput) -> bool {
        if !self.state.is_open() {
            return false;
        }
        if swallow_input(raw) && self.state.input_attempt() {
            if let Some(session) = &self.session {
                win::attention(session.thread.load(Ordering::Acquire));
            }
        }
        true
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
    use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled};
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
        assert!(!s.input_attempt(), "no flash for a dialog on its way out");
        assert!(!s.close(), "cancel is sent once, then retried by poll");
        assert_eq!(s.finished(generation), Outcome::Discard);
        assert!(!s.is_open());
    }

    #[test]
    fn open_then_input_requests_attention() {
        let mut s = PickerState::default();
        assert!(!s.input_attempt());
        s.begin().unwrap();
        assert!(s.input_attempt());
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
        let mut raw = egui::RawInput {
            events: vec![press(true)],
            ..Default::default()
        };
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
        let mut picker = FilePicker::<u32>::default();
        let mut raw = egui::RawInput::default();
        assert!(!picker.gate_input(&mut raw));

        let (tx, rx) = crossbeam_channel::unbounded();
        picker.adopt(rx);
        assert!(picker.is_open());
        raw.events = vec![press(true)];
        assert!(picker.gate_input(&mut raw));
        assert_eq!(picker.poll(&ctx), None);
        tx.send(7).unwrap();
        assert_eq!(picker.poll(&ctx), Some(7));
        assert!(!picker.is_open());
    }

    #[test]
    fn picker_close_discards_the_dialog_result() {
        let ctx = egui::Context::default();
        let mut picker = FilePicker::<u32>::default();
        let (tx, rx) = crossbeam_channel::unbounded();
        picker.adopt(rx);
        picker.close();
        assert!(picker.is_open());
        tx.send(7).unwrap();
        assert_eq!(picker.poll(&ctx), None);
        assert!(!picker.is_open());
    }

    #[test]
    fn a_vanished_dialog_thread_ungates_the_window() {
        let ctx = egui::Context::default();
        let mut picker = FilePicker::<u32>::default();
        let (tx, rx) = crossbeam_channel::unbounded();
        picker.adopt(rx);
        drop(tx);
        assert_eq!(picker.poll(&ctx), None);
        assert!(!picker.is_open());
    }

    /// Raises a real dialog, cancels it from this thread, and checks the
    /// session ends with nothing delivered. Ignored: it puts a window on the
    /// desktop for a moment.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_dialog_is_cancelled_by_close() {
        let ctx = egui::Context::default();
        let mut picker = FilePicker::<Option<Vec<PathBuf>>>::default();
        assert!(picker.open(PickRequest::file().title("atlas-shell test"), |p| p));
        let deadline = Instant::now() + Duration::from_secs(10);
        while picker
            .session
            .as_ref()
            .is_some_and(|s| win::dialogs(s.thread.load(Ordering::Acquire)).is_empty())
        {
            assert!(Instant::now() < deadline, "dialog never appeared");
            std::thread::sleep(Duration::from_millis(20));
        }
        picker.close();
        while picker.is_open() {
            assert!(Instant::now() < deadline, "dialog ignored the cancel");
            assert_eq!(picker.poll(&ctx), None);
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// With a real owner window: the dialog is owned by it, `Show` disables
    /// it, `poll` re-enables it while the dialog stays up, and a cancel
    /// still ends the session. Ignored for the same reason as above.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_owned_dialog_keeps_its_owner_enabled() {
        use windows::core::w;
        use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetWindow, IsWindowVisible,
            PeekMessageW, TranslateMessage, GW_OWNER, MSG, PM_REMOVE, WINDOW_EX_STYLE,
            WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        };
        // The dialog thread's EnableWindow is a cross-thread send to this one.
        let pump = || unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        };
        let owner = unsafe {
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
        .unwrap();
        let enabled = || unsafe { IsWindowEnabled(owner) }.as_bool();
        let wait = |what: &str, done: &mut dyn FnMut() -> bool| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done() {
                assert!(Instant::now() < deadline, "timed out: {what}");
                pump();
                std::thread::sleep(Duration::from_millis(10));
            }
        };

        let ctx = egui::Context::default();
        let mut picker = FilePicker::<Option<Vec<PathBuf>>>::default();
        picker.set_owner(Some(DialogOwner {
            hwnd: std::num::NonZeroIsize::new(owner.0 as isize).unwrap(),
        }));
        assert!(picker.open(PickRequest::folder().title("atlas-shell owned test"), |p| p));
        let thread = picker.session.as_ref().unwrap().thread.clone();
        let mut dialog = None;
        wait("dialog", &mut || {
            dialog = win::dialogs(thread.load(Ordering::Acquire))
                .first()
                .copied();
            dialog.is_some()
        });
        let dialog = dialog.unwrap();
        assert_eq!(unsafe { GetWindow(dialog, GW_OWNER) }.ok(), Some(owner));
        wait("Show disables its owner", &mut || !enabled());

        assert_eq!(picker.poll(&ctx), None);
        pump();
        assert!(enabled(), "poll re-enables the owner");
        assert!(
            unsafe { IsWindowVisible(dialog) }.as_bool(),
            "dialog stays up"
        );

        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::PointerButton {
            pos: egui::pos2(5.0, 5.0),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        assert!(picker.gate_input(&mut raw), "attention path runs");

        picker.close();
        wait("cancel", &mut || {
            assert_eq!(picker.poll(&ctx), None);
            !picker.is_open()
        });
        assert!(enabled());
        unsafe {
            let _ = DestroyWindow(owner);
        }
    }
}
