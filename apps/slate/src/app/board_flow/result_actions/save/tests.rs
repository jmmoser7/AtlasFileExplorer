use super::*;

#[test]
fn batch_save_preserves_pixels_and_refuses_source_or_existing_file_overwrite() {
    let (mut h, host) = super::super::tests::fixture("result-batch-save");
    let address = ResultAddress {
        tab: h.app.tab().id,
        host,
        binding: h.app.result_binding(host).unwrap(),
        outputs: h
            .app
            .agent_images(host)
            .iter()
            .skip(1)
            .take(2)
            .map(|image| image.id.clone())
            .collect(),
    };
    let (doc, nodes) = h.app.result_payload(&address);
    let snapshot = SaveSnapshot {
        address,
        doc,
        nodes,
        workbook: None,
        extensions: vec!["png".into(); 2],
    };
    let destinations = [
        h.base.join("render scene_001.png"),
        h.base.join("render scene_002.png"),
    ];
    assert_eq!(worker::save(&snapshot, &destinations, true).unwrap(), 2);
    for (index, destination) in destinations.iter().enumerate() {
        let actual = image::open(destination).unwrap().to_rgba8();
        assert_eq!(actual.dimensions(), (20, 10));
        assert_eq!(actual.get_pixel(0, 0)[3], 192);
        assert_eq!(actual.get_pixel(0, 0)[0], (index as u8 + 1) * 40);
    }
    assert!(worker::save(&snapshot, &destinations, true)
        .unwrap_err()
        .contains("already exists"));
    let NodeKind::Image(image) = &snapshot.nodes[0].kind else {
        unreachable!()
    };
    let original = crate::app::image_composite::item_file(&snapshot.doc, None, image.item).unwrap();
    assert!(
        worker::save(&snapshot, &[original, h.base.join("other.png")], false)
            .unwrap_err()
            .contains("source images")
    );
}
