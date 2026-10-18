#[test]
fn pointer_tracks_physical_capture_resolution() {
    let point = super::capture_point(320.0, 180.0, 3.0);
    assert_eq!((point.x, point.y), (960, 540));
    let point = super::capture_point(320.0, 180.0, 6.0);
    assert_eq!((point.x, point.y), (1920, 1080));
}
use super::probe::{host, pump};
use super::*;
use slate_doc::scene::WebSourceKind;
use std::time::{Duration, Instant};

#[test]
fn a_windows_path_becomes_a_file_uri_and_a_url_is_left_alone() {
    let cases = [
        (
            r"C:\dash boards\index.html",
            "file:///C:/dash%20boards/index.html",
        ),
        (
            r"\\?\C:\dash boards\index.html",
            "file:///C:/dash%20boards/index.html",
        ),
        (
            r"\\server\share\dash boards\index.html",
            "file://server/share/dash%20boards/index.html",
        ),
        (
            r"\\?\UNC\server\share\dash boards\index.html",
            "file://server/share/dash%20boards/index.html",
        ),
        (
            r"\\?\C:\études\a #%?.html",
            "file:///C:/%C3%A9tudes/a%20%23%25%3F.html",
        ),
        (
            r"\\?\UNC\server\share\études\a #%?.html",
            "file://server/share/%C3%A9tudes/a%20%23%25%3F.html",
        ),
    ];
    for (path, expected) in cases {
        assert_eq!(navigate_uri(path).unwrap(), expected, "{path}");
    }
    for uri in [
        "https://example.com/a?b=1#c",
        "http://example.com/",
        "file:///C:/x%20y.html",
        "about:blank",
    ] {
        assert_eq!(
            navigate_uri(uri).unwrap(),
            uri,
            "an existing URI is not re-encoded"
        );
    }
}

#[test]
fn unsupported_page_paths_fail_before_webview_navigation() {
    for path in ["dash/index.html", r"C:relative.html", r"\\.\PIPE\slate"] {
        let error = navigate_uri(path).unwrap_err();
        assert_eq!(error.code(), windows::Win32::Foundation::E_INVALIDARG);
        assert!(error.message().contains("convert the local page path"));
    }
}

