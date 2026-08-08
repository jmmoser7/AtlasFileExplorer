//! Headless Slate stability tests: drive the real frame loop through a plain
//! `egui::Context` (no eframe window) with the real thumbnail pool, exercising
//! the tag model, both presentations, tabs, and workbook save/load.

use super::lens::LensStatus;
use super::*;
use eframe::egui::{Pos2, Rect as ERect, Vec2 as EVec2};
use slate_doc::ViewKind;

struct Harness {
    ctx: egui::Context,
    app: SlateApp,
    base: PathBuf,
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

impl Harness {
    fn new(tag: &str) -> Harness {
        let base = std::env::temp_dir().join(format!(
            "slate_test_{}_{}_{}",
            tag,
            std::process::id(),
            now_nanos()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let ctx = egui::Context::default();
        let mut app = SlateApp::with_ctx(&ctx, None);
        // Tool kits come from the built-in set only: a `.slatekit` file sitting
        // in the developer's own kit folder must not change a test result.
        app.kits = kits::KitState::builtin_only();
        Harness { ctx, app, base }
    }

    fn frame(&mut self) {
        self.frame_with(|_| {});
    }

    /// One frame with real input, which is the only way to test what the board
    /// and a focused page each do with the same wheel notch or keystroke.
    fn frame_with(&mut self, prepare: impl FnOnce(&mut egui::RawInput)) {
        let mut input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
            ..Default::default()
        };
        prepare(&mut input);
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let _ = ctx.run(input, |c| app.update_app(c));
        assert_invariants(&self.app);
    }

    /// A workbook with two facet groups, three tags, and three linked files
    /// (one uncategorized, one single-tagged, one cross-group tagged).
    fn seed(&mut self) -> (TagId, TagId, TagId) {
        self.app.leave_home();
        self.app.ensure_work_tab();
        let files: Vec<PathBuf> = (0..3)
            .map(|i| {
                let p = self.base.join(format!("file{i}.png"));
                std::fs::write(&p, b"png-ish").unwrap();
                p
            })
            .collect();
        let ids = self.app.add_paths(&files);
        assert_eq!(ids.len(), 3);

        let size = self.app.doc_mut().add_group("Size");
        let color = self.app.doc_mut().add_group("Color");
        let big = self.app.doc_mut().add_tag(size, "Big", [1, 2, 3]).unwrap();
        let small = self
            .app
            .doc_mut()
            .add_tag(size, "Small", [4, 5, 6])
            .unwrap();
        let red = self.app.doc_mut().add_tag(color, "Red", [7, 8, 9]).unwrap();

        self.app.assign_tag(&[ids[1]], big);
        self.app.assign_tag(&[ids[2]], big);
        self.app.assign_tag(&[ids[2]], red);
        (big, small, red)
    }
}

fn assert_invariants(app: &SlateApp) {
    if app.at_home && app.tabs.is_empty() {
        return;
    }
    assert!(
        !app.tabs.is_empty(),
        "work tabs must exist when not at home"
    );
    assert!(app.active_tab < app.tabs.len(), "active tab in bounds");
    for id in &app.selection {
        assert!(
            app.doc().item(*id).is_some(),
            "selection must reference live items"
        );
    }
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
fn grid_and_venn_views_render_seeded_doc() {
    let mut h = Harness::new("views");
    h.seed();
    for _ in 0..5 {
        h.frame();
    }
    h.app.doc_mut().view.active_view = ViewKind::Venn;
    for _ in 0..5 {
        h.frame();
    }
    // One uncategorized item stays out of the Venn circles.
    assert_eq!(h.app.doc().uncategorized_items().len(), 1);
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
    // The seeded tab is dirty: closing must be refused.
    h.app.close_tab(0);
    assert_eq!(h.app.tabs.len(), 2);
    // The blank tab closes fine.
    h.app.close_tab(1);
    assert_eq!(h.app.tabs.len(), 1);
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

// ----- board (authored canvas) ---------------------------------------------------

use slate_doc::scene::{FrameNode, NodeKind, Rgba, WorldRect};

impl Harness {
    /// A frame at (0,0)-(800,450) tagged with the given tag, via the same
    /// journaled path the UI uses.
    fn seed_frame(&mut self, tag: Option<TagId>) -> NodeId {
        let node = self.app.doc_mut().scene.build_node(
            WorldRect::new(0.0, 0.0, 800.0, 450.0),
            NodeKind::Frame(FrameNode {
                title: "Slide 1".into(),
                order: 0,
                fill: Rgba::WHITE,
                assignments: std::collections::BTreeMap::new(),
            }),
        );
        let id = self.app.add_nodes(vec![node])[0];
        if let Some(tag) = tag {
            let group = self.app.doc().tag(tag).unwrap().0.id;
            self.app.patch_nodes(&[id], |n| {
                if let NodeKind::Frame(f) = &mut n.kind {
                    f.assignments.insert(group, tag);
                }
            });
        }
        id
    }
}

#[test]
fn board_view_renders_and_survives_frames() {
    let mut h = Harness::new("board_render");
    h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.seed_frame(None);
    let items: Vec<ItemId> = h.app.doc().items.iter().map(|i| i.id).collect();
    h.app
        .place_items_on_board(&items, eframe::egui::Pos2::new(100.0, 100.0));
    for _ in 0..5 {
        h.frame();
    }
    // 1 frame + 3 images.
    assert_eq!(h.app.doc().scene.nodes.len(), 4);
}

#[test]
fn drop_on_tagged_frame_inherits_tag() {
    let mut h = Harness::new("board_inherit");
    let (big, _small, _red) = h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let _frame = h.seed_frame(Some(big));
    // The uncategorized item (index 0) dropped inside the frame.
    let item = h.app.doc().items[0].id;
    assert!(h.app.doc().item(item).unwrap().assignments.is_empty());
    h.app
        .place_items_on_board(&[item], eframe::egui::Pos2::new(400.0, 225.0));
    assert!(h.app.doc().items_with_tag(big).contains(&item));
    // Dropped outside a frame: stays untagged.
    let mut h2 = Harness::new("board_inherit2");
    let (big2, ..) = h2.seed();
    h2.seed_frame(Some(big2));
    let item2 = h2.app.doc().items[0].id;
    h2.app
        .place_items_on_board(&[item2], eframe::egui::Pos2::new(5000.0, 5000.0));
    assert!(!h2.app.doc().items_with_tag(big2).contains(&item2));
}

#[test]
fn board_undo_redo_round_trip() {
    let mut h = Harness::new("board_undo");
    h.seed();
    let frame = h.seed_frame(None);
    // Patch the frame's rect via the journaled path.
    h.app
        .patch_nodes(&[frame], |n| n.rect = n.rect.translated(100.0, 0.0));
    assert_eq!(h.app.doc().scene.node(frame).unwrap().rect.x, 100.0);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(frame).unwrap().rect.x, 0.0);
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.node(frame).unwrap().rect.x, 100.0);
    // Undo twice removes the frame entirely (creation was journaled too).
    h.app.board_undo();
    h.app.board_undo();
    assert!(h.app.doc().scene.node(frame).is_none());
    h.frame();
}

#[test]
fn duplicate_and_delete_board_nodes() {
    let mut h = Harness::new("board_dup");
    h.seed();
    let frame = h.seed_frame(None);
    let dups = h.app.duplicate_board_nodes(&[frame], 24.0, 24.0);
    assert_eq!(dups.len(), 1);
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    let dup_rect = h.app.doc().scene.node(dups[0]).unwrap().rect;
    assert_eq!(dup_rect.x, 24.0);
    // Selection moved to the copy.
    assert!(h.app.board_sel.contains(&dups[0]));
    h.app.delete_board_nodes(&dups);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(h.app.board_sel.is_empty());
    // Undo the delete brings it back.
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    h.frame();
}

#[test]
fn scene_persists_through_save_and_reload() {
    let mut h = Harness::new("board_persist");
    h.seed();
    h.seed_frame(None);
    let items: Vec<ItemId> = h.app.doc().items.iter().map(|i| i.id).collect();
    h.app
        .place_items_on_board(&items, eframe::egui::Pos2::new(200.0, 200.0));
    let path = h.base.join("board.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());

    let mut h2 = Harness::new("board_persist2");
    h2.app.open_doc_at(path);
    assert_eq!(h2.app.doc().scene.nodes.len(), 4);
    assert_eq!(h2.app.doc().scene.frames_in_order().len(), 1);
    h2.frame();
}

#[test]
fn presentation_mode_enters_and_exits() {
    let mut h = Harness::new("board_present");
    h.seed();
    // No frames: refuses to present.
    h.app.start_present(None);
    assert!(h.app.presenting.is_none());
    h.seed_frame(None);
    h.app.start_present(None);
    assert!(h.app.presenting.is_some());
    for _ in 0..3 {
        h.frame();
    }
    h.app.stop_present();
    assert!(h.app.presenting.is_none());
    h.frame();
}

#[test]
fn export_artifact_writes_html() {
    let mut h = Harness::new("board_export");
    h.seed();
    h.seed_frame(None);
    let items: Vec<ItemId> = h.app.doc().items.iter().map(|i| i.id).collect();
    h.app
        .place_items_on_board(&items, eframe::egui::Pos2::new(200.0, 200.0));
    let out = h.base.join("export");
    h.app.do_export(out.clone());
    let deck = out.join("Untitled-slides").join("index.html");
    assert!(deck.exists(), "expected {deck:?} to exist");
    let html = std::fs::read_to_string(deck).unwrap();
    assert!(html.contains("<section"));
    h.frame();
}

// ----- media kinds & workbook-in-workbook guards ---------------------------------

#[test]
fn slate_files_never_become_items() {
    let mut h = Harness::new("wb_guard");
    // A real workbook file on disk plus a plain image.
    let wb_path = h.base.join("other.slate");
    SlateDoc::new("Other").save_to(&wb_path).unwrap();
    let img_path = h.base.join("pic.png");
    std::fs::write(&img_path, b"png-ish").unwrap();

    let ids = h.app.add_paths(&[wb_path.clone(), img_path]);
    // Only the image became an item; the workbook was queued to open.
    assert_eq!(ids.len(), 1);
    assert_eq!(h.app.doc().items.len(), 1);
    assert_eq!(h.app.pending_workbooks, vec![wb_path.clone()]);

    // The frame pump opens it as a tab.
    h.frame();
    assert!(h.app.pending_workbooks.is_empty());
    assert_eq!(h.app.tabs.len(), 2);
    assert_eq!(h.app.tab().doc.name, "Other");
    assert_eq!(h.app.tab().path.as_deref(), Some(wb_path.as_path()));
}

#[test]
fn opening_same_workbook_twice_focuses_existing_tab() {
    let mut h = Harness::new("wb_dedupe");
    let path = h.base.join("one.slate");
    SlateDoc::new("One").save_to(&path).unwrap();

    h.app.open_doc_at(path.clone());
    assert_eq!(h.app.tabs.len(), 1); // blank tab was reused
    h.app.new_tab();
    assert_eq!(h.app.active_tab, 1);

    // Re-opening switches back to the existing tab instead of loading twice.
    h.app.open_doc_at(path);
    assert_eq!(h.app.tabs.len(), 2);
    assert_eq!(h.app.active_tab, 0);
    h.frame();
}

#[test]
fn workbook_cannot_load_into_itself() {
    let mut h = Harness::new("wb_self");
    h.seed();
    let path = h.base.join("self.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());
    let items_before = h.app.doc().items.len();

    // "Add" the workbook's own file to itself (drop / add-files flow).
    let ids = h.app.add_paths(&[path]);
    h.frame();
    // No self-item, no second tab — dedupe lands on the same tab.
    assert!(ids.is_empty());
    assert_eq!(h.app.doc().items.len(), items_before);
    assert_eq!(h.app.tabs.len(), 1);
}

#[test]
fn video_trim_settings_survive_save_and_reload() {
    use slate_doc::scene::VideoOpts;

    let mut h = Harness::new("video_trim");
    let clip = h.base.join("clip.mp4");
    std::fs::write(&clip, b"not really mp4").unwrap();
    let ids = h.app.add_paths(&[clip]);
    h.app
        .place_items_on_board(&ids, eframe::egui::Pos2::new(100.0, 100.0));
    let node_id = h.app.doc().scene.nodes[0].id;
    h.app.patch_nodes(&[node_id], |n| {
        if let NodeKind::Image(i) = &mut n.kind {
            i.video = VideoOpts {
                start: 3.0,
                end: Some(11.0),
                controls: true,
                ..VideoOpts::default()
            };
        }
    });

    let path = h.base.join("video.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());

    let mut h2 = Harness::new("video_trim2");
    h2.app.open_doc_at(path);
    let NodeKind::Image(img) = &h2.app.doc().scene.nodes[0].kind else {
        panic!("expected image node");
    };
    assert_eq!(img.video.start, 3.0);
    assert_eq!(img.video.end, Some(11.0));
    assert!(img.video.controls);
    h2.frame();
}

#[test]
fn export_renders_kind_specific_cards() {
    let mut h = Harness::new("kind_cards");
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.seed_frame(None);
    let notes = h.base.join("notes.md");
    std::fs::write(&notes, "# Title\nbody text").unwrap();
    let clip = h.base.join("clip.mp4");
    std::fs::write(&clip, b"fake").unwrap();
    let report = h.base.join("report.pdf");
    std::fs::write(&report, b"%PDF fake").unwrap();

    let ids = h.app.add_paths(&[notes, clip, report]);
    assert_eq!(ids.len(), 3);
    // Drop at the frame center: the multi-item grid is centered on the drop
    // point, so this keeps all three cards inside the exported frame.
    h.app
        .place_items_on_board(&ids, eframe::egui::Pos2::new(400.0, 225.0));
    for _ in 0..3 {
        h.frame(); // board paints snippet cards / badges without panicking
    }

    let out = h.base.join("export");
    h.app.do_export(out.clone());
    let html = std::fs::read_to_string(out.join("Untitled-slides").join("index.html")).unwrap();
    assert!(html.contains("class=\"textcard\""), "text snippet card");
    assert!(html.contains("# Title"), "snippet content");
    assert!(html.contains("<video"), "web-safe video element");
    assert!(
        html.contains("<span class=\"badge\">PDF</span>"),
        "pdf card badge"
    );
    h.frame();
}

// ----- lazy full-resolution previews ----------------------------------------------

#[test]
fn full_res_preview_upgrades_and_evicts() {
    let mut h = Harness::new("preview");
    // Tests must not depend on the developer's persisted settings file.
    h.app.settings.preview = settings::PreviewSettings::default();
    let p = h.base.join("real.png");
    image::RgbaImage::from_pixel(600, 400, image::Rgba([10, 200, 30, 255]))
        .save(&p)
        .unwrap();
    let ids = h.app.add_paths(&[p]);
    let key = h.app.doc().item(ids[0]).unwrap().cache_key.clone();

    // Below the upgrade threshold nothing is queued.
    let _ = h.app.item_texture(ids[0], 100.0);
    assert!(h.app.preview_slots.is_empty());

    // A zoomed-in paint queues one decode; frames drain it into the cache.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let _ = h.app.item_texture(ids[0], 800.0);
        h.frame();
        if h.app.preview_cache.contains_key(&key) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "preview never arrived"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let entry = h.app.preview_cache.get(&key).unwrap();
    // A 600×400 source decoded toward tier 1024 is exhausted: it satisfies
    // every future zoom level without re-decoding.
    assert_eq!(entry.px, preview::PX_EXACT);
    assert_eq!(entry.bytes, 600 * 400 * 4);
    let tex = h.app.item_texture(ids[0], 800.0).expect("preview texture");
    assert_eq!(tex.size(), [600, 400], "preview replaced the 192px thumb");
    assert_eq!(h.app.preview_cache_stats(), (1, 600 * 400 * 4));

    // Shrinking the budget evicts entries once they age past the two-frame
    // protection window (the default zoomed-out grid never touches them).
    h.app.settings.preview.budget_mb = 0;
    for _ in 0..3 {
        h.frame();
    }
    assert!(
        h.app.preview_cache.is_empty(),
        "over-budget preview evicted"
    );
}

/// Seeded "png-ish" bytes decode as neither thumbnail nor preview: the key
/// must land in the failed set and never be re-requested.
#[test]
fn undecodable_sources_fail_once_and_stop_asking() {
    let mut h = Harness::new("preview_fail");
    h.app.settings.preview = settings::PreviewSettings::default();
    h.seed();
    let item = h.app.doc().items[0].id;
    let key = h.app.doc().item(item).unwrap().cache_key.clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let _ = h.app.item_texture(item, 800.0);
        h.frame();
        if h.app.preview_failed.contains(&key) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "failure never recorded"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // Failed keys are never re-requested…
    let _ = h.app.item_texture(item, 800.0);
    assert!(h.app.preview_slots.is_empty());
    // …until the cache is cleared (environment may have changed).
    h.app.clear_preview_cache();
    assert!(h.app.preview_failed.is_empty());
}

#[test]
fn remove_group_strips_assignments_via_menu_path() {
    let mut h = Harness::new("rmgroup");
    let (_big, _small, red) = h.seed();
    let group = h.app.doc().groups[0].id; // Size
    h.app.doc_mut().remove_group(group);
    for item in &h.app.doc().items {
        assert!(!item.assignments.contains_key(&group));
    }
    // Red assignment (other group) survives.
    assert_eq!(h.app.doc().items_with_tag(red).len(), 1);
    h.frame();
}

/// Lens view: empty state, then analysis on a minimal Cargo workspace.
#[test]
fn lens_view_pumps_without_panic() {
    let mut h = Harness::new("lens");
    h.app.doc_mut().view.active_view = ViewKind::Lens;

    for _ in 0..5 {
        h.frame();
    }
    assert_eq!(h.app.lens.status, LensStatus::Idle);

    let root = h.base.join("mini-crate");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"mini\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();

    h.app.doc_mut().lens_root = Some(root);
    h.app.lens_rescan();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        h.frame();
        match &h.app.lens.status {
            LensStatus::Ready => break,
            LensStatus::Error(msg) => panic!("lens analysis failed: {msg}"),
            LensStatus::Analyzing | LensStatus::Idle => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "lens analysis timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }

    assert!(h.app.lens.graph.is_some());
    for _ in 0..3 {
        h.frame();
    }
}

#[test]
fn path_node_add_undo_via_journal() {
    use slate_doc::scene::{PathData, PathSeg, ShapeKind, ShapeNode};
    let mut h = Harness::new("path_journal");
    h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: Some(PathData {
                start: [0.0, 0.5],
                segs: vec![PathSeg::Line { to: [1.0, 0.5] }],
                closed: false,
            }),
        }),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().scene.node(id).is_none());
    h.frame();
}

// ---------- keymap wave 2b ----------

/// A horizontal open path stroke at (x, y)..(x+100, y).
fn add_stroke(app: &mut SlateApp, x: f32, y: f32) -> NodeId {
    use slate_doc::scene::{PathData, PathSeg, ShapeKind, ShapeNode};
    let rect = slate_doc::scene::WorldRect::new(x, y, 100.0, 1.0);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: Some(PathData {
                start: [0.0, 0.5],
                segs: vec![PathSeg::Line { to: [1.0, 0.5] }],
                closed: false,
            }),
        }),
    );
    let ids = app.add_nodes(vec![node]);
    ids[0]
}

