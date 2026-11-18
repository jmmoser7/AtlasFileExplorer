use super::*;
use crate::app::tests::Harness;

const ARCHIVE: &[u8] = br#"<Archive name="Root"><chunks><chunk name="Definition"><chunks><chunk name="DefinitionObjects" /></chunks></chunk></chunks></Archive>"#;

#[test]
fn returns_only_the_staged_grasshopper_archive_and_preserves_source() {
    let h = Harness::new("grasshopper-authoring-staging");
    let original = h.base.join("source.gh");
    std::fs::write(&original, b"binary Grasshopper input").unwrap();
    let draft = h.base.join("staging").join("run");
    let (copy, empty_source) = stage(&original, &draft).unwrap();
    assert!(!empty_source);
    assert_eq!(copy.file_name().unwrap(), "source.gh");
    assert_eq!(
        std::fs::read(&copy).unwrap(),
        std::fs::read(&original).unwrap()
    );
    let publish = h.base.join("assets").join(REVISION);
    assert!(validate_and_publish(&draft, &publish, empty_source).is_err());
    std::fs::write(draft.join(REVISION), b"<Archive></Archive>").unwrap();
    assert!(validate_and_publish(&draft, &publish, empty_source).is_err());
    assert!(!publish.exists());
    std::fs::write(draft.join(REVISION), ARCHIVE).unwrap();
    let (saved, size, _) = validate_and_publish(&draft, &publish, empty_source).unwrap();
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

#[test]
fn a_text_recipe_only_applies_to_a_verified_blank_source_and_publishes_one_inert_archive() {
    let h = Harness::new("grasshopper-blank-recipe");
    let fixture = h.base.join("blank.ghx");
    std::fs::write(&fixture,br#"<Archive><chunks><chunk name="Definition"><chunks><chunk name="DefinitionObjects"><items><item name="ObjectCount">0</item></items></chunk></chunks></chunk></chunks></Archive>"#).unwrap();
    let draft = h.base.join("staged-blank");
    let (copied, blank) = stage(&fixture, &draft).unwrap();
    assert!(blank);
    assert!(
        authoring_instructions(&copied, &draft.join(REVISION), blank).contains("parameters.json")
    );
    std::fs::write(
        draft.join("parameters.json"),
        br#"{"version":1,"title":"Example","inputs":[]}"#,
    )
    .unwrap();
    std::fs::write(draft.join("geometry.cs"), "model = Point3d.Origin;").unwrap();
    let out = h.base.join("revision").join(REVISION);
    assert!(validate_and_publish(&draft, &out, false).is_err());
    assert!(!out.exists());
    validate_and_publish(&draft, &out, blank).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert!(validate_archive(&bytes).is_ok());
    assert!(!recipe::empty_xml(&bytes));
    assert_eq!(
        std::fs::read(&fixture).unwrap(),
        std::fs::read(copied).unwrap()
    );
    // A present invalid archive is an explicit error, even with a valid basis.
    std::fs::write(draft.join(REVISION), b"not xml").unwrap();
    assert!(validate_and_publish(&draft, &out, blank).is_err());
    assert_eq!(std::fs::read(&out).unwrap(), bytes);
}
