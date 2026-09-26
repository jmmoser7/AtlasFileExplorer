//! Headless checks for the image layer palette, image-on-image drops, and
//! the Corners crop toggles, driven through the real frame loop.

use super::board::BoardTool;
use super::board_properties::Panel;
use super::tests::Harness;
use eframe::egui::{self, Color32, Pos2, Rect};
use slate_doc::scene::{ImageNode, NodeKind, WorldRect};
use slate_doc::{NodeId, ViewKind};

fn png(h: &Harness, name: &str, rgb: [u8; 3]) -> std::path::PathBuf {
    let path = h.base.join(name);
    image::RgbaImage::from_pixel(32, 24, image::Rgba([rgb[0], rgb[1], rgb[2], 255]))
        .save(&path)
        .unwrap();
    path
}

fn board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    h
}

fn place_image(h: &mut Harness, name: &str, rect: WorldRect) -> NodeId {
    let path = png(h, name, [200, 180, 160]);
    let item = h.app.add_paths(&[path])[0];
    let node = h
        .app
        .doc_mut()
        .scene
        .build_node(rect, NodeKind::Image(ImageNode::new(item)));
    h.app.add_nodes(vec![node])[0]
}

/// One image selected on an empty board.
fn photo_board(tag: &str) -> (Harness, NodeId) {
    let mut h = board(tag);
    let id = place_image(
        &mut h,
        "photo.png",
        WorldRect::new(-120.0, -80.0, 240.0, 160.0),
    );
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    (h, id)
}

fn arm(h: &mut Harness, tool: BoardTool) {
    h.app.set_board_tool(tool);
    h.app.sync_image_paint_for_tool();
}

fn screen_rect(h: &Harness, id: NodeId) -> Rect {
    let n = h.app.doc().scene.node(id).unwrap();
    h.app.board_xf().rect_w2s(n.rect)
}

fn layers(h: &Harness, id: NodeId) -> Vec<slate_doc::PaintLayer> {
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(img) => img.paint_layers.clone(),
        _ => panic!("image"),
    }
}

fn walk<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::Shape>) {
    match shape {
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        s => out.push(s),
    }
}

fn shapes(out: &egui::FullOutput) -> Vec<&egui::Shape> {
    let mut all = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut all);
    }
    all
}

fn circles(out: &egui::FullOutput) -> Vec<egui::epaint::CircleShape> {
    shapes(out)
        .into_iter()
        .filter_map(|s| match s {
            egui::Shape::Circle(c) => Some(*c),
            _ => None,
        })
        .collect()
}

fn text_rect(out: &egui::FullOutput, label: &str) -> Option<Rect> {
    shapes(out).into_iter().find_map(|s| match s {
        egui::Shape::Text(t) if t.galley.text() == label => {
            Some(Rect::from_min_size(t.pos, t.galley.size()))
        }
        _ => None,
    })
}

fn render(h: &mut Harness) -> egui::FullOutput {
    h.frame_output(|_| {})
}

fn move_to(h: &mut Harness, p: Pos2) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
}

fn button(h: &mut Harness, p: Pos2, pressed: bool) {
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(p));
        i.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    });
}

fn click(h: &mut Harness, p: Pos2) {
    move_to(h, p);
    button(h, p, true);
    button(h, p, false);
    h.frame();
}

/// The layer palette's capsule and its circled `+`: object chrome that sits
/// below the image on screen.
fn palette_hits(h: &Harness, image: Rect) -> Option<(Rect, Rect)> {
    let below: Vec<Rect> = h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .copied()
        .filter(|r| r.top() > image.bottom())
        .collect();
    let capsule = below
        .iter()
        .copied()
        .max_by(|a, b| a.width().total_cmp(&b.width()))?;
    let plus = below
        .iter()
        .copied()
        .find(|r| r.width() < capsule.height() && r.left() >= capsule.right())?;
    Some((capsule, plus))
}

/// Ring circles painted inside `capsule`, left to right: (center, radius, accent?).
fn rings_in(h: &mut Harness, capsule: Rect) -> Vec<(Pos2, f32, bool)> {
    let out = render(h);
    let accent = h.app.palette().accent;
    let mut rings: Vec<_> = circles(&out)
        .into_iter()
        .filter(|c| c.fill == Color32::TRANSPARENT && capsule.contains(c.center))
        .map(|c| (c.center, c.radius, c.stroke.color == accent))
        .collect();
    rings.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
    rings
}

