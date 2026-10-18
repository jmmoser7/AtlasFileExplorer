use super::*;
use atlas_agent::TurnPolicy;

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
