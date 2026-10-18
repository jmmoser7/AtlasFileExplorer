//! Transcript geometry and theme checks through the real board paint path.
use super::*;

#[test]
fn agent_full_transcript_scales_without_rewrapping_between_raster_steps() {
    let mut h = crate::app::tests::Harness::new("full_chat_zoom");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    h.app.agents.local_turns.insert(
        id,
        vec![AgentTurn {
            role: "assistant".into(),
            text: "A stable conversation line with enough detail to wrap over multiple rows and preserve every glyph position while the board zooms.".into(),
            at: 0,
        }],
    );
    h.app.agents.output_epoch += 1;
    use crate::app::tests::{block_drift, frame_on, painted_block};
    let first = frame_on(&mut h, id, 1.0);
    let first =
        painted_block(&first, "A stable conversation line", 1.0).expect("the transcript paints");
    assert!(first.rows.len() > 1, "the reference transcript wraps");
    for zoom in [1.01, 1.02, 1.03, 0.3, 2.7] {
        let output = frame_on(&mut h, id, zoom);
        let current = painted_block(&output, "A stable conversation line", zoom)
            .expect("the transcript paints through zoom");
        assert!(
            block_drift(&first, &current, 0.02).is_none(),
            "zoom {zoom} changed transcript glyph positions"
        );
    }
}

#[test]
fn agent_transcript_repaints_theme_without_changing_world_geometry() {
    let mut h = crate::app::tests::Harness::new("agent_summary_raster");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    h.app.agents.local_turns.insert(
        id,
        vec![AgentTurn {
            role: "assistant".into(),
            text: "Readable in both themes".into(),
            at: 0,
        }],
    );
    h.app.agents.output_epoch += 1;
    use crate::app::tests::{block_drift, frame_on, painted_block};
    fn ink(shape: &egui::Shape) -> Option<Color32> {
        match shape {
            egui::Shape::Text(text)
                if text.galley.text().starts_with("Readable in both themes") =>
            {
                Some(text.override_text_color.unwrap_or(text.fallback_color))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(ink),
            _ => None,
        }
    }
    h.app.dark_mode = true;
    let dark = frame_on(&mut h, id, 1.0);
    let dark_ink = dark
        .shapes
        .iter()
        .find_map(|shape| ink(&shape.shape))
        .expect("dark transcript ink");
    h.app.dark_mode = false;
    let light = frame_on(&mut h, id, 1.0);
    let light_ink = light
        .shapes
        .iter()
        .find_map(|shape| ink(&shape.shape))
        .expect("light transcript ink");
    assert_ne!(
        dark_ink, light_ink,
        "theme changes repaint the public transcript"
    );
    let reference = painted_block(&light, "Readable in both themes", 1.0).unwrap();
    let zoom = frame_on(&mut h, id, 3.0);
    let zoom_ink = zoom
        .shapes
        .iter()
        .find_map(|shape| ink(&shape.shape))
        .expect("zoomed transcript ink");
    assert_eq!(zoom_ink, light_ink);
    let zoomed = painted_block(&zoom, "Readable in both themes", 3.0).unwrap();
    assert!(
        block_drift(&reference, &zoomed, 0.02).is_none(),
        "zoom keeps the same world-unit glyph positions"
    );
}
