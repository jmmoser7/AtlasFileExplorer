//! Participant cursor regressions share the lifecycle fixture and real dispatcher.
use super::*;
use serde_json::{json, Value};
use std::time::Duration;

fn joined() -> (Harness, String) {
    let mut h = host();
    let parent = profile("Dot");
    let who = parent.id.clone();
    h.app.join_presence(parent, None).unwrap();
    h.app
        .presence_send_chat(&who, "Collect helper outcomes")
        .unwrap();
    assert!(!h.app.presence.agents[0].request.is_empty());
    (h, who)
}

fn retained(h: &mut Harness, parent: &str, count: usize, escaped: bool) -> Vec<String> {
    let ids: Vec<_> = (0..count).map(|n| format!("retained-{n}")).collect();
    let grant = h.app.presence.agents[0].grant.as_mut().unwrap();
    grant.conversation_ids = ids.clone();
    for (n, id) in ids.iter().enumerate() {
        // Existing lifecycle tests verify capture; these are already-retained public facts.
        h.app.presence.helper_endpoints.insert(id.clone());
        h.app
            .presence
            .helper_results
            .push(helper_scope::HelperResult {
                id: id.clone(),
                parent_id: parent.into(),
                request_id: format!("helper-request-{n}"),
                name: if escaped {
                    format!("{}🦀", "\0".repeat(255))
                } else {
                    format!("Helper {n}")
                },
                result: if escaped {
                    "\0".repeat(2048)
                } else {
                    "🦀".repeat(2048)
                },
                status: if escaped {
                    "\0".repeat(2048)
                } else {
                    "Finished ✓".into()
                },
                outcome: if n % 2 == 0 { "success" } else { "error" }.into(),
                expired: n % 3 == 0,
                retired_at: Instant::now(),
            });
    }
    ids
}

fn call(h: &Harness, sequence: u64, ids: Vec<String>, offset: usize) -> BoardCall {
    let parent = &h.app.presence.agents[0];
    BoardCall {
        id: format!("participant-{sequence}-{offset}"),
        session: parent.binding.clone(),
        request_id: parent.request.clone(),
        sequence,
        operation: BoardOperation::ReadParticipants {
            participant_ids: ids,
            offset,
            limit: 8,
        },
    }
}

fn begin(h: &mut Harness, call: &BoardCall) -> Option<BoardReply> {
    let mut parent = h.app.presence.agents.remove(0);
    let reply = h.app.presence_call(&mut parent, call);
    h.app.presence.agents.insert(0, parent);
    reply
}

fn finish(h: &mut Harness) -> BoardReply {
    let mut parent = h.app.presence.agents.remove(0);
    let reply = h
        .app
        .advance_presence_read(&mut parent, &mut 256, Duration::ZERO)
        .unwrap();
    h.app.presence.agents.insert(0, parent);
    reply
}

fn read(h: &mut Harness, call: &BoardCall) -> BoardReply {
    begin(h, call).unwrap_or_else(|| finish(h))
}

#[test]
fn retained_participants_have_an_independent_cursor_and_direct_provenance_lookup() {
    let (mut h, who) = joined();
    assert!(h.app.doc().scene.nodes.is_empty());
    let ids = retained(&mut h, &who, 11, false);
    let mut offset = 0;
    let mut reached = Vec::new();
    let mut sequence = 0;
    loop {
        let query = call(&h, sequence, vec![], offset);
        let reply = read(&mut h, &query);
        assert!(reply.success, "{reply:?}");
        assert_eq!(reply.result["total"], 11);
        let page = reply.result["participants"].as_array().unwrap();
        assert!(!page.is_empty());
        reached.extend(
            page.iter()
                .map(|item| item["id"].as_str().unwrap().to_owned()),
        );
        assert_eq!(page[0]["parent_id"], who);
        assert!(page[0]["request_id"]
            .as_str()
            .unwrap()
            .starts_with("helper-request-"));
        sequence += 1;
        let Some(next) = reply.result["next_offset"].as_u64() else {
            break;
        };
        assert_eq!(next as usize, offset + page.len());
        offset = next as usize;
    }
    assert!(sequence > 1);
    assert_eq!(reached.len(), ids.len());
    for id in &ids {
        assert_eq!(reached.iter().filter(|item| *item == id).count(), 1);
    }
    let query = call(&h, sequence, vec![ids[9].clone()], 0);
    let direct = read(&mut h, &query);
    assert!(direct.success, "{direct:?}");
    assert_eq!(direct.result["total"], 1);
    assert_eq!(direct.result["participants"][0]["id"], ids[9]);
    assert_eq!(
        direct.result["participants"][0]["request_id"],
        "helper-request-9"
    );
    assert_eq!(direct.result["participants"][0]["outcome"], "error");
    let query = call(&h, sequence + 1, vec![], 1_000_000);
    let end = read(&mut h, &query);
    assert!(end.success);
    assert_eq!(end.result["participants"], json!([]));
    assert_eq!(end.result["next_offset"], Value::Null);
}

