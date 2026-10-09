//! Headless app smoke tests: tabs, save, and tag buckets.

use super::*;

#[test]
fn update_restart_checks_inactive_workbooks_and_pending_dialogs() {
    let mut h = Harness::new("update_restart");
    h.app.new_tab();
    h.app.tabs[0].dirty = true;
    h.app.new_tab();
    assert_ne!(h.app.active_tab, 0);
    assert!(h.app.update_close_blocked().is_some());
    h.app.tabs[0].dirty = false;
    assert!(h.app.update_close_blocked().is_none());
    let (tx, rx) = crossbeam_channel::unbounded();
    assert!(h.app.picker.adopt(rx));
    assert!(h.app.update_close_blocked().is_some());
    drop(tx);
    h.app.drain_pickers(&h.ctx);
    assert!(h.app.update_close_blocked().is_none());

    // Any slot of the app counts: the AI panel's, or the hosted Atlas
    // window's, which shares the one-dialog rule with Slate's own.
    let hosted = h.app.dialogs.other_window();
    let mut atlas_slot = hosted.picker::<()>();
    let (tx, rx) = crossbeam_channel::unbounded();
    assert!(atlas_slot.adopt(rx));
    assert!(h.app.update_close_blocked().is_some());
    assert!(
        !h.app.picker.adopt(crossbeam_channel::unbounded().1),
        "one dialog across Slate and the hosted Atlas window"
    );
    drop(tx);
    assert_eq!(atlas_slot.poll(&h.ctx), None);
    assert!(h.app.update_close_blocked().is_none());
}

#[test]
fn headless_preferences_and_recents_are_instance_local() {
    let mut first = Harness::new("prefs_first");
    first.app.settings.board_wire_routing = slate_doc::WireRouting::Orthogonal;
    first.app.settings.save();
    first.app.dock_pins.push("selection".into());
    first.app.save_chrome_prefs();
    first
        .app
        .recents
        .record(first.base.join("fixture.slate"), "Fixture");

    let second = Harness::new("prefs_second");
    assert_eq!(second.app.settings, settings::SlateSettings::default());
    assert_eq!(
        second.app.board_wire_routing,
        slate_doc::WireRouting::Bezier
    );
    assert!(second.app.dock_pins.is_empty());
    assert!(second.app.dock_icon_strips.is_empty());
    assert!(second.app.recents.entries.is_empty());
}

#[test]
fn empty_app_pumps_frames() {
    let mut h = Harness::new("empty");
    for _ in 0..5 {
        h.frame();
    }
}

/// Placed 3D models must be safe headless (no GL): the board paints the
/// thumbnail/placeholder path, unlocking is refused with a toast instead of
/// creating a live viewport, and the model camera stays journalable.
#[test]
fn model_nodes_survive_headless_frames() {
    let mut h = Harness::new("model3d");
    let model = h.base.join("tower.3dm");
    std::fs::write(&model, b"3D Geometry File Format fake").unwrap();
    let ids = h.app.add_paths(&[model]);
    assert_eq!(ids.len(), 1);

    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app
        .place_items_on_board(&[ids[0]], Pos2::new(200.0, 200.0));
    let node_id = h.app.doc().scene.nodes.last().unwrap().id;
    assert!(
        h.app.model_node_info(node_id).is_some(),
        "classified as model"
    );
    for _ in 0..5 {
        h.frame();
    }

    // No GL in the harness: unlock refuses politely.
    h.app.unlock_model(node_id);
    assert!(h.app.model3d.live.is_empty());
    assert!(!h.app.toasts.is_empty(), "user told why");
    for _ in 0..3 {
        h.frame();
    }

    // The camera pose is plain journaled node state.
    h.app.reset_model_camera(node_id);
    let cam_before = match &h.app.doc().scene.node(node_id).unwrap().kind {
        slate_doc::scene::NodeKind::Image(img) => img.model,
        _ => unreachable!(),
    };
    h.app.patch_nodes(&[node_id], |n| {
        if let slate_doc::scene::NodeKind::Image(img) = &mut n.kind {
            img.model.yaw = 1.0;
            img.model.distance = 25.0;
        }
    });
    h.app.board_undo();
    let cam_after_undo = match &h.app.doc().scene.node(node_id).unwrap().kind {
        slate_doc::scene::NodeKind::Image(img) => img.model,
        _ => unreachable!(),
    };
    assert_eq!(cam_before, cam_after_undo);

    // Deleting the node while (hypothetically) tracked must not wedge the
    // per-frame upkeep.
    h.app.delete_board_nodes(&[node_id]);
    for _ in 0..3 {
        h.frame();
    }
}

#[test]
fn mutual_exclusion_within_group() {
    let mut h = Harness::new("exclusive");
    let (big, small, red) = h.seed();
    let id = h.app.doc().items[1].id;
    // Re-tagging within the same group replaces; across groups combines.
    h.app.assign_tag(&[id], small);
    h.app.assign_tag(&[id], red);
    let item = h.app.doc().item(id).unwrap();
    assert_eq!(item.assignments.len(), 2);
    assert!(!h.app.doc().items_with_tag(big).contains(&id));
    assert!(h.app.doc().items_with_tag(small).contains(&id));
    assert!(h.app.doc().items_with_tag(red).contains(&id));
    h.frame();
}

