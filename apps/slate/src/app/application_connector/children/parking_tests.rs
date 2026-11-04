//! Real hidden HWND ownership with an in-memory acknowledged worker queue.
use super::*;
use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::*;

struct Window(HWND);
impl Window {
    fn new() -> Self {
        Self(
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("STATIC"),
                    w!("Child hide fixture"),
                    WS_OVERLAPPEDWINDOW,
                    40,
                    50,
                    800,
                    600,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .expect("hidden fixture HWND"),
        )
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        if unsafe { IsWindow(Some(self.0)).as_bool() } {
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }
}
struct Fixture {
    child: ChildSurface,
    native: Window,
    host: Window,
    key: BindingKey,
    tx: Sender<worker::Action>,
    rx: Receiver<worker::Action>,
}
impl Fixture {
    fn new(owner: Option<HWND>) -> Self {
        let native = Window::new();
        let host = Window::new();
        if let Some(owner) = owner {
            unsafe {
                SetWindowLongPtrW(native.0, GWLP_HWNDPARENT, owner.0 as isize);
            }
        }
        let surface = NativeSurface::attach_full_ui(
            native.0 .0 as isize,
            std::process::id(),
            host.0 .0 as isize,
        )
        .unwrap();
        let key = BindingKey {
            worker: "worker-a".into(),
            source: "source-a".into(),
            document: "doc-a".into(),
        };
        let child = ChildSurface {
            native: Some(surface),
            worker: Some(key.worker.clone()),
            hide_command: Some("test.window.hide".into()),
            ..Default::default()
        };
        let (tx, rx) = crossbeam_channel::bounded(1);
        Self {
            child,
            native,
            host,
            key,
            tx,
            rx,
        }
    }
    fn park(&mut self) {
        self.child
            .park_until_ack(&self.tx, &self.key, "test-window");
    }
    fn request(&self) -> hosting::FocusRequest {
        let worker::Action::FocusChild {
            key,
            surface,
            command,
            request,
        } = self.rx.try_recv().unwrap()
        else {
            panic!("managed child hide expected")
        };
        assert!(key == self.key);
        assert_eq!(
            (surface.as_str(), command.as_str()),
            ("test-window", "test.window.hide")
        );
        assert!(!request.focused);
        request
    }
    fn owns_slot(&self) -> bool {
        !unsafe { GetPropW(self.host.0, w!("AtlasConnector.SurfaceHost.v1")) }.is_invalid()
    }
}

#[test]
fn pending_managed_hide_retains_real_owner_until_matching_ack() {
    let mut f = Fixture::new(None);
    f.park();
    let hide = f.request();
    assert!(f.child.native.is_some() && f.owns_slot());
    assert!(!unsafe { IsWindowVisible(f.native.0).as_bool() });
    f.child
        .complete_focus(&f.key, Some(&f.key), hide, Ok(()), false);
    assert!(f.child.native.is_none() && !f.owns_slot());
    assert!(unsafe { IsWindow(Some(f.native.0)).as_bool() });
}

#[test]
fn queue_full_retries_admission_but_failed_hide_never_replays_itself() {
    let mut f = Fixture::new(None);
    assert!(f
        .tx
        .try_send(worker::Action::DiscoverSurfaces { launch: None })
        .is_ok());
    f.park();
    assert!(f.child.native.is_some() && f.owns_slot());
    f.rx.try_recv().unwrap();
    f.park();
    let hide = f.request();
    f.child.complete_focus(
        &f.key,
        Some(&f.key),
        hide,
        Err("managed hide failed".into()),
        false,
    );
    f.park();
    assert!(f.rx.try_recv().is_err());
    assert!(f.child.native.is_some() && f.owns_slot());
}

#[test]
fn stale_scope_and_late_hide_cannot_release_a_reentered_child() {
    let mut f = Fixture::new(None);
    f.park();
    let hide = f.request();
    let mut newer = f.key.clone();
    newer.worker = "worker-b".into();
    f.child
        .complete_focus(&f.key, Some(&newer), hide, Ok(()), false);
    assert!(f.child.native.is_some() && f.owns_slot());
    f.child.parking_key = None;
    f.child.host.begin_intent();
    let show = f.child.host.request(true, false).unwrap();
    f.child.host.queued(show);
    f.child
        .complete_focus(&f.key, Some(&f.key), hide, Ok(()), true);
    f.child
        .complete_focus(&f.key, Some(&f.key), show, Ok(()), true);
    assert!(f.child.native.is_some() && f.owns_slot());
    assert_eq!(f.child.status, "Native application window");
    assert!(f.child.close_scope());
    assert!(!f.owns_slot());
    f.child
        .complete_focus(&f.key, Some(&f.key), hide, Ok(()), false);
    assert!(f.child.native.is_none());
}

#[test]
fn failed_hidden_detach_keeps_the_guard_and_closed_window_cleans_it() {
    let original_owner = Window::new();
    let mut f = Fixture::new(Some(original_owner.0));
    f.park();
    let hide = f.request();
    unsafe {
        DestroyWindow(original_owner.0).unwrap();
    }
    f.child
        .complete_focus(&f.key, Some(&f.key), hide, Ok(()), false);
    assert!(f.child.native.is_some() && f.owns_slot());
    assert!(f.child.status.contains("parking"));
    unsafe {
        DestroyWindow(f.native.0).unwrap();
    }
    assert!(f.child.close_scope());
    assert!(f.child.native.is_none() && !f.owns_slot());
}
