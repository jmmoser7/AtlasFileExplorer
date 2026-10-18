//! Authored-only read pages and real scoped text proposals; no provider launch.
use super::super::*;
use super::{add, host};
use crate::app::tests::Harness;
use serde_json::{json, Value};
use slate_doc::scene::{CmdAuthor, ImageNode, NodeKind, WorldRect};
use std::time::Duration;

fn setup(words: &str) -> (Harness, String, slate_doc::NodeId) {
    let mut h = host();
    h.app.ensure_work_tab();
    h.app.tab_mut().path = Some(h.base.join("authored.slate"));
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.create_text",
            &[],
            &json!({"x":0,"y":0,"width":140,"height":50,"text":words}),
        )
        .unwrap();
    h.app.commit_scene(prepared.cmds);
    let target = h.app.doc().scene.nodes[0].id;
    let who = add(&mut h, "Text collaborator");
    let a = &mut h.app.presence.agents[0];
    a.request = "authored-text-request".into();
    a.grant.as_mut().unwrap().readable_board = false;
    a.grant.as_mut().unwrap().readable_nodes = vec![target.0.to_string()];
    h.app
        .presence
        .roster
        .session_mut(&a.motion)
        .unwrap()
        .begin_request(id(&a.request).unwrap())
        .unwrap();
    (h, who, target)
}

fn call(
    h: &mut Harness,
    target: slate_doc::NodeId,
    command: &str,
    arguments: Value,
) -> Option<BoardReply> {
    let mut a = h.app.presence.agents.remove(0);
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: a.calls as u64,
        operation: BoardOperation::Invoke {
            command: command.into(),
            targets: vec![target.0.to_string()],
            arguments,
        },
    };
    let reply = h.app.presence_call_with_context(&h.ctx, &mut a, &call);
    h.app.presence.agents.insert(0, a);
    reply
}
fn page(h: &mut Harness, target: slate_doc::NodeId, arguments: Value) -> BoardReply {
    if let Some(reply) = call(h, target, "presence.read_text", arguments) {
        return reply;
    }
    for _ in 0..40 {
        let mut a = h.app.presence.agents.remove(0);
        let mut budget = 4;
        let reply = h
            .app
            .advance_presence_read(&mut a, &mut budget, Duration::from_millis(50));
        h.app.presence.agents.insert(0, a);
        if let Some(reply) = reply {
            return reply;
        }
    }
    panic!("bounded authored page did not finish");
}

#[test]
fn unicode_byte_pages_resume_without_skipping_or_repeating_scalars() {
    let (mut h, _, target) = setup("A🦊é界B");
    let original = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    let mut arguments = json!({"cursor":0,"limit":4});
    let mut joined = String::new();
    let mut cursors = Vec::new();
    loop {
        let reply = page(&mut h, target, arguments);
        assert!(reply.success, "{:?}", reply.result);
        joined.push_str(reply.result["text"].as_str().unwrap());
        assert_eq!(reply.result["total_bytes"], 11);
        let Some(next) = reply.result["next_cursor"].as_u64() else {
            break;
        };
        cursors.push(next);
        arguments = json!({"cursor":next,"limit":4,"generation":reply.result["generation"]});
    }
    assert_eq!(joined, "A🦊é界B");
    assert_eq!(cursors, [1, 5, 7]);
    assert_eq!(h.app.doc().scene, original);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.presence.agents[0]
        .grant
        .as_ref()
        .unwrap()
        .readable_sources
        .is_empty());
}

#[test]
fn text_pages_reject_bad_boundaries_stale_continuations_keys_and_empty_scope() {
    let (mut h, _, target) = setup("A🦊B");
    let generation = h.app.doc().scene.scene_gen();
    for arguments in [
        json!({"cursor":2,"generation":generation}),
        json!({"cursor":1}),
        json!({"limit":1}),
        json!({"cursor":-1}),
        json!({"source":"private.txt"}),
        json!({"cursor":1,"generation":generation+1}),
    ] {
        assert!(!page(&mut h, target, arguments).success);
    }
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_nodes
        .clear();
    assert!(!page(&mut h, target, json!({})).success);
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_nodes = vec![target.0.to_string()];
    assert!(call(&mut h, target, "presence.read_text", json!({})).is_none());
    h.app
        .patch_nodes(&[target], |node| node.rotation_deg += 1.0);
    let mut a = h.app.presence.agents.remove(0);
    let reply = h
        .app
        .advance_presence_read(&mut a, &mut 4, Duration::from_millis(50))
        .unwrap();
    h.app.presence.agents.insert(0, a);
    assert!(!reply.success && reply.result.to_string().contains("changed"));
}

