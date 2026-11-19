use super::*;

#[test]
fn navigation_start_invalidates_the_old_control_document_before_completion() {
    let mut pending = Pending {
        url: Some("file:///C:/board/control.html".into()),
        ..Default::default()
    };
    let old_epoch = pending.document_generation;
    pending.begin_navigation(Some(10));
    assert!(pending.url.is_none());
    assert_ne!(pending.document_generation, old_epoch);
    assert!(!pending.finish_navigation(
        Some(9),
        Some("file:///C:/board/control.html".into()),
        true
    ));
    assert!(pending.url.is_none());
    assert!(pending.finish_navigation(Some(10), Some("https://example.com/".into()), true));
    assert_eq!(pending.url.as_deref(), Some("https://example.com/"));
    let replaced_epoch = pending.document_generation;
    pending.begin_navigation(Some(11));
    assert!(pending.url.is_none());
    assert_ne!(pending.document_generation, replaced_epoch);
    assert!(pending.finish_navigation(
        Some(11),
        Some("file:///C:/board/control.html".into()),
        true
    ));
}

#[test]
fn failed_or_unknown_navigation_cannot_restore_control_readiness() {
    let mut pending = Pending::default();
    pending.begin_navigation(Some(20));
    assert!(!pending.finish_navigation(None, Some("file:///C:/board/control.html".into()), true));
    assert!(pending.finish_navigation(
        Some(20),
        Some("file:///C:/board/control.html".into()),
        false
    ));
    assert!(pending.url.is_none());
}

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
