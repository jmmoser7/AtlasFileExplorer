//! Minimal vector icons for the board create toolbar.
//!
//! Drawn with egui strokes so they stay crisp at any DPI and match the palette.

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Icon glyphs for toolbar buttons and hover submenus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolIcon {
    Select,
    Pan,
    Frame,
    Shapes,
    Rect,
    Ellipse,
    Curve,
    Line,
    Arc,
    Polyline,
    Bezier,
    Pen,
    Text,
    Ruler,
    ChevronRight,
    ChevronLeft,
    Grid,
    Snap,
    Align,
    Brush,
    Eraser,
    Eyedropper,
    Sticky,
    DirectSelect,
    Colors,
}

impl ToolIcon {
    pub fn label(self) -> &'static str {
        match self {
            ToolIcon::Select => "Select",
            ToolIcon::Pan => "Pan",
            ToolIcon::Frame => "Frame",
            ToolIcon::Shapes => "Shapes",
            ToolIcon::Rect => "Rectangle",
            ToolIcon::Ellipse => "Ellipse",
            ToolIcon::Curve => "Curve",
            ToolIcon::Line => "Line",
            ToolIcon::Arc => "Arc",
            ToolIcon::Polyline => "Polyline",
            ToolIcon::Bezier => "Bezier",
            ToolIcon::Pen => "Pen",
            ToolIcon::Text => "Text",
            ToolIcon::Ruler => "Measure",
            ToolIcon::ChevronRight => "Expand",
            ToolIcon::ChevronLeft => "Collapse",
            ToolIcon::Grid => "Grid",
            ToolIcon::Snap => "Snap to grid",
            ToolIcon::Align => "Align",
            ToolIcon::Brush => "Brush",
            ToolIcon::Eraser => "Eraser",
            ToolIcon::Eyedropper => "Eyedropper",
            ToolIcon::Sticky => "Sticky note",
            ToolIcon::DirectSelect => "Direct select",
            ToolIcon::Colors => "Colors",
        }
    }
}

fn pt(r: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(r.min.x + r.width() * x, r.min.y + r.height() * y)
}

fn stroke_w(r: Rect) -> f32 {
    (r.width() * 0.075).clamp(1.15, 1.85)
}

