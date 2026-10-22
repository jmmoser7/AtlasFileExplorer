use super::*;

#[test]
fn saved_conversation_send_joins_freshly_after_cleanup_and_preserves_history() {
    let mut h = host();
    let profile = StoredAgent {
        id: atlas_agent::request_id(),
        name: "Dot".into(),
        hat: "none".into(),
        provider: "local".into(),
        retired: false,
    };
    let who = profile.id.clone();
    h.app.settings.collaborators.push(profile.clone());
    h.app.join_presence(profile.clone(), None).unwrap();
    let (old_session, old_grant) = {
        let agent = &h.app.presence.agents[0];
        (
            agent.binding.session_id.clone(),
            agent.binding.grant_id.clone(),
        )
    };
    {
        let thread = h.app.presence.conversations.edit(&who).unwrap();
        thread.push(
            "assistant",
            "Earlier answer",
            "Dot",
            Some("local"),
            "",
            "complete",
        );
        thread.set_draft("Keep this draft until I send");
        thread.set_draft_images(vec![PathBuf::from("draft reference.png")]);
    }

    // A normal close cleans the old runtime; the conversation store and saved
    // non-retired identity stand in for their durable copies after restart.
    h.app.stop_presence(&who, true).unwrap();
    assert!(h.app.presence.agents[0].grant.is_none());
    assert!(!h.app.presence.agents[0].inflight);
    h.app.presence.agents.clear();
    h.app.presence.roster = PresenceRoster::default();
    assert_eq!(
        h.app.presence_conversation(&who).unwrap().draft,
        "Keep this draft until I send"
    );
    assert_eq!(h.app.presence_conversation(&who).unwrap().turns.len(), 1);
    assert_eq!(
        h.app.presence_conversation(&who).unwrap().draft_images,
        [PathBuf::from("draft reference.png")]
    );

    h.app
        .presence_send_chat(&who, "Continue the conversation")
        .unwrap();

    let agent = h
        .app
        .presence
        .agents
        .iter()
        .find(|agent| agent.profile.id == who)
        .unwrap();
    assert_ne!(agent.binding.session_id, old_session);
    assert_ne!(agent.binding.grant_id, old_grant);
    assert!(
        agent.grant.is_some(),
        "fresh join must issue a new board grant"
    );
    let thread = h.app.presence_conversation(&who).unwrap();
    assert_eq!(thread.turns[0].text, "Earlier answer");
    assert!(thread
        .turns
        .iter()
        .any(|turn| turn.role == "user" && turn.text == "Continue the conversation"));
    assert!(
        thread.draft.is_empty(),
        "accepted Send consumes the saved draft"
    );
    let sent = thread
        .turns
        .iter()
        .find(|turn| turn.role == "user")
        .unwrap();
    assert_eq!(
        thread.message_images.get(&sent.request).unwrap(),
        &[PathBuf::from("draft reference.png")]
    );
    assert!(thread.draft_images.is_empty());
}

#[test]
fn sending_to_an_explicitly_retired_profile_never_rejoins_it() {
    let mut h = host();
    let profile = StoredAgent {
        id: atlas_agent::request_id(),
        name: "Retired Dot".into(),
        hat: "none".into(),
        provider: "local".into(),
        retired: true,
    };
    let who = profile.id.clone();
    h.app.settings.collaborators.push(profile);
    h.app.presence_save_draft(&who, "Still here").unwrap();

    let error = h.app.presence_send_chat(&who, "Wake up").unwrap_err();
    assert!(error.contains("retired"), "unexpected refusal: {error}");
    assert!(h.app.presence.agents.is_empty());
    assert_eq!(h.app.presence.roster.profiles().count(), 0);
    assert_eq!(
        h.app.presence_conversation(&who).unwrap().draft,
        "Still here"
    );
}

#[test]
fn retirement_keeps_consent_but_revokes_live_authority_until_a_fresh_join() {
    let mut h = host();
    let access_path = h.base.join("provider-access.json");
    h.app.set_agent_access_store_for_tests(access_path.clone());
    let profile = StoredAgent {
        id: atlas_agent::request_id(),
        name: "Consented collaborator".into(),
        hat: "none".into(),
        provider: "codex".into(),
        retired: false,
    };
    let who = profile.id.clone();
    h.app.join_presence(profile.clone(), None).unwrap();
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::SetAutonomy {
                id: who.clone(),
                enabled: true,
            },
        )
        .unwrap();
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::SetFullAccess {
                id: who.clone(),
                enabled: true,
            },
        )
        .unwrap();
    let saved_policy = h.app.settings.agent_consent_policies[0].clone();
    let renamed = "Consented collaborator renamed";
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::Rename {
                id: who.clone(),
                name: renamed.into(),
            },
        )
        .unwrap();
    let mut expected_policy = saved_policy;
    expected_policy.identity_name = renamed.into();
    assert_eq!(h.app.settings.agent_consent_policies[0], expected_policy);
    let mut profile = profile;
    profile.name = renamed.into();
    let old_session = h.app.presence.agents[0].binding.session_id.clone();
    let old_grant = h.app.presence.agents[0].binding.grant_id.clone();
    assert!(atlas_ai::access::granted_in(&access_path, &old_session));

    h.app.retire_presence(&who, "bow").unwrap();
    assert!(h.app.presence.agents[0].grant.is_none());
    assert!(!atlas_ai::access::granted_in(&access_path, &old_session));
    assert_eq!(h.app.settings.agent_consent_policies.len(), 1);
    assert!(
        h.app.settings.agent_consent_policies[0]
            .capabilities
            .mutation_autonomy
    );
    assert!(
        h.app.settings.agent_consent_policies[0]
            .capabilities
            .provider_full_access
    );

    h.app.join_presence(profile, None).unwrap();
    let rebound = &h.app.presence.agents[0];
    assert_ne!(rebound.binding.session_id, old_session);
    assert_ne!(rebound.binding.grant_id, old_grant);
    assert!(rebound.grant.as_ref().unwrap().mutation_autonomy);
    assert!(h.app.presence_full_access_for(&who));
    let rebound_session = rebound.binding.session_id.clone();
    assert!(atlas_ai::access::granted_in(&access_path, &rebound_session));

    h.app.stop_presence(&who, true).unwrap();
    assert!(h.app.settings.agent_consent_policies.is_empty());
    assert!(!atlas_ai::access::granted_in(
        &access_path,
        &rebound_session
    ));
    let _ = std::fs::remove_file(access_path);
}
