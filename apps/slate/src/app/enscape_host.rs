//! Host Enscape's own standalone inside a board card after explicit activation.
//!
//! Keeps its last picture as the thumbnail; workbook loading never launches it.
//!
//! Unavailable files get fixed explanations on the card and in a toast.
//! Those sentences are the contract; tests lock the wording.
#![cfg_attr(not(windows), allow(dead_code))] // Off Windows only the refusal path is live.

#[cfg(windows)]
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(windows)]
use slate_doc::NodeId;

pub const NOT_WINDOWS: &str =
    "Enscape standalones only run on Windows. This computer can't open this walkthrough.";
pub const MISSING: &str = "This Enscape file is missing on this computer.";
pub const CLOUD: &str =
    "This Enscape file is online-only. Make it available on this computer, then try again.";
pub const ROTATED: &str = "This card is rotated. Enscape can only run inside an upright card.";
pub const BUSY: &str = "An Enscape walkthrough is already open. Close it before opening another.";
pub const WRONG_MACHINE: &str = "This Enscape standalone can't run on this computer. It was built for a different Windows system.";
pub const NO_WINDOW: &str =
    "Enscape started, but no window appeared. It may be incompatible with this computer.";
pub const NO_HOST: &str = "Enscape can't attach to this window on this computer.";
pub const OPENING: &str = "Opening Enscape…";

pub const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

/// Facts known before any process is created.
pub struct OpenFacts {
    pub windows: bool,
    pub exists: bool,
    pub cloud_only: bool,
    pub rotated: bool,
    pub already_open: bool,
}

/// Why this computer will not start the walkthrough. `None` means launch may proceed.
pub fn refusal(facts: OpenFacts) -> Option<&'static str> {
    if !facts.windows {
        return Some(NOT_WINDOWS);
    }
    if !facts.exists {
        return Some(MISSING);
    }
    if facts.cloud_only {
        return Some(CLOUD);
    }
    if facts.rotated {
        return Some(ROTATED);
    }
    if facts.already_open {
        return Some(BUSY);
    }
    None
}

/// Win32 codes that mean the executable is not for this machine.
/// 193 = `ERROR_BAD_EXE_FORMAT`, 216 = `ERROR_EXE_MACHINE_TYPE_MISMATCH`.
pub fn machine_mismatch_message(win32_code: u32) -> Option<&'static str> {
    match win32_code {
        193 | 216 => Some(WRONG_MACHINE),
        _ => None,
    }
}

pub fn win32_from_hresult(hresult: i32) -> u32 {
    let code = hresult as u32;
    if code & 0xFFFF_0000 == 0x8007_0000 {
        code & 0xFFFF
    } else {
        code
    }
}

/// Sentence on the resting card. Non-Windows machines see the refusal
/// immediately, not only after a double-click.
pub fn resting_line() -> &'static str {
    if cfg!(windows) {
        model_preview::ENSCAPE_CARD
    } else {
        NOT_WINDOWS
    }
}

pub struct Live {
    pub since: Instant,
    #[cfg(windows)]
    surface: Option<atlas_connector_runtime::surface::NativeSurface>,
    #[cfg(windows)]
    surface_error: std::cell::RefCell<Option<String>>,
    // Fields drop in declaration order: restore the UI guard before job close.
    #[cfg(windows)]
    os: OsSession,
}

impl Live {
    pub fn timed_out(&self) -> bool {
        self.since.elapsed() > OPEN_TIMEOUT
    }

    #[cfg(windows)]
    pub fn has_window(&self) -> bool {
        self.surface.as_ref().is_some_and(|s| s.handle().is_ok())
    }

    #[cfg(windows)]
    pub fn surface_error(&self) -> Option<String> {
        self.surface_error.borrow().clone()
    }

    #[cfg(not(windows))]
    pub fn has_window(&self) -> bool {
        false
    }
}

#[cfg(windows)]
struct OsSession {
    job: windows::Win32::Foundation::HANDLE,
    process: windows::Win32::Foundation::HANDLE,
    thread: windows::Win32::Foundation::HANDLE,
    pid: u32,
}

// SAFETY: only transferable kernel handles and a PID cross the launch worker.
// The !Send native surface is created later inside UI-owned Live.
#[cfg(windows)]
unsafe impl Send for OsSession {}

