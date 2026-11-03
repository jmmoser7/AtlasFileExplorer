#![cfg(windows)]
#[path = "surface_windows/parking.rs"]
mod parking;

use atlas_connector_runtime::surface::{NativeSurface, SurfaceError, SurfaceRect};
use windows::core::w;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
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
    owner: isize,
    menu: isize,
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
            owner: GetWindow(window.0, GW_OWNER).unwrap_or_default().0 as isize,
            menu: GetMenu(window.0).0 as isize,
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

// These windows stay off-screen so native acceptance can use the real desktop.
fn full_ui_host() -> OwnedWindow {
    let window = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    unsafe {
        SetWindowPos(
            window.0,
            None,
            -30_000,
            -30_000,
            800,
            600,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .unwrap();
    }
    window
}

fn add_native_menu(window: &OwnedWindow) {
    unsafe {
        let menu = CreateMenu().unwrap();
        AppendMenuW(menu, MF_STRING, 1, w!("File")).unwrap();
        AppendMenuW(menu, MF_STRING, 2, w!("Edit")).unwrap();
        AppendMenuW(menu, MF_STRING, 3, w!("View")).unwrap();
        // DestroyWindow owns destruction of its assigned menu.
        SetMenu(window.0, Some(menu)).unwrap();
        assert_eq!(GetMenuItemCount(Some(menu)), 3);
    }
}

#[test]
fn full_ui_preserves_real_menu_without_border_places_and_restores_native_frame() {
    let original_owner = full_ui_host();
    let host = full_ui_host();
    let window = OwnedWindow::new(
        Some(original_owner.0),
        WS_OVERLAPPEDWINDOW,
        WS_EX_TOOLWINDOW,
    );
    add_native_menu(&window);
    let before = snapshot(&window);
    assert_eq!(before.owner, original_owner.raw());
    assert_ne!(before.menu, 0);
    let mut surface =
        NativeSurface::attach_full_ui(window.raw(), unsafe { GetCurrentProcessId() }, host.raw())
            .unwrap();
    let attached = snapshot(&window);
    assert_eq!(attached.owner, host.raw());
    assert_eq!(
        attached.style,
        before.style
            & !((WS_CAPTION | WS_THICKFRAME | WS_BORDER | WS_DLGFRAME | WS_MAXIMIZE | WS_MINIMIZE).0
                as isize)
    );
    assert_eq!(
        attached.exstyle,
        before.exstyle
            & !((WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_DLGMODALFRAME | WS_EX_STATICEDGE).0
                as isize)
    );
    assert_eq!(attached.menu, before.menu);
    assert!(!unsafe { IsChild(host.0, window.0).as_bool() });
    let rect = SurfaceRect {
        x: 21,
        y: 25,
        width: 320,
        height: 240,
    };
    let mut origin = POINT {
        x: rect.x,
        y: rect.y,
    };
    assert!(unsafe { ClientToScreen(host.0, &mut origin).as_bool() });
    surface.place(rect).unwrap();
    assert_eq!(
        snapshot(&window).rect,
        (
            origin.x,
            origin.y,
            origin.x + rect.width,
            origin.y + rect.height
        )
    );
    assert_eq!(snapshot(&window).menu, before.menu);
    surface.hide().unwrap();
    surface.detach().unwrap();
    assert_eq!(snapshot(&window), before);
    assert_eq!(surface.handle(), Err(SurfaceError::Detached));
    NativeSurface::attach_full_ui(window.raw(), unsafe { GetCurrentProcessId() }, host.raw())
        .unwrap()
        .detach()
        .unwrap();
}

#[test]
fn full_ui_wrong_pid_thread_and_child_windows_are_refused_without_mutation() {
    let host = full_ui_host();
    let window = full_ui_host();
    add_native_menu(&window);
    let before = snapshot(&window);
    let pid = unsafe { GetCurrentProcessId() };
    assert!(matches!(
        NativeSurface::attach_full_ui(window.raw(), pid + 1, host.raw()),
        Err(SurfaceError::WrongProcess { .. })
    ));
    assert!(matches!(
        NativeSurface::attach_full_ui(0, pid, host.raw()),
        Err(SurfaceError::InvalidWindow)
    ));
    let (source, target) = (window.raw(), host.raw());
    std::thread::spawn(move || {
        assert!(matches!(
            NativeSurface::attach_full_ui(source, pid, target),
            Err(SurfaceError::WrongThread)
        ));
    })
    .join()
    .unwrap();
    let child = OwnedWindow::new(Some(host.0), WS_CHILD, WINDOW_EX_STYLE::default());
    let child_before = snapshot(&child);
    assert!(matches!(
        NativeSurface::attach_full_ui(child.raw(), pid, host.raw()),
        Err(SurfaceError::InvalidWindow)
    ));
    assert!(matches!(
        NativeSurface::attach_full_ui(window.raw(), pid, child.raw()),
        Err(SurfaceError::InvalidWindow)
    ));
    assert_eq!(snapshot(&window), before);
    assert_eq!(snapshot(&child), child_before);
}

#[test]
fn full_ui_and_child_attachment_share_one_host_slot_and_drop_restoration() {
    let host = full_ui_host();
    let window = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let other = full_ui_host();
    add_native_menu(&window);
    let before = snapshot(&window);
    let pid = unsafe { GetCurrentProcessId() };
    {
        let surface = NativeSurface::attach_full_ui(window.raw(), pid, host.raw()).unwrap();
        assert!(matches!(
            NativeSurface::attach(window.raw(), pid, host.raw()),
            Err(SurfaceError::AlreadyAttached)
        ));
        assert!(matches!(
            NativeSurface::attach(other.raw(), pid, host.raw()),
            Err(SurfaceError::AlreadyAttached)
        ));
        assert!(matches!(
            surface.place(SurfaceRect {
                x: 0,
                y: 0,
                width: 0,
                height: 50
            }),
            Err(SurfaceError::InvalidRect)
        ));
    }
    assert_eq!(snapshot(&window), before);
    let child_surface = attach(&other, &host);
    assert!(matches!(
        NativeSurface::attach_full_ui(window.raw(), pid, host.raw()),
        Err(SurfaceError::AlreadyAttached)
    ));
    drop(child_surface);
    assert_eq!(snapshot(&window), before);
}