#[test]
fn escaped_participant_pages_follow_actual_emitted_count_within_wire_budget() {
    let (mut h, who) = joined();
    let ids = retained(&mut h, &who, 11, true);
    let mut offset = 0;
    let mut reached = Vec::new();
    let mut sequence = 0;
    loop {
        let query = call(&h, sequence, vec![], offset);
        let reply = read(&mut h, &query);
        assert!(reply.success, "{reply:?}");
        assert!(serde_json::to_vec(&reply.result).unwrap().len() <= 60_000);
        assert!(serde_json::to_vec(&reply).unwrap().len() <= MAX_BOARD_MESSAGE_BYTES as usize);
        assert_eq!(reply.result["total"], 11);
        let page = reply.result["participants"].as_array().unwrap();
        assert!(!page.is_empty());
        if sequence == 0 {
            assert!(page.len() < 8, "the fixture must exercise byte spill");
        }
        reached.extend(
            page.iter()
                .map(|item| item["id"].as_str().unwrap().to_owned()),
        );
        sequence += 1;
        let Some(next) = reply.result["next_offset"].as_u64() else {
            break;
        };
        assert_eq!(next as usize, offset + page.len());
        offset = next as usize;
    }
    assert_eq!(reached.len(), ids.len());
    for id in ids {
        assert_eq!(reached.iter().filter(|item| **item == id).count(), 1);
    }
}

#[test]
fn participant_scope_includes_the_removed_caller_exactly_once_and_can_remove_it() {
    let (mut h, who) = joined();
    let query = call(&h, 0, vec![who.clone()], 0);
    let direct = read(&mut h, &query);
    assert!(direct.success);
    assert_eq!(direct.result["total"], 1);
    assert_eq!(direct.result["participants"][0]["id"], who);
    let query = call(&h, 1, vec![], 0);
    let all = read(&mut h, &query);
    assert!(all.success);
    assert_eq!(all.result["total"], 1);
    assert_eq!(all.result["participants"].as_array().unwrap().len(), 1);
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .conversation_ids
        .clear();
    let query = call(&h, 2, vec![who], 0);
    assert!(!read(&mut h, &query).success);
    let query = call(&h, 3, vec![], 0);
    let empty = read(&mut h, &query);
    assert!(empty.success);
    assert_eq!(empty.result["total"], 0);
}

#[test]
fn participant_reads_keep_parent_isolation_and_expired_scope_pruning() {
    let (mut h, who) = joined();
    let ids = retained(&mut h, &who, 2, false);
    h.app.presence.helper_results[1].parent_id = "another-parent".into();
    let query = call(&h, 0, vec![ids[1].clone()], 0);
    let refused = read(&mut h, &query);
    assert!(!refused.success);
    assert!(refused.result["error"]
        .as_str()
        .unwrap()
        .contains("only to its parent"));
    let query = call(&h, 1, vec![], 0);
    let scoped = read(&mut h, &query);
    assert!(scoped.success);
    assert_eq!(scoped.result["total"], 1);
    assert_eq!(scoped.result["participants"][0]["id"], ids[0]);
    h.app.presence.helper_results[0].retired_at = Instant::now() - Duration::from_secs(901);
    h.app.presence_pump(&h.ctx);
    assert!(!h.app.presence.agents[0]
        .grant
        .as_ref()
        .unwrap()
        .conversation_ids
        .contains(&ids[0]));
    let query = call(&h, 2, vec![ids[0].clone()], 0);
    let stale = read(&mut h, &query);
    assert!(!stale.success);
    assert!(stale.result["error"].as_str().unwrap().contains("outside"));
}

#[test]
fn participant_reads_recheck_inflight_scope_request_and_session() {
    for change in ["explicit-scope", "enumerated-scope", "request", "session"] {
        let (mut h, who) = joined();
        let ids = retained(&mut h, &who, 1, false);
        let explicit = if change == "enumerated-scope" {
            vec![]
        } else {
            ids
        };
        let query = call(&h, 0, explicit, 0);
        assert!(begin(&mut h, &query).is_none());
        let parent = &mut h.app.presence.agents[0];
        match change {
            "explicit-scope" | "enumerated-scope" => {
                parent.grant.as_mut().unwrap().conversation_ids.clear()
            }
            "request" => parent.request = atlas_agent::request_id(),
            "session" => parent.binding.session_id = atlas_agent::request_id(),
            _ => unreachable!(),
        }
        let reply = finish(&mut h);
        if change == "enumerated-scope" {
            assert!(reply.success);
            assert_eq!(reply.result["participants"], json!([]));
            assert_eq!(reply.result["total"], 0);
        } else {
            assert!(!reply.success, "{change}: {reply:?}");
        }
        assert!(h.app.presence.agents[0].read.is_none());
    }
}

