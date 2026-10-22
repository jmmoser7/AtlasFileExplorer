//! Crop owns transient controls immediately, including small-image fallback.
use super::*;

#[test]
fn crop_hides_image_chrome_and_restores_it_after_exit() {
    for tiny in [false, true] {
        let (mut h, ids) = crop_board("crop_chrome_suppression", 1);
        if tiny {
            h.app
                .patch_nodes(&ids, |n| n.rect = WorldRect::new(0.0, 0.0, 24.0, 18.0));
        }
        h.frame();
        h.frame();
        assert!(!h.app.shape_properties.chrome_hits.is_empty());
        crop_via_corners_panel(&mut h);
        h.frame();
        assert_eq!(h.app.board_crop, Some(ids[0]));
        assert!(
            h.app.shape_properties.chrome_hits.is_empty(),
            "tiny={tiny}: hidden controls must not take input"
        );
        crop_key(&mut h, egui::Key::Escape, true);
        crop_key(&mut h, egui::Key::Escape, false);
        if h.app.board_crop.is_some() {
            crop_key(&mut h, egui::Key::Escape, true);
            crop_key(&mut h, egui::Key::Escape, false);
        }
        assert!(h.app.board_crop.is_none());
        for _ in 0..4 {
            h.frame();
        }
        assert!(!h.app.shape_properties.chrome_hits.is_empty());
    }
}
