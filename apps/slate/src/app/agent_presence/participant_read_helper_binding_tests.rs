use super::*;
use serde_json::{json, Value};
use std::time::Duration;

#[test]
fn helper_dispatch_rebinds_parent_scope_for_list_and_utf8_text_pages() {
    let (mut h, who) = joined();
    let (dir, binding, request) = {
        let parent = &h.app.presence.agents[0];
        (
            parent.dir.clone(),
            parent.binding.clone(),
            parent.request.clone(),
        )
    };
    let spawn = BoardCall {
        id: "spawn-helper".into(),
        session: binding.clone(),
        request_id: request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "presence.helper".into(),
            targets: vec![],
            arguments: json!({"task":"Return a short result"}),
        },
    };
    let reply = begin(&mut h, &spawn).expect("helper dispatch replies immediately");
    assert!(reply.success, "{reply:?}");
    let helper = reply.result["helper"].as_str().unwrap().to_owned();
    assert_eq!(h.app.presence.agents[0].request, request);
    assert_eq!(h.app.presence.agents[0].binding, binding);

    let session_path = dir.join("board-session.json");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = std::fs::read(&session_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if state.as_ref().is_some_and(|state| {
            state["request_id"] == request
                && state["session"]["session_id"] == binding.session_id
                && state["grant"]["conversation_ids"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == &helper))
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "updated worker binding was not written"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    // The worker must accept the helper's scoped text-page request under the
    // refreshed grant while keeping the parent's session and request intact.
    let generation = "helper-result-generation";
    let worker_page = BoardCall {
        id: "helper-text-page".into(),
        session: binding.clone(),
        request_id: request.clone(),
        sequence: 1,
        operation: BoardOperation::ReadParticipants {
            participant_ids: vec![helper.clone()],
            offset: 0,
            limit: 1,
            text_cursor: Some(512),
            text_limit: Some(128),
            text_generation: Some(generation.into()),
        },
    };
    std::fs::create_dir_all(dir.join("board-calls")).unwrap();
    std::fs::write(
        dir.join("board-calls/helper-text-page.json"),
        serde_json::to_vec(&worker_page).unwrap(),
    )
    .unwrap();
    let mut accepted = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while accepted.is_empty() {
        accepted.extend(h.app.presence.links.poll_limit(&dir, 1));
        assert!(
            !dir.join("board-replies/helper-text-page.json").exists(),
            "worker rejected the scoped helper page"
        );
        assert!(
            Instant::now() < deadline,
            "worker did not accept helper page"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(accepted, vec![worker_page]);

    let list_call = call(&h, 1, vec![], 0);
    let listed = read(&mut h, &list_call);
    assert!(listed.success, "{listed:?}");
    assert!(listed.result["participants"]
        .as_array()
        .unwrap()
        .iter()
        .any(|participant| participant["id"] == helper));

    let original = "🦀".repeat(300);
    h.app
        .presence
        .helper_results
        .push(helper_scope::HelperResult {
            id: helper.clone(),
            parent_id: who,
            request_id: generation.into(),
            name: "Helper".into(),
            result: original.clone(),
            total_bytes: original.len(),
            truncated: false,
            status: "Finished".into(),
            outcome: "success".into(),
            expired: false,
            retired_at: Instant::now(),
        });
    let page = BoardCall {
        id: "helper-host-text-page".into(),
        session: binding,
        request_id: request,
        sequence: 2,
        operation: BoardOperation::ReadParticipants {
            participant_ids: vec![helper],
            offset: 0,
            limit: 1,
            text_cursor: Some(512),
            text_limit: Some(128),
            text_generation: Some(generation.into()),
        },
    };
    let page = read(&mut h, &page);
    assert!(page.success, "{page:?}");
    assert_eq!(page.result["text_cursor"], 512);
    assert_eq!(page.result["text"], &original[512..640]);
    assert_eq!(page.result["next_text_cursor"], 640);
}
