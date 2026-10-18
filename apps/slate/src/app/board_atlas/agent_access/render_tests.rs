//! Retained real-frame capture for collaborator analysis overlays on File Atlas.
use super::super::*;
use crate::app::tests::{capture_frame, review_shot, FrameRaster, Harness};
use std::{collections::HashSet, fs, thread, time::Duration};

#[test]
#[ignore = "retains an actual rendered Atlas analysis overlay for visual review"]
fn discovered_file_analysis_highlight_keeps_camera_and_human_selection() {
    let mut h = Harness::new("atlas_agent_highlight_capture");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.tab_mut().cam.z = 0.65;
    h.app.tab_mut().cam.offset = egui::vec2(240.0, -130.0);

    let root = h.base.join("atlas-source");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("target.txt"), b"exact discovered Atlas target\n").unwrap();
    fs::write(root.join("other.txt"), b"human selection remains here\n").unwrap();
    let png = image::RgbaImage::from_pixel(48, 48, image::Rgba([70, 140, 210, 255]));
    png.save(root.join("preview.png")).unwrap();

    h.app.place_bound_atlas_at(egui::pos2(240.0, -130.0), &root);
    let portal = h.app.doc().scene.nodes[0].id;
    let mut raster = FrameRaster::new(1440, 900);
    let mut ready = false;
    for _ in 0..200 {
        let _ = capture_frame(&mut h, &mut raster, |_| {});
        let scan = h.app.agent_atlas_scan_state(portal);
        let tree_ready = h
            .app
            .atlas_lenses
            .views
            .get(&portal)
            .is_some_and(|view| view.tree.is_some());
        if scan.is_some_and(|(seen, done)| seen >= 3 && done) && tree_ready {
            ready = true;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        ready,
        "the existing Atlas scanner did not finish within the bounded wait"
    );

    let (target, other) = {
        let view = &h.app.atlas_lenses.views[&portal];
        let session = &h.app.atlas_lenses.sessions[&view.session_key];
        let target = session.rel_to_id["target.txt"];
        let other = session.rel_to_id["other.txt"];
        assert_ne!(target, other);
        assert_ne!(
            view.tree.as_ref().unwrap().file_pos[target as usize].place,
            atlas_core::tree::FilePlace::Hidden,
            "the exact target must be present in the painted Atlas map"
        );
        (target, other)
    };
    h.app.atlas_lenses.views.get_mut(&portal).unwrap().selection = HashSet::from([other]);
    let before = h.app.atlas_lenses.views[&portal].cam;
    let human_selection = h.app.atlas_lenses.views[&portal].selection.clone();
    assert!(h
        .app
        .agent_atlas_locator_known(portal, "target.txt")
        .unwrap());

    h.app
        .agent_atlas_highlight(portal, "target.txt", "capture-request", "Dot")
        .unwrap();
    assert_eq!(
        h.app.atlas_lenses.views[&portal]
            .agent_highlight_union
            .as_ref(),
        &HashSet::from([target])
    );
    let out = capture_frame(&mut h, &mut raster, |_| {});
    let after = h.app.atlas_lenses.views[&portal].cam;
    assert_eq!(
        after.offset, before.offset,
        "analysis must not pan the Atlas camera"
    );
    assert_eq!(after.z, before.z, "analysis must not zoom the Atlas camera");
    assert_eq!(h.app.atlas_lenses.views[&portal].selection, human_selection);

    review_shot(
        &mut h,
        &mut raster,
        out,
        "slate-agent-atlas-portal",
        "analysis-highlight",
    );
}
