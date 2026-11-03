use super::*;
use crate::app::tests::Harness;
use serde_json::json;

fn host() -> Harness {
    let mut h = super::super::tests::host();
    let path = h.base.join("witnessed-production.slate");
    h.app.doc().save_to(&path).unwrap();
    h.app.tab_mut().path = Some(path);
    h.app.ai.config.workspace_dir = None;
    h
}
fn begin(h: &mut Harness, author: &str) -> Result<(Vec<NodeId>, String), String> {
    h.app.production_witness_command(
        &h.ctx,
        &h.base,
        author,
        "board.objects.create_frame",
        &[],
        &json!({"x":24,"y":32,"width":240,"height":160}),
    )
}
fn finish(h: &mut Harness, ticket: &str) -> Vec<NodeId> {
    for _ in 0..160 {
        super::super::tests::proposal_support::tick(h);
        if let Some(created) = h.app.production_witness_poll(ticket).unwrap() {
            return created;
        }
    }
    panic!("real production witness did not finish");
}

#[test]
fn production_waits_for_real_witness_before_committing_and_returning_ids() {
    let mut h = host();
    let preferences = h.app.settings.collaborators.clone();
    let before = h.app.tab().journal.undo_depth();
    let (created, ticket) = begin(&mut h, "Architect").unwrap();
    assert_eq!(created.len(), 1);
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    assert_eq!(h.app.production_witness_poll(&ticket).unwrap(), None);
    assert_eq!(finish(&mut h, &ticket), created);
    assert!(h.app.doc().scene.node(created[0]).is_some());
    assert!(h.app.tab().journal.undo_depth() > before);
    assert_eq!(h.app.ai.config.workspace_dir, None);
    assert_eq!(h.app.settings.collaborators, preferences);
}

#[test]
fn production_grants_only_current_command_and_preserves_other_conversations() {
    let mut h = host();
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    let ordinary = super::super::tests::add(&mut h, "Existing collaborator");
    let original = h.app.presence.agents[0].grant.clone();
    h.app.ai.config.workspace_dir = None;
    let (_, ticket) = begin(&mut h, "Architect").unwrap();
    let actor = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.name == "Architect")
        .unwrap();
    let who = actor.profile.id.clone();
    let binding = actor.binding.clone();
    let grant = actor.grant.as_ref().unwrap();
    assert_eq!(grant.commands, ["board.objects.create_frame"]);
    assert_eq!(grant.max_actions, 1);
    assert!(grant.mutation_autonomy && grant.readable_board);
    assert!(grant.readable_sources.is_empty() && grant.conversation_ids.is_empty());
    assert_eq!(actor.profile.provider, "local");
    assert_eq!(actor.profile.hat, "none");
    finish(&mut h, &ticket);
    let (_, other_ticket) = begin(&mut h, "Critic").unwrap();
    let ordinary_grant = &h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == ordinary)
        .unwrap()
        .grant;
    assert_eq!(ordinary_grant, &original);
    assert!(h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == who)
        .unwrap()
        .grant
        .as_ref()
        .unwrap()
        .conversation_ids
        .is_empty());
    finish(&mut h, &other_ticket);
    let (_, next) = begin(&mut h, "Architect").unwrap();
    assert_eq!(
        h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap()
            .binding,
        binding
    );
    finish(&mut h, &next);
}

#[test]
fn production_pause_resume_invalidates_the_grant_and_cancels_uncommitted_work() {
    let mut h = host();
    let (_, ticket) = begin(&mut h, "Architect").unwrap();
    let who = h.app.presence.agents[0].profile.id.clone();
    for paused in [true, false] {
        h.app
            .apply_presence_action(
                &h.ctx,
                chrome::PresenceAction::Pause {
                    id: who.clone(),
                    paused,
                },
            )
            .unwrap();
    }
    for _ in 0..10 {
        super::super::tests::proposal_support::tick(&mut h);
    }
    assert!(h.app.doc().scene.nodes.is_empty());
    assert!(h.app.presence.agents[0].work.is_none());
    assert!(h.app.production_witness_poll(&ticket).is_err());
    assert!(begin(&mut h, "Architect").is_err());
}

#[test]
fn production_refuses_revoked_departed_and_foreign_root_actors() {
    let mut h = host();
    let (_, ticket) = begin(&mut h, "Architect").unwrap();
    let who = h.app.presence.agents[0].profile.id.clone();
    h.app.stop_presence(&who, true).unwrap();
    assert!(h.app.production_witness_poll(&ticket).is_err());
    assert!(begin(&mut h, "Architect").is_err());
    h.app.presence.agents.clear();
    assert!(
        begin(&mut h, "Architect").is_err(),
        "departure cannot silently rejoin"
    );
    let foreign = h.base.join("unrelated-root");
    std::fs::create_dir_all(&foreign).unwrap();
    assert!(h
        .app
        .production_witness_command(
            &h.ctx,
            &foreign,
            "Other architect",
            "board.objects.create_frame",
            &[],
            &json!({"x":0,"y":0,"width":100,"height":100})
        )
        .is_err());
    assert!(h.app.doc().scene.nodes.is_empty());
}