fn add_rect(app: &mut SlateApp, x: f32, y: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let rect = slate_doc::scene::WorldRect::new(x, y, 80.0, 60.0);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(slate_doc::scene::Rgba::WHITE),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: None,
        }),
    );
    let ids = app.add_nodes(vec![node]);
    ids[0]
}

/// Eraser: three touched strokes are removed as one journal group — one
/// undo restores all of them.
#[test]
fn eraser_release_is_one_undo_group() {
    let mut h = Harness::new("eraser");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let ids: Vec<NodeId> = (0..3)
        .map(|i| add_stroke(&mut h.app, 0.0, i as f32 * 50.0))
        .collect();

    // The eraser circle over the middle of the first stroke hits it.
    let hits = h.app.eraser_hits_at(Pos2::new(50.0, 0.5));
    assert_eq!(hits, vec![ids[0]]);

    h.app.finish_erase(ids.clone());
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 3, "one undo restores all");
    h.frame();
}

/// Hidden and locked semantics: hit-testing, select-all, and the escape
/// hatches (show all / unlock all / force pick).
#[test]
fn hidden_and_locked_leave_selection_paths() {
    let mut h = Harness::new("flags");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 200.0, 0.0);

    h.app.board_sel = [a].into_iter().collect();
    assert_eq!(h.app.cmd_hide_selection(), 1);
    assert!(h.app.board_sel.is_empty(), "hide clears the selection");
    assert!(board_path::board_pick_node(&h.app.doc().scene, 40.0, 30.0, 1.0).is_none());

    h.app.board_sel = [b].into_iter().collect();
    assert_eq!(h.app.cmd_lock_selection(), 1);
    assert!(board_path::board_pick_node(&h.app.doc().scene, 240.0, 30.0, 1.0).is_none());
    // The Ctrl+Shift+click escape hatch still reaches it.
    assert_eq!(
        board_path::board_pick_node_ex(&h.app.doc().scene, 240.0, 30.0, 1.0, true),
        Some(b)
    );

    assert_eq!(h.app.hidden_locked_counts(), (1, 1));
    assert_eq!(h.app.cmd_show_all_hidden(), 1);
    assert_eq!(h.app.cmd_unlock_all(), 1);
    assert_eq!(h.app.hidden_locked_counts(), (0, 0));
    // Both journaled: two undos restore the flags.
    h.app.board_undo();
    h.app.board_undo();
    assert_eq!(h.app.hidden_locked_counts(), (1, 1));
    h.frame();
}

