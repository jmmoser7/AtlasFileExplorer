use super::*;

#[test]
fn deep_file_requests_stay_with_their_document_and_display_context() {
    let page = resource_url("file:///C:/study/page.html#section").unwrap();
    let allowed = |uri, context| resource_url(uri).and_then(|url| eligible(&page, &url, context));
    assert!(allowed(
        "file:///C:/study/page.html",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT
    )
    .is_some());
    assert!(allowed(
        "file:///C:/study/assets/style.css",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET
    )
    .is_some());
    assert!(allowed(
        "file:///C:/study/b.js",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH
    )
    .is_none());
    assert!(allowed(
        "file:///C:/study/other.html",
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT
    )
    .is_none());
    for uri in [
        "https://remote.example/a.js",
        "file://server/share/a.js",
        "file:///C:/study/%2e%2e/a.js",
        "file:///C:/study/..%2Fa.js",
        "file:///C:/study/..%5ca.js",
        "file:///C:/study/a%3Ahidden.js",
        "file:///C:/study/a%00b.js",
    ] {
        assert!(
            allowed(uri, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT).is_none(),
            "{uri}"
        );
    }
}
