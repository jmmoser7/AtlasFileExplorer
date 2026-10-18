//! Incoming preview behaviors retained through the canonical runtime.
use super::*;

#[test]
fn debug_growth_is_ephemeral_retreats_and_pins_the_current_mask() {
    let (mut h, id) = fixture("segment_growth_owner");
    let p = inject(&mut h, id);
    let before = h.app.image_segments.result.as_ref().unwrap().mask.clone();
    let scene = h.app.doc().scene.nodes.clone();
    if std::env::var("SLATE_SEGMENT_GROW").ok().as_deref() != Some("1") {
        h.app.segment_grow_step(p, 1.0);
        h.app.segment_grow_step(p + egui::vec2(1.5, 0.0), 1.0);
        assert_eq!(h.app.image_segments.result.as_ref().unwrap().mask, before);
    }
    h.app.image_segments.grow_debug = true;
    h.app.image_segments.grow_last = None;
    h.app.segment_grow_step(p, 1.0);
    h.app.segment_grow_step(p + egui::vec2(1.5, 0.0), 1.0);
    assert_ne!(h.app.image_segments.result.as_ref().unwrap().mask, before);
    h.app.segment_grow_step(p + egui::vec2(40.0, 0.0), 1.0);
    assert_eq!(h.app.image_segments.result.as_ref().unwrap().mask, before);
    h.app.segment_grow_step(p + egui::vec2(41.5, 0.0), 1.0);
    let grown = h.app.image_segments.result.as_ref().unwrap().mask.clone();
    h.app.pin_hover_selection();
    assert_eq!(
        h.app.image_selection_clip(id).as_deref(),
        Some(grown.as_slice())
    );
    assert_eq!(h.app.doc().scene.nodes, scene);
    h.app.image_segments.clear();
    assert!(h.app.image_segments.grow_past.is_empty());
    assert!(h.app.image_segments.grow_last.is_none());
}

#[test]
fn drag_ghost_keeps_crop_rotation_holes_and_canvas_scale() {
    for zoom in [0.5, 1.0, 2.0] {
        let (mut h, id) = fixture("segment_ghost_geometry");
        let node = h.app.doc_mut().scene.node_mut(id).unwrap();
        node.rotation_deg = 35.0;
        let NodeKind::Image(img) = &mut node.kind else {
            unreachable!()
        };
        img.crop = slate_doc::scene::Crop {
            x: 0.1,
            y: 0.1,
            w: 0.8,
            h: 0.8,
        };
        h.app.tab_mut().cam.z = zoom;
        h.frame();
        let p = h.app.install_stub_highlight(id);
        let source = h.app.doc().scene.node(id).unwrap().clone();
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(p));
            input.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
        });
        move_to(&mut h, p + egui::vec2(120.0, 20.0) * zoom);
        let drag = h.app.image_segments.drag.as_ref().unwrap();
        let delta = drag.delta;
        let home = drag.cutout.as_ref().unwrap().clone();
        let ghost = h.app.image_mask_drag_ghost().unwrap();
        assert_eq!(ghost.rect.x, home.rect.x + delta.x);
        assert_eq!(ghost.rect.y, home.rect.y + delta.y);
        assert!(
            (ghost.rect.w - 325.0).abs() < 0.01,
            "mask is 0.65 of 500 content units"
        );
        assert!(
            (ghost.rect.h - 280.0).abs() < 0.01,
            "mask is 0.7 of 400 content units"
        );
        let NodeKind::Image(source_img) = &source.kind else {
            unreachable!()
        };
        let basis = slate_doc::image_paint::image_content_rect(&source, source_img);
        let expected_center = slate_doc::geom::orbit_point(
            source.rect.center(),
            (basis.x + 0.475 * basis.w, basis.y + 0.5 * basis.h),
            source.rotation_deg,
        );
        let (home_x, home_y) = home.rect.center();
        assert!((home_x - expected_center.0).abs() < 0.01);
        assert!((home_y - expected_center.1).abs() < 0.01);
        assert_eq!(ghost.rotation_deg, source.rotation_deg);
        let NodeKind::Image(img) = &ghost.kind else {
            unreachable!()
        };
        assert!((img.crop.x - 0.15).abs() < 1e-5);
        assert!((img.crop.y - 0.15).abs() < 1e-5);
        assert!((img.crop.w - 0.65).abs() < 1e-5);
        assert!((img.crop.h - 0.7).abs() < 1e-5);
        let clip = ghost.clip.as_ref().unwrap();
        assert_eq!(clip.fill_rule, slate_doc::scene::PathFillRule::EvenOdd);
        assert_eq!(clip.extra.len(), 1, "the silhouette hole follows the ghost");
        assert_eq!(h.app.doc().scene.nodes, vec![source]);
    }
}