/// Deleting a node degrades wires anchored to it to Free ends in the same
/// undo group; undo restores the anchor.
#[test]
fn delete_degrades_connector_ends_to_free() {
    use slate_doc::scene::{ConnectorEnd, NodeKind, Side};
    let mut h = Harness::new("wire_degrade");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 300.0, 0.0);
    let wire = h
        .app
        .add_connector(
            ConnectorEnd::Anchored {
                node: a,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: b,
                side: Side::Left,
                t: 0.5,
            },
        )
        .expect("wire added");

    h.app.delete_board_nodes(&[b]);
    let conn = match &h.app.doc().scene.node(wire).unwrap().kind {
        NodeKind::Connector(c) => c.clone(),
        _ => panic!("connector"),
    };
    assert!(matches!(conn.a, ConnectorEnd::Anchored { node, .. } if node == a));
    match conn.b {
        ConnectorEnd::Free { point } => assert_eq!(point, [300.0, 30.0]),
        other => panic!("must degrade to Free, got {other:?}"),
    }

    h.app.board_undo();
    let conn = match &h.app.doc().scene.node(wire).unwrap().kind {
        NodeKind::Connector(c) => c.clone(),
        _ => panic!("connector"),
    };
    assert!(matches!(conn.b, ConnectorEnd::Anchored { node, .. } if node == b));
    h.frame();
}

/// Ctrl+J over two open paths joins nearest endpoints into one node that
/// keeps the first path's style — one Remove+Add group (one undo).
#[test]
fn join_two_open_paths_keeps_first_style() {
    use slate_doc::scene::{NodeKind, ShapeKind};
    let mut h = Harness::new("join");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let a = add_stroke(&mut h.app, 0.0, 0.0);
    let b = add_stroke(&mut h.app, 150.0, 0.0);
    h.app.board_sel = [a, b].into_iter().collect();

    assert!(h.app.cmd_join());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let joined = &h.app.doc().scene.nodes[0];
    match &joined.kind {
        NodeKind::Shape(s) => {
            assert_eq!(s.shape, ShapeKind::Path);
            let p = s.path.as_ref().unwrap();
            assert!(!p.closed);
            assert_eq!(p.point_count(), 4, "two 2-anchor paths bridged");
        }
        _ => panic!("joined node must be a path shape"),
    }
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2, "one undo splits back");
    h.frame();
}

