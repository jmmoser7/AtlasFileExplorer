use super::{SurfaceError, SurfaceRect};
use std::{marker::PhantomData, rc::Rc};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    GetLastError, SetLastError, HANDLE, HWND, POINT, RECT, WIN32_ERROR,
};

use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

const OWNER_PROPERTY: PCWSTR = windows::core::w!("AtlasConnector.BorrowedSurface.v1");
const HOST_PROPERTY: PCWSTR = windows::core::w!("AtlasConnector.SurfaceHost.v1");
mod placement;
mod region;
mod saved;
use region::WindowRegion;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AttachmentKind {
    Child,
    OwnedFullUi,
}

struct SavedWindow {
    parent: Option<HWND>,
    owner: isize,
    style: isize,
    exstyle: isize,
    placement: WINDOWPLACEMENT,
    rect: RECT,
    visible: bool,
    region: WindowRegion,
}

/// A borrowed HWND with reversible parenting. It is intentionally neither
/// `Send` nor `Sync`: use it only on the thread owning the host HWND.
///
/// ```compile_fail
/// # use atlas_connector_runtime::surface::NativeSurface;
/// fn move_to_worker(surface: NativeSurface) {
///     std::thread::spawn(move || drop(surface));
/// }
/// ```
pub struct NativeSurface {
    child: HWND,
    host: HWND,
    pid: u32,
    child_thread: u32,
    host_thread: u32,
    kind: AttachmentKind,
    identity: Box<u8>,
    saved: SavedWindow,
    attached: bool,
    _ui_thread: PhantomData<Rc<()>>,
}

impl NativeSurface {
    /// `expected_pid` must come from an authenticated OS peer, not a claim in
    /// application JSON. The host HWND must belong to this process/UI thread.
    pub fn attach(child: isize, expected_pid: u32, parent: isize) -> Result<Self, SurfaceError> {
        Self::attach_kind(child, expected_pid, parent, AttachmentKind::Child)
    }

    /// Hosts a full native UI as an owned top-level window, retaining its native
    /// menu while removing the outer caption/resize frame inside the portal.
    /// Original styles and window region are restored on detach.
    /// Both HWNDs must already be top-level windows.
    /// The caller must place/park it with the portal: ownership does not clip it
    /// to the host client area. Authentication and UI-thread rules match `attach`.
    pub fn attach_full_ui(
        window: isize,
        expected_pid: u32,
        owner: isize,
    ) -> Result<Self, SurfaceError> {
        Self::attach_kind(window, expected_pid, owner, AttachmentKind::OwnedFullUi)
    }