#[cfg(windows)]
impl Drop for OsSession {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;
        unsafe {
            // `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` ends the standalone and
            // any extractor child when the job handle closes.
            let _ = CloseHandle(self.job);
            let _ = CloseHandle(self.process);
            let _ = CloseHandle(self.thread);
        }
    }
}

#[cfg(windows)]
pub enum LaunchError {
    WrongMachine,
    Other(String),
}

#[cfg(windows)]
pub struct Started(Instant, OsSession);

#[cfg(windows)]
impl Started {
    pub fn into_live(self) -> Live {
        Live {
            since: self.0,
            surface: None,
            surface_error: Default::default(),
            os: self.1,
        }
    }
}

#[cfg(windows)]
pub fn launch(path: &Path, _node: NodeId, _cache_key: String) -> Result<Started, LaunchError> {
    let os = spawn(path)?;
    Ok(Started(Instant::now(), os))
}

#[cfg(windows)]
pub fn poll(live: &mut Live, parent: isize) {
    if parent == 0 || live.surface.is_some() || live.surface_error().is_some() {
        return;
    }
    let Some(hwnd) = find_window(live.os.job, live.os.pid) else {
        return;
    };
    let mut pid = 0;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    match atlas_connector_runtime::surface::NativeSurface::attach(hwnd.0 as isize, pid, parent) {
        Ok(surface) => live.surface = Some(surface),
        Err(error) => record_surface_error(live, error),
    }
}

/// Move the hosted window to `rect` in physical pixels of the frame client.
/// An empty rect hides it (the card is off the canvas).
#[cfg(windows)]
pub fn place(live: &Live, rect: Option<(i32, i32, i32, i32)>) {
    use atlas_connector_runtime::surface::SurfaceRect;
    let Some(surface) = live.surface.as_ref() else {
        return;
    };
    let result = match rect {
        Some((x, y, w, h)) if w >= 8 && h >= 8 => surface.place(SurfaceRect {
            x,
            y,
            width: w,
            height: h,
        }),
        _ => surface.hide(),
    };
    if let Err(error) = result {
        record_surface_error(live, error);
    }
}

#[cfg(windows)]
fn record_surface_error(live: &Live, error: atlas_connector_runtime::surface::SurfaceError) {
    live.surface_error
        .borrow_mut()
        .get_or_insert_with(|| error.to_string());
}

#[cfg(windows)]
pub fn capture(live: &Live) -> Option<(u32, u32, Vec<u8>)> {
    let hwnd = live.surface.as_ref()?.handle().ok()?;
    atlas_connector_runtime::surface::capture_window_rgba(
        hwnd,
        live.surface.as_ref()?.process_id().ok()?,
    )
}

#[cfg(windows)]
fn spawn(path: &Path) -> Result<OsSession, LaunchError> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows::Win32::System::Threading::{
        CreateProcessW, ResumeThread, TerminateProcess, CREATE_SUSPENDED, PROCESS_INFORMATION,
        STARTUPINFOW,
    };

    let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .map_err(|e| LaunchError::Other(e.to_string()))?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        if let Err(err) = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            std::mem::size_of_val(&limits) as u32,
        ) {
            let _ = CloseHandle(job);
            return Err(LaunchError::Other(err.to_string()));
        }
    }

    let application = wide_path(path);
    let mut command = wide(&format!("\"{}\"", path.display()));
    let directory = path.parent().map(wide_path).unwrap_or_else(|| wide("."));
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_SUSPENDED,
            None,
            PCWSTR(directory.as_ptr()),
            &startup,
            &mut info,
        )
    };
    if let Err(err) = created {
        let _ = unsafe { CloseHandle(job) };
        let code = win32_from_hresult(err.code().0);
        if machine_mismatch_message(code).is_some() {
            return Err(LaunchError::WrongMachine);
        }
        return Err(LaunchError::Other(err.to_string()));
    }
    unsafe {
        if let Err(err) = AssignProcessToJobObject(job, info.hProcess) {
            let _ = TerminateProcess(info.hProcess, 1);
            let _ = CloseHandle(info.hProcess);
            let _ = CloseHandle(info.hThread);
            let _ = CloseHandle(job);
            return Err(LaunchError::Other(err.to_string()));
        }
        if ResumeThread(info.hThread) == u32::MAX {
            let _ = TerminateProcess(info.hProcess, 1);
            let _ = CloseHandle(info.hProcess);
            let _ = CloseHandle(info.hThread);
            let _ = CloseHandle(job);
            return Err(LaunchError::Other("Enscape could not be started.".into()));
        }
    }
    Ok(OsSession {
        job,
        process: info.hProcess,
        thread: info.hThread,
        pid: info.dwProcessId,
    })
}