/// Sticky Tab-spawn: the sibling lands one note-width + gap to the right,
/// keeps the fill preset, and takes the caret.
#[test]
fn sticky_tab_spawn_offsets_right() {
    use slate_doc::scene::NodeKind;
    let mut h = Harness::new("sticky");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;

    h.app.place_sticky_at(Pos2::new(0.0, 0.0));
    let first = *h.app.board_sel.iter().next().expect("sticky selected");
    assert!(h.app.text_edit.as_ref().is_some_and(|(id, _)| *id == first));
    let r0 = h.app.doc().scene.node(first).unwrap().rect;
    assert_eq!((r0.w, r0.h), (200.0, 200.0));

    h.app.spawn_adjacent_sticky(first, 1.0);
    let second = *h.app.board_sel.iter().next().expect("sibling selected");
    assert_ne!(second, first);
    let n = h.app.doc().scene.node(second).unwrap();
    assert_eq!(n.rect.x, r0.x + r0.w + 24.0);
    assert_eq!(n.rect.y, r0.y);
    match &n.kind {
        NodeKind::Text(t) => assert_eq!(t.fill, Some(board_color::STICKY_FILL)),
        _ => panic!("sticky is a text node"),
    }
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(id, _)| *id == second));
    h.frame();
}

// ---------- Line tool golden paths (contracts/line.md GP1–GP6) ----------

fn line_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Line);
    h
}

fn assert_endpoints(app: &SlateApp, a: Pos2, b: Pos2) -> NodeId {
    assert_eq!(app.doc().scene.nodes.len(), 1, "exactly one node committed");
    let node = &app.doc().scene.nodes[0];
    let (pa, pb) = board_line::line_endpoints(node).expect("a simple line node");
    for (got, want) in [(pa, a), (pb, b)] {
        assert!(
            (got - want).length() < 0.05,
            "endpoint {got:?} != expected {want:?}"
        );
    }
    node.id
}

/// GP1 — click grammar: L · click (100,100) · move · click (200,100) →
/// one parametric line in the fg color, tool back to Select, one undo.
#[test]
fn line_gp1_click_grammar() {
    let mut h = line_board("line_gp1");
    let started = h.app.line_begin(Pos2::new(100.0, 100.0), false);
    assert!(started, "first press places the first point");
    h.app.line_release(Pos2::new(100.0, 100.0), true, false);
    assert!(h.app.line_draft.is_some(), "click keeps the draft live");
    assert!(h.app.doc().scene.nodes.is_empty());

    h.app.line_hover(Pos2::new(200.0, 100.0), false);
    assert!(!h.app.line_begin(Pos2::new(200.0, 100.0), false));
    h.app.line_release(Pos2::new(200.0, 100.0), false, false);

    let id = assert_endpoints(&h.app, Pos2::new(100.0, 100.0), Pos2::new(200.0, 100.0));
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot (D02)");
    assert!(h.app.line_draft.is_none());
    match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::scene::NodeKind::Shape(s) => {
            assert_eq!(s.stroke.color, h.app.board_colors.fg, "stroke = fg (D11)");
            assert_eq!(
                s.stroke.cap,
                slate_doc::scene::StrokeCap::Square,
                "draft curves use square end caps (D11)"
            );
            assert!(s.fill.is_none());
        }
        _ => panic!("line commits as a shape node"),
    }
    h.app.board_undo();
    assert!(
        h.app.doc().scene.nodes.is_empty(),
        "one gesture = one undo (D11)"
    );
    h.frame();
}

/// GP2 — drag grammar: press (0,0) · drag · release (50,80) → identical
/// node shape to GP1's grammar.
#[test]
fn line_gp2_drag_grammar() {
    let mut h = line_board("line_gp2");
    let started = h.app.line_begin(Pos2::new(0.0, 0.0), false);
    h.app.line_hover(Pos2::new(50.0, 80.0), false);
    h.app.line_release(Pos2::new(50.0, 80.0), started, false);
    assert_endpoints(&h.app, Pos2::new(0.0, 0.0), Pos2::new(50.0, 80.0));
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    h.frame();
}

/// GP3 — ortho one-shot: F8 off, first point (0,0), Shift held, cursor at
/// (97,4) → the end point projects onto the nearest 45° axis: (97,0)
/// (DominantOrtho projection, constraints spec §1).
#[test]
fn line_gp3_shift_inverts_ortho() {
    let mut h = line_board("line_gp3");
    assert!(!h.app.board_ortho, "F8 persistent state off");
    h.app.line_begin(Pos2::new(0.0, 0.0), false);
    h.app.line_release(Pos2::new(0.0, 0.0), true, false);
    h.app.line_hover(Pos2::new(97.0, 4.0), true);
    h.app.line_begin(Pos2::new(97.0, 4.0), true);
    h.app.line_release(Pos2::new(97.0, 4.0), false, true);
    assert_endpoints(&h.app, Pos2::new(0.0, 0.0), Pos2::new(97.0, 0.0));
    h.frame();
}

/// GP4 — Tab direction lock + typed length: first point (0,0), cursor
/// (30,40), Tab, move anywhere, type 100, Enter → end (60,80).
#[test]
fn line_gp4_tab_lock_and_numeric_entry() {
    let mut h = line_board("line_gp4");
    h.app.line_begin(Pos2::new(0.0, 0.0), false);
    h.app.line_release(Pos2::new(0.0, 0.0), true, false);
    h.app.line_hover(Pos2::new(30.0, 40.0), false);
    h.app.line_toggle_lock();
    assert!(h.app.line_draft.as_ref().unwrap().dir_lock.is_some());
    // Movement now only changes length (D07): far off-axis cursor stays on
    // the locked ray.
    h.app.line_hover(Pos2::new(500.0, -20.0), false);
    for c in ['1', '0', '0'] {
        h.app.line_push_digit(c);
    }
    assert_eq!(h.app.line_draft.as_ref().unwrap().entry, "100");
    assert!(h.app.line_enter_commit());
    assert_endpoints(&h.app, Pos2::new(0.0, 0.0), Pos2::new(60.0, 80.0));
    h.frame();
}

/// GP5 — Esc layering (D12): entry clears → first point removed → tool
/// disarms to Select. Nothing is journaled.
#[test]
fn line_gp5_escape_layering() {
    let mut h = line_board("line_gp5");
    h.app.line_begin(Pos2::new(10.0, 10.0), false);
    h.app.line_release(Pos2::new(10.0, 10.0), true, false);
    h.app.line_push_digit('5');

    let ctx = h.ctx.clone();
    assert!(h
        .app
        .dispatch(&ctx, atlas_commands::CommandId("app.cancel"), None));
    let d = h
        .app
        .line_draft
        .as_ref()
        .expect("draft survives entry clear");
    assert!(d.entry.is_empty(), "first Esc clears the numeric entry");

    assert!(h
        .app
        .dispatch(&ctx, atlas_commands::CommandId("app.cancel"), None));
    assert!(
        h.app.line_draft.is_none(),
        "second Esc removes the first point"
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Line, "still armed");

    assert!(h
        .app
        .dispatch(&ctx, atlas_commands::CommandId("app.cancel"), None));
    assert_eq!(
        h.app.board_tool,
        board::BoardTool::Select,
        "third Esc disarms"
    );
    assert!(h.app.doc().scene.nodes.is_empty(), "nothing journaled");
    h.frame();
}

/// GP6 — endpoint grip edit with F9 grid snap: dragging the end grip of a
/// committed line to (143,7) lands on the 20-unit grid at (140,0); one
/// undo restores the original endpoint.
#[test]
fn line_gp6_grip_edit_snaps_and_journals_once() {
    let mut h = line_board("line_gp6");
    let id = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0))
        .expect("committed line");
    h.app.board_sel = [id].into_iter().collect();
    h.app.board_snap_grid = true;

    let before = h.app.doc().scene.node(id).unwrap().clone();
    h.app.line_grip_update(id, 1, Pos2::new(143.0, 7.0), false);
    h.app.line_grip_record(id, before);

    let node = h.app.doc().scene.node(id).unwrap();
    let (a, b) = board_line::line_endpoints(node).expect("still a simple line");
    assert!((a - Pos2::new(0.0, 0.0)).length() < 0.05, "start untouched");
    assert!(
        (b - Pos2::new(140.0, 0.0)).length() < 0.05,
        "end snapped to the 20u grid, got {b:?}"
    );

    h.app.board_undo();
    let node = h.app.doc().scene.node(id).unwrap();
    let (_, b) = board_line::line_endpoints(node).unwrap();
    assert!(
        (b - Pos2::new(100.0, 0.0)).length() < 0.05,
        "one undo restores the endpoint"
    );
    h.frame();
}

