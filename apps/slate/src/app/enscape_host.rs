//! Host an Enscape standalone inside a board card.
//!
//! The executable is Enscape's own program. Slate starts it only on an
//! explicit double-click, parents its window to the frame, and keeps the
//! last picture as the card thumbnail. Opening a workbook never launches it.
//!
//! A machine that cannot run the file — not Windows, a cloud-only copy, a
//! rotated card, or an executable built for a different Windows system —
//! gets a fixed sentence on the card and in a toast. Those sentences are
//! the contract; tests lock the wording.

use std::path::Path;
use std::time::{Duration, Instant};

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
    pub node: NodeId,
    pub cache_key: String,
    pub since: Instant,
    #[cfg(windows)]
    os: OsSession,
}

impl Live {
    pub fn timed_out(&self) -> bool {
        self.since.elapsed() > OPEN_TIMEOUT
    }

    #[cfg(windows)]
    pub fn has_window(&self) -> bool {
        self.os.hwnd.is_some()
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
    hwnd: Option<windows::Win32::Foundation::HWND>,
}

// Kernel handles are used from the UI thread after a worker starts the
// process. The window itself is created by Enscape and discovered later.
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
pub fn launch(path: &Path, node: NodeId, cache_key: String) -> Result<Live, LaunchError> {
    let os = spawn(path)?;
    Ok(Live {
        node,
        cache_key,
        since: Instant::now(),
        os,
    })
}

#[cfg(windows)]
pub fn poll(live: &mut Live, parent: isize) {
    if parent == 0 || live.os.hwnd.is_some() {
        return;
    }
    let Some(hwnd) = find_window(live.os.job, live.os.pid) else {
        return;
    };
    embed(
        windows::Win32::Foundation::HWND(parent as *mut std::ffi::c_void),
        hwnd,
    );
    live.os.hwnd = Some(hwnd);
}

/// Move the hosted window to `rect` in physical pixels of the frame client.
/// An empty rect hides it (the card is off the canvas).
#[cfg(windows)]
pub fn place(live: &Live, rect: Option<(i32, i32, i32, i32)>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, SWP_ASYNCWINDOWPOS, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE,
    };
    let Some(hwnd) = live.os.hwnd else {
        return;
    };
    unsafe {
        match rect {
            Some((x, y, w, h)) if w >= 8 && h >= 8 => {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    x,
                    y,
                    w,
                    h,
                    SWP_NOZORDER | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
                );
            }
            _ => {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }
}

#[cfg(windows)]
pub fn capture(live: &Live) -> Option<(u32, u32, Vec<u8>)> {
    live.os.hwnd.and_then(capture_hwnd)
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
    let mut startup = STARTUPINFOW::default();
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
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
        hwnd: None,
    })
}

#[cfg(windows)]
fn embed(parent: windows::Win32::Foundation::HWND, child: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetParent, SetWindowLongPtrW, GWL_STYLE, WS_CAPTION, WS_CHILD, WS_POPUP,
        WS_THICKFRAME,
    };
    unsafe {
        let style = GetWindowLongPtrW(child, GWL_STYLE) as u32;
        let style = (style & !(WS_POPUP.0 | WS_CAPTION.0 | WS_THICKFRAME.0)) | WS_CHILD.0;
        SetWindowLongPtrW(child, GWL_STYLE, style as isize);
        let _ = SetParent(child, Some(parent));
    }
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
fn capture_hwnd(hwnd: windows::Win32::Foundation::HWND) -> Option<(u32, u32, Vec<u8>)> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, PW_RENDERFULLCONTENT};
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect).ok()?;
        let w = rect.right.max(0) as u32;
        let h = rect.bottom.max(0) as u32;
        if w < 8 || h < 8 || w > 4096 || h > 4096 {
            return None;
        }
        let window_dc = GetDC(Some(hwnd));
        if window_dc.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(window_dc));
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: h as i32,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bitmap =
            CreateDIBSection(Some(window_dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok();
        let Some(bitmap) = bitmap else {
            let _ = DeleteDC(mem);
            ReleaseDC(Some(hwnd), window_dc);
            return None;
        };
        let old = SelectObject(mem, HGDIOBJ(bitmap.0));
        let printed = print_window(hwnd, mem, PW_RENDERFULLCONTENT).as_bool();
        let mut rgba = None;
        if printed && !bits.is_null() {
            let count = (w as usize) * (h as usize);
            let src = std::slice::from_raw_parts(bits as *const u8, count * 4);
            let mut out = vec![0u8; count * 4];
            let row = (w as usize) * 4;
            for y in 0..h as usize {
                let src_row = (h as usize - 1 - y) * row;
                let dst_row = y * row;
                for x in 0..w as usize {
                    let s = src_row + x * 4;
                    let d = dst_row + x * 4;
                    out[d] = src[s + 2];
                    out[d + 1] = src[s + 1];
                    out[d + 2] = src[s];
                    out[d + 3] = 255;
                }
            }
            rgba = Some((w, h, out));
        }
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem);
        ReleaseDC(Some(hwnd), window_dc);
        rgba
    }
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn PrintWindow(
        hwnd: windows::Win32::Foundation::HWND,
        hdc: windows::Win32::Graphics::Gdi::HDC,
        nflags: u32,
    ) -> windows::core::BOOL;
}

#[cfg(windows)]
fn print_window(
    hwnd: windows::Win32::Foundation::HWND,
    hdc: windows::Win32::Graphics::Gdi::HDC,
    flags: u32,
) -> windows::core::BOOL {
    unsafe { PrintWindow(hwnd, hdc, flags) }
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
