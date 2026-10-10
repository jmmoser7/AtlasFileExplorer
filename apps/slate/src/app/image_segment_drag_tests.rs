//! Pointer scripts for the highlight's tag, the layer squircle, the image
//! menu, and dragging a highlight out as a sticker.
use super::tests::{fixture, inject, move_to};
use super::*;
use crate::app::board::BoardTool;
use crate::app::tests::Harness;

fn painted_text(out: &egui::FullOutput) -> Vec<String> {
    fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
            egui::Shape::Vec(list) => list.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut labels = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut labels);
    }
    labels
}

#[test]
fn the_image_menu_lists_layers() {
    let (mut h, id) = fixture("segment_layers_menu");
    let p = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .center();
    h.app.board_menu = Some((id, p));
    h.frame();
    let out = h.frame_output(|_| {});
    let labels = painted_text(&out);
    assert!(
        labels.iter().any(|label| label == "Layers"),
        "the image menu lists Layers: {labels:?}"
    );
}

fn button(h: &mut Harness, p: Pos2, pressed: bool) {
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(p));
        i.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        })
    });
}

#[test]
fn a_highlight_dragged_out_is_an_undoable_clipped_sticker() {
    let (mut h, id) = fixture("segment_sticker");
    let p = inject(&mut h, id);
    move_to(&mut h, p);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let count = h.app.doc().scene.nodes.len();
    let outside = h.app.board_xf().w2s(Pos2::new(360.0, 0.0));
    button(&mut h, p, true);
    for t in [0.25, 0.5, 0.75, 1.0] {
        move_to(&mut h, p + (outside - p) * t);
    }
    assert!(
        h.app.image_segments.drag_delta.is_some(),
        "the cut-out follows the pointer mid-drag"
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        count,
        "nothing commits mid-drag"
    );
    button(&mut h, outside, false);
    h.frame();

    assert_eq!(
        h.app.doc().scene.node(id).unwrap(),
        &before,
        "the photo neither moves nor gains a layer"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), count + 1);
    let sticker = h.app.doc().scene.nodes.last().unwrap().clone();
    let NodeKind::Image(img) = &sticker.kind else {
        panic!("a sticker is a picture")
    };
    let clip = sticker.clip.as_ref().expect("svg clip path");
    assert_eq!(clip.extra.len(), 1, "the hole is a second contour");
    assert_eq!(clip.fill_rule, slate_doc::scene::PathFillRule::EvenOdd);
    // Mask bbox 0.15..0.8 × 0.15..0.85 of a 400 × 320 photo.
    assert!((img.crop.x - 0.15).abs() < 1e-4 && (img.crop.w - 0.65).abs() < 1e-4);
    assert!((sticker.rect.w - 260.0).abs() < 0.1 && (sticker.rect.h - 224.0).abs() < 0.1);
    let delta = h.app.board_xf().s2w(outside) - h.app.board_xf().s2w(p);
    let home = (-200.0 + 0.475 * 400.0, -160.0 + 0.5 * 320.0);
    let center = sticker.rect.center();
    assert!(
        (center.0 - (home.0 + delta.x)).abs() < 0.5 && (center.1 - (home.1 + delta.y)).abs() < 0.5,
        "the sticker lands where the cut-out was dropped"
    );
    assert!(
        h.app.image_segments.result.is_none(),
        "the highlight is spent"
    );

    let photo = h.app.doc().item(img.item).unwrap().path.clone();
    let mut assets = slate_artifact::AssetMap::default();
    assets.insert(photo, "assets/photo.png".into());
    let html = slate_artifact::render_html(h.app.doc(), &assets);
    assert!(
        html.contains("clip-path:path(evenodd, 'M 0.0 0.0 L 260.0 0.0"),
        "the export clips with the same even-odd path at the sticker's size"
    );
    assert!(
        html.contains(&format!(
            "width:{:.4}%;height:{:.4}%;left:{:.4}%;top:{:.4}%;",
            100.0 / img.crop.w,
            100.0 / img.crop.h,
            -img.crop.x / img.crop.w * 100.0,
            -img.crop.y / img.crop.h * 100.0,
        )),
        "the export samples the same crop window"
    );

    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
}

#[test]
fn a_highlight_dragged_but_dropped_on_its_photo_places_nothing() {
    let (mut h, id) = fixture("segment_sticker_inside");
    let p = inject(&mut h, id);
    move_to(&mut h, p);
    let count = h.app.doc().scene.nodes.len();
    button(&mut h, p, true);
    move_to(&mut h, p + egui::vec2(30.0, 10.0));
    button(&mut h, p + egui::vec2(30.0, 10.0), false);
    h.frame();
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert!(
        h.app.image_segments.tag_at.is_none(),
        "a drag is not a click"
    );
}

fn label_center(out: &egui::FullOutput, label: &str) -> Option<Pos2> {
    fn walk(shape: &egui::Shape, label: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            egui::Shape::Vec(list) => list.iter().find_map(|s| walk(s, label)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|c| walk(&c.shape, label))
}

fn click(h: &mut Harness, p: Pos2) {
    move_to(h, p);
    button(h, p, true);
    button(h, p, false);
}

#[test]
fn a_created_layer_is_reached_from_the_layer_squircle() {
    let (mut h, id) = fixture("segment_layer_squircle");
    let p = inject(&mut h, id);
    move_to(&mut h, p);
    click(&mut h, p);
    let tag = h.app.image_segments.tag_rects[0].center();
    click(&mut h, tag);
    assert_eq!(h.app.image_segments.layers_menu, Some(id));
    h.frame();
    assert_eq!(
        h.app.image_segments.layer_rects.len(),
        2,
        "Layer 1 and Add layer, no title row"
    );
    let host = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let row = h.app.image_segments.layer_rects[0];
    assert!(
        host.contains(row.center()),
        "the squircle sits at the top of the image"
    );
    assert!(row.center().y < host.center().y);
    click(&mut h, row.center());
    assert_eq!(h.app.board_tool, BoardTool::Brush);
    assert!(h.app.board_sel.contains(&id));
    let session = h.app.image_paint_session().expect("painting the picture");
    assert_eq!((session.image, session.layer_index), (id, 0));
    assert!(h.app.image_segments.layers_menu.is_none());
}

#[test]
fn the_image_menu_opens_the_squircle_and_its_last_row_adds_a_layer() {
    let (mut h, id) = fixture("segment_layers_add");
    let p = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .center();
    h.app.board_menu = Some((id, p));
    h.frame();
    let out = h.frame_output(|_| {});
    let item = label_center(&out, "Layers").expect("Layers in the image menu");
    click(&mut h, item);
    assert_eq!(h.app.image_segments.layers_menu, Some(id));
    h.frame();
    let add = *h.app.image_segments.layer_rects.last().unwrap();
    click(&mut h, add.center());
    let layers = |h: &Harness| match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(img) => img.paint_layers.len(),
        _ => unreachable!(),
    };
    assert_eq!(layers(&h), 1);
    assert_eq!(h.app.image_paint_session().map(|s| s.layer_index), Some(0));
    h.app.board_undo();
    assert_eq!(layers(&h), 0, "adding a layer is one journaled step");
}
