//! Review sheets for the pack chooser and Connections (PK1 / PK2).

use super::*;

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn pk2_chooser_and_connections() {
    let mut h = line_board("pk2_packs");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    atlas_ai::packs::set_health_override("comfy", atlas_ai::packs::PackHealth::Missing);
    atlas_ai::packs::set_health_override("codex", atlas_ai::packs::PackHealth::Missing);
    atlas_ai::packs::set_health_override("cursor", atlas_ai::packs::PackHealth::Missing);
    atlas_ai::packs::set_health_override("openai-image", atlas_ai::packs::PackHealth::Missing);
    h.app.ai.packs.refresh(None);
    for _ in 0..40 {
        h.app.ensure_agent_programs();
        if !h.app.ai.packs.probe_pending() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let mut raster = FrameRaster::new(1440, 900);
    h.app.place_agent_portal_at(Pos2::new(200.0, 80.0));
    h.app.tab_mut().cam.z = 1.0;
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "PK2", "chooser_no_key");

    h.app.ai.packs.key_entry = true;
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "PK2", "key_entry");

    h.app.ai.packs.key_entry = false;
    h.app.tab_mut().chrome.advanced_open = true;
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "PK1", "connections");
    atlas_ai::packs::clear_health_overrides();
}