// --- 4. Replace / Add as layer ----------------------------------------------

/// Target on the left, source on the right; source selected.
fn two_images(tag: &str) -> (Harness, NodeId, NodeId) {
    let mut h = board(tag);
    let target = place_image(
        &mut h,
        "a.png",
        WorldRect::new(-360.0, -100.0, 240.0, 180.0),
    );
    let source = place_image(&mut h, "b.png", WorldRect::new(120.0, -60.0, 120.0, 120.0));
    h.app.board_sel = std::iter::once(source).collect();
    h.frame();
    (h, target, source)
}

fn item_of(h: &Harness, id: NodeId) -> slate_doc::ItemId {
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(img) => img.item,
        _ => panic!("image"),
    }
}

/// Drag the source onto the target and release on the named capsule.
fn drag_onto_and_choose(h: &mut Harness, target: NodeId, source: NodeId, label: &str) {
    let from = screen_rect(h, source).center();
    let onto = screen_rect(h, target).center();
    button(h, from, true);
    for i in 1..=8 {
        move_to(h, from.lerp(onto, i as f32 / 8.0));
    }
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(onto)));
    let choice = text_rect(&out, label)
        .unwrap_or_else(|| panic!("{label} capsule shows while the image is over the target"));
    for i in 1..=4 {
        move_to(h, onto.lerp(choice.center(), i as f32 / 4.0));
    }
    button(h, choice.center(), false);
    h.frame();
}

#[test]
fn dragging_an_image_onto_another_can_replace_it() {
    let (mut h, target, source) = two_images("drop_node_replace");
    let (before_target, before_source) = (item_of(&h, target), item_of(&h, source));
    drag_onto_and_choose(&mut h, target, source, "Replace");
    assert_eq!(
        item_of(&h, target),
        before_source,
        "the target shows the source"
    );
    assert!(
        h.app.doc().scene.node(source).is_none(),
        "the source is consumed"
    );
    h.app.board_undo();
    assert_eq!(item_of(&h, target), before_target);
    assert!(
        h.app.doc().scene.node(source).is_some(),
        "one undo restores both"
    );
}

#[test]
fn dragging_an_image_onto_another_can_add_it_as_a_layer() {
    let (mut h, target, source) = two_images("drop_node_layer");
    let before_target = item_of(&h, target);
    drag_onto_and_choose(&mut h, target, source, "Add as layer");
    assert_eq!(item_of(&h, target), before_target, "the base stays");
    assert_eq!(layers(&h, target).len(), 1);
    assert_eq!(layers(&h, target)[0].nodes.len(), 1);
    assert!(
        h.app.doc().scene.node(source).is_none(),
        "the source is consumed"
    );
    h.app.board_undo();
    assert!(layers(&h, target).is_empty());
    assert!(
        h.app.doc().scene.node(source).is_some(),
        "one undo restores both"
    );
}

fn drop_file_and_choose(h: &mut Harness, target: NodeId, label: &str) -> std::path::PathBuf {
    let file = png(h, "dropped.png", [10, 200, 90]);
    let at = screen_rect(h, target).center();
    h.app
        .external_drop
        .push_test(super::external_drop::DropEvent {
            payload: super::external_drop::Payload::Files(vec![file.clone()]),
            at,
            alt: false,
        });
    h.frame();
    let out = render(h);
    let choice = text_rect(&out, label)
        .unwrap_or_else(|| panic!("{label} capsule shows after a file drop on an image"));
    click(h, choice.center());
    file
}

#[test]
fn dropping_a_file_on_an_image_can_replace_it() {
    let (mut h, target, _) = two_images("drop_file_replace");
    let nodes = h.app.doc().scene.nodes.len();
    let file = drop_file_and_choose(&mut h, target, "Replace");
    let item = item_of(&h, target);
    assert_eq!(h.app.doc().item(item).map(|i| i.path.clone()), Some(file));
    assert_eq!(h.app.doc().scene.nodes.len(), nodes, "no new board node");
    assert!(h.app.image_drop.is_none(), "the capsules are dismissed");
}

