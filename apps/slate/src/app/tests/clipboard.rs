//! Copying pictures to the clipboard and pasting them back.

use super::*;

/// Copying one picture puts a real bitmap on the clipboard at the picture's
/// own resolution, plus the linked file and Slate's own format. The text
/// slot is empty, never the node JSON.
#[test]
fn copying_a_picture_puts_its_bitmap_on_the_clipboard() {
    let mut h = web_board("copy_picture_bitmap");
    let (id, path) = add_two_tone_picture(
        &mut h,
        "two.png",
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
    );
    copy_selection(&mut h, &[id]);
    let bitmap = copied_bitmap(&h);
    assert_eq!(bitmap.dimensions(), (40, 20));
    assert_eq!(bitmap.get_pixel(5, 10).0, RED);
    assert_eq!(bitmap.get_pixel(35, 10).0, BLUE);

    let write = h.app.os_clipboard.last_write().unwrap();
    assert_eq!(write.text, None, "no JSON in the text slot");
    assert_eq!(write.files, vec![path]);
    let nodes: Vec<slate_doc::scene::Node> = serde_json::from_str(&write.nodes_json).unwrap();
    assert_eq!(nodes.len(), 1);
    assert!(matches!(nodes[0].kind, slate_doc::NodeKind::Image(_)));
}