/// P1.curve.create-style — inspector edit on one line seeds the next commit.
#[test]
fn line_create_matches_last_edited_style() {
    let mut h = line_board("line_last_style");
    let id = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(50.0, 0.0))
        .expect("first line");
    let custom = slate_doc::scene::Stroke {
        width: 7.0,
        color: slate_doc::scene::Rgba([10, 20, 30, 255]),
        dash: slate_doc::scene::Dash::Dashed,
        cap: slate_doc::scene::StrokeCap::Butt,
        join: slate_doc::scene::StrokeJoin::Bevel,
        profile: slate_doc::scene::WidthProfile::Uniform,
    };
    h.app.patch_nodes(&[id], |n| {
        n.opacity = 0.5;
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke = custom;
        }
    });

    h.app.set_board_tool(board::BoardTool::Line);
    h.app.line_begin(Pos2::new(0.0, 10.0), false);
    h.app.line_release(Pos2::new(0.0, 10.0), true, false);
    h.app.line_hover(Pos2::new(80.0, 10.0), false);
    h.app.line_release(Pos2::new(80.0, 10.0), false, false);

    let node = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| n.id != id)
        .expect("second line");
    assert!((node.opacity - 0.5).abs() < f32::EPSILON);
    if let slate_doc::scene::NodeKind::Shape(s) = &node.kind {
        assert_eq!(s.stroke, custom);
    } else {
        panic!("expected shape");
    }
    h.frame();
}

/// P1.curve.grips — homogeneous multi-line selection is grip-only (no group
/// bbox resize affordance).
#[test]
fn line_multi_select_all_simple_lines() {
    let mut h = line_board("line_multi");
    let a = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0))
        .unwrap();
    let b = h
        .app
        .commit_line(Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0))
        .unwrap();
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.selection_all_simple_lines());
    h.frame();
}

/// P1.curve.pick — click inside the node AABB but off the stroke misses.
#[test]
fn line_pick_stroke_not_bbox() {
    let mut h = line_board("line_pick");
    let id = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(100.0, 100.0))
        .unwrap();
    let scene = &h.app.doc().scene;
    assert_eq!(
        board_path::board_pick_node(scene, 50.0, 50.0, 1.0),
        Some(id)
    );
    assert!(
        board_path::board_pick_node(scene, 50.0, 10.0, 1.0).is_none(),
        "interior bbox point off the diagonal must not select"
    );
    h.frame();
}

// ---------- tool kits: the result of a gesture comes from data ----------

fn kit_board(tag: &str, tool: board::BoardTool) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(tool);
    h
}

fn drag(h: &mut Harness, tool: board::BoardTool, a: Pos2, b: Pos2) {
    h.app.finish_draw(a, b, tool, egui::Modifiers::default());
}

/// The rectangle tool's fill and stroke come from `core.slatekit`, resolved
/// against the live palette — the same shape the constants in `finish_draw`
/// used to build.
#[test]
fn a_drawn_rectangle_takes_its_style_from_the_kit_recipe() {
    let mut h = kit_board("kit_rect", board::BoardTool::RectShape);
    let accent = board::to_rgba(h.app.palette().accent);
    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 120.0),
    );

    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a shape node");
    };
    assert_eq!(s.shape, slate_doc::scene::ShapeKind::Rect);
    let [r, g, b, _] = accent.0;
    assert_eq!(s.fill, Some(slate_doc::scene::Rgba([r, g, b, 60])));
    assert_eq!(s.stroke.width, 2.0);
    assert_eq!(s.stroke.color, accent);
    assert_eq!(s.corner, slate_doc::scene::Corner::Square);
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot");
    h.frame();
}

/// A user kit that reuses a built-in tool's id replaces what that tool
/// produces — no rebuild, no change to the shipped kit. This is the whole
/// point of the split.
#[test]
fn a_user_kit_overrides_what_a_builtin_tool_produces() {
    let mut h = kit_board("kit_override", board::BoardTool::RectShape);
    let dir = h.base.join("tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mine.slatekit"),
        r##"
        format_version = 1
        id = "mine"
        name = "Mine"

        [[tool]]
        id = "rect"
        name = "Rounded rectangle"
        grammar = "drag_rect"

          [tool.recipe]
          kind = "shape"
          node = "rect"
          fill = "#123456"
          corner = { rounded = { radius = 12.0 } }
          stroke = { width = 4.0, color = "#e8443a", join = "round" }
        "##,
    )
    .unwrap();
    h.app.kits = kits::KitState::load_from(Some(&dir), &[]);
    assert_eq!(h.app.kits.errors().count(), 0);

    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 120.0),
    );

    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a shape node");
    };
    assert_eq!(
        s.fill,
        Some(slate_doc::scene::Rgba([0x12, 0x34, 0x56, 255]))
    );
    assert_eq!(s.stroke.width, 4.0);
    assert_eq!(
        s.stroke.color,
        slate_doc::scene::Rgba([0xe8, 0x44, 0x3a, 255])
    );
    assert_eq!(s.stroke.join, slate_doc::scene::StrokeJoin::Round);
    assert_eq!(s.corner, slate_doc::scene::Corner::Rounded { radius: 12.0 });

    // Tools the user did not override are untouched.
    drag(
        &mut h,
        board::BoardTool::Ellipse,
        Pos2::new(0.0, 300.0),
        Pos2::new(100.0, 400.0),
    );
    let NodeKind::Shape(e) = &h.app.doc().scene.nodes[1].kind else {
        panic!("expected a shape node");
    };
    assert_eq!(e.shape, slate_doc::scene::ShapeKind::Ellipse);
    assert_eq!(e.stroke.width, 2.0);
    h.frame();
}

/// A kit whose grammar this build does not implement costs that one tool and
/// leaves the built-in it tried to shadow in place.
#[test]
fn a_kit_tool_with_an_unknown_grammar_leaves_the_builtin_working() {
    let mut h = kit_board("kit_unknown", board::BoardTool::RectShape);
    let dir = h.base.join("tools");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("future.slatekit"),
        r##"
        format_version = 1
        id = "future"
        name = "From a later build"

        [[tool]]
        id = "rect"
        name = "Constrained rectangle"
        grammar = "constraint_solve"
        recipe = { kind = "shape", node = "rect", fill = "#123456" }
        "##,
    )
    .unwrap();
    h.app.kits = kits::KitState::load_from(Some(&dir), &[]);
    assert_eq!(h.app.kits.errors().count(), 1, "reported, not fatal");

    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 120.0),
    );
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a shape node");
    };
    let accent = board::to_rgba(h.app.palette().accent);
    assert_eq!(s.stroke.color, accent, "the built-in rect still applies");
    h.frame();
}