#[test]
fn explicit_empty_override_is_a_page_but_missing_override_never_reads_linked_bytes() {
    let (mut h, _, _) = setup("Text");
    let item = h.app.doc_mut().add_item(
        h.base.join("never-read.txt"),
        "never-read.txt",
        100,
        0,
        "txt",
    );
    let mut image = ImageNode::new(item);
    image.text_override = Some(String::new());
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Image(image),
    );
    let target = h.app.add_nodes(vec![node])[0];
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_nodes = vec![target.0.to_string()];
    let reply = page(&mut h, target, json!({}));
    assert!(reply.success && reply.result["text"] == "" && reply.result["next_cursor"].is_null());
    h.app.patch_nodes(&[target], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.text_override = None;
        }
    });
    assert!(!page(&mut h, target, json!({})).success);
    assert!(!h.base.join("never-read.txt").exists());
}

#[test]
fn revoking_node_scope_during_visible_read_prevents_the_text_reply() {
    let (mut h, _, target) = setup("Do not disclose after revocation");
    assert!(call(&mut h, target, "presence.read_text", json!({})).is_none());
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_nodes
        .clear();
    let mut a = h.app.presence.agents.remove(0);
    let mut budget = 4;
    let reply = h
        .app
        .advance_presence_read(&mut a, &mut budget, Duration::from_millis(50))
        .unwrap();
    h.app.presence.agents.insert(0, a);
    assert!(!reply.success);
    assert!(reply.result.get("text").is_none());
    assert!(!reply.result.to_string().contains("Do not disclose"));
}

#[test]
fn approved_text_write_is_visible_one_attributed_undo_and_agent_at_text_is_not_a_human_send() {
    let (mut h, who, target) = setup("Before");
    h.frame();
    let before = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    let selection = h.app.board_sel.clone();
    let camera = (h.app.tab().cam.offset, h.app.tab().cam.z);
    let mentions = h.app.presence.mention_hashes.clone();
    assert!(call(
        &mut h,
        target,
        "board.objects.set_text",
        json!({"text":"@Text collaborator: replacement words"})
    )
    .is_none());
    for _ in 0..100 {
        super::proposal_support::tick(&mut h);
        if h.app.presence.agents[0].work.is_none() {
            break;
        }
    }
    assert_eq!(h.app.doc().scene, before);
    let proposal = h.app.presence_proposal_rows(&who)[0].id.to_owned();
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::AcceptProposal {
                participant: who.clone(),
                id: proposal,
            },
        )
        .unwrap();
    let mut writing = false;
    for _ in 0..100 {
        super::proposal_support::tick(&mut h);
        if h.app.presence.agents[0].work.is_none() {
            break;
        }
        assert_eq!(h.app.doc().scene, before);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        if let Some(node) = h.app.presence_preview(target) {
            assert_eq!(h.app.presence.agents[0].activity, "writing");
            writing |= slate_doc::authored_text::text(node)
                == Some("@Text collaborator: replacement words");
        }
    }
    assert!(writing);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Text collaborator".into()))
    );
    assert_eq!(h.app.board_sel, selection);
    assert_eq!((h.app.tab().cam.offset, h.app.tab().cam.z), camera);
    assert!(
        h.app.text_edit.is_none()
            && h.app.text_doc_edit.is_none()
            && h.app.text_box_draft.is_none()
    );
    assert!(h
        .app
        .presence_conversation(&who)
        .is_none_or(|thread| !thread.turns.iter().any(|turn| turn.role == "user")));
    assert_eq!(h.app.presence.mention_hashes, mentions);
    assert_eq!(h.app.presence.agents[0].request, "authored-text-request");
    assert!(!h.app.presence.agents[0].inflight);
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene, before);
}

#[test]
fn cancelling_approved_text_preserves_authored_words_and_undo_depth() {
    let (mut h, who, target) = setup("Before");
    h.frame();
    let before = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(call(
        &mut h,
        target,
        "board.objects.set_text",
        json!({"text":"Different words"})
    )
    .is_none());
    for _ in 0..100 {
        super::proposal_support::tick(&mut h);
        if h.app.presence.agents[0].work.is_none() {
            break;
        }
    }
    let proposal = h.app.presence_proposal_rows(&who)[0].id.to_owned();
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::AcceptProposal {
                participant: who.clone(),
                id: proposal,
            },
        )
        .unwrap();
    for _ in 0..50 {
        super::proposal_support::tick(&mut h);
        if h.app.presence_preview(target).is_some() {
            break;
        }
    }
    assert!(h.app.presence_preview(target).is_some());
    h.app.stop_presence(&who, false).unwrap();
    for _ in 0..50 {
        super::proposal_support::tick(&mut h);
    }
    assert_eq!(h.app.doc().scene, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.presence_preview(target).is_none());
}
