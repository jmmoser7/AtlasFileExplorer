//! Crop-mode boards, handles, and pictures.

use super::*;

/// `n` croppable 200×150 images in a row 60 world units apart, all selected,
/// the camera at 1:1 and centered on the row so screen px equal world units.
pub(crate) fn crop_board(tag: &str, n: usize) -> (Harness, Vec<NodeId>) {
    let mut h = kit_board(tag, board::BoardTool::Select);
    let paths: Vec<PathBuf> = (0..n)
        .map(|i| {
            let p = h.base.join(format!("crop{i}.png"));
            image::RgbaImage::from_pixel(8, 6, image::Rgba([90, 140, 200, 255]))
                .save(&p)
                .unwrap();
            p
        })
        .collect();
    let items = h.app.add_paths(&paths);
    let nodes: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            h.app.doc_mut().scene.build_node(
                WorldRect::new(i as f32 * 260.0, 0.0, 200.0, 150.0),
                NodeKind::Image(slate_doc::scene::ImageNode::new(*item)),
            )
        })
        .collect();
    let ids = h.app.add_nodes(nodes);
    h.app.board_sel = ids.iter().copied().collect();
    let row_w = n as f32 * 260.0 - 60.0;
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = EVec2::new(row_w * 0.5, 75.0);
    h.frame();
    h.frame();
    for id in &ids {
        assert!(h.app.croppable_image(*id), "fixture images must crop");
    }
    (h, ids)
}

/// Turn crop on through the registry command (C) with no panel open.
pub(crate) fn crop_via_command(h: &mut Harness) {
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.crop"), None));
    assert!(h.app.board_crop.is_some());
}

pub(crate) fn crop_pointer(h: &mut Harness, p: Pos2, button: Option<bool>) {
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(p));
        if let Some(pressed) = button {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    });
}

pub(crate) fn crop_drag_handle(h: &Harness) -> Option<(NodeId, u8, usize)> {
    match &h.app.board_drag {
        Some(board::BoardDrag::CropEdge {
            id, handle, peers, ..
        }) => Some((*id, *handle, peers.len())),
        _ => None,
    }
}

/// Turn crop on the way D01 describes: open the Corners squircle, then pick
/// Crop in its Off/Crop control. The panel stays open (D09).
pub(crate) fn crop_via_corners_panel(h: &mut Harness) {
    h.app.shape_properties.panel = Some(board_properties::Panel::Corners);
    for _ in 0..4 {
        h.frame();
    }
    let first = *h.app.board_sel.iter().min_by_key(|id| id.0).unwrap();
    h.app.enter_crop_mode(first);
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Corners)
    );
}

pub(crate) fn crop_key(h: &mut Harness, key: egui::Key, pressed: bool) {
    h.frame_with(|i| {
        i.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
}

pub(crate) fn crop_of(h: &Harness, id: NodeId) -> slate_doc::scene::Crop {
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(img) => img.crop,
        _ => panic!("image node"),
    }
}

/// Crop mode, the whole selection of `n`, and the Corners squircle are all
/// still up after the frame that just ran.
pub(crate) fn assert_crop_holds(h: &Harness, n: usize, when: &str) {
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Corners),
        "{when}: Corners panel closed"
    );
    assert!(h.app.board_crop.is_some(), "{when}: crop mode left");
    assert_eq!(h.app.board_sel.len(), n, "{when}: {:?}", h.app.board_sel);
}

pub(crate) fn crop_idle_frames(h: &mut Harness, n: usize, when: &str) {
    for f in 0..6 {
        h.frame();
        assert_crop_holds(h, n, &format!("{when}, idle frame {f}"));
    }
}

pub(crate) fn add_picture(h: &mut Harness, rect: slate_doc::scene::WorldRect) -> NodeId {
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    h.app.add_nodes(vec![node])[0]
}

pub(crate) fn picture_flips(h: &Harness, id: NodeId) -> (bool, bool) {
    match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::NodeKind::Image(img) => (img.flip_x, img.flip_y),
        _ => panic!("image"),
    }
}

pub(crate) const RED: [u8; 4] = [255, 0, 0, 255];

pub(crate) const BLUE: [u8; 4] = [0, 0, 255, 255];

/// A 40 x 20 PNG, red on the left half and blue on the right, placed at
/// `rect`.
pub(crate) fn add_two_tone_picture(
    h: &mut Harness,
    name: &str,
    rect: slate_doc::scene::WorldRect,
) -> (NodeId, PathBuf) {
    let p = h.base.join(name);
    image::RgbaImage::from_fn(40, 20, |x, _| image::Rgba(if x < 20 { RED } else { BLUE }))
        .save(&p)
        .unwrap();
    let item = h.app.add_paths(std::slice::from_ref(&p))[0];
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    (h.app.add_nodes(vec![node])[0], p)
}