#[test]
fn stop_revoke_and_retirement_cancel_participant_reads_and_refuse_old_calls() {
    for change in ["stop", "revoke", "retire"] {
        let (mut h, who) = joined();
        retained(&mut h, &who, 1, false);
        let query = call(&h, 0, vec![], 0);
        assert!(begin(&mut h, &query).is_none());
        match change {
            "stop" => h.app.stop_presence(&who, false).unwrap(),
            "revoke" => h.app.stop_presence(&who, true).unwrap(),
            "retire" => h.app.retire_presence(&who, "bow").unwrap(),
            _ => unreachable!(),
        }
        assert!(h.app.presence.agents[0].read.is_none());
        let reply = begin(&mut h, &query).expect("old authority must not start a read");
        assert!(!reply.success, "{change}: {reply:?}");
        assert!(h.app.presence.agents[0].read.is_none());
    }
}

#[test]
fn participant_dispatch_enforces_duplicate_sequences_and_action_limits() {
    let (mut h, who) = joined();
    retained(&mut h, &who, 1, false);
    h.app.presence.agents[0].grant.as_mut().unwrap().max_actions = 2;
    let query = call(&h, 0, vec![], 0);
    assert!(read(&mut h, &query).success);
    let mut duplicate = query.clone();
    duplicate.id = "different-call-same-sequence".into();
    let refused = read(&mut h, &duplicate);
    assert!(!refused.success);
    assert!(refused.result["error"]
        .as_str()
        .unwrap()
        .contains("duplicate sequence"));
    assert_eq!(h.app.presence.agents[0].calls, 1);
    let query = call(&h, 1, vec![], 0);
    assert!(read(&mut h, &query).success);
    assert_eq!(h.app.presence.agents[0].calls, 2);
    let query = call(&h, 2, vec![], 0);
    let refused = read(&mut h, &query);
    assert!(!refused.success);
    assert!(refused.result["error"].as_str().unwrap().contains("budget"));
    assert_eq!(h.app.presence.agents[0].calls, 2);
}

#[test]
fn participant_reads_survive_unrelated_journaled_scene_changes() {
    let (mut h, who) = joined();
    retained(&mut h, &who, 1, false);
    let query = call(&h, 0, vec![], 0);
    assert!(begin(&mut h, &query).is_none());
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.create_shape",
            &[],
            &json!({"x":0,"y":0,"width":40,"height":40}),
        )
        .unwrap();
    assert!(h.app.commit_scene(prepared.cmds));
    let reply = finish(&mut h);
    assert!(reply.success, "{reply:?}");
    assert_eq!(reply.result["total"], 1);
}

#[test]
fn ordinary_board_reads_preserve_legacy_live_descriptors_and_excerpt_bounds() {
    let (mut h, _) = joined();
    let peer_count = atlas_agent::presence::SESSION_LIMIT - 1;
    for n in 0..peer_count {
        h.app
            .join_presence(profile(&format!("Peer {n}")), None)
            .unwrap();
        let peer = h.app.presence.agents.last_mut().unwrap();
        peer.status = "z".repeat(600);
        if n != 0 {
            peer.status = "Ready".into();
        }
        peer.snapshot = Some(Arc::new(AgentSession {
            turns: vec![atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "🦀".repeat(512),
                at: 0,
            }],
            ..Default::default()
        }));
    }
    assert_eq!(
        h.app.presence.agents.len(),
        atlas_agent::presence::SESSION_LIMIT
    );
    let mut query = call(&h, 0, vec![], 0);
    query.operation = BoardOperation::Read {
        node_ids: vec![],
        offset: 0,
        limit: 32,
    };
    let reply = read(&mut h, &query);
    assert!(reply.success, "{reply:?}");
    let peers = reply.result["participants"].as_array().unwrap();
    assert_eq!(peers.len(), peer_count);
    assert_eq!(peers[0]["name"], "Peer 0");
    assert_eq!(peers[0]["status"].as_str().unwrap().len(), 600);
    assert!(peers
        .iter()
        .all(|peer| peer["result"].as_str().unwrap().chars().count() == 256));
    assert!(serde_json::to_vec(&reply.result).unwrap().len() <= 60_000);
}