    fn attach_kind(
        child: isize,
        expected_pid: u32,
        parent: isize,
        kind: AttachmentKind,
    ) -> Result<Self, SurfaceError> {
        let child = HWND(child as *mut _);
        let host = HWND(parent as *mut _);
        let (pid, child_thread) = window_identity(child)?;
        if pid != expected_pid || expected_pid == 0 {
            return Err(SurfaceError::WrongProcess {
                expected: expected_pid,
                actual: pid,
            });
        }
        let (host_pid, host_thread) = window_identity(host)?;
        if host_pid != unsafe { GetCurrentProcessId() } {
            return Err(SurfaceError::HostUnavailable);
        }
        if host_thread != unsafe { GetCurrentThreadId() } {
            return Err(SurfaceError::WrongThread);
        }
        if child == host || unsafe { IsChild(child, host).as_bool() } {
            return Err(SurfaceError::InvalidWindow);
        }
        if !unsafe { GetPropW(child, OWNER_PROPERTY) }.is_invalid() {
            return Err(SurfaceError::AlreadyAttached);
        }
        if !unsafe { GetPropW(host, HOST_PROPERTY) }.is_invalid() {
            return Err(SurfaceError::AlreadyAttached);
        }
        let saved = SavedWindow::read(child)?;
        if kind == AttachmentKind::OwnedFullUi
            && (saved.style as u32 & WS_CHILD.0 != 0
                || get_long(host, GWL_STYLE)? as u32 & WS_CHILD.0 != 0)
        {
            return Err(SurfaceError::InvalidWindow);
        }
        let mut guard = Self {
            child,
            host,
            pid,
            child_thread,
            host_thread,
            kind,
            saved,
            identity: Box::new(0),
            attached: false,
            _ui_thread: PhantomData,
        };
        unsafe { SetPropW(child, OWNER_PROPERTY, Some(guard.token())) }
            .map_err(|_| win32("SetPropW"))?;
        guard.attached = true;
        unsafe { SetPropW(host, HOST_PROPERTY, Some(guard.token())) }
            .map_err(|_| win32("SetPropW (host)"))?;
        let result = (|| {
            guard.validate()?;
            match kind {
                AttachmentKind::Child => {
                    let style = (guard.saved.style as u32
                        & !(WS_POPUP.0 | WS_CAPTION.0 | WS_THICKFRAME.0))
                        | WS_CHILD.0;
                    set_long(child, GWL_STYLE, style as isize)?;
                    set_parent(child, Some(host))?;
                }
                // Top-level ownership retains the native menu. Only the
                // exterior frame belongs to Slate while the surface is hosted.
                AttachmentKind::OwnedFullUi => {
                    // Hide its former desktop extent until the first host placement.
                    WindowRegion::clip(child, 0, 0, 0, 0)?;
                    set_long(child, GWLP_HWNDPARENT, host.0 as isize)?;
                    let style = guard.saved.style as u32
                        & !(WS_CAPTION.0
                            | WS_THICKFRAME.0
                            | WS_BORDER.0
                            | WS_DLGFRAME.0
                            | WS_MAXIMIZE.0
                            | WS_MINIMIZE.0);
                    let exstyle = guard.saved.exstyle as u32
                        & !(WS_EX_WINDOWEDGE.0
                            | WS_EX_CLIENTEDGE.0
                            | WS_EX_DLGMODALFRAME.0
                            | WS_EX_STATICEDGE.0);
                    set_long(child, GWL_STYLE, style as isize)?;
                    set_long(child, GWL_EXSTYLE, exstyle as isize)?;
                }
            }
            unsafe {
                SetWindowPos(
                    child,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                )
            }
            .map_err(|_| win32("SetWindowPos"))
        })();
        // A failed attachment drops the guard and attempts to undo every change.
        result?;
        Ok(guard)
    }

    pub fn process_id(&self) -> Result<u32, SurfaceError> {
        self.validate()?;
        Ok(self.pid)
    }

    pub fn handle(&self) -> Result<isize, SurfaceError> {
        self.validate()?;
        Ok(self.child.0 as isize)
    }

    /// Queues hide without waiting for the foreign UI. Success means accepted
    /// for dispatch, not proof that an unresponsive application processed it.
    pub fn hide(&self) -> Result<(), SurfaceError> {
        self.validate_host()?;
        // Automatic parking runs on the frame path. Never join foreign input
        // queues or wait for a foreign window procedure while hiding it.
        if !unsafe { ShowWindowAsync(self.child, SW_HIDE).as_bool() } {
            return Err(win32("ShowWindowAsync"));
        }
        Ok(())
    }

    /// Foreign-thread focus belongs to the application's connector UI thread.
    /// The guard refuses it rather than joining input queues and risking a stall.
    pub fn focus(&self) -> Result<(), SurfaceError> {
        self.validate_host()?;
        focus_window(self.child)
    }

