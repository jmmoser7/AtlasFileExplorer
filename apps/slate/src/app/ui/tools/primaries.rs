//! Slate primary item data; shell owns painting and layout.
use super::*;

pub(super) fn items(app: &SlateApp) -> [DockItem<'static>; 7] {
    let view = app.doc().view.active_view;
    let board = view == ViewKind::Board;
    let tool = app.board_tool;

    let shape_active = matches!(
        tool,
        BoardTool::RectShape
            | BoardTool::Ellipse
            | BoardTool::Line
            | BoardTool::Arc
            | BoardTool::Polyline
            | BoardTool::BezierSpan
            | BoardTool::Pen
            | BoardTool::Brush
            | BoardTool::Eraser
            | BoardTool::Smooth
    );

    [
        DockItem {
            id: "app.feedback.open",
            label: "Feedback",
            description: "Report a bug or request a feature.",
            icon: DockIcon::Feedback,
            kind: DockItemKind::Action,
            active: app.feedback.phase != atlas_shell::feedback::FeedbackPhase::Closed,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.media",
            label: "Media",
            description: "Place images and print media, 3D models, or video. PowerPoint renders as PDF pages.",
            icon: DockIcon::Custom(icon_media),
            kind: DockItemKind::Tool,
            active: false,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.frame",
            label: "Frame",
            description: "Place a slide frame — Letter, Tabloid, 16:9, or a custom size.",
            icon: frame_preset_icon(app.board_frame_preset),
            kind: DockItemKind::Tool,
            active: tool == BoardTool::Frame,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.portals",
            label: "Portals",
            description: "Drop a portal onto the board (File Atlas, Agent, Web, or a nested Slate board).",
            icon: DockIcon::Custom(icon_portals),
            kind: DockItemKind::Tool,
            active: matches!(
                tool,
                BoardTool::AgentPortal | BoardTool::WebPortal | BoardTool::AtlasPortal | BoardTool::SlatePortal
            ),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.shapes",
            label: "Shapes",
            description: "Rectangles, ellipses, and curve tools (line, pen, brush, eraser…).",
            icon: DockIcon::Custom(icon_shapes),
            kind: DockItemKind::Tool,
            active: shape_active,
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.text",
            label: "Text",
            description: "Place a text block or sticky note on the board.",
            icon: DockIcon::Custom(icon_text),
            kind: DockItemKind::Tool,
            active: matches!(tool, BoardTool::Text | BoardTool::Sticky),
            visible: board,
            gap_before: false,
        },
        DockItem {
            id: "tool.actions",
            label: "Actions",
            description: "Edit commands — Trim (Ctrl+T), Split (Ctrl+Shift+T), Join (Ctrl+J).",
            icon: DockIcon::Actions,
            kind: DockItemKind::Tool,
            active: matches!(tool, BoardTool::Trim | BoardTool::Split),
            visible: board,
            gap_before: true,
        },
    ]
}