#[cfg(windows)]
fn find_window(
    job: windows::Win32::Foundation::HANDLE,
    root_pid: u32,
) -> Option<windows::Win32::Foundation::HWND> {
    let mut pids = job_pids(job);
    if !pids.contains(&root_pid) {
        pids.push(root_pid);
    }
    largest_window(&pids)
}

#[cfg(windows)]
fn job_pids(job: windows::Win32::Foundation::HANDLE) -> Vec<u32> {
    use windows::Win32::System::JobObjects::{
        JobObjectBasicProcessIdList, QueryInformationJobObject, JOBOBJECT_BASIC_PROCESS_ID_LIST,
    };
    let mut bytes = vec![
        0u8;
        std::mem::size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>()
            + 64 * std::mem::size_of::<usize>()
    ];
    let mut returned = 0u32;
    let ok = unsafe {
        QueryInformationJobObject(
            Some(job),
            JobObjectBasicProcessIdList,
            bytes.as_mut_ptr() as *mut _,
            bytes.len() as u32,
            Some(&mut returned),
        )
    };
    if ok.is_err() {
        return Vec::new();
    }
    let header = unsafe { &*(bytes.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST) };
    let count = header.NumberOfProcessIdsInList as usize;
    let list_ptr = header.ProcessIdList.as_ptr();
    (0..count)
        .map(|i| unsafe { *list_ptr.add(i) } as u32)
        .filter(|pid| *pid != 0)
        .collect()
}

#[cfg(windows)]
struct WindowSearch {
    pids: Vec<u32>,
    best: windows::Win32::Foundation::HWND,
    best_area: i64,
}

#[cfg(windows)]
fn largest_window(pids: &[u32]) -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    let mut search = WindowSearch {
        pids: pids.to_vec(),
        best: HWND::default(),
        best_area: 0,
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_window),
            LPARAM(&mut search as *mut WindowSearch as isize),
        );
    }
    (search.best_area > 0).then_some(search.best)
}

#[cfg(windows)]
unsafe extern "system" fn enum_window(
    hwnd: windows::Win32::Foundation::HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::core::BOOL {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowThreadProcessId, IsWindowVisible,
    };
    let search = unsafe { &mut *(lparam.0 as *mut WindowSearch) };
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return windows::core::BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if !search.pids.contains(&pid) {
            return windows::core::BOOL(1);
        }
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_err() {
            return windows::core::BOOL(1);
        }
        let area = i64::from(rect.right) * i64::from(rect.bottom);
        if area > search.best_area && area >= 64 * 64 {
            search.best = hwnd;
            search.best_area = area;
        }
        windows::core::BOOL(1)
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn wide_path(path: &Path) -> Vec<u16> {
    wide(&path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(windows: bool) -> OpenFacts {
        OpenFacts {
            windows,
            exists: true,
            cloud_only: false,
            rotated: false,
            already_open: false,
        }
    }

    #[test]
    fn other_systems_are_told_windows_is_required() {
        let msg = refusal(facts(false)).unwrap();
        assert!(msg.contains("only run on Windows"));
        assert!(msg.contains("can't open"));
    }

    #[test]
    fn a_missing_cloud_rotated_or_busy_file_names_the_reason() {
        let mut f = facts(true);
        f.exists = false;
        assert!(refusal(f).unwrap().contains("missing"));
        let mut f = facts(true);
        f.cloud_only = true;
        assert!(refusal(f).unwrap().contains("online-only"));
        let mut f = facts(true);
        f.rotated = true;
        assert!(refusal(f).unwrap().contains("rotated"));
        let mut f = facts(true);
        f.already_open = true;
        assert!(refusal(f).unwrap().contains("already open"));
        assert!(refusal(facts(true)).is_none());
    }

    #[test]
    fn a_foreign_executable_is_called_out_as_the_wrong_system() {
        assert!(machine_mismatch_message(193)
            .unwrap()
            .contains("different Windows system"));
        assert!(machine_mismatch_message(216)
            .unwrap()
            .contains("different Windows system"));
        assert!(machine_mismatch_message(2).is_none());
        assert_eq!(win32_from_hresult(0x8007_00C1u32 as i32), 193);
    }
}