/// A picture stored as a workbook-relative `assets/pasted/…` locator still
/// copies as a bitmap and names the resolved file on disk.
#[test]
fn copying_a_pasted_assets_picture_puts_its_bitmap_on_the_clipboard() {
    use slate_doc::scene::{ImageNode, WorldRect};
    let mut h = web_board("copy_pasted_assets_picture");
    let book = h.base.join("Book.slate");
    h.app.tab_mut().path = Some(book.clone());
    let pasted =
        atlas_core::workbook_assets::paste_dir(Some(&book), &atlas_core::index::data_dir())
            .join("copy-paste.png");
    if let Some(dir) = pasted.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    image::RgbaImage::from_fn(40, 20, |x, _| image::Rgba(if x < 20 { RED } else { BLUE }))
        .save(&pasted)
        .unwrap();
    let item = h.app.item_for_path(&pasted).unwrap();
    assert!(
        h.app
            .doc()
            .item(item)
            .unwrap()
            .path
            .to_string_lossy()
            .replace('\\', "/")
            .starts_with("assets/pasted/"),
        "stored locator stays relative"
    );
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::NodeKind::Image(ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    copy_selection(&mut h, &[id]);
    let bitmap = copied_bitmap(&h);
    assert_eq!(bitmap.dimensions(), (40, 20));
    assert_eq!(bitmap.get_pixel(5, 10).0, RED);
    let write = h.app.os_clipboard.last_write().unwrap();
    assert_eq!(write.files, vec![pasted]);
}

/// The bitmap is the picture as displayed: rotation, crop, mirror, filters,
/// and paint layers all land in the pixels.
#[test]
fn a_copied_bitmap_carries_rotation_crop_mirror_filters_and_ink() {
    use slate_doc::scene::{ShapeKind, ShapeNode, WorldRect};
    let mut h = web_board("copy_picture_as_shown");
    let (id, _) = add_two_tone_picture(&mut h, "two.png", WorldRect::new(0.0, 0.0, 200.0, 100.0));

    h.app.patch_nodes(&[id], |n| n.rotation_deg = 90.0);
    copy_selection(&mut h, &[id]);
    let turned = copied_bitmap(&h);
    assert_eq!(turned.dimensions(), (20, 40));
    // A clockwise quarter turn puts the red left half on top.
    assert_eq!(turned.get_pixel(10, 5).0, RED);
    assert_eq!(turned.get_pixel(10, 35).0, BLUE);

    h.app.patch_nodes(&[id], |n| {
        n.rotation_deg = 0.0;
        // The crop tool keeps the window's aspect: half the width, half the box.
        n.rect.w = 100.0;
        if let slate_doc::NodeKind::Image(img) = &mut n.kind {
            img.crop = slate_doc::scene::Crop {
                x: 0.5,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            };
        }
    });
    copy_selection(&mut h, &[id]);
    let cropped = copied_bitmap(&h);
    assert_eq!(cropped.dimensions(), (20, 20));
    assert!(cropped.pixels().all(|p| p.0 == BLUE));

    h.app.patch_nodes(&[id], |n| {
        n.rect.w = 200.0;
        if let slate_doc::NodeKind::Image(img) = &mut n.kind {
            img.crop = slate_doc::scene::Crop::full();
            img.flip_x = true;
            img.adjust.invert = 1.0;
        }
    });
    copy_selection(&mut h, &[id]);
    let flipped = copied_bitmap(&h);
    // Mirrored, the blue half is on the left; inverted, blue reads yellow.
    assert_eq!(flipped.get_pixel(5, 10).0, [255, 255, 0, 255]);
    assert_eq!(flipped.get_pixel(35, 10).0, [0, 255, 255, 255]);

    let mut scene = slate_doc::scene::Scene::default();
    let ink = scene.build_node(
        WorldRect::new(0.0, 0.0, 0.25, 1.0),
        slate_doc::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(slate_doc::scene::Rgba::opaque(0, 255, 0)),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    h.app.patch_nodes(&[id], |n| {
        if let slate_doc::NodeKind::Image(img) = &mut n.kind {
            img.flip_x = false;
            img.adjust.invert = 0.0;
            let mut layer = slate_doc::PaintLayer::new(slate_doc::PaintLayerId(1));
            layer.nodes.push(ink.clone());
            img.paint_layers.push(layer);
        }
    });
    copy_selection(&mut h, &[id]);
    let inked = copied_bitmap(&h);
    assert_eq!(
        inked.get_pixel(3, 10).0,
        [0, 255, 0, 255],
        "the paint layer"
    );
    assert_eq!(inked.get_pixel(15, 10).0, RED);
}

/// Several pictures copy as one bitmap laid out as they sit on the board.
#[test]
fn copying_several_pictures_composes_one_bitmap() {
    use slate_doc::scene::WorldRect;
    let mut h = web_board("copy_two_pictures");
    let (a, _) = add_two_tone_picture(&mut h, "a.png", WorldRect::new(0.0, 0.0, 200.0, 100.0));
    let (b, _) = add_two_tone_picture(&mut h, "b.png", WorldRect::new(300.0, 0.0, 200.0, 100.0));
    copy_selection(&mut h, &[a, b]);
    let bitmap = copied_bitmap(&h);
    assert_eq!(bitmap.dimensions(), (100, 20));
    assert_eq!(bitmap.get_pixel(5, 10).0, RED);
    assert_eq!(bitmap.get_pixel(50, 10).0[3], 0, "the gap is transparent");
    assert_eq!(bitmap.get_pixel(95, 10).0, BLUE);
    assert_eq!(h.app.os_clipboard.last_write().unwrap().files.len(), 2);
}

/// A mixed selection keeps Slate's own format and offers plain text, not
/// JSON. Pasting inside Slate still lands the nodes, not the fallback.
#[test]
fn a_mixed_copy_offers_plain_text_and_still_pastes_nodes() {
    use slate_doc::scene::WorldRect;
    let mut h = web_board("copy_mixed");
    let (pic, _) = add_two_tone_picture(&mut h, "two.png", WorldRect::new(0.0, 0.0, 200.0, 100.0));
    let words = h.app.doc_mut().scene.build_node(
        WorldRect::new(300.0, 0.0, 200.0, 50.0),
        slate_doc::NodeKind::Text(slate_doc::scene::TextNode {
            text: "Hello board".into(),
            family: Default::default(),
            size: 24.0,
            color: slate_doc::scene::Rgba::BLACK,
            align: Default::default(),
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    let words = h.app.add_nodes(vec![words])[0];
    copy_selection(&mut h, &[pic, words]);
    let write = h.app.os_clipboard.last_write().unwrap();
    assert_eq!(write.text.as_deref(), Some("Hello board"));
    assert!(write.png.is_none() && write.dibv5.is_none());
    let json = write.nodes_json.clone();
    assert_ne!(write.text.as_deref(), Some(json.as_str()));

    let before = h.app.doc().scene.nodes.len();
    h.app.pending_paste_text = Some("Hello board".into());
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.paste"), None));
    assert_eq!(h.app.doc().scene.nodes.len(), before + 2);
    let texts = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| matches!(&n.kind, slate_doc::NodeKind::Text(t) if t.text == "Hello board"))
        .count();
    assert_eq!(texts, 2, "the text node itself, not a pasted string");
}

/// Copy then paste a picture inside Slate: the node comes back with its
/// crop and mirror, not as a flattened new bitmap.
#[test]
fn a_copied_picture_pastes_back_as_the_same_node() {
    use slate_doc::scene::WorldRect;
    let mut h = web_board("copy_paste_picture");
    let (id, _) = add_two_tone_picture(&mut h, "two.png", WorldRect::new(0.0, 0.0, 200.0, 100.0));
    h.app.patch_nodes(&[id], |n| {
        if let slate_doc::NodeKind::Image(img) = &mut n.kind {
            img.crop.w = 0.5;
            img.flip_y = true;
        }
    });
    let original = match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::NodeKind::Image(img) => img.clone(),
        _ => unreachable!(),
    };
    copy_selection(&mut h, &[id]);
    let items = h.app.doc().items.len();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.paste"), None));
    assert_eq!(h.app.doc().items.len(), items, "no new pasted file");
    let pasted = *h.app.board_sel.iter().next().unwrap();
    assert_ne!(pasted, id);
    match &h.app.doc().scene.node(pasted).unwrap().kind {
        slate_doc::NodeKind::Image(img) => assert_eq!(img, &original),
        _ => panic!("image"),
    }
}