/// Pull frames until one satisfies `want`, pumping the message loop.
fn run_until(
    host: &mut Webview2Host,
    id: NodeId,
    req: &WebRequest,
    secs: u64,
    want: impl Fn(&egui::ColorImage) -> bool,
) -> (usize, bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut frames = 0usize;
    let mut hit = false;
    while Instant::now() < deadline && !hit {
        pump();
        // `admit` is idempotent, and re-calling it is how the environment's
        // late arrival gets picked up — exactly what the pump does.
        host.admit(id, req);
        if let Some(img) = host.take_frame(id) {
            frames += 1;
            hit = want(&img);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    (frames, hit)
}

/// The one test that proves the pipeline end to end: a real browser, a real
/// composition visual, a real capture session, real pixels. It needs a
/// desktop session and the Evergreen runtime, so it is `#[ignore]`d and run
/// deliberately:
///
/// ```powershell
/// cargo test -p slate --lib board_web_win -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn a_local_page_paints_into_a_texture() {
    let dir = std::env::temp_dir().join("slate-web-probe-local");
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("canonical #100% étude.html");
    std::fs::write(
        &page,
        "<html><body style=\"margin:0;background:#ff0000\"></body></html>",
    )
    .unwrap();
    let page = page.canonicalize().unwrap();
    assert!(page.to_string_lossy().starts_with(r"\\?\"));

    let mut host =
        host("local").expect("no WebView2 runtime, GPU device, or compositor on this machine");
    let id = NodeId(1);
    let req = WebRequest {
        // Artifact revisions use canonical Windows paths. URI conversion is
        // the host's job, including namespace removal and filename encoding.
        target: page.to_string_lossy().into_owned(),
        kind: WebSourceKind::LocalFile,
        width_css: 320,
        height_css: 200,
        raster_w: 320,
        raster_h: 200,
        link_script: String::new(),
        anchor_px: None,
        profile: "local".into(),
    };
    let (frames, red) = run_until(&mut host, id, &req, 45, |img| {
        img.pixels
            .iter()
            .any(|p| p.r() > 200 && p.g() < 80 && p.b() < 80)
    });
    println!(
        "frames: {frames}, red: {red}, error: {:?}",
        host.load_error(id)
    );
    assert!(host.load_error(id).is_none(), "{:?}", host.load_error(id));
    assert!(frames > 0, "the capture session never produced a frame");
    assert!(red, "frames arrived but the page never painted");
}

/// Browser parity: an arbitrary public page loads and paints. Needs the
/// network as well as a desktop session.
#[test]
#[ignore]
fn an_arbitrary_public_page_loads_and_paints() {
    let mut host = host("remote").expect("no WebView2 runtime on this machine");
    let id = NodeId(2);
    let req = WebRequest {
        target: "https://example.com/".into(),
        kind: WebSourceKind::Remote,
        width_css: 1024,
        height_css: 700,
        raster_w: 1024,
        raster_h: 700,
        link_script: String::new(),
        anchor_px: None,
        profile: slate_doc::scene::web_profile_name("https://example.com/"),
    };
    // Any page that renders text puts dark pixels on a light background;
    // an unpainted capture is uniformly transparent.
    let (frames, painted) = run_until(&mut host, id, &req, 60, |img| {
        img.pixels.iter().any(|p| p.a() > 200 && p.r() < 120)
    });
    println!(
        "frames: {frames}, painted: {painted}, error: {:?}",
        host.load_error(id)
    );
    assert!(frames > 0, "the capture session never produced a frame");
    assert!(painted, "example.com never rendered any content");
}

/// Eviction releases the browser and the capture session, which is what
/// keeps a board of a hundred pages to a bounded number of processes.
#[test]
#[ignore]
fn eviction_releases_the_view() {
    let mut host = host("evict").expect("no WebView2 runtime on this machine");
    let id = NodeId(3);
    let req = WebRequest {
        target: "about:blank".into(),
        kind: WebSourceKind::Remote,
        width_css: 200,
        height_css: 120,
        raster_w: 200,
        raster_h: 120,
        link_script: String::new(),
        anchor_px: None,
        profile: "local".into(),
    };
    let (frames, _) = run_until(&mut host, id, &req, 30, |_| true);
    assert!(frames > 0);
    host.evict(id);
    pump();
    assert!(host.views.is_empty(), "the view outlived its slot");
    assert!(host.take_frame(id).is_none());
}

/// Run a fixed expression in the portal's page and wait for its JSON.
fn eval(host: &Webview2Host, id: NodeId, script: &str, secs: u64) -> Option<String> {
    let webview = host.webview(id)?;
    let out: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let sink = out.clone();
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |_, json| {
        *sink.borrow_mut() = Some(json);
        Ok(())
    }));
    let script = wide(script);
    unsafe { webview.ExecuteScript(PCWSTR(script.as_ptr()), &handler) }.ok()?;
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline && out.borrow().is_none() {
        pump();
        std::thread::sleep(Duration::from_millis(20));
    }
    let json = out.borrow_mut().take();
    json
}

fn pump_until(
    host: &mut Webview2Host,
    id: NodeId,
    req: &WebRequest,
    secs: u64,
    done: impl Fn(&Webview2Host) -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        pump();
        host.admit(id, req);
        if done(host) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn local_request(page: &std::path::Path) -> WebRequest {
    WebRequest {
        target: page.to_string_lossy().into_owned(),
        kind: WebSourceKind::LocalFile,
        width_css: 400,
        height_css: 300,
        raster_w: 400,
        raster_h: 300,
        link_script: String::new(),
        anchor_px: None,
        profile: "local".into(),
    }
}

/// The sign-in shape end to end: a sized `window.open` gets its own owned
/// window, the pop-up reaches its opener, and `window.close()` closes the
/// window while the portal stays on its page.
///
/// ```powershell
/// cargo test -p slate --lib popup -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn a_sign_in_popup_keeps_its_opener_and_closes_itself() {
    let dir = std::env::temp_dir().join("slate-web-probe-popup");
    std::fs::create_dir_all(&dir).unwrap();
    let opener = dir.join("opener.html");
    std::fs::write(
        &opener,
        "<html><body><script>\
         addEventListener('message', e => { document.title = e.data; });\
         window.open('signin.html', 'signin', 'width=420,height=360');\
         </script></body></html>",
    )
    .unwrap();
    std::fs::write(
        dir.join("signin.html"),
        "<html><body><script>\
         window.opener.postMessage('signed-in', '*');\
         setTimeout(() => window.close(), 300);\
         </script></body></html>",
    )
    .unwrap();

    let mut host = host("popup").expect("no WebView2 runtime on this machine");
    let id = NodeId(7);
    let req = local_request(&opener);
    let opened = pump_until(&mut host, id, &req, 45, |h| h.holds_popup(id));
    assert!(opened, "the sized window.open never became an owned window");
    let closed = pump_until(&mut host, id, &req, 30, |h| !h.holds_popup(id));
    assert!(
        closed,
        "window.close() in the pop-up did not close its window"
    );
    let title = eval(&host, id, "document.title", 10);
    println!("opener title: {title:?}, url: {:?}", host.current_url(id));
    assert_eq!(
        title.as_deref(),
        Some("\"signed-in\""),
        "the opener never heard back"
    );
    assert!(
        host.current_url(id)
            .is_some_and(|u| u.ends_with("opener.html")),
        "the portal was navigated away from the opener"
    );
}

/// A bare `window.open` (no size, no position) keeps the old behaviour: the
/// portal navigates in place and no window appears.
#[test]
#[ignore]
fn a_featureless_window_open_navigates_the_portal_in_place() {
    let dir = std::env::temp_dir().join("slate-web-probe-blank");
    std::fs::create_dir_all(&dir).unwrap();
    let start = dir.join("start.html");
    std::fs::write(
        &start,
        "<html><body><script>window.open('next.html');</script></body></html>",
    )
    .unwrap();
    std::fs::write(dir.join("next.html"), "<html><body>next</body></html>").unwrap();

    let mut host = host("blank").expect("no WebView2 runtime on this machine");
    let id = NodeId(8);
    let req = local_request(&start);
    let moved = pump_until(&mut host, id, &req, 45, |h| {
        h.current_url(id).is_some_and(|u| u.ends_with("next.html"))
    });
    println!("url: {:?}", host.current_url(id));
    assert!(moved, "the portal did not follow the link in place");
    assert!(!host.holds_popup(id), "a link must not open a window");
}

/// Evicting a portal closes the sign-in window its page opened.
#[test]
#[ignore]
fn evicting_the_portal_closes_its_popup() {
    let dir = std::env::temp_dir().join("slate-web-probe-popup-evict");
    std::fs::create_dir_all(&dir).unwrap();
    let opener = dir.join("opener.html");
    std::fs::write(
        &opener,
        "<html><body><script>window.open('about:blank', 'x', 'width=300,height=200');</script></body></html>",
    )
    .unwrap();
    let mut host = host("popup-evict").expect("no WebView2 runtime on this machine");
    let id = NodeId(9);
    let req = local_request(&opener);
    assert!(pump_until(&mut host, id, &req, 45, |h| h.holds_popup(id)));
    host.evict(id);
    pump();
    assert!(!host.holds_popup(id));
    assert!(
        POPUP_SLOTS.with(|s| s.borrow().is_empty()),
        "the pop-up window outlived its portal"
    );
}

/// What Google's sign-in page says to WebView2, without entering anything.
/// Prints the user agent and the page's visible text; asserts nothing
/// about Google's policy, which can change.
#[test]
#[ignore]
fn google_sign_in_page_in_webview2() {
    let mut host = host("google").expect("no WebView2 runtime on this machine");
    let id = NodeId(10);
    let req = WebRequest {
        target: "https://accounts.google.com/".into(),
        kind: WebSourceKind::Remote,
        width_css: 1024,
        height_css: 768,
        raster_w: 1024,
        raster_h: 768,
        link_script: String::new(),
        anchor_px: None,
        profile: slate_doc::scene::web_profile_name("https://accounts.google.com/"),
    };
    let loaded = pump_until(&mut host, id, &req, 60, |h| {
        h.current_url(id)
            .is_some_and(|u| u.contains("accounts.google.com/") && u.len() > 30)
    });
    // Let the sign-in form render after the redirect.
    let settle = Instant::now() + Duration::from_secs(5);
    while Instant::now() < settle {
        pump();
        host.admit(id, &req);
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("loaded: {loaded}, url: {:?}", host.current_url(id));
    println!("error: {:?}", host.load_error(id));
    println!(
        "userAgent: {:?}",
        eval(&host, id, "navigator.userAgent", 10)
    );
    println!(
        "webdriver: {:?}",
        eval(&host, id, "String(navigator.webdriver)", 10)
    );
    println!(
        "text: {:?}",
        eval(
            &host,
            id,
            "document.body ? document.body.innerText.slice(0, 800) : ''",
            10
        )
    );
}
