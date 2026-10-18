use super::*;
use crate::app::tests::Harness;

fn host() -> (Harness, String) {
    let mut h = Harness::new("presence_avatar_input");
    h.app.at_home = false;
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    let who = atlas_agent::request_id();
    h.app
        .join_presence(
            StoredAgent {
                id: who.clone(),
                name: "Dot".into(),
                provider: "local".into(),
                hat: "none".into(),
                retired: false,
            },
            None,
        )
        .unwrap();
    h.frame();
    (h, who)
}
fn hold(h: &mut Harness, who: &str, until: f64) {
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::Pause {
                id: who.into(),
                paused: true,
            },
        )
        .unwrap();
    h.app.presence.avatar_hold = Some(InputHold {
        who: who.into(),
        request: h.app.presence.agents[0].request.clone(),
        origin: egui::Pos2::ZERO,
        until,
    });
}
#[test]
fn single_click_hold_expires_but_addressing_establishes_a_listening_pause() {
    let (mut h, who) = host();
    hold(&mut h, &who, -1.0);
    h.app.presence_release_avatar_hold(&h.ctx, None, false);
    assert!(!h.app.presence.agents[0].paused);
    assert!(h.app.presence.chrome.canvas_chat().is_none());
    hold(&mut h, &who, 10.0);
    h.app.presence_address(&h.ctx, &who).unwrap();
    h.app.presence_release_avatar_hold(&h.ctx, None, false);
    assert!(h.app.presence.agents[0].paused);
    assert_eq!(h.app.presence.agents[0].activity, "listening");
    assert_eq!(h.app.presence.chrome.canvas_chat(), Some(who.as_str()));
}
#[test]
fn explicit_user_pause_cannot_be_undone_by_a_previous_temporary_hold() {
    let (mut h, who) = host();
    hold(&mut h, &who, 10.0);
    assert!(h.app.dispatch_presence(
        &h.ctx,
        "presence.pause",
        Some(&serde_json::json!({"id":who,"paused":true}).to_string())
    ));
    assert!(h.app.presence.avatar_hold.is_none());
    h.app.presence_release_avatar_hold(&h.ctx, None, false);
    assert!(h.app.presence.agents[0].paused);
}
#[test]
fn removing_photo_fences_pending_import_and_retains_conversation_and_draft() {
    let (mut h, _) = host();
    h.app.presence_send_chat("self", "Keep this note").unwrap();
    h.app
        .presence_save_draft("self", "Keep this draft")
        .unwrap();
    h.app.settings.presence_identity.photo = Some(h.base.join("old.png"));
    let (tx, rx) = std::sync::mpsc::channel::<Result<Option<Photo>, String>>();
    h.app.presence.photo.rx = Some(rx);
    h.app.presence.photo.avatar = Some(vec![1, 2, 3]);
    h.app.presence_remove_photo("self").unwrap();
    assert!(tx.send(Ok(None)).is_err());
    assert!(h.app.settings.presence_identity.photo.is_none());
    assert!(h.app.presence.photo.avatar().is_none());
    assert!(h.app.presence.photo.texture().is_none());
    let c = h.app.presence_conversation("self").unwrap();
    assert_eq!(c.turns[0].text, "Keep this note");
    assert_eq!(c.draft, "Keep this draft");
}

#[test]
fn stale_provider_progress_cannot_expire_an_addressed_listening_pause() {
    let (mut h, who) = host();
    let quiet = Instant::now() - std::time::Duration::from_secs(9);
    h.app.presence.agents[0].last_active = quiet;
    let binding = h.app.presence.agents[0].binding.clone();
    h.app.presence_address(&h.ctx, &who).unwrap();
    for _ in 0..9 {
        h.frame();
        let a = &h.app.presence.agents[0];
        assert!(a.paused);
        assert_eq!(a.activity, "listening");
        assert_eq!(a.status, "Listening; task suspended");
        assert_eq!(h.app.presence.avatar_activity(a), "listening");
        assert_eq!(
            a.last_active, quiet,
            "local addressing is not provider progress"
        );
        assert_eq!(a.binding, binding);
        assert!(a.request.is_empty() && !a.inflight);
        assert!(a.work.is_none() && a.read.is_none());
        assert!(a.runtime.is_none() && a.cursor.is_none());
    }
    h.app.presence.chrome.close_chat();
    h.frame();
    let a = &h.app.presence.agents[0];
    assert!(a.paused);
    assert_eq!(a.activity, "listening");
    assert_eq!(h.app.presence.avatar_activity(a), "listening");
    assert_eq!(a.last_active, quiet);
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::Pause {
                id: who,
                paused: false,
            },
        )
        .unwrap();
    assert_eq!(h.app.presence.agents[0].activity, "idle");
    h.frame();
    let a = &h.app.presence.agents[0];
    assert!(!a.paused);
    assert_eq!(
        a.activity, "resting",
        "ordinary quiet fallback remains active after Resume"
    );
    assert_eq!(a.last_active, quiet);
}

#[test]
fn paused_quiet_fallback_preserves_existing_failure_diagnostics() {
    let (mut h, who) = host();
    h.app
        .set_presence_paused(&who, true, controls::PauseCause::Human)
        .unwrap();
    let quiet = Instant::now() - std::time::Duration::from_secs(9);
    for diagnostic in ["error", "offline", "blocked"] {
        let a = &mut h.app.presence.agents[0];
        a.last_active = quiet;
        a.activity = diagnostic.into();
        a.status = "Keep this real diagnostic".into();
        h.frame();
        let a = &h.app.presence.agents[0];
        assert!(a.paused);
        assert_eq!(a.activity, diagnostic);
        assert_eq!(a.status, "Keep this real diagnostic");
        assert_eq!(a.last_active, quiet);
    }
}
