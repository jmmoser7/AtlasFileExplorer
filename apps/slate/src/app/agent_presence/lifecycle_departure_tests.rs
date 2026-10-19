use super::*;

#[test]
fn completed_helpers_vary_their_farewell_without_restarting_a_departure() {
    let mut h = host();
    let parent = profile("Dot");
    let parent_id = parent.id.clone();
    h.app.join_presence(parent, None).unwrap();
    let mut previous = None;
    let mut styles = std::collections::HashSet::new();
    for _ in 0..6 {
        let helper = profile("Helper");
        let who = helper.id.clone();
        h.app
            .join_presence(helper, Some(parent_id.clone()))
            .unwrap();
        h.app.retire_completed_helper(&who).unwrap();
        let agent = h
            .app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap();
        let departure = agent.departure.clone().unwrap();
        assert!(agent.grant.is_none());
        assert_ne!(departure.1, chrome::RANDOM_DEPARTURE);
        assert_ne!(departure.1, "portal", "there is no visible portal");
        assert_ne!(previous.as_ref(), Some(&departure.1));
        styles.insert(departure.1.clone());
        previous = Some(departure.1.clone());

        h.app
            .retire_presence(&who, chrome::RANDOM_DEPARTURE)
            .unwrap();
        let agent = h
            .app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap();
        assert_eq!(agent.departure.as_ref().unwrap(), &departure);
    }
    assert_eq!(styles.len(), 3, "all three non-portal farewells get a turn");
    assert!(h
        .app
        .presence
        .agents
        .iter()
        .any(|a| a.profile.id == parent_id && a.departure.is_none()));
}

#[test]
fn explicit_farewell_is_preserved_for_parent_and_helpers() {
    let mut h = host();
    let parent = profile("Dot");
    let parent_id = parent.id.clone();
    h.app.join_presence(parent, None).unwrap();
    h.app
        .join_presence(profile("Helper"), Some(parent_id.clone()))
        .unwrap();
    h.app.retire_presence(&parent_id, "edge").unwrap();
    assert_eq!(h.app.presence.agents.len(), 2);
    for agent in &h.app.presence.agents {
        assert_eq!(agent.departure.as_ref().unwrap().1, "edge");
        assert!(agent.grant.is_none());
    }
}
