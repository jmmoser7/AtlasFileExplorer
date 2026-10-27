//! Portal action and maximize controls owned by the shared tab chrome.
use super::TabChromeColors;
use crate::theme::Palette;
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

/// Windows / Slate caption maximize glyph: a square made of four corner
/// chevrons. `restored` paints the overlapping-squares restore mark.
/// Do not use a Unicode box — those fight the tab typeface.
pub fn paint_maximize_glyph(painter: &egui::Painter, hit: Rect, color: Color32, restored: bool) {
    // Slim portal bars are short; the window-control slot is wider than it
    // is tall. Size the glyph from the larger axis so it stays readable.
    let scale = (hit.width().max(hit.height()) / 30.0).max(0.01);
    let c = hit.center();
    let s = Stroke::new(1.2 * scale, color);
    if restored {
        let r = Rect::from_center_size(c + Vec2::new(-1.0, 1.0) * scale, Vec2::splat(8.0 * scale));
        let back = r.translate(Vec2::new(2.5 * scale, -2.5 * scale));
        painter.line_segment([back.left_top(), back.right_top()], s);
        painter.line_segment([back.right_top(), back.right_bottom()], s);
        painter.rect_stroke(r, 1.0 * scale, s, StrokeKind::Middle);
        return;
    }
    let half = 4.5 * scale;
    let arm = 3.2 * scale;
    let r = Rect::from_center_size(c, Vec2::splat(half * 2.0));
    painter.line_segment([r.left_top(), Pos2::new(r.left() + arm, r.top())], s);
    painter.line_segment([r.left_top(), Pos2::new(r.left(), r.top() + arm)], s);
    painter.line_segment([r.right_top(), Pos2::new(r.right() - arm, r.top())], s);
    painter.line_segment([r.right_top(), Pos2::new(r.right(), r.top() + arm)], s);
    painter.line_segment(
        [r.right_bottom(), Pos2::new(r.right() - arm, r.bottom())],
        s,
    );
    painter.line_segment(
        [r.right_bottom(), Pos2::new(r.right(), r.bottom() - arm)],
        s,
    );
    painter.line_segment([r.left_bottom(), Pos2::new(r.left() + arm, r.bottom())], s);
    painter.line_segment([r.left_bottom(), Pos2::new(r.left(), r.bottom() - arm)], s);
}

/// Floating maximize hit on a portal that has no identity tab (or whose tab
/// is folded). Sits on the node, upper-right. Returns true on click.
pub fn portal_maximize_button(
    ui: &Ui,
    palette: &Palette,
    rect: Rect,
    id_salt: u64,
    maximized: bool,
) -> bool {
    if rect.width() < 4.0 || rect.height() < 4.0 {
        return false;
    }
    let resp = ui.interact(
        rect,
        ui.id().with(("portal_max", id_salt, maximized)),
        Sense::click(),
    );
    paint_portal_max_hover(ui, palette, rect, &resp, maximized);
    resp.clicked()
}

/// A labelled portal-mode action, using the same chrome colors and canvas scale.
pub fn portal_action_button(
    ui: &Ui,
    palette: &Palette,
    rect: Rect,
    id: egui::Id,
    label: &str,
    zoom: f32,
) -> bool {
    if rect.width() <= 0.0 || rect.height() <= 0.0 || !crate::canvas_text::legible(12.0 * zoom) {
        return false;
    }
    let response = ui.interact(rect, id, Sense::click()).on_hover_text(label);
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let colors = TabChromeColors::from_palette(palette, &crate::tokens::current().topbar);
    painter.rect_filled(
        rect,
        5.0 * zoom,
        if response.hovered() {
            colors.inactive_hover
        } else {
            colors.bar
        },
    );
    painter.rect_stroke(
        rect,
        5.0 * zoom,
        Stroke::new(0.8 * zoom, palette.border_strong),
        StrokeKind::Inside,
    );
    crate::canvas_text::text(
        &painter,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0 * zoom),
        palette.ink,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    response.clicked()
}

/// Hover wash, hand cursor, and tooltip for the portal maximize / restore slot.
pub(super) fn paint_portal_max_hover(
    ui: &Ui,
    palette: &Palette,
    rect: Rect,
    resp: &egui::Response,
    maximized: bool,
) {
    let hovered = resp.hovered();
    if hovered {
        let colors = TabChromeColors::from_palette(palette, &crate::tokens::current().topbar);
        ui.painter().rect_filled(rect, 0.0, colors.inactive_hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    paint_maximize_glyph(
        ui.painter(),
        rect,
        if hovered { palette.ink } else { palette.sub },
        maximized,
    );
    resp.clone()
        .on_hover_text(if maximized { "Restore" } else { "Maximize" });
}
