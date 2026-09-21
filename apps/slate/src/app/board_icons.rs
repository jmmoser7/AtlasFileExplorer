//! Minimal vector icons for the board create toolbar.
//!
//! Drawn with egui strokes so they stay crisp at any DPI and match the palette.

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// Icon glyphs for toolbar buttons and hover submenus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolIcon {
    Media,
    Image,
    Model,
    Video,
    Select,
    Pan,
    Frame,
    FrameLetter,
    FrameTabloid,
    FrameWide,
    FrameCustom,
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
    /// Board Portals family (dock icon + flyout).
    Portals,
    /// Web portal subtype (embedded page / local HTML dashboard).
    WebPortal,
    /// File Atlas lens portal subtype.
    AtlasLens,
    /// Trim (cutters, then click the dying piece).
    Trim,
    /// Join (open endpoints, or region union).
    Join,
    /// Split (cutters, then click — keep every piece).
    Split,
}

impl ToolIcon {
    pub fn label(self) -> &'static str {
        match self {
            ToolIcon::Media => "Media",
            ToolIcon::Image => "Image",
            ToolIcon::Model => "3D",
            ToolIcon::Video => "Video",
            ToolIcon::Select => "Select",
            ToolIcon::Pan => "Pan",
            ToolIcon::Frame => "Frame",
            ToolIcon::FrameLetter => "8.5 × 11",
            ToolIcon::FrameTabloid => "11 × 17",
            ToolIcon::FrameWide => "16:9",
            ToolIcon::FrameCustom => "Custom",
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
            ToolIcon::Portals => "Portals",
            ToolIcon::WebPortal => "Web portal",
            ToolIcon::AtlasLens => "File Atlas",
            ToolIcon::Trim => "Trim",
            ToolIcon::Join => "Join",
            ToolIcon::Split => "Split",
        }
    }
}

/// Paint through the shared shell icon catalog.
pub fn paint_tool_icon(painter: &egui::Painter, r: Rect, icon: ToolIcon, color: Color32) {
    use atlas_shell::icons::{self, Icon};
    let icon = match icon {
        ToolIcon::Media => Icon::Media,
        ToolIcon::Image => Icon::Image,
        ToolIcon::Model => Icon::Model,
        ToolIcon::Video => Icon::Video,
        ToolIcon::Select => Icon::Select,
        ToolIcon::Pan => Icon::Pan,
        ToolIcon::Frame => Icon::Frame,
        ToolIcon::FrameLetter => Icon::FrameLetter,
        ToolIcon::FrameTabloid => Icon::FrameTabloid,
        ToolIcon::FrameWide => Icon::FrameWide,
        ToolIcon::FrameCustom => Icon::FrameCustom,
        ToolIcon::Shapes => Icon::Shapes,
        ToolIcon::Rect => Icon::Rect,
        ToolIcon::Ellipse => Icon::Ellipse,
        ToolIcon::Curve => Icon::Pen,
        ToolIcon::Line => Icon::Line,
        ToolIcon::Arc => Icon::Arc,
        ToolIcon::Polyline => Icon::Polyline,
        ToolIcon::Bezier => Icon::Bezier,
        ToolIcon::Pen => Icon::Pen,
        ToolIcon::Text => Icon::Text,
        ToolIcon::Ruler => Icon::Ruler,
        ToolIcon::ChevronRight => Icon::ChevronRight,
        ToolIcon::ChevronLeft => Icon::ChevronLeft,
        ToolIcon::Grid => Icon::Grid,
        ToolIcon::Snap => Icon::Snap,
        ToolIcon::Align => Icon::Align,
        ToolIcon::Brush => Icon::Brush,
        ToolIcon::Eraser => Icon::Eraser,
        ToolIcon::Eyedropper => Icon::Eyedropper,
        ToolIcon::Sticky => Icon::Sticky,
        ToolIcon::DirectSelect => Icon::DirectSelect,
        ToolIcon::Colors => Icon::Colors,
        ToolIcon::Portals => Icon::Portals,
        ToolIcon::AtlasLens => Icon::AtlasLens,
        ToolIcon::WebPortal => Icon::WebPortal,
        ToolIcon::Trim => Icon::Trim,
        ToolIcon::Join => Icon::Join,
        ToolIcon::Split => Icon::Split,
    };
    icons::paint(painter, r, icon, color);
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