#[test]
fn dropping_a_file_on_an_image_can_add_it_as_a_layer() {
    let (mut h, target, _) = two_images("drop_file_layer");
    let nodes = h.app.doc().scene.nodes.len();
    let base = item_of(&h, target);
    drop_file_and_choose(&mut h, target, "Add as layer");
    assert_eq!(item_of(&h, target), base, "the base stays");
    assert_eq!(layers(&h, target).len(), 1);
    assert_eq!(layers(&h, target)[0].nodes.len(), 1);
    assert_eq!(h.app.doc().scene.nodes.len(), nodes, "no new board node");
}

/// egui still holds the pointer's last position from before an OS drag, so
/// the first frame after a drop must not read it as the pointer leaving.
#[test]
fn a_file_drop_offer_survives_a_stale_pointer_position() {
    let (mut h, target, _) = two_images("drop_file_stale_pointer");
    move_to(&mut h, Pos2::new(1300.0, 820.0));
    let file = png(&h, "dropped.png", [10, 200, 90]);
    let at = screen_rect(&h, target).center();
    h.app
        .external_drop
        .push_test(super::external_drop::DropEvent {
            payload: super::external_drop::Payload::Files(vec![file]),
            at,
            alt: false,
        });
    h.frame();
    h.frame();
    assert!(
        h.app.image_drop.is_some(),
        "the offer waits for the pointer"
    );
    let out = render(&mut h);
    let replace = text_rect(&out, "Replace").expect("Replace capsule");
    click(&mut h, replace.center());
    assert_eq!(layers(&h, target).len(), 0);
    assert!(h.app.image_drop.is_none());
}

/// A press that drifts a few pixels on a capsule is still that choice, and
/// never drags the image under it.
#[test]
fn a_sloppy_click_on_a_file_drop_capsule_still_chooses() {
    let (mut h, target, _) = two_images("drop_file_sloppy_click");
    let rect = h.app.doc().scene.node(target).unwrap().rect;
    let file = png(&h, "dropped.png", [10, 200, 90]);
    let at = screen_rect(&h, target).center();
    h.app
        .external_drop
        .push_test(super::external_drop::DropEvent {
            payload: super::external_drop::Payload::Files(vec![file]),
            at,
            alt: false,
        });
    h.frame();
    let out = render(&mut h);
    let add = text_rect(&out, "Add as layer").expect("Add as layer capsule");
    let p = add.center();
    move_to(&mut h, p);
    button(&mut h, p, true);
    for dx in [3.0, 6.0, 9.0, 12.0] {
        move_to(&mut h, p + egui::vec2(dx, 1.0));
    }
    button(&mut h, p + egui::vec2(12.0, 1.0), false);
    h.frame();
    assert_eq!(
        h.app.doc().scene.node(target).unwrap().rect,
        rect,
        "the target did not move"
    );
    assert_eq!(
        layers(&h, target).len(),
        1,
        "the drifting click chose Add as layer"
    );
}

// --- 6. Crop toggles pack inside the Corners capsule ------------------------

#[test]
fn crop_toggles_pack_inside_the_primary_corners_capsule() {
    let (mut h, _id) = photo_board("crop_toggles_packed");
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = Some(Panel::Corners);
    h.frame();
    h.frame();
    let z = h.app.tab().cam.z;
    let editor = h
        .app
        .shape_properties
        .chrome_hits
        .iter()
        .copied()
        .find(|r| (r.width() - atlas_shell::selection_tools::EDITOR_WIDTH * z).abs() < 0.01)
        .expect("corner editor is live");
    assert!(
        (editor.height() - atlas_shell::selection_tools::CORNER_HEIGHT * z).abs() < 0.01,
        "the primary capsule keeps its fillet height: {editor:?}"
    );
    let out = render(&mut h);
    for label in ["Fillet", "Chamfer", "Off", "Crop", "%", "u"] {
        let r = text_rect(&out, label).unwrap_or_else(|| panic!("{label} is painted"));
        assert!(
            editor.contains(r.center()),
            "{label} at {r:?} sits inside the primary capsule {editor:?}"
        );
    }
    let crop = text_rect(&out, "Crop").unwrap().center();
    click(&mut h, crop);
    assert!(
        h.app.board_crop.is_some(),
        "the packed Crop toggle still enters crop mode"
    );
}
