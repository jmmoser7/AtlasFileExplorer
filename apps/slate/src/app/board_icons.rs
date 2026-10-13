//! Minimal vector icons for the board create toolbar.
//!
//! Drawn with egui strokes so they stay crisp at any DPI and match the palette.

use eframe::egui::{self, Color32, Rect};

/// Icon glyphs for toolbar buttons and hover submenus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolIcon {
    Media,
    Image,
    Model,
    Video,
    /// Word, spreadsheets, CSV, and source files on the Media palette.
    TextDoc,
    Frame,
    FrameLetter,
    FrameTabloid,
    FrameWide,
    FrameCustom,
    Shapes,
    Rect,
    Ellipse,
    Polygon,
    Line,
    Arc,
    Polyline,
    Bezier,
    Pen,
    Text,
    Brush,
    Eraser,
    Smooth,
    Sticky,
    Colors,
    /// Board Portals family (dock icon + flyout).
    Portals,
    /// Agent portal tool.
    Agent,
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

/// Paint through the shared shell icon catalog.
pub fn paint_tool_icon(painter: &egui::Painter, r: Rect, icon: ToolIcon, color: Color32) {
    use atlas_shell::icons::{self, Icon};
    let icon = match icon {
        ToolIcon::Media => Icon::Media,
        ToolIcon::Image => Icon::Image,
        ToolIcon::Model => Icon::Model,
        ToolIcon::Video => Icon::Video,
        ToolIcon::TextDoc => Icon::TextDoc,
        ToolIcon::Frame => Icon::Frame,
        ToolIcon::FrameLetter => Icon::FrameLetter,
        ToolIcon::FrameTabloid => Icon::FrameTabloid,
        ToolIcon::FrameWide => Icon::FrameWide,
        ToolIcon::FrameCustom => Icon::FrameCustom,
        ToolIcon::Shapes => Icon::Shapes,
        ToolIcon::Rect => Icon::Rect,
        ToolIcon::Ellipse => Icon::Ellipse,
        ToolIcon::Polygon => Icon::Polygon,
        ToolIcon::Line => Icon::Line,
        ToolIcon::Arc => Icon::Arc,
        ToolIcon::Polyline => Icon::Polyline,
        ToolIcon::Bezier => Icon::Bezier,
        ToolIcon::Pen => Icon::Pen,
        ToolIcon::Text => Icon::Text,
        ToolIcon::Brush => Icon::Brush,
        ToolIcon::Eraser => Icon::Eraser,
        ToolIcon::Smooth => Icon::Smooth,
        ToolIcon::Sticky => Icon::Sticky,
        ToolIcon::Colors => Icon::Colors,
        ToolIcon::Portals => Icon::Portals,
        ToolIcon::Agent => Icon::Agent,
        ToolIcon::AtlasLens => Icon::AtlasLens,
        ToolIcon::WebPortal => Icon::WebPortal,
        ToolIcon::Trim => Icon::Trim,
        ToolIcon::Join => Icon::Join,
        ToolIcon::Split => Icon::Split,
    };
    icons::paint(painter, r, icon, color);
}