/// Paint a tool icon into `r` (square-ish rect).
pub fn paint_tool_icon(painter: &egui::Painter, r: Rect, icon: ToolIcon, color: Color32) {
    let w = stroke_w(r);
    let s = Stroke::new(w, color);

    match icon {
        ToolIcon::Select => {
            // Pointer arrow — tip upper-left, tail lower-right.
            let tip = pt(r, 0.18, 0.16);
            let tail = pt(r, 0.84, 0.86);
            let wing = pt(r, 0.30, 0.30);
            painter.line_segment([tip, tail], s);
            painter.line_segment([tip, wing], s);
            painter.line_segment([wing, pt(r, 0.38, 0.22)], s);
        }
        ToolIcon::Pan => {
            // Open hand — palm block + finger strokes + thumb hook.
            let palm = Rect::from_min_max(pt(r, 0.28, 0.50), pt(r, 0.88, 0.92));
            painter.rect_stroke(palm, 2.0, s, egui::StrokeKind::Inside);
            for x in [0.36, 0.48, 0.60, 0.72] {
                painter.line_segment([pt(r, x, 0.18), pt(r, x, 0.50)], s);
            }
            painter.line_segment([pt(r, 0.14, 0.62), pt(r, 0.28, 0.48)], s);
            painter.line_segment([pt(r, 0.14, 0.62), pt(r, 0.20, 0.74)], s);
        }
        ToolIcon::Frame => {
            // Slide frame — outer rect + title-bar tick.
            let outer = Rect::from_min_max(pt(r, 0.14, 0.12), pt(r, 0.86, 0.88));
            painter.rect_stroke(outer, 1.5, s, egui::StrokeKind::Inside);
            painter.line_segment([pt(r, 0.14, 0.24), pt(r, 0.86, 0.24)], s);
        }
        ToolIcon::Shapes => {
            let rect = Rect::from_min_max(pt(r, 0.10, 0.28), pt(r, 0.58, 0.82));
            painter.rect_stroke(rect, 1.5, s, egui::StrokeKind::Inside);
            painter.add(egui::Shape::circle_stroke(
                pt(r, 0.68, 0.38),
                r.width() * 0.22,
                s,
            ));
        }
        ToolIcon::Rect => {
            let rect = Rect::from_min_max(pt(r, 0.16, 0.22), pt(r, 0.84, 0.78));
            painter.rect_stroke(rect, 1.5, s, egui::StrokeKind::Inside);
        }
        ToolIcon::Ellipse => {
            painter.add(egui::Shape::ellipse_stroke(
                r.center(),
                Vec2::new(r.width() * 0.34, r.height() * 0.28),
                s,
            ));
        }
        ToolIcon::Curve => {
            // Pen nib + short stroke (curve tool family).
            painter.line_segment([pt(r, 0.20, 0.78), pt(r, 0.46, 0.22)], s);
            painter.line_segment([pt(r, 0.46, 0.22), pt(r, 0.54, 0.34)], s);
            painter.line_segment([pt(r, 0.54, 0.34), pt(r, 0.82, 0.28)], s);
            painter.line_segment([pt(r, 0.20, 0.78), pt(r, 0.34, 0.62)], s);
        }
        ToolIcon::Line => {
            painter.line_segment([pt(r, 0.16, 0.82), pt(r, 0.84, 0.18)], s);
        }
        ToolIcon::Arc => {
            painter.add(egui::Shape::line(
                vec![
                    pt(r, 0.14, 0.72),
                    pt(r, 0.28, 0.38),
                    pt(r, 0.56, 0.22),
                    pt(r, 0.82, 0.34),
                ],
                s,
            ));
        }
        ToolIcon::Polyline => {
            painter.add(egui::Shape::line(
                vec![
                    pt(r, 0.12, 0.70),
                    pt(r, 0.38, 0.48),
                    pt(r, 0.52, 0.62),
                    pt(r, 0.72, 0.28),
                    pt(r, 0.88, 0.44),
                ],
                s,
            ));
        }
        ToolIcon::Bezier => {
            painter.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    [
                        pt(r, 0.12, 0.72),
                        pt(r, 0.36, 0.18),
                        pt(r, 0.64, 0.82),
                        pt(r, 0.88, 0.28),
                    ],
                    false,
                    Color32::TRANSPARENT,
                    s,
                ),
            ));
        }
        ToolIcon::Pen => {
            painter.add(egui::Shape::line(
                vec![
                    pt(r, 0.14, 0.78),
                    pt(r, 0.30, 0.62),
                    pt(r, 0.42, 0.70),
                    pt(r, 0.58, 0.40),
                    pt(r, 0.78, 0.52),
                    pt(r, 0.88, 0.30),
                ],
                s,
            ));
        }
        ToolIcon::Text => {
            // Serif T inside a light text-box hint.
            let box_r = Rect::from_min_max(pt(r, 0.14, 0.20), pt(r, 0.86, 0.80));
            painter.rect_stroke(
                box_r,
                1.5,
                Stroke::new(w * 0.85, color.gamma_multiply(0.55)),
                egui::StrokeKind::Inside,
            );
            painter.line_segment([pt(r, 0.28, 0.32), pt(r, 0.72, 0.32)], s);
            painter.line_segment([pt(r, 0.50, 0.32), pt(r, 0.50, 0.72)], s);
        }
        ToolIcon::Ruler => {
            // Ruler bar with tick marks.
            let bar = Rect::from_min_max(pt(r, 0.22, 0.38), pt(r, 0.86, 0.62));
            painter.rect_stroke(bar, 1.5, s, egui::StrokeKind::Inside);
            for x in [0.30, 0.42, 0.54, 0.66, 0.78] {
                let h = if (x - 0.54f32).abs() < 0.01 {
                    0.22
                } else {
                    0.14
                };
                painter.line_segment([pt(r, x, 0.38), pt(r, x, 0.38 + h)], s);
            }
        }
        ToolIcon::ChevronRight => {
            painter.add(egui::Shape::line(
                vec![pt(r, 0.34, 0.22), pt(r, 0.62, 0.50), pt(r, 0.34, 0.78)],
                s,
            ));
        }
        ToolIcon::ChevronLeft => {
            painter.add(egui::Shape::line(
                vec![pt(r, 0.66, 0.22), pt(r, 0.38, 0.50), pt(r, 0.66, 0.78)],
                s,
            ));
        }
        ToolIcon::Grid => {
            // 3×3 lattice.
            for f in [0.38, 0.62] {
                painter.line_segment([pt(r, 0.16, f), pt(r, 0.84, f)], s);
                painter.line_segment([pt(r, f, 0.16), pt(r, f, 0.84)], s);
            }
            let outer = Rect::from_min_max(pt(r, 0.16, 0.16), pt(r, 0.84, 0.84));
            painter.rect_stroke(outer, 1.0, s, egui::StrokeKind::Inside);
        }
        ToolIcon::Snap => {
            // Horseshoe magnet with pole ticks and a target dot.
            painter.add(egui::Shape::line(
                vec![
                    pt(r, 0.30, 0.20),
                    pt(r, 0.30, 0.52),
                    pt(r, 0.38, 0.68),
                    pt(r, 0.54, 0.72),
                    pt(r, 0.68, 0.62),
                    pt(r, 0.72, 0.44),
                    pt(r, 0.72, 0.20),
                ],
                s,
            ));
            painter.line_segment([pt(r, 0.24, 0.28), pt(r, 0.38, 0.28)], s);
            painter.line_segment([pt(r, 0.64, 0.28), pt(r, 0.80, 0.28)], s);
            painter.circle_filled(pt(r, 0.51, 0.88), r.width() * 0.05, color);
        }
        ToolIcon::Align => {
            // Two offset bars snapping to a shared left datum.
            painter.line_segment([pt(r, 0.24, 0.14), pt(r, 0.24, 0.86)], s);
            let top = Rect::from_min_max(pt(r, 0.30, 0.24), pt(r, 0.84, 0.42));
            let bottom = Rect::from_min_max(pt(r, 0.30, 0.58), pt(r, 0.64, 0.76));
            painter.rect_stroke(top, 1.0, s, egui::StrokeKind::Inside);
            painter.rect_stroke(bottom, 1.0, s, egui::StrokeKind::Inside);
        }
        ToolIcon::Brush => {
            // Handle + ferrule + expressive tip stroke.
            painter.line_segment([pt(r, 0.72, 0.14), pt(r, 0.44, 0.50)], s);
            painter.line_segment([pt(r, 0.80, 0.22), pt(r, 0.52, 0.58)], s);
            painter.line_segment([pt(r, 0.44, 0.50), pt(r, 0.52, 0.58)], s);
            painter.add(egui::Shape::convex_polygon(
                vec![pt(r, 0.44, 0.50), pt(r, 0.52, 0.58), pt(r, 0.24, 0.82)],
                color,
                Stroke::NONE,
            ));
        }
        ToolIcon::Eraser => {
            // Tilted eraser block over a swept line.
            let a = pt(r, 0.34, 0.24);
            let b = pt(r, 0.62, 0.16);
            let c = pt(r, 0.82, 0.48);
            let d = pt(r, 0.54, 0.58);
            painter.add(egui::Shape::closed_line(vec![a, b, c, d], s));
            painter.line_segment([pt(r, 0.42, 0.40), pt(r, 0.66, 0.32)], s);
            painter.line_segment([pt(r, 0.16, 0.80), pt(r, 0.70, 0.80)], s);
        }
        ToolIcon::Eyedropper => {
            // Dropper body + tip + sample drop.
            painter.line_segment([pt(r, 0.74, 0.16), pt(r, 0.36, 0.58)], s);
            painter.line_segment([pt(r, 0.66, 0.10), pt(r, 0.86, 0.30)], s);
            painter.add(egui::Shape::convex_polygon(
                vec![pt(r, 0.36, 0.58), pt(r, 0.44, 0.66), pt(r, 0.20, 0.84)],
                color,
                Stroke::NONE,
            ));
        }
        ToolIcon::Sticky => {
            // Square note with a dog-eared corner.
            painter.add(egui::Shape::line(
                vec![
                    pt(r, 0.18, 0.18),
                    pt(r, 0.82, 0.18),
                    pt(r, 0.82, 0.60),
                    pt(r, 0.60, 0.82),
                    pt(r, 0.18, 0.82),
                    pt(r, 0.18, 0.18),
                ],
                s,
            ));
            painter.line_segment([pt(r, 0.82, 0.60), pt(r, 0.60, 0.60)], s);
            painter.line_segment([pt(r, 0.60, 0.60), pt(r, 0.60, 0.82)], s);
        }
        ToolIcon::DirectSelect => {
            // Hollow pointer (the white-arrow convention).
            let tip = pt(r, 0.30, 0.14);
            painter.add(egui::Shape::closed_line(
                vec![
                    tip,
                    pt(r, 0.30, 0.70),
                    pt(r, 0.44, 0.56),
                    pt(r, 0.56, 0.82),
                    pt(r, 0.64, 0.76),
                    pt(r, 0.52, 0.52),
                    pt(r, 0.70, 0.52),
                ],
                s,
            ));
        }
        ToolIcon::Colors => {
            // Overlapping fg/bg swatches.
            let back = Rect::from_min_max(pt(r, 0.38, 0.38), pt(r, 0.84, 0.84));
            painter.rect_stroke(back, 1.5, s, egui::StrokeKind::Inside);
            let front = Rect::from_min_max(pt(r, 0.16, 0.16), pt(r, 0.62, 0.62));
            painter.rect_filled(front, 1.5, color);
        }
    }
}