/// Placing frames claims consecutive slide orders and numbers their titles
/// from the recipe's `{n}` substitution.
#[test]
fn placed_frames_claim_consecutive_slide_orders() {
    let mut h = kit_board("kit_frame", board::BoardTool::Frame);
    h.app.place_frame_at(Pos2::new(0.0, 0.0));
    h.app.place_frame_at(Pos2::new(2000.0, 0.0));

    let frames: Vec<(u32, String)> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Frame(f) => Some((f.order, f.title.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        frames,
        vec![(0, "Slide 1".to_string()), (1, "Slide 2".to_string())]
    );
    // The frame preset, not the recipe, sizes a click-placed frame.
    let (w, h_) = h.app.board_frame_preset.size();
    assert_eq!(
        (
            h.app.doc().scene.nodes[0].rect.w,
            h.app.doc().scene.nodes[0].rect.h
        ),
        (w, h_)
    );
    h.frame();
}

/// A click-placed Repository Lens portal takes the recipe's default size and
/// stays unbound — a kit must not ship a path from its author's machine.
#[test]
fn a_placed_repository_lens_portal_is_unbound_at_the_recipe_size() {
    let mut h = kit_board("kit_portal", board::BoardTool::RepoLens);
    h.app.place_repo_lens_at(Pos2::new(0.0, 0.0));

    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Portal(p) = &node.kind else {
        panic!("expected a portal node");
    };
    assert_eq!(p.class, slate_doc::scene::PortalClass::Generated);
    assert_eq!(p.kind, slate_doc::scene::PortalKind::RepoLens);
    assert!(p.source.is_none(), "unbound until the user chooses a repo");
    assert_eq!(p.query, slate_doc::scene::RepoPortalQuery::default());
    assert_eq!(
        (node.rect.w, node.rect.h),
        (
            slate_doc::scene::REPO_PORTAL_DEFAULT_W,
            slate_doc::scene::REPO_PORTAL_DEFAULT_H
        )
    );
    h.frame();
}

// ---------------------------------------------------------------------------
// Web portal golden paths (contracts/portal-web-embed.md)
// ---------------------------------------------------------------------------

/// A host that reports a working runtime and hands out a solid frame, so the
/// pool, the states, input routing, and bake can all be driven without a
/// browser. The log is shared so a test can read what the page was sent.
#[derive(Default)]
struct FakeLog {
    admitted: std::collections::HashSet<slate_doc::NodeId>,
    inputs: Vec<board_web::WebInput>,
}

#[derive(Default, Clone)]
struct FakeWebHost(std::rc::Rc<std::cell::RefCell<FakeLog>>);

impl FakeWebHost {
    fn inputs(&self) -> Vec<board_web::WebInput> {
        self.0.borrow().inputs.clone()
    }
    fn sent<T>(&self, pick: impl Fn(&board_web::WebInput) -> Option<T>) -> Vec<T> {
        self.inputs().iter().filter_map(pick).collect()
    }
}

impl board_web::WebHost for FakeWebHost {
    fn available(&self) -> bool {
        true
    }
    fn admit(&mut self, id: slate_doc::NodeId, _req: &board_web::WebRequest) {
        self.0.borrow_mut().admitted.insert(id);
    }
    fn evict(&mut self, id: slate_doc::NodeId) {
        self.0.borrow_mut().admitted.remove(&id);
    }
    fn take_frame(&mut self, id: slate_doc::NodeId) -> Option<egui::ColorImage> {
        self.0
            .borrow()
            .admitted
            .contains(&id)
            .then(|| egui::ColorImage::new([8, 8], egui::Color32::from_rgb(30, 90, 160)))
    }
    fn capture_poster(&mut self, _id: slate_doc::NodeId) -> Option<egui::ColorImage> {
        Some(egui::ColorImage::new(
            [8, 8],
            egui::Color32::from_rgb(30, 90, 160),
        ))
    }
    fn send_input(&mut self, _id: slate_doc::NodeId, input: board_web::WebInput) {
        self.0.borrow_mut().inputs.push(input);
    }
    fn cursor(&self, _id: slate_doc::NodeId) -> Option<egui::CursorIcon> {
        None
    }
    fn load_error(&self, _id: slate_doc::NodeId) -> Option<String> {
        None
    }
}

fn web_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.kits = kits::KitState::builtin_only();
    h
}

fn with_fake_host(h: &mut Harness) -> FakeWebHost {
    let host = FakeWebHost::default();
    h.app.web.set_host(Box::new(host.clone()));
    host
}

/// Report the portals as painted at a given on-screen height, which is what
/// the pool sorts by, and run frames until the pipeline settles. Geometry is
/// normally recorded during painting; a headless harness supplies it directly,
/// and it only sticks once the pump has made the derived view.
fn web_settle(h: &mut Harness, sizes: &[(slate_doc::NodeId, f32)], frames: usize) {
    let clip = ERect::from_min_size(Pos2::ZERO, EVec2::new(4000.0, 4000.0));
    for _ in 0..frames {
        for (id, height) in sizes {
            let r = ERect::from_min_size(Pos2::ZERO, EVec2::new(height * 1.78, *height));
            h.app.note_web_geometry(*id, r, clip);
        }
        h.frame();
    }
}
fn only_portal(h: &Harness) -> (slate_doc::NodeId, slate_doc::scene::PortalNode) {
    let node = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == slate_doc::scene::PortalKind::Web))
        .expect("a web portal on the board");
    let NodeKind::Portal(p) = &node.kind else {
        unreachable!()
    };
    (node.id, p.clone())
}

/// GP1 — the draw grammar always commits an unbound portal: binding never
/// happens inside a gesture (D03).
#[test]
fn gp1_a_drawn_web_portal_commits_unbound() {
    let mut h = web_board("web_gp1");
    h.app.set_board_tool(board::BoardTool::WebPortal);
    drag(
        &mut h,
        board::BoardTool::WebPortal,
        Pos2::new(0.0, 0.0),
        Pos2::new(640.0, 360.0),
    );
    let (id, p) = only_portal(&h);
    assert_eq!(p.class, slate_doc::scene::PortalClass::Host);
    assert!(p.source.is_none(), "a draw never binds");
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot (D02)");
    h.frame();
    assert_eq!(h.app.web.state(id), board_web::WebState::Unbound);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty(), "one gesture, one undo");
    h.frame();
}

/// GP1b — a click places the shared portal default size (P2.PortalPlace.click).
#[test]
fn gp1_a_clicked_web_portal_takes_the_shared_portal_default_size() {
    let mut h = web_board("web_gp1b");
    h.app.place_web_portal_at(Pos2::new(0.0, 0.0));
    let node = &h.app.doc().scene.nodes[0];
    assert_eq!(
        (node.rect.w, node.rect.h),
        (
            slate_doc::scene::PORTAL_DEFAULT_W,
            slate_doc::scene::PORTAL_DEFAULT_H
        )
    );
    h.frame();
}

/// GP2 — dropping an HTML file on the board makes a portal, not a text card,
/// and the locator is stored workbook-relative (D01, Art. IX.2).
#[test]
fn gp2_dropping_a_dashboard_makes_a_portal_and_leaves_other_files_alone() {
    let mut h = web_board("web_gp2");
    let page = h.base.join("dash.html");
    std::fs::write(&page, "<h1>hi</h1>").unwrap();
    let photo = h.base.join("photo.png");
    std::fs::write(&photo, [0u8; 8]).unwrap();

    let rest = h
        .app
        .divert_web_drops(&[page.clone(), photo.clone()], Pos2::ZERO);
    assert_eq!(rest, vec![photo], "only the page is diverted");
    let (_, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some(page.to_string_lossy().as_ref()),
        "unsaved workbook keeps the absolute path"
    );
    h.frame();
}

/// GP2b — a folder is a page only when it actually holds an entry file.
#[test]
fn gp2_a_folder_is_a_portal_only_when_it_holds_an_entry_file() {
    let mut h = web_board("web_gp2b");
    let with_entry = h.base.join("dashboard");
    std::fs::create_dir_all(&with_entry).unwrap();
    std::fs::write(with_entry.join("index.html"), "<h1>hi</h1>").unwrap();
    let plain = h.base.join("photos");
    std::fs::create_dir_all(&plain).unwrap();

    assert!(board_web::is_web_drop(&with_entry));
    assert!(!board_web::is_web_drop(&plain));
    let rest = h
        .app
        .divert_web_drops(&[with_entry, plain.clone()], Pos2::ZERO);
    assert_eq!(rest, vec![plain]);
    h.frame();
}

/// A folder that only ships `index.htm` still binds to that entry — the default
/// `index.html` must not silently send Navigate to a missing file.
#[test]
fn a_folder_that_only_has_index_htm_binds_that_entry() {
    let mut h = web_board("web_htm");
    let dash = h.base.join("legacy");
    std::fs::create_dir_all(&dash).unwrap();
    std::fs::write(dash.join("index.htm"), "<h1>legacy</h1>").unwrap();
    h.app.divert_web_drops(&[dash], Pos2::ZERO);
    let (_, p) = only_portal(&h);
    assert_eq!(p.web_ref().entry, "index.htm");
    h.frame();
}

/// GP11 — `portal.web.source` with a detail binds the same way a human does,
/// so an agent with an autonomy grant reaches the same journaled path (D27).
#[test]
fn gp11_portal_web_source_detail_binds_a_url() {
    let mut h = web_board("web_gp11");
    with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.board_sel = std::iter::once(id).collect();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("portal.web.source"),
        Some("https://example.com/from-agent".into()),
    ));
    let (_, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some("https://example.com/from-agent")
    );
    assert!(
        h.app.web.has_consent("https://example.com"),
        "binding is the permission for that origin"
    );
    h.frame();
}