#[test]
fn combination_buckets_drive_grid_sections() {
    let mut h = Harness::new("buckets");
    let (big, _small, red) = h.seed();
    let all: Vec<TagId> = vec![big, red];
    let buckets = h.app.doc().combination_buckets(&all);
    assert_eq!(buckets.get(&vec![big]).map(|v| v.len()), Some(1));
    assert_eq!(buckets.get(&vec![big, red]).map(|v| v.len()), Some(1));
    h.frame();
}

#[test]
fn tab_lifecycle_is_safe() {
    let mut h = Harness::new("tabs");
    h.seed();
    h.frame();
    h.app.new_tab();
    h.frame();
    assert_eq!(h.app.tabs.len(), 2);
    h.app.switch_tab(0);
    h.frame();
    // The seeded tab is dirty: closing asks, and does not drop the tab yet.
    h.app.close_tab(0);
    assert_eq!(h.app.tabs.len(), 2);
    assert!(h.app.unsaved_close.is_some());
    h.app
        .apply_unsaved_choice(&h.ctx, atlas_shell::widgets::ConfirmChoice::Cancel);
    assert_eq!(h.app.tabs.len(), 2);
    h.app.close_tab(0);
    h.app
        .apply_unsaved_choice(&h.ctx, atlas_shell::widgets::ConfirmChoice::Secondary);
    assert_eq!(h.app.tabs.len(), 1);
    assert!(h.app.unsaved_close.is_none());
    // The remaining blank tab closes without a prompt.
    h.app.close_tab(0);
    assert!(h.app.tabs.is_empty() || h.app.tabs.iter().all(|t| !t.dirty));
    h.frame();
}

#[test]
fn save_and_reopen_round_trip() {
    let mut h = Harness::new("saveload");
    let (big, _small, red) = h.seed();
    let path = h.base.join("work.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());
    assert!(!h.app.tab().dirty);
    assert_eq!(h.app.doc().name, "work");

    let mut h2 = Harness::new("saveload2");
    h2.app.open_doc_at(path);
    h2.frame();
    let doc = h2.app.doc();
    assert_eq!(doc.items.len(), 3);
    assert_eq!(doc.groups.len(), 2);
    assert_eq!(doc.items_with_tag(big).len(), 2);
    assert_eq!(doc.items_with_tag(red).len(), 1);
}

/// Twelve covered recents, then pointer-driven Home frames.
/// After warm-up the shelf must not stat or read cover bytes.
#[test]
fn home_cover_frames_do_not_touch_the_filesystem() {
    let mut h = Harness::new("home-budget");
    let dir = h.base.join("covers");
    std::fs::create_dir_all(&dir).unwrap();
    h.app.recents.entries = (0..12)
        .map(|i| {
            let cover = dir.join(format!("c{i}.png"));
            image::RgbaImage::from_pixel(32, 32, image::Rgba([i as u8, 40, 80, 255]))
                .save(&cover)
                .unwrap();
            atlas_shell::recent::RecentEntry {
                path: h.base.join(format!("book{i}.slate")),
                title: format!("Workbook {i}"),
                opened_at: 0,
                cover: Some(cover),
            }
        })
        .collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.app.home.texture_count() < 12 || !h.app.home.cover_io_settled() {
        h.frame_with(|input| {
            input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(200.0, 400.0)));
        });
        assert!(
            std::time::Instant::now() < deadline,
            "covers did not settle (textures={}, settled={})",
            h.app.home.texture_count(),
            h.app.home.cover_io_settled()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.frame();
    // First use creates the data dir once per process; keep it out of the count.
    let _ = atlas_core::index::data_dir();
    atlas_core::fs_probe::reset();
    let mut samples = Vec::with_capacity(30);
    for i in 0..30 {
        let t = std::time::Instant::now();
        let x = 80.0 + i as f32 * 17.0;
        h.frame_with(|input| {
            input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(x, 360.0 + i as f32)));
        });
        samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let probes = atlas_core::fs_probe::count();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = samples[samples.len() / 2];
    let p95 = samples[samples.len() * 95 / 100];
    eprintln!("home_frame_ms p50={p50:.2} p95={p95:.2} fs_probes={probes}");
    assert_eq!(probes, 0, "home frames probed the filesystem after warm-up");
}

/// Machine-local: time until `SlateApp::with_ctx` returns (headless `new`).
/// Not a CI assertion — fonts and the data dir dominate, and they vary by machine.
#[test]
#[ignore = "machine-local constructor timing"]
fn home_startup_constructor_time() {
    let t0 = std::time::Instant::now();
    let h = Harness::new("ctor-time");
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "slate with_ctx constructor_ms={ms:.1} profile=dev at_home={}",
        h.app.at_home
    );
}
