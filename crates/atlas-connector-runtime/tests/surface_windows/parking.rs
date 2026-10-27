use super::*;
use windows::Win32::Graphics::Gdi::{GetWindowRgnBox, NULLREGION};

#[test]
fn full_ui_park_holds_slot_until_confirmed_hidden_release_then_allows_handoff() {
    let host = full_ui_host();
    let main = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let next = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let pid = unsafe { GetCurrentProcessId() };
    let mut guard = NativeSurface::attach_full_ui(main.raw(), pid, host.raw()).unwrap();
    guard.park().unwrap();
    assert!(matches!(
        NativeSurface::attach_full_ui(next.raw(), pid, host.raw()),
        Err(SurfaceError::AlreadyAttached)
    ));
    unsafe {
        let _ = ShowWindow(main.0, SW_HIDE);
    }
    assert!(!unsafe { IsWindowVisible(main.0) }.as_bool());
    guard.detach_hidden().unwrap();
    assert!(!snapshot(&main).visible);
    NativeSurface::attach_full_ui(next.raw(), pid, host.raw())
        .unwrap()
        .detach_hidden()
        .unwrap();
}

#[test]
fn full_ui_ready_placement_and_empty_parking_keep_authenticated_ownership() {
    let host = full_ui_host();
    let window = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let original = snapshot(&window);
    let mut surface =
        NativeSurface::attach_full_ui(window.raw(), unsafe { GetCurrentProcessId() }, host.raw())
            .unwrap();
    let rect = SurfaceRect {
        x: 15,
        y: 20,
        width: 800,
        height: 500,
    };
    let clip = SurfaceRect {
        x: 25,
        y: 30,
        width: 700,
        height: 400,
    };
    assert!(
        !surface.place_clipped_ready(rect, clip).unwrap(),
        "first placement cannot reveal old desktop bounds"
    );
    assert!(surface.place_clipped_ready(rect, clip).unwrap());
    let mut bounds = RECT::default();
    unsafe {
        GetWindowRgnBox(window.0, &mut bounds);
    }
    assert_eq!(rect_tuple(bounds), (10, 10, 710, 410));
    surface.park().unwrap();
    assert_eq!(snapshot(&window).owner, host.raw());
    assert_eq!(surface.handle().unwrap(), window.raw());
    unsafe {
        let _ = ShowWindow(window.0, SW_SHOWNOACTIVATE);
    }
    let mut empty = RECT::default();
    let kind = unsafe { GetWindowRgnBox(window.0, &mut empty) };
    assert_eq!(kind.0, NULLREGION.0);
    assert_eq!(empty.right - empty.left, 0);
    assert_eq!(empty.bottom - empty.top, 0);
    surface.detach_hidden().unwrap();
    let restored = snapshot(&window);
    assert_eq!(restored.owner, original.owner);
    assert!(!restored.visible);
    assert_eq!(restored.rect, original.rect);
}

#[test]
fn full_ui_clips_requested_rect_and_restores_original_window_region() {
    use windows::Win32::Graphics::Gdi::{
        CreateRectRgn, DeleteObject, EqualRgn, GetWindowRgn, GetWindowRgnBox, SetWindowRgn,
    };
    let host = full_ui_host();
    let window = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    unsafe {
        let original = CreateRectRgn(7, 9, 177, 139);
        assert_ne!(SetWindowRgn(window.0, Some(original), false), 0);
    }
    let original_copy = unsafe { CreateRectRgn(0, 0, 0, 0) };
    assert_ne!(unsafe { GetWindowRgn(window.0, original_copy) }.0, 0);
    let mut surface =
        NativeSurface::attach_full_ui(window.raw(), unsafe { GetCurrentProcessId() }, host.raw())
            .unwrap();
    surface
        .place(SurfaceRect {
            x: 10,
            y: 12,
            width: 120,
            height: 90,
        })
        .unwrap();
    let mut bounds = RECT::default();
    assert_ne!(unsafe { GetWindowRgnBox(window.0, &mut bounds) }.0, 0);
    assert_eq!(
        (bounds.left, bounds.top, bounds.right, bounds.bottom),
        (0, 0, 120, 90)
    );
    surface.detach().unwrap();
    let restored = unsafe { CreateRectRgn(0, 0, 0, 0) };
    assert_ne!(unsafe { GetWindowRgn(window.0, restored) }.0, 0);
    assert!(unsafe { EqualRgn(original_copy, restored) }.as_bool());
    unsafe {
        let _ = DeleteObject(original_copy.into());
        let _ = DeleteObject(restored.into());
    }
}

#[test]
fn full_ui_clip_preserves_full_layout_and_hidden_detach_releases_host() {
    use windows::Win32::Graphics::Gdi::GetWindowRgnBox;
    let host = full_ui_host();
    let window = OwnedWindow::new(None, WS_OVERLAPPEDWINDOW, WINDOW_EX_STYLE::default());
    let before = snapshot(&window);
    let mut surface =
        NativeSurface::attach_full_ui(window.raw(), unsafe { GetCurrentProcessId() }, host.raw())
            .unwrap();
    surface
        .place_clipped(
            SurfaceRect {
                x: -50,
                y: -30,
                width: 640,
                height: 480,
            },
            SurfaceRect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
        )
        .unwrap();
    let current = snapshot(&window);
    assert_eq!(
        (
            current.rect.2 - current.rect.0,
            current.rect.3 - current.rect.1
        ),
        (640, 480)
    );
    let mut bounds = RECT::default();
    assert_ne!(unsafe { GetWindowRgnBox(window.0, &mut bounds) }.0, 0);
    assert_eq!(
        (bounds.left, bounds.top, bounds.right, bounds.bottom),
        (50, 30, 640, 480)
    );
    surface.detach_hidden().unwrap();
    let hidden = snapshot(&window);
    assert!(!hidden.visible);
    assert_eq!(
        hidden.style & !(WS_VISIBLE.0 as isize),
        before.style & !(WS_VISIBLE.0 as isize)
    );
    assert_eq!(hidden.owner, before.owner);
    assert_eq!(hidden.rect, before.rect);
    NativeSurface::attach_full_ui(window.raw(), unsafe { GetCurrentProcessId() }, host.raw())
        .unwrap()
        .detach_hidden()
        .unwrap();
}
