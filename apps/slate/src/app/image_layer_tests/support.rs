use super::*;
use slate_doc::{scene::ImageNode, ViewKind};

pub(super) fn png(h: &Harness, name: &str, rgb: [u8; 3]) -> std::path::PathBuf {
    let path = h.base.join(name);
    image::RgbaImage::from_pixel(32, 24, image::Rgba([rgb[0], rgb[1], rgb[2], 255]))
        .save(&path)
        .unwrap();
    path
}

pub(super) fn board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    h
}

pub(super) fn place_image(h: &mut Harness, name: &str, rect: WorldRect) -> NodeId {
    let path = png(h, name, [200, 180, 160]);
    let item = h.app.add_paths(&[path])[0];
    let node = h
        .app
        .doc_mut()
        .scene
        .build_node(rect, NodeKind::Image(ImageNode::new(item)));
    h.app.add_nodes(vec![node])[0]
}

pub(super) fn photo_board(tag: &str) -> (Harness, NodeId) {
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

pub(super) fn arm(h: &mut Harness, tool: BoardTool) {
    h.app.set_board_tool(tool);
    h.app.sync_image_paint_for_tool();
}

pub(super) fn open_layers_in_select(h: &mut Harness) {
    h.app.set_board_tool(BoardTool::Select);
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = Some(Panel::Layers);
}

pub(super) fn screen_rect(h: &Harness, id: NodeId) -> Rect {
    let n = h.app.doc().scene.node(id).unwrap();
    h.app.board_xf().rect_w2s(n.rect)
}

pub(super) fn layers(h: &Harness, id: NodeId) -> Vec<slate_doc::PaintLayer> {
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(img) => {
            let result = h.app.image_result_id(id).expect("selected image result");
            slate_doc::image_paint::layers_for_result(img, &result).to_vec()
        }
        _ => panic!("image"),
    }
}

fn walk<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::Shape>) {
    match shape {
        egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        s => out.push(s),
    }
}

pub(super) fn shapes(out: &egui::FullOutput) -> Vec<&egui::Shape> {
    let mut all = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut all);
    }
    all
}

pub(super) fn circles(out: &egui::FullOutput) -> Vec<egui::epaint::CircleShape> {
    shapes(out)
        .into_iter()
        .filter_map(|s| match s {
            egui::Shape::Circle(c) => Some(*c),
            _ => None,
        })
        .collect()
}

pub(super) fn text_rect(out: &egui::FullOutput, label: &str) -> Option<Rect> {
    shapes(out).into_iter().find_map(|s| match s {
        egui::Shape::Text(t) if t.galley.text() == label => {
            Some(Rect::from_min_size(t.pos, t.galley.size()))
        }
        _ => None,
    })
}

pub(super) fn render(h: &mut Harness) -> egui::FullOutput {
    h.frame_output(|_| {})
}

pub(super) fn move_to(h: &mut Harness, p: Pos2) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
}

pub(super) fn button(h: &mut Harness, p: Pos2, pressed: bool) {
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

pub(super) fn click(h: &mut Harness, p: Pos2) {
    move_to(h, p);
    button(h, p, true);
    button(h, p, false);
    h.frame();
}

pub(super) fn settle_image_panel(h: &mut Harness) {
    let now = h.ctx.input(|input| input.time);
    h.frame_with(|input| input.time = Some(now + 1.0));
}

pub(super) fn layer_palette_layout(
    h: &Harness,
    id: NodeId,
) -> atlas_shell::selection_tools::LayerPaletteLayout {
    let image = screen_rect(h, id);
    let zoom = h.app.tab().cam.z;
    let presentation = atlas_shell::selection_tools::image_presentation_layout(
        image,
        zoom,
        atlas_shell::selection_tools::ImagePanelKind::Layers,
    );
    let session = h.app.image_paint_session().expect("image paint session");
    atlas_shell::selection_tools::image_layer_palette_layout(
        &presentation,
        layers(h, id).len(),
        h.app
            .doc()
            .view
            .recent_colors
            .as_deref()
            .map_or(0, |colors| colors.len()),
        zoom,
        Some(session.layer_index),
    )
}

pub(super) fn rings_in(h: &mut Harness, capsule: Rect) -> Vec<(Pos2, f32, bool)> {
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
