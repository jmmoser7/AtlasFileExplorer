use atlas_connector::*;
fn metadata() -> CaptureMetadata {
    let source = SourceScope {
        source_id: "model-1".into(),
        revision: "r1".into(),
    };
    CaptureMetadata {
        source: source.clone(),
        view: ViewSnapshot {
            source,
            schema: "org.slate.rhino.view".into(),
            schema_version: 1,
            fidelity: ViewFidelity::ExactNative,
            state: serde_json::json!({"camera":"<&\"'", "projection":"parallel"}),
        },
        captured_at: "2026-10-25T22:00:00Z".into(),
        width_px: 1920,
        height_px: 1080,
        consistency: CaptureConsistency::Atomic,
    }
}
#[test]
fn native_projection_and_source_roundtrip_without_xml_execution() {
    let meta = metadata();
    let xmp = capture_xmp(&meta).unwrap();
    assert_eq!(parse_capture_xmp(&xmp).unwrap(), Some(meta));
    assert!(!xmp.contains("<&"));
    assert_eq!(parse_capture_xmp("<x:xmpmeta />").unwrap(), None);
}
#[test]
fn mismatched_source_and_malformed_packets_are_rejected() {
    let mut meta = metadata();
    meta.view.source.revision = "other".into();
    assert!(capture_xmp(&meta).is_err());
    let xmp = capture_xmp(&metadata()).unwrap();
    assert!(parse_capture_xmp(&format!("{xmp}{xmp}")).is_err());
    assert!(parse_capture_xmp(&xmp.replace("</slatecc:metadata>", "")).is_err());
}

#[test]
fn saved_camera_rebinds_only_to_independently_verified_identical_content() {
    let mut meta = metadata();
    let digest = "a".repeat(64);
    annotate_capture_content(&mut meta, &digest, 1024).unwrap();
    let original = meta.clone();
    let current = SourceScope {
        source_id: "moved-workbook-model".into(),
        revision: "fresh-native-session".into(),
    };
    assert!(view_for_restore(&meta, &current, None).is_err());
    assert!(view_for_restore(&meta, &current, Some((&"b".repeat(64), 1024))).is_err());
    assert!(view_for_restore(&meta, &current, Some((&digest, 1025))).is_err());
    let restored = view_for_restore(&meta, &current, Some((&digest, 1024))).unwrap();
    assert_eq!(restored.source, current);
    assert_eq!(restored.state["camera"], meta.view.state["camera"]);
    assert_eq!(
        restored.state["slate_restore_origin"],
        serde_json::to_value(&meta.source).unwrap()
    );
    assert_eq!(
        meta, original,
        "restoration must not rewrite the captured provenance"
    );
    assert_eq!(
        view_for_restore(&meta, &meta.source, None).unwrap(),
        meta.view
    );
}

#[test]
fn invalid_content_and_posters_do_not_create_restoration_authority() {
    let mut meta = metadata();
    assert!(annotate_capture_content(&mut meta, "not-sha256", 42).is_err());
    assert!(annotate_capture_content(&mut meta, &"a".repeat(64), 0).is_err());
    meta.view.fidelity = ViewFidelity::PosterOnly;
    assert!(view_for_restore(&meta, &meta.source, None).is_err());
}
