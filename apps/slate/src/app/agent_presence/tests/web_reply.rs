use super::*;

fn finish(h: &mut crate::app::tests::Harness, a: &mut Collaborator) -> BoardReply {
    for _ in 0..80 {
        if let Some(reply) =
            h.app
                .advance_presence_portal_call(&h.ctx, a, Duration::from_millis(100))
        {
            return reply;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("portal call did not complete")
}

#[test]
fn web_read_and_capture_replies_report_only_safe_live_page_identity() {
    let mut h = web_board("web_reply_current_url");
    let host = with_fake_host(&mut h);
    let authored = "https://example.com/start";
    let current = "https://synthetic-user:synthetic-password@example.com/results?code=synthetic-code&state=synthetic-state#access_token=synthetic-token";
    let reported = "https://example.com/results";
    h.app.paste_web_url(authored, egui::Pos2::ZERO);
    let (portal_id, _) = only_portal(&h);
    h.app.web_allow_origin(portal_id);
    h.app
        .web
        .grant_consent(slate_doc::scene::web_origin(current).unwrap());
    h.frame();
    assert!(h.app.portal_maximize(portal_id));
    h.frame();
    assert!(h.app.web.is_live(portal_id));
    h.app.web_blur();
    host.set_current_url(portal_id, current);

    let cache_dir = h.base.join("web-still-cache");
    let key = crate::app::board_web_stills::StillCache::key(authored);
    let mut cache = crate::app::board_web_stills::StillCache::new(cache_dir.clone());
    cache.store_text(&key, "The result page is loaded.");
    let text_path = cache_dir.join(format!("{key}.txt"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !text_path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(text_path.exists(), "the fixture text was persisted");
    h.app.web.set_stills(cache);

    join(&mut h);
    let mut a = h.app.presence.agents.remove(0);
    begin_request(&mut a);
    h.app
        .presence
        .roster
        .session_mut(&a.motion)
        .unwrap()
        .begin_request(id(&a.request).unwrap())
        .unwrap();
    let read = call(
        &a,
        0,
        BoardOperation::Invoke {
            command: "portal.web.read_text".into(),
            targets: vec![portal_id.0.to_string()],
            arguments: json!({}),
        },
    );
    assert!(h
        .app
        .begin_presence_portal_call(Some(&h.ctx), &mut a, &read)
        .unwrap()
        .is_none());
    let read_reply = finish(&mut h, &mut a);
    assert!(read_reply.success, "{:?}", read_reply.result);
    assert!(read_reply.result["text"]
        .as_str()
        .unwrap()
        .contains("result page"));
    assert_eq!(read_reply.result["url"], reported);
    assert_safe_url_reply(&read_reply);
    assert_eq!(host.current_url(portal_id).as_deref(), Some(current));

    host.0.borrow_mut().current_urls.remove(&portal_id);
    let unknown_url_read = call(
        &a,
        1,
        BoardOperation::Invoke {
            command: "portal.web.read_text".into(),
            targets: vec![portal_id.0.to_string()],
            arguments: json!({}),
        },
    );
    assert!(h
        .app
        .begin_presence_portal_call(Some(&h.ctx), &mut a, &unknown_url_read)
        .unwrap()
        .is_none());
    let unknown_url_reply = finish(&mut h, &mut a);
    assert!(unknown_url_reply.success, "{:?}", unknown_url_reply.result);
    assert!(unknown_url_reply.result["url"].is_null());
    host.set_current_url(portal_id, current);

    let capture = call(
        &a,
        2,
        BoardOperation::Invoke {
            command: "portal.web.capture".into(),
            targets: vec![portal_id.0.to_string()],
            arguments: json!({}),
        },
    );
    assert!(h
        .app
        .begin_presence_portal_call(Some(&h.ctx), &mut a, &capture)
        .unwrap()
        .is_none());
    let capture_reply = finish(&mut h, &mut a);
    assert!(capture_reply.success, "{:?}", capture_reply.result);
    assert_eq!(capture_reply.result["url"], reported);
    assert_safe_url_reply(&capture_reply);
    assert!(capture_reply.result["image_source"].as_str().is_some());
    assert_eq!(host.current_url(portal_id).as_deref(), Some(current));
    assert_ne!(current, authored);

    host.0.borrow_mut().current_urls.remove(&portal_id);
    let unknown_url_capture = call(
        &a,
        3,
        BoardOperation::Invoke {
            command: "portal.web.capture".into(),
            targets: vec![portal_id.0.to_string()],
            arguments: json!({}),
        },
    );
    assert!(h
        .app
        .begin_presence_portal_call(Some(&h.ctx), &mut a, &unknown_url_capture)
        .unwrap()
        .is_none());
    let unknown_capture_reply = finish(&mut h, &mut a);
    assert!(
        unknown_capture_reply.success,
        "{:?}",
        unknown_capture_reply.result
    );
    assert!(unknown_capture_reply.result["url"].is_null());
    h.app.presence.agents.insert(0, a);
    let _ = std::fs::remove_dir_all(cache_dir);
}

fn assert_safe_url_reply(reply: &BoardReply) {
    let serialized = serde_json::to_string(reply).unwrap();
    for private in [
        "synthetic-user",
        "synthetic-password",
        "synthetic-code",
        "synthetic-state",
        "synthetic-token",
    ] {
        assert!(!serialized.contains(private), "reply disclosed {private}");
    }
}

#[test]
fn diagnostic_url_retains_page_paths_without_query_or_fragment() {
    let mut h = web_board("web_reply_url_components");
    let host = with_fake_host(&mut h);
    h.app
        .paste_web_url("https://example.com/start", egui::Pos2::ZERO);
    let (portal_id, _) = only_portal(&h);
    h.frame();
    for (current, reported) in [
        (
            "https://example.com/results?query=final#section",
            "https://example.com/results",
        ),
        (
            "https://example.com:8443/a%20b?private=value#route",
            "https://example.com:8443/a%20b",
        ),
        (
            "file:///C:/synthetic-dashboard/index.html?private=value#route",
            "file:///C:/synthetic-dashboard/index.html",
        ),
    ] {
        host.set_current_url(portal_id, current);
        assert_eq!(
            h.app.agent_web_current_url(portal_id).as_deref(),
            Some(reported)
        );
        assert_eq!(host.current_url(portal_id).as_deref(), Some(current));
    }
    for unsupported in [
        "not a URL",
        "data:text/plain,synthetic-secret",
        "javascript:synthetic_secret()",
        "about:blank",
    ] {
        host.set_current_url(portal_id, unsupported);
        assert!(h.app.agent_web_current_url(portal_id).is_none());
        assert_eq!(host.current_url(portal_id).as_deref(), Some(unsupported));
    }
}
