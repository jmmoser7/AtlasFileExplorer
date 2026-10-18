use crate::*;
#[test]
fn manifest_reads_items_and_the_legacy_image_forms() {
    let m: ReturnManifest = serde_json::from_str(
        r#"{"id":"q3","title":"Q3","items":[{"path":"dash.html","as":"graphic"},{"path":"data.csv","feeds":"dash.html"},{"path":"shots","as":"images"}]}"#,
    )
    .unwrap();
    assert_eq!(m.items.len(), 3);
    assert_eq!(m.items[0].face, Face::Graphic);
    assert_eq!(m.items[1].face, Face::Auto);
    assert_eq!(m.items[1].feeds.as_deref(), Some("dash.html"));
    assert_eq!(m.items[2].face, Face::Images);

    let legacy: ReturnManifest = serde_json::from_str(
        r#"{"id":"penn","kind":"images","title":"Penn","path":"penn-images"}"#,
    )
    .unwrap();
    assert_eq!(legacy.title, "Penn");
    assert_eq!(
        legacy.items,
        vec![ReturnItem {
            path: "penn-images".into(),
            face: Face::Images,
            feeds: None
        }]
    );
    let list: ReturnManifest =
        serde_json::from_str(r#"{"id":"p","kind":"images","paths":["a.png","b.png"]}"#).unwrap();
    assert_eq!(list.items.len(), 2);
    assert!(list.items.iter().all(|i| i.face == Face::Images));

    let round: ReturnManifest = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
    assert_eq!(round, m);
    assert!(serde_json::to_string(&m)
        .unwrap()
        .contains("\"as\":\"graphic\""));
}

#[test]
fn manifest_reads_a_goal_verdict() {
    let m: ReturnManifest =
        serde_json::from_str(r#"{"id":"g","goal":{"status":"not_met","reason":"tests fail"}}"#)
            .unwrap();
    assert!(m.items.is_empty());
    assert_eq!(
        m.goal,
        Some(GoalVerdict {
            status: GoalStatus::NotMet,
            reason: "tests fail".into()
        })
    );
    let met: ReturnManifest =
        serde_json::from_str(r#"{"items":[],"goal":{"status":"met"}}"#).unwrap();
    assert_eq!(met.goal.unwrap().status, GoalStatus::Met);
    assert!(serde_json::from_str::<ReturnManifest>(r#"{"goal":{"status":"done"}}"#).is_err());
    let set = DeliverableSet {
        id: "s".into(),
        turn: 1,
        title: String::new(),
        items: vec![],
        missing: vec![],
        goal: None,
    };
    assert!(!serde_json::to_string(&set).unwrap().contains("goal"));
}

#[test]
fn version_records_omit_absent_measurements() {
    let v = Version {
        turn: 1,
        path: "versions/t001/a.html".into(),
        bytes: 10,
        added: None,
        removed: None,
        skipped: None,
    };
    let text = serde_json::to_string(&v).unwrap();
    assert!(!text.contains("added") && !text.contains("skipped"));
    let d: Deliverables = serde_json::from_str(r#"{"version":1}"#).unwrap();
    assert!(d.sets.is_empty());
}