/// Square toolbar chip with a painted icon.
pub fn tool_icon_button(
    ui: &mut Ui,
    icon: ToolIcon,
    selected: bool,
    ink: Color32,
    accent: Color32,
    hover_fill: Color32,
    selected_fill: Color32,
) -> Response {
    let size = Vec2::splat(28.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter()
                .rect_filled(rect.shrink(1.0), 5.0, selected_fill);
        } else if response.hovered() {
            ui.painter().rect_filled(rect.shrink(1.0), 5.0, hover_fill);
        }
        let color = if selected { accent } else { ink };
        paint_tool_icon(ui.painter(), rect.shrink(5.0), icon, color);
    }
    response
}

/// Submenu row: small icon + label, with an optional right-aligned dim hotkey.
pub fn tool_menu_row(
    ui: &mut Ui,
    icon: ToolIcon,
    label: &str,
    hotkey: Option<&str>,
    selected: bool,
    ink: Color32,
    sub: Color32,
) -> Response {
    ui.horizontal(|ui| {
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
        paint_tool_icon(ui.painter(), icon_rect.shrink(1.0), icon, ink);
        let resp = ui.selectable_label(selected, label);
        if let Some(key) = hotkey {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(key).small().color(sub));
            });
        }
        resp
    })
    .inner
}

/// Tiny bottom-right triangle marking a toolbar button that owns a flyout
/// submenu (Adobe/Figma convention).
pub fn paint_flyout_corner(painter: &egui::Painter, r: Rect, color: Color32) {
    let a = Pos2::new(r.max.x - 3.5, r.max.y - 8.0);
    let b = Pos2::new(r.max.x - 3.5, r.max.y - 3.5);
    let c = Pos2::new(r.max.x - 8.0, r.max.y - 3.5);
    painter.add(egui::Shape::convex_polygon(
        vec![a, b, c],
        color,
        Stroke::NONE,
    ));
}
