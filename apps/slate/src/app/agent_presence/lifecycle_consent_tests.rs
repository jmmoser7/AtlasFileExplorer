use super::*;

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

    let mut restored_profile = profile;
    restored_profile.retired = false;
    h.app.join_presence(restored_profile, None).unwrap();
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
