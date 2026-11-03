use super::*;
use atlas_agent::TurnPolicy;

#[test]
fn custom_image_capability_and_direct_request_validation_agree() {
    let mut request = AgentRequest {
        image: Some(atlas_agent::ImageParams {
            aspect: atlas_agent::Aspect::Custom {
                width: 16,
                height: 9,
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    for provider in ["codex-image", "openai-image", "unknown"] {
        assert!(!custom_image_aspect_supported(provider, false));
        assert!(validate_image_parameters(provider, &request).is_err());
    }
    assert!(custom_image_aspect_supported("comfy", false));
    assert!(validate_image_parameters("comfy", &request).is_ok());
    request.image.as_mut().unwrap().aspect = atlas_agent::Aspect::Custom {
        width: 0,
        height: 9,
    };
    assert!(validate_image_parameters("comfy", &request).is_err());
    request.image.as_mut().unwrap().aspect = atlas_agent::Aspect::Custom {
        width: 16,
        height: 9,
    };
    request.inputs.wired.push(atlas_agent::ContextItem {
        node: 1,
        text: String::new(),
        images: vec!["source.png".into()],
        depth: None,
        outputs: Default::default(),
        active: None,
        slot: Some(atlas_agent::InputSlot::Media),
    });
    assert!(!custom_image_aspect_supported("comfy", true));
    assert!(validate_image_parameters("comfy", &request).is_err());
    request.image.as_mut().unwrap().aspect = atlas_agent::Aspect::Wide;
    request.image.as_mut().unwrap().seed = Some(42);
    assert!(image_seed_supported("comfy"));
    assert!(!image_seed_supported("openai-image"));
    assert!(validate_image_parameters("openai-image", &request).is_err());
    assert!(
        validate_image_parameters("comfy", &request).is_ok(),
        "legacy source-preserving preset contract remains unchanged"
    );
}

#[test]
fn a_saved_workbook_receives_generated_images_beside_it() {
    let workbook = Path::new("/boards/deck/Board.slate");
    let link = Path::new("/links/session-a");
    let (dir, _) = image_output_dir(Some(workbook), "openai-image", link);
    let text = dir.to_string_lossy().replace('\\', "/");
    assert!(
        text.contains("/boards/deck/assets/generated/openai-image/"),
        "{text}"
    );
    let (unsaved, tag) = image_output_dir(None, "openai-image", link);
    assert_eq!(tag, "session-a");
    assert!(unsaved.ends_with(Path::new("openai-image").join("session-a")));
}

#[test]
fn relayed_turns_need_the_relay_grant_and_read_only_never_gets_full_access() {
    let own = AgentRequest::default();
    let relayed = AgentRequest {
        relayed_from: Some("Codex · reviewer".into()),
        ..Default::default()
    };
    let read_only = AgentRequest {
        policy: TurnPolicy::ReadOnly,
        ..Default::default()
    };
    assert!(turn_full_access(true, false, &own));
    assert!(!turn_full_access(false, true, &own));
    assert!(!turn_full_access(true, false, &relayed));
    assert!(turn_full_access(true, true, &relayed));
    assert!(!turn_full_access(false, true, &relayed));
    for (granted, relay) in [(true, true), (true, false), (false, false)] {
        assert!(!turn_full_access(granted, relay, &read_only));
        let relayed_read_only = AgentRequest {
            relayed_from: Some("Cursor · builder".into()),
            ..read_only.clone()
        };
        assert!(!turn_full_access(granted, relay, &relayed_read_only));
    }
}

#[test]
fn admitted_snapshot_retains_codex_policy_and_never_overrides_turn_restrictions() {
    let enabled: crate::access::Snapshot = serde_json::from_value(serde_json::json!({
        "global_default_full": true, "overrides": {"denied": false}, "relay": ["relayed"]
    }))
    .unwrap();
    let disabled: crate::access::Snapshot = serde_json::from_value(serde_json::json!({
        "global_default_full": false
    }))
    .unwrap();
    let mut request = AgentRequest::default();
    assert!(request_full_access(Some("new"), &request, Some(&enabled)));
    assert!(!request_full_access(Some("new"), &request, Some(&disabled)));
    assert!(!request_full_access(
        Some("denied"),
        &request,
        Some(&enabled)
    ));
    request.relayed_from = Some("reviewer".into());
    assert!(!request_full_access(Some("new"), &request, Some(&enabled)));
    assert!(request_full_access(
        Some("relayed"),
        &request,
        Some(&enabled)
    ));
    request.policy = TurnPolicy::ReadOnly;
    assert!(!request_full_access(
        Some("relayed"),
        &request,
        Some(&enabled)
    ));
}