    /// Releases focus only if it belongs to this surface or a descendant.
    pub fn blur(&self) -> Result<(), SurfaceError> {
        self.validate_host()?;
        if self.child_thread != self.host_thread {
            return Err(SurfaceError::FocusRequiresConnector);
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        unsafe { GetGUIThreadInfo(self.child_thread, &mut info) }
            .map_err(|_| win32("GetGUIThreadInfo"))?;
        if info.hwndFocus == self.child || unsafe { IsChild(self.child, info.hwndFocus).as_bool() }
        {
            focus_window(self.host)?;
        }
        Ok(())
    }

    /// Restores parent/owner, both styles, placement, bounds and visibility.
    /// Errors leave the guard attached so a caller can retry before dropping it.
    pub fn detach(&mut self) -> Result<(), SurfaceError> {
        self.detach_visibility(true)
    }

    /// Release the host slot and restore native ownership/styles/placement
    /// while leaving the application parked. Never closes its document.
    pub fn detach_hidden(&mut self) -> Result<(), SurfaceError> {
        self.detach_visibility(false)
    }

    fn detach_visibility(&mut self, restore_visible: bool) -> Result<(), SurfaceError> {
        if !self.attached {
            return Ok(());
        }
        self.validate()?;
        self.saved.restore(self.child, restore_visible)?;
        unsafe { RemovePropW(self.child, OWNER_PROPERTY) }.map_err(|_| win32("RemovePropW"))?;
        self.release_host();
        self.attached = false;
        Ok(())
    }

    fn token(&self) -> HANDLE {
        HANDLE(self.identity.as_ref() as *const u8 as *mut _)
    }

    fn validate(&self) -> Result<(), SurfaceError> {
        if !self.attached {
            return Err(SurfaceError::Detached);
        }
        if unsafe { GetCurrentThreadId() } != self.host_thread {
            return Err(SurfaceError::WrongThread);
        }
        let identity = window_identity(self.child).map_err(|_| SurfaceError::IdentityChanged)?;
        if identity != (self.pid, self.child_thread)
            || unsafe { GetPropW(self.child, OWNER_PROPERTY) } != self.token()
        {
            return Err(SurfaceError::IdentityChanged);
        }
        Ok(())
    }

    fn validate_host(&self) -> Result<(), SurfaceError> {
        self.validate()?;
        let identity = window_identity(self.host).map_err(|_| SurfaceError::HostUnavailable)?;
        if identity != (unsafe { GetCurrentProcessId() }, self.host_thread) {
            return Err(SurfaceError::HostUnavailable);
        }
        if unsafe { GetPropW(self.host, HOST_PROPERTY) } != self.token() {
            return Err(SurfaceError::HostUnavailable);
        }
        Ok(())
    }

    fn release_host(&self) {
        if unsafe { GetPropW(self.host, HOST_PROPERTY) } == self.token() {
            let _ = unsafe { RemovePropW(self.host, HOST_PROPERTY) };
        }
    }
}

impl Drop for NativeSurface {
    fn drop(&mut self) {
        if let Err(error) = self.detach() {
            eprintln!("Could not detach borrowed application surface: {error}");
            self.release_host();
            if unsafe { GetPropW(self.child, OWNER_PROPERTY) } == self.token() {
                let _ = unsafe { RemovePropW(self.child, OWNER_PROPERTY) };
            }
        }
    }
}

fn window_identity(hwnd: HWND) -> Result<(u32, u32), SurfaceError> {
    if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return Err(SurfaceError::InvalidWindow);
    }
    let mut pid = 0;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 || thread == 0 {
        return Err(SurfaceError::InvalidWindow);
    }
    Ok((pid, thread))
}

fn set_parent(child: HWND, parent: Option<HWND>) -> Result<(), SurfaceError> {
    unsafe {
        SetLastError(WIN32_ERROR(0));
    }
    // A null previous parent is a successful return for a top-level window.
    let result = unsafe { SetParent(child, parent) };
    if result.is_err() && unsafe { GetLastError().0 } != 0 {
        return Err(win32("SetParent"));
    }
    Ok(())
}

fn get_long(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> Result<isize, SurfaceError> {
    unsafe {
        SetLastError(WIN32_ERROR(0));
    }
    let value = unsafe { GetWindowLongPtrW(hwnd, index) };
    if value == 0 && unsafe { GetLastError().0 } != 0 {
        return Err(win32("GetWindowLongPtrW"));
    }
    Ok(value)
}

fn set_long(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> Result<(), SurfaceError> {
    // Restoring an already ownerless HWND to owner 0 can report error 1400
    // despite leaving a valid window unchanged. Avoid redundant writes, while
    // still validating the handle and reporting failures of actual changes.
    if get_long(hwnd, index)? == value {
        return Ok(());
    }
    unsafe {
        SetLastError(WIN32_ERROR(0));
    }
    let previous = unsafe { SetWindowLongPtrW(hwnd, index, value) };
    if previous == 0 && unsafe { GetLastError().0 } != 0 {
        return Err(win32("SetWindowLongPtrW"));
    }
    Ok(())
}

fn focus_window(hwnd: HWND) -> Result<(), SurfaceError> {
    let current = unsafe { GetCurrentThreadId() };
    let (_, target) = window_identity(hwnd)?;
    if current != target {
        return Err(SurfaceError::FocusRequiresConnector);
    }
    unsafe {
        SetLastError(WIN32_ERROR(0));
    }
    let _ = unsafe { SetFocus(Some(hwnd)) };
    if unsafe { GetFocus() } == hwnd {
        Ok(())
    } else {
        Err(win32("SetFocus"))
    }
}

fn win32(operation: &'static str) -> SurfaceError {
    SurfaceError::Win32 {
        operation,
        code: unsafe { GetLastError().0 },
    }
}
