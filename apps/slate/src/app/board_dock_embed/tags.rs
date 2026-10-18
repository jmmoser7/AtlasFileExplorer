//! Dynamic tag recipes in the existing journaled DockStrip interpreter.
use super::*;
use crate::app::ui::tools::tags as recipe;
use atlas_shell::selection_tools as chrome;

pub(super) fn drop(app: &mut SlateApp, ctx: &egui::Context) {
    let tags = recipe::recipe(app, &[]);
    if tags.is_empty() {
        return;
    }
    let tokens = dock_tokens();
    let items = recipe::palette_items(&tags, &recipe::selected(app, ctx));
    let layout = chrome::measure_tag_palette(ctx, &items, dock::flyout_icon_size(&tokens));
    let card = layout.natural_size(&tokens);
    let z = app.tab().cam.z.max(0.05);
    let center = app
        .board_xf()
        .s2w(dock::take_canvas_drop_position(ctx).unwrap_or(app.canvas_rect.center()));
    let rect = slate_doc::scene::WorldRect::new(
        center.x - card.x / z * 0.5,
        center.y - card.y / z * 0.5,
        card.x / z,
        card.y / z,
    );
    let node = app.doc_mut().scene.build_node(
        rect,
        NodeKind::DockStrip(DockStripNode {
            palette_id: "tool.tags".into(),
            visible: tags.iter().map(|tag| tag.id.0.to_string()).collect(),
        }),
    );
    app.add_nodes(vec![node]);
}

pub(super) fn hit(
    app: &SlateApp,
    ctx: &egui::Context,
    dest: Rect,
    strip: &DockStripNode,
    point: Pos2,
) -> Option<String> {
    let tags = recipe::recipe(app, &strip.visible);
    let tokens = dock_tokens();
    let items = recipe::palette_items(&tags, &recipe::selected(app, ctx));
    let layout = chrome::measure_tag_palette(ctx, &items, dock::flyout_icon_size(&tokens));
    let placement = layout.fit(dest, &tokens);
    for (index, tag) in tags.iter().enumerate() {
        let slot = layout.slot(placement.origin, index, placement.scale);
        if slot.contains(point) {
            let action = if ctx.input(|input| input.modifiers.shift || input.modifiers.ctrl) {
                "select"
            } else if point.y <= slot.center().y {
                "show"
            } else {
                "hide"
            };
            return Some(format!("tag.{action}.{}", tag.id.0));
        }
    }
    None
}

pub(super) fn paint(
    ui: &egui::Ui,
    dest: Rect,
    strip: &DockStripNode,
    app: &SlateApp,
    tokens: &DockTokens,
) {
    let tags = recipe::recipe(app, &strip.visible);
    let selected = recipe::selected(app, ui.ctx());
    let items = recipe::palette_items(&tags, &selected);
    chrome::paint_tag_palette_card(ui, dest, &items, tokens, app.palette());
}
