use super::*;
use std::time::Instant;

#[test]
fn candidate_cycling_uses_the_selected_sam_contours() {
    let (mut h, id) = fixture("segment_candidate_cycle");
    inject(&mut h, id);
    let second = vec![vec![[0.1, 0.1], [0.3, 0.1], [0.3, 0.3], [0.1, 0.3]]];
    let result = h.app.image_segments.result.as_mut().unwrap();
    result.candidates = std::sync::Arc::new(vec![
        atlas_segment::Candidate {
            quality: 0.7,
            contours: contours(),
        },
        atlas_segment::Candidate {
            quality: 0.9,
            contours: second.clone(),
        },
    ]);
    h.app.pin_hover_selection();
    h.app.cycle_image_selection_candidate(true);
    let selected = h.app.image_segments.pinned.as_ref().unwrap();
    assert_eq!(selected.result.selected, 1);
    assert_eq!(*selected.result.mask, second);
}

#[test]
fn pending_prompt_refinement_keeps_its_hover_context_across_frames() {
    let (mut h, id) = fixture("segment_refine_delay");
    inject(&mut h, id);
    h.app.pin_hover_selection();
    let pinned = h.app.image_segments.pinned.as_ref().unwrap().clone();
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.image_segments.done = Some(rx);
    h.app.image_segments.serial = 11;
    h.app.image_segments.pending = Some(11);
    h.app.image_segments.hover = Some(Hover {
        tab: pinned.tab,
        host: pinned.host,
        gen: h.app.scene_gen,
        source_key: pinned.source_key.clone(),
        point_norm: [0.7, 0.7],
        prompts: pinned.prompts.clone(),
        screen: pinned.screen,
        since: Instant::now(),
    });

    h.frame();
    assert!(
        h.app.image_segments.hover.is_some(),
        "pending refinement keeps the worker's request context"
    );
    let refined = vec![vec![[0.1, 0.1], [0.4, 0.1], [0.4, 0.4], [0.1, 0.4]]];
    tx.send((
        11,
        Ok(atlas_segment::Prediction {
            candidates: vec![atlas_segment::Candidate {
                quality: 0.95,
                contours: refined.clone(),
            }],
        }),
    ))
    .unwrap();
    h.frame();
    let selection = h.app.image_segments.pinned.as_ref().unwrap();
    assert_eq!(*selection.result.mask, refined);
    assert!(h.app.image_segments.hover.is_none());
    assert!(h.app.doc().scene.node(id).is_some());
}

#[test]
fn lasso_delete_enter_and_escape_never_reach_board_delete() {
    let (mut h, id) = fixture("segment_lasso_keys");
    h.app.start_image_lasso(id);
    h.app.image_segments.lasso.as_mut().unwrap().points =
        vec![[0.1, 0.1], [0.8, 0.1], [0.8, 0.8], [0.1, 0.8]];
    let key = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    h.frame_with(|input| input.events.push(key(egui::Key::Delete)));
    assert_eq!(h.app.image_segments.lasso.as_ref().unwrap().points.len(), 3);
    assert!(h.app.doc().scene.node(id).is_some());
    h.frame_with(|input| input.events.push(key(egui::Key::Escape)));
    assert!(h.app.image_segments.lasso.is_none());
    assert!(h.app.doc().scene.node(id).is_some());

    h.app.start_image_lasso(id);
    h.app.image_segments.lasso.as_mut().unwrap().points = vec![[0.1, 0.1], [0.8, 0.1], [0.8, 0.8]];
    h.frame_with(|input| input.events.push(key(egui::Key::Enter)));
    assert!(h.app.image_segments.lasso.is_none());
    assert!(h.app.image_selection_clip(id).is_some());
    assert!(h.app.doc().scene.node(id).is_some());
}
