#![cfg(windows)]

use atlas_connector_runtime::surface::{NativeSurface, SurfaceError, SurfaceRect};
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::*;

struct OwnedWindow(HWND);

impl OwnedWindow {
    fn new(parent: Option<HWND>, style: WINDOW_STYLE, exstyle: WINDOW_EX_STYLE) -> Self {
        let hwnd = unsafe {
            CreateWindowExW(
                exstyle,
                w!("STATIC"),
                w!("Unsaved connector test document"),
                style,
                45,
                65,
                260,
                180,
                parent,
                None,
                None,
                None,
            )
        }
        .expect("create real test HWND");
        Self(hwnd)
    }

    fn raw(&self) -> isize {
        self.0 .0 as isize
    }
}

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0).expect("test HWND still belongs to the test");
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    parent: isize,
    style: isize,
    exstyle: isize,
    rect: (i32, i32, i32, i32),
    normal: (i32, i32, i32, i32),
    show: u32,
    flags: WINDOWPLACEMENT_FLAGS,
    min: (i32, i32),
    max: (i32, i32),
    visible: bool,
}

fn snapshot(window: &OwnedWindow) -> Snapshot {
    let mut rect = RECT::default();
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe {
        GetWindowRect(window.0, &mut rect).unwrap();
        GetWindowPlacement(window.0, &mut placement).unwrap();
        Snapshot {
            parent: GetParent(window.0).unwrap_or_default().0 as isize,
            style: GetWindowLongPtrW(window.0, GWL_STYLE),
            exstyle: GetWindowLongPtrW(window.0, GWL_EXSTYLE),
            rect: rect_tuple(rect),
            normal: rect_tuple(placement.rcNormalPosition),
            show: placement.showCmd,
            flags: placement.flags,
            min: (placement.ptMinPosition.x, placement.ptMinPosition.y),
            max: (placement.ptMaxPosition.x, placement.ptMaxPosition.y),
            visible: IsWindowVisible(window.0).as_bool(),
        }
    }
}

fn rect_tuple(rect: RECT) -> (i32, i32, i32, i32) {
    (rect.left, rect.top, rect.right, rect.bottom)
}

fn host() -> OwnedWindow {
    OwnedWindow::new(
        None,
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        WINDOW_EX_STYLE::default(),
    )
}

fn attach(window: &OwnedWindow, host: &OwnedWindow) -> NativeSurface {
    NativeSurface::attach(window.raw(), unsafe { GetCurrentProcessId() }, host.raw()).unwrap()
}

fn exercise(surface: &NativeSurface) {
    surface
        .place(SurfaceRect {
            x: 12,
            y: 18,
            width: 160,
            height: 120,
        })
        .unwrap();
    surface.focus().unwrap();
    surface.blur().unwrap();
    surface.hide().unwrap();
}

#[test]
fn explicit_detach_restores_hidden_top_level_window_without_closing_document() {
    let host = host();
    let child = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WS_EX_TOOLWINDOW);
    let before = snapshot(&child);
    let pid = unsafe { GetCurrentProcessId() };
    let mut surface = attach(&child, &host);
    assert_eq!(snapshot(&child).parent, host.raw());
    assert_eq!(snapshot(&child).style as u32 & WS_CHILD.0, WS_CHILD.0);
    exercise(&surface);
    surface.detach().unwrap();
    surface.detach().unwrap();
    assert_eq!(surface.handle(), Err(SurfaceError::Detached));
    assert_eq!(snapshot(&child), before);
    assert_eq!(unsafe { GetCurrentProcessId() }, pid);
    let mut title = [0u16; 80];
    let count = unsafe { GetWindowTextW(child.0, &mut title) } as usize;
    assert_eq!(
        String::from_utf16_lossy(&title[..count]),
        "Unsaved connector test document"
    );
}

#[test]
fn drop_restores_existing_child_parent_styles_bounds_and_visibility() {
    let original_parent = host();
    let target_host = host();
    let child = OwnedWindow::new(
        Some(original_parent.0),
        WS_CHILD | WS_VISIBLE | WS_BORDER,
        WS_EX_CLIENTEDGE,
    );
    let before = snapshot(&child);
    {
        let surface = attach(&child, &target_host);
        exercise(&surface);
    }
    assert_eq!(snapshot(&child), before);
    assert!(unsafe { IsWindow(Some(child.0)).as_bool() });
    // Restoration also released the slot on the host and child.
    attach(&child, &target_host).detach().unwrap();
}

#[test]
fn top_level_owner_is_restored_on_drop() {
    let owner = host();
    let target_host = host();
    let child = OwnedWindow::new(
        Some(owner.0),
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        WS_EX_APPWINDOW,
    );
    let before = snapshot(&child);
    drop(attach(&child, &target_host));
    assert_eq!(snapshot(&child), before);
}

#[test]
fn child_of_hidden_parent_keeps_its_own_visibility_flag() {
    let original_parent = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let target_host = host();
    let child = OwnedWindow::new(
        Some(original_parent.0),
        WS_CHILD | WS_VISIBLE | WS_BORDER,
        WINDOW_EX_STYLE::default(),
    );
    let before = snapshot(&child);
    assert!(!before.visible);
    let mut surface = attach(&child, &target_host);
    surface.detach().unwrap();
    assert_eq!(snapshot(&child), before);
    // An explicit successful release must also make the host slot reusable.
    attach(&child, &target_host).detach().unwrap();
}

#[test]
fn wrong_pid_invalid_handle_and_double_attachment_leave_windows_untouched() {
    let host = host();
    let child = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let before = snapshot(&child);
    let pid = unsafe { GetCurrentProcessId() };
    assert!(matches!(
        NativeSurface::attach(child.raw(), pid + 1, host.raw()),
        Err(SurfaceError::WrongProcess { .. })
    ));
    assert!(matches!(
        NativeSurface::attach(0, pid, host.raw()),
        Err(SurfaceError::InvalidWindow)
    ));
    assert_eq!(snapshot(&child), before);
    let surface = attach(&child, &host);
    assert!(matches!(
        NativeSurface::attach(child.raw(), pid, host.raw()),
        Err(SurfaceError::AlreadyAttached)
    ));
    assert!(matches!(
        surface.place(SurfaceRect {
            x: 0,
            y: 0,
            width: 0,
            height: 10
        }),
        Err(SurfaceError::InvalidRect)
    ));
    drop(surface);
    assert_eq!(snapshot(&child), before);
}

#[test]
fn attachment_requires_the_host_ui_thread() {
    let host = host();
    let child = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let before = snapshot(&child);
    let (source, target, pid) = (child.raw(), host.raw(), unsafe { GetCurrentProcessId() });
    std::thread::spawn(move || {
        assert!(matches!(
            NativeSurface::attach(source, pid, target),
            Err(SurfaceError::WrongThread)
        ));
    })
    .join()
    .unwrap();
    assert_eq!(snapshot(&child), before);
}

#[test]
fn destroyed_window_identity_is_refused_without_touching_replacement() {
    let host = host();
    let child = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let surface = attach(&child, &host);
    drop(child);
    let replacement = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let before = snapshot(&replacement);
    assert_eq!(surface.handle(), Err(SurfaceError::IdentityChanged));
    drop(surface);
    assert_eq!(snapshot(&replacement), before);
    attach(&replacement, &host).detach().unwrap();
}
