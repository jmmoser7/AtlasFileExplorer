use super::*;
use crate::app::tests::{frame_on, Harness};

#[test]
fn ordinary_video_uses_the_image_generator_editor_and_keeps_text_mode() {
    let mut h = Harness::new("video-image-agent-draft");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let id = place(&mut h, "input.mp4", 0.0);
    h.app.patch_nodes(&[id], |node| {
        node.rect = slate_doc::WorldRect::new(0.0, 0.0, 640.0, 300.0)
    });
    h.app.board_sel = [id].into_iter().collect();
    h.app
        .open_object_panel(crate::app::board_properties::Panel::Agent);
    frame_on(&mut h, id, 0.8);
    let settings = h
        .ctx
        .data_mut(|d| d.get_temp::<Rect>(Id::new("painted-generator-options-hit")));
    let play = h
        .ctx
        .data_mut(|d| d.get_temp::<Rect>(Id::new(("painted-generator-play-hit", id.0))));
    assert!(settings.is_some_and(|r| r.is_positive()));
    assert!(play.is_some_and(|r| r.is_positive()));
    assert!(h.app.node_is_video(id));
    assert!(!h.app.has_attached_generator_editor(id));
    let painter = h.ctx.layer_painter(egui::LayerId::background());
    let compact = h.app.video_image_draft_height(&painter, id, 640.0).unwrap();
    assert_eq!(
        compact,
        atlas_shell::selection_tools::CORNER_HEIGHT
            + 8.0
            + h.app.image_agent_body_height_for_width(&painter, id, 640.0)
    );
    let draft = h.app.agents.flow.drafts.get_mut(&id).unwrap();
    draft.prompt = "Describe the current frame".into();
    draft.kind = SpawnKind::Text;
    frame_on(&mut h, id, 0.8);
    assert!(h
        .app
        .video_image_draft_height(&painter, id, 640.0)
        .is_none());
    assert_eq!(
        h.app.agents.flow.drafts[&id].prompt,
        "Describe the current frame"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}