/// GP3 — pasting a URL is itself the permission for that origin, so the page
/// loads without a second gesture; the permission still never journals and
/// never reaches the saved workbook (D32, D26).
#[test]
fn gp3_a_pasted_url_loads_without_a_second_gesture() {
    let mut h = web_board("web_gp3");
    with_fake_host(&mut h);
    let journal_before = h.app.tab().journal.undo_depth();
    assert!(h.app.paste_web_url("https://example.com/dash", Pos2::ZERO));
    let (id, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some("https://example.com/dash")
    );
    assert!(h.app.web.has_consent("https://example.com"));
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        journal_before + 1,
        "placing the portal is one command; consent is not a command at all"
    );
    let saved = serde_json::to_string(&h.app.doc().scene).unwrap();
    assert!(
        !saved.contains("consent"),
        "consent never reaches the document"
    );
    assert_eq!(
        saved.matches("https://example.com").count(),
        1,
        "the origin appears as the locator and nowhere else"
    );

    web_settle(&mut h, &[(id, 600.0)], 2);
    assert!(h.app.web.is_live(id), "no gate between paste and pixels");
}

/// GP3b — the case the gate is actually for: a workbook reopened from disk
/// holds pages nobody in this session has permitted, and opening it must not
/// quietly start talking to them.
#[test]
fn gp3_a_page_restored_from_disk_waits_for_permission() {
    let mut h = web_board("web_gp3b");
    h.app.paste_web_url("https://example.com/dash", Pos2::ZERO);
    let path = h.base.join("hub.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, path.clone());

    let mut h2 = Harness::new("web_gp3b_reopen");
    with_fake_host(&mut h2);
    h2.app.open_doc_at(path);
    h2.app.doc_mut().view.active_view = ViewKind::Board;
    h2.frame();
    let (id, _) = only_portal(&h2);
    assert!(!h2.app.web.has_consent("https://example.com"));
    web_settle(&mut h2, &[(id, 600.0)], 2);
    assert_eq!(
        h2.app.web.state(id),
        board_web::WebState::Blocked {
            origin: "https://example.com".into()
        }
    );
    assert_eq!(h2.app.web.live_count(), 0, "a blocked page runs nothing");

    let journal_before = h2.app.tab().journal.undo_depth();
    h2.app.web_allow_origin(id);
    assert_eq!(
        h2.app.tab().journal.undo_depth(),
        journal_before,
        "consent is a local decision, never a journaled command"
    );
    web_settle(&mut h2, &[(id, 600.0)], 2);
    assert!(h2.app.web.is_live(id));
}

/// Focus is the human overriding the size budget: a page too small to earn a
/// slot on its own gets one the moment it is double-clicked into.
#[test]
fn a_focused_page_runs_however_small_it_is_painted() {
    let mut h = web_board("web_focus_small");
    with_fake_host(&mut h);
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    web_settle(&mut h, &[(id, 120.0)], 2);
    assert_eq!(h.app.web.state(id), board_web::WebState::TooSmall);

    h.app.web_focus(id);
    web_settle(&mut h, &[(id, 120.0)], 2);
    assert!(
        h.app.web.is_live(id),
        "double-clicking in must not be answered with \"too small\""
    );
}

/// GP4 — focus is about the keyboard, not the page: Esc releases it without
/// tearing the view down (D12, D22).
#[test]
fn gp4_releasing_input_focus_leaves_the_page_running() {
    let mut h = web_board("web_gp4");
    with_fake_host(&mut h);
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    web_settle(&mut h, &[(id, 600.0)], 3);
    assert!(h.app.web.is_live(id));

    h.app.web_focus(id);
    assert_eq!(h.app.web.focused, Some(id));
    assert!(h.app.web_blur(), "Esc peels focus");
    assert_eq!(h.app.web.focused, None);
    web_settle(&mut h, &[(id, 600.0)], 1);
    assert!(
        h.app.web.is_live(id),
        "the page keeps rendering after focus leaves"
    );
}

/// GP5 — a portal painted below the live threshold runs nothing and says so,
/// rather than silently doing nothing (D23, D30).
#[test]
fn gp5_a_page_painted_too_small_runs_nothing_and_says_so() {
    let mut h = web_board("web_gp5");
    with_fake_host(&mut h);
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    web_settle(&mut h, &[(id, 120.0)], 2);
    assert_eq!(h.app.web.state(id), board_web::WebState::TooSmall);
    assert_eq!(h.app.web.live_count(), 0);
}

/// GP7 — a source that disappears names the locator it tried and keeps its last
/// poster, rather than reading as a bug (P1.portal.health, D30).
#[test]
fn gp7_a_missing_local_source_names_the_locator_it_tried() {
    let mut h = web_board("web_gp7");
    with_fake_host(&mut h);
    let page = h.base.join("gone.html");
    std::fs::write(&page, "<h1>hi</h1>").unwrap();
    h.app
        .divert_web_drops(std::slice::from_ref(&page), Pos2::ZERO);
    let (id, _) = only_portal(&h);
    web_settle(&mut h, &[(id, 600.0)], 2);

    std::fs::remove_file(&page).unwrap();
    // The poll floor is a second, and the probe itself is off-thread — step
    // past the floor, then pump until the result lands.
    std::thread::sleep(std::time::Duration::from_secs_f32(
        board_web::POLL_SECS + 0.05,
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        web_settle(&mut h, &[(id, 600.0)], 1);
        if matches!(h.app.web.state(id), board_web::WebState::Missing { .. }) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for Missing, last state {:?}",
            h.app.web.state(id)
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    match h.app.web.state(id) {
        board_web::WebState::Missing { locator } => {
            assert!(locator.contains("gone.html"), "names what it tried");
        }
        other => panic!("expected Missing, got {other:?}"),
    }
}

/// GP8 — the research-hub case end to end: twelve eligible pages, six webviews.
#[test]
fn gp8_twelve_eligible_pages_run_exactly_the_pool() {
    let mut h = web_board("web_gp8");
    with_fake_host(&mut h);
    let mut ids = Vec::new();
    for i in 0..12 {
        h.app
            .paste_web_url(&format!("https://example.com/{i}"), Pos2::ZERO);
        let node = h.app.doc().scene.nodes.last().unwrap();
        ids.push(node.id);
    }
    h.app.web.grant_consent("https://example.com");
    let sizes: Vec<(slate_doc::NodeId, f32)> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, 400.0 + i as f32))
        .collect();
    // Enough frames for the pool to fill and the capped upload budget to drain.
    web_settle(&mut h, &sizes, 8);
    assert_eq!(
        h.app.web.live_count(),
        board_web::LIVE_POOL,
        "a board full of pages costs a bounded number of processes"
    );
    let live = ids
        .iter()
        .filter(|id| h.app.web.state(**id) == board_web::WebState::Live)
        .count();
    let budgeted = ids
        .iter()
        .filter(|id| h.app.web.state(**id) == board_web::WebState::Budgeted)
        .count();
    assert_eq!(live, board_web::LIVE_POOL);
    assert_eq!(
        budgeted,
        ids.len() - board_web::LIVE_POOL,
        "the rest say they are waiting rather than looking broken"
    );
    // The biggest pages win the slots — the pool is area-ordered (D29).
    for id in ids.iter().skip(ids.len() - board_web::LIVE_POOL) {
        assert!(h.app.web.is_live(*id));
    }
}

/// GP9/GP10 — export is a serialization, not a screenshot: a local dashboard
/// travels inside the artifact and still runs; a remote page cannot be copied,
/// so it exports as a poster that points at where it came from (D26, Art. IV).
#[test]
fn gp9_export_packages_a_local_page_and_points_at_a_remote_one() {
    let mut h = web_board("web_export");
    with_fake_host(&mut h);
    h.seed_frame(None);

    let dash = h.base.join("dashboard");
    std::fs::create_dir_all(dash.join("data")).unwrap();
    std::fs::write(dash.join("index.html"), "<h1>numbers</h1>").unwrap();
    std::fs::write(dash.join("data").join("rows.json"), "[]").unwrap();
    h.app.divert_web_drops(&[dash], Pos2::ZERO);
    let local = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.paste_web_url("https://example.com/live", Pos2::ZERO);
    let remote = h.app.doc().scene.nodes.last().unwrap().id;
    // Both inside the seeded 800x450 frame, so both land on the slide.
    h.app.patch_nodes(&[local], |n| {
        n.rect = WorldRect::new(20.0, 20.0, 320.0, 180.0);
    });
    h.app.patch_nodes(&[remote], |n| {
        n.rect = WorldRect::new(400.0, 20.0, 320.0, 180.0);
    });
    h.frame();

    let out = h.base.join("export");
    h.app.do_export(out.clone());
    let deck = out.join("Untitled-slides");
    let html = std::fs::read_to_string(deck.join("index.html")).unwrap();

    assert!(
        html.contains("<iframe"),
        "the packaged dashboard still runs in the artifact"
    );
    assert!(
        html.contains("sandbox=\"allow-scripts allow-same-origin\""),
        "scripts and its own data files, nothing wider (D32)"
    );
    assert!(
        html.contains("Packaged from"),
        "a copy names where it came from (Art. IX.3)"
    );
    let copied: Vec<PathBuf> = walk_files(&deck)
        .into_iter()
        .filter(|p| p.ends_with("rows.json"))
        .collect();
    assert_eq!(copied.len(), 1, "the whole folder travels, not just entry");

    assert!(
        html.contains("https://example.com/live"),
        "the remote page exports as a pointer"
    );
    assert!(
        !html.contains("<iframe src=\"https://example.com/live\""),
        "a remote page is not silently reloaded from the artifact"
    );
    h.frame();
}

fn walk_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

// --- what happens once you are inside the page ------------------------------

/// A portal that fills the canvas and holds input focus, so the pointer at the
/// screen centre is unambiguously inside its page.
fn focused_page(tag: &str) -> (Harness, slate_doc::NodeId, FakeWebHost) {
    let mut h = web_board(tag);
    let host = with_fake_host(&mut h);
    h.app.paste_web_url("https://example.com/app", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.zoom_to_rect(rect);
    h.frame();
    h.app.web_focus(id);
    h.frame();
    (h, id, host)
}

fn center() -> Pos2 {
    Pos2::new(720.0, 500.0)
}

/// Hover the page and send one wheel notch.
fn wheel_over_page(h: &mut Harness, dy: f32) {
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(center()));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: EVec2::new(0.0, dy),
            modifiers: egui::Modifiers::default(),
        });
    });
}

