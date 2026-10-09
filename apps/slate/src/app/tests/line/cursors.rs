//! Armed drawing-tool cursors.

use super::*;

/// Stated: every armed drawing tool shows a crosshair or a circle cursor.
/// The match is exhaustive, so a new tool cannot ship without a choice.
#[test]
fn every_armed_tool_names_its_cursor() {
    use board::BoardTool as T;
    use board_place::ArmedCursor as C;
    for tool in T::ALL {
        let want = match tool {
            T::Brush | T::Eraser | T::Smooth | T::Pen => C::TipCircle,
            T::Line
            | T::Arc
            | T::Polyline
            | T::BezierSpan
            | T::RectShape
            | T::Ellipse
            | T::Polygon
            | T::Trim
            | T::Split
            | T::Deck => C::Crosshair,
            T::Frame
            | T::Text
            | T::Sticky
            | T::AgentPortal
            | T::WebPortal
            | T::AtlasPortal
            | T::SlatePortal => C::Ghost,
            T::Select | T::Pan | T::DirectSelect | T::Eyedropper => C::Own,
        };
        assert_eq!(board_place::armed_cursor(tool), want, "{tool:?}");
    }
}

/// Stated: line, arc, polyline, Bézier, and the shape tools hover with a
/// crosshair; brush, eraser, smooth, and pen hide the arrow under their tip.
#[test]
fn armed_drawing_tools_hover_with_a_crosshair_or_tip_circle() {
    use board::BoardTool as T;
    let mut h = line_board("armed_cursors");
    h.frame();
    let c = h.app.canvas_rect.center();
    let hover = |h: &mut Harness, tool: T| {
        h.app.set_board_tool(tool);
        h.frame_with(pointer_to(c, false));
        h.frame_output(pointer_to(c + EVec2::new(4.0, 0.0), false))
            .platform_output
            .cursor_icon
    };
    for tool in [
        T::Line,
        T::Arc,
        T::Polyline,
        T::BezierSpan,
        T::RectShape,
        T::Ellipse,
        T::Polygon,
    ] {
        assert_eq!(hover(&mut h, tool), egui::CursorIcon::Crosshair, "{tool:?}");
    }
    for tool in [T::Brush, T::Eraser, T::Smooth, T::Pen] {
        assert_eq!(hover(&mut h, tool), egui::CursorIcon::None, "{tool:?}");
    }
}
