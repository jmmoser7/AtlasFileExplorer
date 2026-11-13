use super::*;
use crate::app::tests::Harness;

const ARCHIVE: &[u8] = br#"<Archive name="Root"><chunks><chunk name="Definition"><chunks><chunk name="DefinitionObjects" /></chunks></chunk></chunks></Archive>"#;

#[test]
fn returns_only_the_staged_grasshopper_archive_and_preserves_source() {
    let h = Harness::new("grasshopper-authoring-staging");
    let original = h.base.join("source.gh");
    std::fs::write(&original, b"binary Grasshopper input").unwrap();
    let draft = h.base.join("staging").join("run");
    let copy = stage(&original, &draft).unwrap();
    assert_eq!(copy.file_name().unwrap(), "source.gh");
    assert_eq!(
        std::fs::read(&copy).unwrap(),
        std::fs::read(&original).unwrap()
    );
    let publish = h.base.join("assets").join(REVISION);
    assert!(validate_and_publish(&draft, &publish).is_err());
    std::fs::write(draft.join(REVISION), b"<Archive></Archive>").unwrap();
    assert!(validate_and_publish(&draft, &publish).is_err());
    assert!(!publish.exists());
    std::fs::write(draft.join(REVISION), ARCHIVE).unwrap();
    let (saved, size, _) = validate_and_publish(&draft, &publish).unwrap();
    assert_eq!(saved, publish);
    assert_eq!(size, ARCHIVE.len() as u64);
    assert_eq!(std::fs::read(&publish).unwrap(), ARCHIVE);
    assert_eq!(
        std::fs::read(original).unwrap(),
        b"binary Grasshopper input"
    );
}

#[test]
fn revisions_stay_inert_until_the_person_confirms() {
    let mut h = Harness::new("grasshopper-authoring-selection");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let source = h.base.join("source.ghx");
    let revised = h.base.join(REVISION);
    std::fs::write(&source, ARCHIVE).unwrap();
    std::fs::write(&revised, ARCHIVE).unwrap();
    let item = h.app.item_for_ready_path(&source, ARCHIVE.len() as u64, 0);
    let mut image = slate_doc::scene::ImageNode::new(item);
    image.agent = PortalNode::unbound_agent("model", "codex").agent;
    image
        .agent
        .as_mut()
        .unwrap()
        .generator
        .model_outputs
        .push(ModelOutputRef {
            id: "revision".into(),
            source: slate_doc::SourceUri {
                locator: revised.to_string_lossy().into_owned(),
            },
            size: ARCHIVE.len() as u64,
            mtime: atlas_core::metadata::mtime_of(&std::fs::metadata(&revised).unwrap()),
        });
    let target = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 320.0, 220.0),
        NodeKind::Image(image),
    );
    let node = target.id;
    h.app.add_nodes(vec![target]);
    assert!(h
        .app
        .prepare_registered_model_output(node, "revision")
        .is_err());
    assert!(h
        .app
        .select_model_generator_output(node, "revision")
        .is_err());
    let detail = serde_json::json!({"node":node,"output":"revision"}).to_string();
    assert!(h.app.select_model_output(Some(&detail)));
    assert_eq!(h.app.image_item(node), Some(item));
    assert_eq!(
        h.app.agents.grasshopper_confirm,
        Some((node, "revision".into()))
    );
    std::fs::remove_file(&revised).unwrap();
    assert!(h
        .app
        .select_approved_grasshopper_output(node, "revision")
        .is_err());
    assert_eq!(h.app.image_item(node), Some(item));
    std::fs::write(&revised, ARCHIVE).unwrap();
    let metadata = std::fs::metadata(&revised).unwrap();
    h.app.patch_nodes(&[node], |target| {
        let agent = slate_doc::agent_chat::agent_mut(target).unwrap();
        agent.generator.model_outputs[0].mtime = atlas_core::metadata::mtime_of(&metadata);
    });
    h.app
        .select_approved_grasshopper_output(node, "revision")
        .unwrap();
    assert_eq!(
        h.app.image_item(node).and_then(|id| h.app.item_path(id)),
        Some(revised)
    );
    assert_eq!(std::fs::read(source).unwrap(), ARCHIVE);
}