/// The headline of this contract: with the pointer inside a focused page, the
/// wheel scrolls the page and the board does not zoom (D22).
#[test]
fn the_wheel_inside_a_focused_page_scrolls_it_instead_of_zooming_the_board() {
    let (mut h, _id, host) = focused_page("web_wheel");
    let zoom_before = h.app.tab().cam.z;
    wheel_over_page(&mut h, -50.0);

    let wheels: Vec<f32> = host.sent(|i| match i {
        board_web::WebInput::Wheel { delta, .. } => Some(*delta),
        _ => None,
    });
    assert!(!wheels.is_empty(), "the page never saw the wheel");
    assert_eq!(
        h.app.tab().cam.z,
        zoom_before,
        "the board must not zoom under the pointer"
    );
}

/// With focus released, the same notch is the camera's again.
#[test]
fn the_wheel_zooms_the_board_again_once_focus_is_released() {
    let (mut h, _id, host) = focused_page("web_wheel_release");
    h.app.web_blur();
    let zoom_before = h.app.tab().cam.z;
    wheel_over_page(&mut h, -50.0);

    assert_ne!(
        h.app.tab().cam.z,
        zoom_before,
        "the board zooms when no page holds the pointer"
    );
    assert!(
        host.sent(|i| matches!(i, board_web::WebInput::Wheel { .. }).then_some(()))
            .is_empty(),
        "an unfocused page hears nothing"
    );
}

/// Typing into a form must not run board commands: bare letters are the page's
/// while it holds focus, and they arrive as text.
#[test]
fn typing_into_a_page_does_not_reach_the_board_tools() {
    let (mut h, _id, host) = focused_page("web_typing");
    let tool_before = h.app.board_tool;
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(center()));
        // "r" is the rectangle tool's bare-letter shortcut.
        input.events.push(egui::Event::Key {
            key: egui::Key::R,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
        input.events.push(egui::Event::Text("r".into()));
    });

    assert_eq!(h.app.board_tool, tool_before, "no tool switch while typing");
    let text: Vec<char> = host.sent(|i| match i {
        board_web::WebInput::Text(c) => Some(*c),
        _ => None,
    });
    assert_eq!(text, vec!['r'], "the character reached the page");
}

/// Esc is the one key the page never gets, because it is how the human gets
/// back out (D22).
#[test]
fn escape_peels_focus_and_never_reaches_the_page() {
    let (mut h, id, host) = focused_page("web_escape");
    assert_eq!(h.app.web.focused, Some(id));
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(center()));
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
    });

    assert_eq!(h.app.web.focused, None, "Esc released the page");
    assert!(
        host.sent(|i| match i {
            board_web::WebInput::Key { key, .. } => Some(*key),
            _ => None,
        })
        .iter()
        .all(|k| *k != egui::Key::Escape),
        "the page never sees Escape"
    );
}

/// A drag inside the page selects text there rather than moving the node or
/// panning the board.
#[test]
fn a_drag_inside_a_focused_page_moves_nothing_on_the_board() {
    let (mut h, id, host) = focused_page("web_drag");
    let rect_before = h.app.doc().scene.node(id).unwrap().rect;
    let cam_before = h.app.tab().cam.offset;
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(center()));
        input.events.push(egui::Event::PointerButton {
            pos: center(),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(center() + EVec2::new(60.0, 20.0)));
    });

    let held: Vec<u8> = host.sent(|i| match i {
        board_web::WebInput::Move { buttons, .. } => Some(*buttons),
        _ => None,
    });
    assert!(
        held.contains(&1),
        "the page must know the button is held, or it cannot select text"
    );
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect_before);
    assert_eq!(h.app.tab().cam.offset, cam_before);
}

/// GP12 — bake adds the poster plus its provenance and leaves the portal alone
/// (D25).
#[test]
fn gp12_bake_copies_the_poster_and_leaves_the_portal_in_place() {
    let mut h = web_board("web_gp12");
    with_fake_host(&mut h);
    let page = h.base.join("dash.html");
    std::fs::write(&page, "<h1>hi</h1>").unwrap();
    h.app.divert_web_drops(&[page], Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.board_sel = std::iter::once(id).collect();

    assert!(h.app.web_bake_selected());
    let kinds: Vec<&str> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .map(|n| match &n.kind {
            NodeKind::Portal(_) => "portal",
            NodeKind::Image(_) => "image",
            NodeKind::Text(_) => "text",
            _ => "other",
        })
        .collect();
    assert!(
        kinds.contains(&"portal"),
        "bake copies, it does not convert"
    );
    assert!(kinds.contains(&"image"));
    assert!(kinds.contains(&"text"));
    let note = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeKind::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .unwrap();
    assert!(note.contains("dash.html"), "provenance names the source");
    assert!(note.contains("captured"), "and when it was captured");
    h.frame();
}

/// Dangerous locators are refused by name rather than quietly not loading
/// (D19, D30).
#[test]
fn a_javascript_locator_is_refused_and_says_why() {
    let mut h = web_board("web_refuse");
    with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.bind_web_source(id, "javascript:alert(1)".into());
    web_settle(&mut h, &[(id, 600.0)], 2);
    match h.app.web.state(id) {
        board_web::WebState::Refused { reason } => {
            assert!(reason.contains("javascript"), "the reason names the scheme");
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(h.app.web.live_count(), 0);
}

/// With no WebView2 runtime, portals still place, bind, and export — they just
/// say what is missing instead of stalling (D29, D30).
#[test]
fn without_a_runtime_a_portal_degrades_instead_of_stalling() {
    let mut h = web_board("web_noruntime");
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    web_settle(&mut h, &[(id, 600.0)], 2);
    assert_eq!(h.app.web.state(id), board_web::WebState::NoRuntime);
    assert_eq!(h.app.web.live_count(), 0);
}

/// One completed draw is one undo step, and undo removes the node.
#[test]
fn a_recipe_driven_draw_is_a_single_undo_step() {
    let mut h = kit_board("kit_undo", board::BoardTool::Ellipse);
    drag(
        &mut h,
        board::BoardTool::Ellipse,
        Pos2::new(0.0, 0.0),
        Pos2::new(80.0, 80.0),
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty(), "one gesture, one undo");
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.frame();
}
