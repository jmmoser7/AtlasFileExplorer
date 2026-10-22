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
fn web_read_and_capture_replies_report_the_live_url_after_navigation() {
    let mut h = web_board("web_reply_current_url");
    let host = with_fake_host(&mut h);
    let authored = "https://example.com/start";
    let current = "https://example.com/results?query=final";
    h.app.paste_web_url(authored, egui::Pos2::ZERO);
    let (portal_id, _) = only_portal(&h);
    h.app.web_allow_origin(portal_id);
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
    assert_eq!(read_reply.result["url"], current);

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
    assert_eq!(capture_reply.result["url"], current);
    assert!(capture_reply.result["image_source"].as_str().is_some());
    assert_ne!(current, authored);
    h.app.presence.agents.insert(0, a);
    let _ = std::fs::remove_dir_all(cache_dir);
}
