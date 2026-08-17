//! Headless Slate stability tests: drive the real frame loop through a plain
//! `egui::Context` (no eframe window) with the real thumbnail pool, exercising
//! the tag model, both presentations, tabs, and workbook save/load.

use super::lens::LensStatus;
use super::{board_align, board_handles, board_place, board_wire, *};
use eframe::egui::{Pos2, Rect as ERect, Vec2 as EVec2};
use slate_doc::{NodeId, ViewKind};

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
                ..Default::default()
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
                ..Default::default()
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

// ---------- Object snaps (contracts/object-snap.md GP1–GP4) ----------

fn osnap_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.board_snap_grid = false;
    h.app.board_osnap = slate_doc::ObjectSnapSet::default();
    h
}

fn add_ellipse(app: &mut SlateApp, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let rect = slate_doc::scene::WorldRect::new(x, y, w, h);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Ellipse,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: None,
        }),
    );
    app.add_nodes(vec![node])[0]
}

fn only_kind(kind: slate_doc::SnapKind) -> slate_doc::ObjectSnapSet {
    let mut set = slate_doc::ObjectSnapSet {
        enabled: true,
        end: false,
        mid: false,
        center: false,
        near: false,
        intersection: false,
        quadrant: false,
        perpendicular: false,
        tangent: false,
    };
    set.set(kind, true);
    set
}

/// GP1 — End: cursor near a rect corner snaps to that corner.
#[test]
fn osnap_gp1_end_on_rect_corner() {
    let mut h = osnap_board("osnap_gp1");
    add_rect(&mut h.app, 200.0, 80.0);
    h.app.board_osnap = only_kind(slate_doc::SnapKind::End);
    let p = h
        .app
        .resolve_point_snap(Pos2::new(202.0, 82.0), &[], None, false, false);
    assert!((p.x - 200.0).abs() < 0.01 && (p.y - 80.0).abs() < 0.01);
    assert_eq!(
        h.app.board_osnap_hit.map(|hit| hit.kind),
        Some(slate_doc::SnapKind::End)
    );
    h.frame();
}

/// GP2 — Tangent is inert on the first pick (no prior point).
#[test]
fn osnap_gp2_tan_inert_on_first_pick() {
    let mut h = osnap_board("osnap_gp2");
    add_ellipse(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    h.app.board_osnap = only_kind(slate_doc::SnapKind::Tangent);
    let cursor = Pos2::new(102.0, 50.0);
    let p = h.app.resolve_point_snap(cursor, &[], None, false, false);
    assert!((p.x - cursor.x).abs() < 0.01 && (p.y - cursor.y).abs() < 0.01);
    assert!(h.app.board_osnap_hit.is_none());
    h.frame();
}

/// GP3 — Tangent from a prior point onto a circle.
#[test]
fn osnap_gp3_tan_from_prior_point() {
    let mut h = osnap_board("osnap_gp3");
    add_ellipse(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    h.app.board_osnap = only_kind(slate_doc::SnapKind::Tangent);
    let from = Pos2::new(200.0, 50.0);
    let pts = slate_doc::osnap::tangents_on_ellipse(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        0.0,
        [from.x, from.y],
    );
    assert_eq!(pts.len(), 2);
    let target = Pos2::new(pts[0][0], pts[0][1]);
    let cursor = Pos2::new(target.x + 1.0, target.y + 1.0);
    let p = h
        .app
        .resolve_point_snap(cursor, &[], Some(from), false, false);
    assert_eq!(
        h.app.board_osnap_hit.map(|hit| hit.kind),
        Some(slate_doc::SnapKind::Tangent)
    );
    let dx = p.x - 50.0;
    let dy = p.y - 50.0;
    let rad = (dx * dx + dy * dy).sqrt();
    assert!(
        (rad - 50.0).abs() < 0.8,
        "tangent snap left the circle: {p:?} r={rad}"
    );
    h.frame();
}

/// GP4 — master disable remembers End but does not fire.
#[test]
fn osnap_gp4_master_disable() {
    let mut h = osnap_board("osnap_gp4");
    add_rect(&mut h.app, 200.0, 80.0);
    h.app.board_osnap = only_kind(slate_doc::SnapKind::End);
    h.app.board_osnap.enabled = false;
    assert!(h
        .app
        .board_osnap
        .is_kind_remembered(slate_doc::SnapKind::End));
    let cursor = Pos2::new(202.0, 82.0);
    let p = h.app.resolve_point_snap(cursor, &[], None, false, false);
    assert!((p.x - cursor.x).abs() < 0.01 && (p.y - cursor.y).abs() < 0.01);
    assert!(h.app.board_osnap_hit.is_none());
    h.frame();
}

/// Armed GhostFollow: a point near an edge snaps and emits a forcefield.
#[test]
fn armed_rect_hover_snaps_and_emits_forcefield() {
    let mut h = osnap_board("armed_hover_snap");
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = true;
    add_rect(&mut h.app, 200.0, 80.0);
    h.app.set_board_tool(board::BoardTool::RectShape);
    let p = h
        .app
        .resolve_point_snap(Pos2::new(203.0, 110.0), &[], None, false, false);
    assert!(
        (p.x - 200.0).abs() < 0.01,
        "armed hotspot should snap to the edge, got {p:?}"
    );
    assert!(
        !h.app.board_snap_guides.is_empty(),
        "GhostFollow hover must emit a forcefield"
    );
    assert_eq!(h.app.board_point_snap, Some(p));
    assert_eq!(h.app.preview_snap_point(Pos2::new(203.0, 110.0)), p);
}

/// DragScale second corner: snap the live rect, not a 0-size cursor that
/// has left the target's row (the forcefield used to go quiet).
#[test]
fn draw_rect_second_corner_emits_forcefield() {
    let mut h = osnap_board("draw_rect_second_corner");
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = true;
    add_rect(&mut h.app, 200.0, 0.0);
    h.app.set_board_tool(board::BoardTool::RectShape);
    let start = Pos2::new(0.0, 0.0);
    let end = Pos2::new(197.0, 280.0);
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(Pos2::new(10.0, 10.0), start, Default::default());
    h.app.update_gesture_for_test(end, Default::default());
    assert!(
        !h.app.board_snap_guides.is_empty(),
        "second corner must emit a forcefield even when the cursor left the row"
    );
    let r = h.app.board_draw_rect.expect("live draw rect");
    assert!(
        (r.x + r.w - 200.0).abs() < 0.5,
        "right edge should snap to 200, got {}",
        r.x + r.w
    );
}

/// Shift during DragScale is aspect, not ortho — smart guides still fire.
#[test]
fn draw_rect_shift_does_not_silence_forcefield() {
    let mut h = osnap_board("draw_rect_shift");
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = true;
    h.app.board_ortho = true;
    add_rect(&mut h.app, 200.0, 0.0);
    h.app.set_board_tool(board::BoardTool::RectShape);
    let start = Pos2::new(0.0, 0.0);
    let end = Pos2::new(197.0, 280.0);
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(Pos2::new(10.0, 10.0), start, Default::default());
    let mut mods = egui::Modifiers::default();
    mods.shift = true;
    h.app.update_gesture_for_test(end, mods);
    assert!(
        !h.app.board_snap_guides.is_empty(),
        "Shift+square must not take the ortho path and skip forcefield"
    );
}

/// Every DragRect tool shares `resolve_draw_rect` (ellipse, frame, portals).
#[test]
fn every_drag_rect_tool_second_corner_snaps() {
    for tool in board::BoardTool::ALL {
        if !tool.places_by_drag_rect() {
            continue;
        }
        let mut h = osnap_board(&format!("draw_{tool:?}_snap"));
        h.app.board_osnap.enabled = false;
        h.app.board_smart_guides = true;
        add_rect(&mut h.app, 200.0, 0.0);
        h.app.set_board_tool(tool);
        let r = h
            .app
            .resolve_draw_rect(Pos2::new(0.0, 0.0), Pos2::new(197.0, 280.0), tool, false);
        assert!(
            !h.app.board_snap_guides.is_empty(),
            "{tool:?} second corner must emit a forcefield"
        );
        assert!(
            (r.x + r.w - 200.0).abs() < 0.5,
            "{tool:?} right edge should snap to 200, got {}",
            r.x + r.w
        );
    }
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

/// A click-placed Status Board portal takes the recipe's default size and
/// stays unbound — a kit must not ship a path from its author's machine.
#[test]
fn a_placed_status_board_portal_is_unbound_at_the_recipe_size() {
    let mut h = kit_board("kit_status_portal", board::BoardTool::StatusBoard);
    h.app.place_status_board_at(Pos2::new(0.0, 0.0));

    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Portal(p) = &node.kind else {
        panic!("expected a portal node");
    };
    assert_eq!(p.class, slate_doc::scene::PortalClass::Generated);
    assert_eq!(p.kind, slate_doc::scene::PortalKind::StatusBoard);
    assert!(
        p.source.is_none(),
        "unbound until the user chooses a snapshot"
    );
    assert_eq!(p.status, slate_doc::scene::StatusPortalQuery::default());
    assert_eq!(
        (node.rect.w, node.rect.h),
        (
            slate_doc::scene::STATUS_PORTAL_DEFAULT_W,
            slate_doc::scene::STATUS_PORTAL_DEFAULT_H
        )
    );
    h.frame();
}

// ---------- Tool arming preview (contracts/tool-arming.md GP1–GP6) ----------

fn arming_board(tag: &str, tool: board::BoardTool) -> Harness {
    kit_board(tag, tool)
}

/// GP1 — arming Frame starts GhostFollow: silhouette kind is live, no node.
#[test]
fn arming_gp1_frame_ghost_follows_without_a_node() {
    let mut h = arming_board("arming_gp1", board::BoardTool::Frame);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::RoundedRect)
    );
    assert!(h.app.doc().scene.nodes.is_empty(), "ghost never commits");
    h.frame();
}

/// GP2 — arm Web portal, then click-place: default size, one-shot, ghost gone.
/// Goes through begin/end gesture — the live path, not `place_web_portal_at`.
#[test]
fn arming_gp2_web_portal_click_place_clears_the_ghost() {
    let mut h = arming_board("arming_gp2", board::BoardTool::WebPortal);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::Portal)
    );
    let world = Pos2::new(0.0, 0.0);
    let drag =
        h.app
            .begin_gesture_for_test(Pos2::new(400.0, 300.0), world, egui::Modifiers::default());
    h.app.board_drag = drag;
    h.app.end_gesture_for_test(
        world,
        Some(Pos2::new(400.0, 300.0)),
        egui::Modifiers::default(),
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot");
    assert!(board_place::ghost_kind(h.app.board_tool).is_none());
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

/// A click (no drag) places the kit default size, centred on the press.
#[test]
fn rect_and_ellipse_click_place_default_size() {
    let mut h = arming_board("click_place_rect", board::BoardTool::RectShape);
    h.app.board_drag = Some(board::BoardDrag::Draw {
        start_world: Pos2::new(40.0, 30.0),
        start_screen: Pos2::new(40.0, 30.0),
        tool: board::BoardTool::RectShape,
    });
    h.app
        .end_gesture_for_test(Pos2::new(40.0, 30.0), None, egui::Modifiers::default());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - board_place::place_tokens::RECT_DEFAULT_W).abs() < 0.01
            && (r.h - board_place::place_tokens::RECT_DEFAULT_H).abs() < 0.01,
        "rect click-place size, got {}×{}",
        r.w,
        r.h
    );
    assert!((r.x + r.w * 0.5 - 40.0).abs() < 0.01);
    assert!((r.y + r.h * 0.5 - 30.0).abs() < 0.01);
    assert_eq!(h.app.board_tool, board::BoardTool::Select);

    let mut h = arming_board("click_place_ellipse", board::BoardTool::Ellipse);
    h.app
        .place_default_at(board::BoardTool::Ellipse, Pos2::new(0.0, 0.0));
    let e = h.app.doc().scene.nodes[0].rect;
    assert!(
        (e.w - board_place::place_tokens::ELLIPSE_DEFAULT_W).abs() < 0.01
            && (e.h - board_place::place_tokens::ELLIPSE_DEFAULT_H).abs() < 0.01
    );
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected an ellipse");
    };
    assert_eq!(s.shape, slate_doc::scene::ShapeKind::Ellipse);
    h.frame();
}

/// Every DragRect tool: click-release (no screen travel) places a default
/// size, not a MIN_DRAW speck, and returns to Select.
#[test]
fn every_drag_rect_tool_click_places_default_size() {
    for tool in board::BoardTool::ALL {
        if !tool.places_by_drag_rect() {
            continue;
        }
        let mut h = arming_board(&format!("click_place_{tool:?}"), tool);
        let world = Pos2::new(80.0, 60.0);
        let screen = Pos2::new(400.0, 300.0);
        h.app.board_drag = h
            .app
            .begin_gesture_for_test(screen, world, egui::Modifiers::default());
        h.app
            .end_gesture_for_test(world, Some(screen), egui::Modifiers::default());
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            1,
            "{tool:?}: click-place created nothing"
        );
        let r = h.app.doc().scene.nodes[0].rect;
        assert!(
            r.w > board::MIN_DRAW * 2.0 && r.h > board::MIN_DRAW * 2.0,
            "{tool:?}: click-place was a speck {}×{}",
            r.w,
            r.h
        );
        assert_eq!(h.app.board_tool, board::BoardTool::Select, "{tool:?}");
    }
}

/// Screen travel, not world travel, splits ClickPlace from DragScale.
/// A zoomed-out 20-world twitch is still a click if the pointer moved 2 px.
#[test]
fn click_vs_drag_uses_screen_pixels_not_world() {
    let mut h = arming_board("place_travel_px", board::BoardTool::RectShape);
    let start = Pos2::new(0.0, 0.0);
    h.app.board_drag = Some(board::BoardDrag::Draw {
        start_world: start,
        start_screen: Pos2::new(200.0, 200.0),
        tool: board::BoardTool::RectShape,
    });
    h.app.end_gesture_for_test(
        Pos2::new(20.0, 0.0),
        Some(Pos2::new(202.0, 200.0)),
        egui::Modifiers::default(),
    );
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - board_place::place_tokens::RECT_DEFAULT_W).abs() < 0.01,
        "2 px screen travel must ClickPlace, got {}×{}",
        r.w,
        r.h
    );
}

/// Drag past the screen threshold still sizes the node (the common path).
#[test]
fn drag_past_threshold_still_scales() {
    let mut h = arming_board("place_drag_scale", board::BoardTool::RectShape);
    let start = Pos2::new(0.0, 0.0);
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(Pos2::new(200.0, 200.0), start, egui::Modifiers::default());
    h.app.end_gesture_for_test(
        Pos2::new(200.0, 120.0),
        Some(Pos2::new(400.0, 320.0)),
        egui::Modifiers::default(),
    );
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - 200.0).abs() < 0.5 && (r.h - 120.0).abs() < 0.5,
        "DragScale size, got {}×{}",
        r.w,
        r.h
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
}

/// The real pointer path: arm Rect, click-release on the canvas (no drag).
/// This is the flow that used to do nothing because it waited for
/// `drag_started`.
#[test]
fn canvas_click_release_places_a_default_rect() {
    let mut h = arming_board("canvas_click_place", board::BoardTool::RectShape);
    h.frame();
    let screen = h.app.canvas_rect.center();
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(screen));
        input.events.push(egui::Event::PointerButton {
            pos: screen,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(screen));
        input.events.push(egui::Event::PointerButton {
            pos: screen,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
    });
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "click-release must place, got {:?}",
        h.app.board_tool
    );
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - board_place::place_tokens::RECT_DEFAULT_W).abs() < 0.01
            && (r.h - board_place::place_tokens::RECT_DEFAULT_H).abs() < 0.01,
        "default size, got {}×{}",
        r.w,
        r.h
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
}

/// GP3 — Rect DragScale + Shift goes through PlaceConstraint (square).
#[test]
fn arming_gp3_rect_shift_drag_is_square() {
    let mut h = arming_board("arming_gp3", board::BoardTool::RectShape);
    let mut mods = egui::Modifiers::default();
    mods.shift = true;
    h.app.finish_draw(
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 40.0),
        board::BoardTool::RectShape,
        mods,
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let r = h.app.doc().scene.nodes[0].rect;
    assert!(
        (r.w - r.h).abs() < 0.01,
        "Shift locks square, got {}×{}",
        r.w,
        r.h
    );
    assert!((r.w - 120.0).abs() < 0.01);
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(board_place::ghost_kind(h.app.board_tool).is_none());
    h.frame();
}

/// GP4 — Ellipse drag uses the same place_rect; tool returns to Select.
#[test]
fn arming_gp4_ellipse_drag_commits_and_disarms() {
    let mut h = arming_board("arming_gp4", board::BoardTool::Ellipse);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::Ellipse)
    );
    h.app.finish_draw(
        Pos2::new(0.0, 0.0),
        Pos2::new(80.0, 50.0),
        board::BoardTool::Ellipse,
        egui::Modifiers::default(),
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected an ellipse");
    };
    assert_eq!(s.shape, slate_doc::scene::ShapeKind::Ellipse);
    let r = h.app.doc().scene.nodes[0].rect;
    assert!((r.w - 80.0).abs() < 0.01 && (r.h - 50.0).abs() < 0.01);
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    h.frame();
}

/// GP5 — Esc while GhostFollow disarms to Select, no node (P0.1 Mode).
#[test]
fn arming_gp5_escape_disarms_without_a_node() {
    let mut h = arming_board("arming_gp5", board::BoardTool::Frame);
    let ctx = h.ctx.clone();
    assert!(h
        .app
        .dispatch(&ctx, atlas_commands::CommandId("app.cancel"), None));
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(h.app.doc().scene.nodes.is_empty());
    assert!(board_place::ghost_kind(h.app.board_tool).is_none());
    h.frame();
}

/// GP6 — switching tools mid-ghost swaps the silhouette, still no node.
#[test]
fn arming_gp6_tool_switch_swaps_the_silhouette() {
    let mut h = arming_board("arming_gp6", board::BoardTool::RectShape);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::RoundedRect)
    );
    h.app.set_board_tool(board::BoardTool::Ellipse);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::Ellipse)
    );
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.set_board_tool(board::BoardTool::Text);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::TextBox)
    );
    h.app.set_board_tool(board::BoardTool::Sticky);
    assert_eq!(
        board_place::ghost_kind(h.app.board_tool),
        Some(board_place::GhostKind::Sticky)
    );
    assert!(h.app.doc().scene.nodes.is_empty());
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
    current_urls: std::collections::HashMap<slate_doc::NodeId, String>,
    navigations: Vec<(slate_doc::NodeId, String)>,
}

#[derive(Default, Clone)]
struct FakeWebHost(std::rc::Rc<std::cell::RefCell<FakeLog>>);

impl FakeWebHost {
    fn inputs(&self) -> Vec<board_web::WebInput> {
        self.0.borrow().inputs.clone()
    }
    fn navigations(&self) -> Vec<(slate_doc::NodeId, String)> {
        self.0.borrow().navigations.clone()
    }
    fn set_current_url(&self, id: slate_doc::NodeId, url: &str) {
        self.0.borrow_mut().current_urls.insert(id, url.to_string());
    }
    fn current_url(&self, id: slate_doc::NodeId) -> Option<String> {
        self.0.borrow().current_urls.get(&id).cloned()
    }
    fn sent<T>(&self, pick: impl Fn(&board_web::WebInput) -> Option<T>) -> Vec<T> {
        self.inputs().iter().filter_map(pick).collect()
    }
}

impl board_web::WebHost for FakeWebHost {
    fn available(&self) -> bool {
        true
    }
    fn admit(&mut self, id: slate_doc::NodeId, req: &board_web::WebRequest) {
        let mut log = self.0.borrow_mut();
        log.admitted.insert(id);
        log.current_urls
            .entry(id)
            .or_insert_with(|| req.target.clone());
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
    fn current_url(&self, id: slate_doc::NodeId) -> Option<String> {
        self.0.borrow().current_urls.get(&id).cloned()
    }
    fn navigate(&mut self, id: slate_doc::NodeId, target: &str) -> bool {
        let mut log = self.0.borrow_mut();
        log.navigations.push((id, target.to_string()));
        log.current_urls.insert(id, target.to_string());
        true
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

/// GP1 — the draw grammar commits the default search surface. Binding a
/// different page remains a later non-modal step (D03).
#[test]
fn gp1_a_drawn_web_portal_commits_the_start_locator() {
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
    let source = p.source.expect("default placement binds the start page");
    assert_eq!(source.locator, board_web::WEB_START_LOCATOR);
    let origin = slate_doc::scene::web_origin(board_web::WEB_START_LOCATOR).expect("start origin");
    assert!(
        h.app.web.has_consent(&origin),
        "placing the start page is the human allowing that origin"
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "one-shot (D02)");
    h.frame();
    assert_ne!(h.app.web.state(id), board_web::WebState::Unbound);
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

/// Home is derived navigation back to the authored locator. It never rewrites
/// the node after the page has followed links internally.
#[test]
fn portal_web_home_returns_to_the_authored_locator_without_journaling() {
    let mut h = web_board("web_home");
    let host = with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::new(0.0, 0.0));
    let (id, before) = only_portal(&h);
    web_settle(&mut h, &[(id, 540.0)], 2);
    host.set_current_url(id, "https://example.com/result");

    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("portal.web.home"), None));

    let (_, after) = only_portal(&h);
    assert_eq!(
        after.source, before.source,
        "Home is not a journaled rebind"
    );
    assert_eq!(
        host.navigations(),
        vec![(id, board_web::WEB_START_LOCATOR.to_string())]
    );
    assert_eq!(
        host.current_url(id)
            .expect("fake host should report its derived location"),
        board_web::WEB_START_LOCATOR
    );
}

/// Binding a fixture snapshot is a journaled Patch; contents regenerate and
/// undo restores the unbound frame (GP2 / GP3).
#[test]
fn a_bound_status_board_lays_out_the_fixture_and_undo_is_frame_only() {
    let mut h = kit_board("kit_status_bind", board::BoardTool::StatusBoard);
    h.app.place_status_board_at(Pos2::new(0.0, 0.0));
    let id = h.app.doc().scene.nodes[0].id;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/status-board/tests/fixtures/project-state.json");
    assert!(fixture.is_file(), "missing fixture {}", fixture.display());
    h.app.bind_portal_source(id, fixture);

    let NodeKind::Portal(p) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal");
    };
    assert!(p.source.is_some(), "bind writes a locator");
    assert!(matches!(p.kind, slate_doc::scene::PortalKind::StatusBoard));

    let mut caption = None;
    for _ in 0..80 {
        h.frame();
        caption = h.app.portals.status_caption(id);
        if caption.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let caption = caption.expect("status layout should arrive");
    assert!(
        caption.contains("664e2e1"),
        "layout caption should name the fixture HEAD: {caption}"
    );

    h.app.board_undo();
    let NodeKind::Portal(p) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal after undo");
    };
    assert!(
        p.source.is_none(),
        "undo is frame-only — source returns to None"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "no orphan baked nodes");
    h.frame();
}

fn agent_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h
}

#[test]
fn a_placed_agent_portal_has_no_identity_tab_and_no_folder() {
    let mut h = agent_board("agent_place");
    h.app.place_agent_portal_at(Pos2::new(0.0, 0.0));
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Portal(p) = &node.kind else {
        panic!("expected a portal");
    };
    assert_eq!(p.kind, slate_doc::scene::PortalKind::Agent);
    assert!(p.source.is_none(), "a new agent portal is unbound");
    assert!(
        !super::board_portal_chrome::uses_identity_tab(p.kind),
        "the web identity tab must not land on agent portals"
    );
    h.frame();
}

#[test]
fn binding_an_agent_portal_folder_survives_save_and_reopen() {
    let mut h = agent_board("agent_bind");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    let folder = h.base.join("climate-grid");
    std::fs::create_dir_all(&folder).unwrap();
    h.app.bind_agent_project(id, folder.clone());

    let NodeKind::Portal(p) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal");
    };
    assert!(p.source.is_some(), "bind journals a locator");
    assert_eq!(p.title, "climate-grid");

    let path = h.base.join("book.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, path.clone());

    let mut h2 = Harness::new("agent_reopen");
    h2.app.open_doc_at(path);
    h2.app.doc_mut().view.active_view = ViewKind::Board;
    h2.frame();
    let NodeKind::Portal(p) = &h2.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal after reopen");
    };
    assert_eq!(p.kind, slate_doc::scene::PortalKind::Agent);
    let locator = p
        .source
        .as_ref()
        .expect("saved workbook remembers the folder")
        .locator
        .as_str();
    let resolved = super::board_portal::resolve_source(h2.app.tab().path.as_deref(), locator);
    let got = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    let want = std::fs::canonicalize(&folder).unwrap_or(folder);
    assert_eq!(got, want);
}

#[test]
fn selecting_an_agent_portal_does_not_capture_the_wheel() {
    let mut h = agent_board("agent_select_no_capture");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let xf = h.app.board_xf();
    let center = xf.rect_w2s(h.app.doc().scene.nodes[0].rect).center();
    assert!(
        !h.app.agent_shelf_captures(&xf, Some(center)),
        "selection is not contents-focus — the board keeps the wheel"
    );
}

#[test]
fn a_focused_agent_portal_captures_the_wheel() {
    let mut h = agent_board("agent_focus_captures");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.agent_focus(id);
    h.frame();
    let xf = h.app.board_xf();
    let center = xf.rect_w2s(h.app.doc().scene.nodes[0].rect).center();
    assert!(
        h.app.agent_shelf_captures(&xf, Some(center)),
        "contents-focus hands the wheel to the shelf"
    );
    assert!(h.app.agent_blur());
    assert!(
        !h.app.agent_shelf_captures(&xf, Some(center)),
        "Esc peels focus and the board keeps the wheel again"
    );
}

#[test]
fn an_agent_channel_survives_save_and_reopen() {
    let mut h = agent_board("agent_channel");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_channel(id, Some("composer-1".into()));
    let path = h.base.join("book.slate");
    let tab = h.app.tab().id;
    h.app.save_doc_to(tab, path.clone());
    let mut h2 = Harness::new("agent_channel_reopen");
    h2.app.open_doc_at(path);
    h2.app.doc_mut().view.active_view = ViewKind::Board;
    h2.frame();
    let NodeKind::Portal(p) = &h2.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal after reopen");
    };
    assert_eq!(
        p.agent.as_ref().and_then(|a| a.channel.as_deref()),
        Some("composer-1")
    );
}

#[test]
fn a_failed_agent_send_names_the_failure() {
    let mut h = agent_board("agent_named_fail");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.ai.config.workspace_dir = Some(std::path::PathBuf::from("/definitely/not/here"));
    *h.app.agents.prompt_mut(id) = "what is 2+2?".into();
    h.app.send_agent_prompt(id);
    let reason = h
        .app
        .agent_failure_reason(id)
        .expect("failure must be a named state, never a blank");
    assert!(
        reason.contains("AI workspace"),
        "named the missing workspace: {reason}"
    );
    assert!(!h.app.agent_is_awaiting(id));
}

#[test]
fn sending_a_prompt_shows_thinking_not_silence() {
    let mut h = agent_board("agent_thinking");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    let ws = h.base.join("ai-ws");
    std::fs::create_dir_all(&ws).unwrap();
    h.app.ai.config.workspace_dir = Some(ws);
    *h.app.agents.prompt_mut(id) = "what is 2+2?".into();
    h.app.send_agent_prompt(id);
    assert!(
        h.app.agent_is_awaiting(id),
        "Send must enter Thinking — never a blank wait"
    );
    assert!(h.app.agent_failure_reason(id).is_none());
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

/// Maximize is derived: the node rect is untouched, Esc restores, and the
/// page keeps its focus (P1.portal.maximize).
#[test]
fn maximize_covers_the_window_without_mutating_the_frame() {
    let mut h = web_board("web_maximize");
    with_fake_host(&mut h);
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.web_maximize(id);
    assert_eq!(h.app.portal_chrome.maximized, Some(id));
    assert_eq!(h.app.web.focused, Some(id));
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rect,
        rect,
        "maximize is not a journaled resize"
    );
    h.app.web_restore();
    assert!(h.app.portal_chrome.maximized.is_none());
    assert_eq!(h.app.web.focused, Some(id), "restore keeps page focus");
}

/// Identity-tab fold is per-portal derived state (P1.portal.chrome).
#[test]
fn portal_tab_fold_toggles_without_a_journal_entry() {
    let mut h = web_board("web_tab_fold");
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    assert!(!h.app.portal_chrome_collapsed(id));
    assert!(h.app.portal_toggle_chrome(id));
    assert!(h.app.portal_chrome_collapsed(id));
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rect,
        rect,
        "folding the tab is not a frame mutation"
    );
}

/// Right-click paste rebinds that portal (journaled Patch).
#[test]
fn paste_url_rebinds_the_selected_portal() {
    let mut h = web_board("web_paste_url");
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.board_sel = std::iter::once(id).collect();
    assert!(h.app.web_paste_url_text("https://example.com/b"));
    let (_, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some("https://example.com/b")
    );
    h.app.board_undo();
    let (_, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().map(|s| s.locator.as_str()),
        Some("https://example.com/a")
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

/// Clicking outside a focused page makes it a preview card again; the click is
/// Slate's, not another page input event.
#[test]
fn primary_click_outside_a_focused_page_releases_focus() {
    let (mut h, id, host) = focused_page("web_click_out");
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap();
    let frame = xf.rect_w2s(node.rect);
    let outside = if frame.left() > 24.0 {
        Pos2::new(frame.left() - 12.0, frame.center().y)
    } else {
        Pos2::new(frame.right() + 12.0, frame.center().y)
    };
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(outside));
        input.events.push(egui::Event::PointerButton {
            pos: outside,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });

    assert_eq!(h.app.web.focused, None, "outside click released the page");
    assert!(
        host.sent(|i| match i {
            board_web::WebInput::Down { .. } => Some(()),
            _ => None,
        })
        .is_empty(),
        "the outside click must not be forwarded to the page"
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

#[test]
fn workbook_camera_round_trips_through_save() {
    let mut h = Harness::new("cam_rt");
    h.seed();
    h.app.tab_mut().cam.offset = EVec2::new(120.0, -40.0);
    h.app.tab_mut().cam.z = 1.6;
    let path = h.base.join("cam.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());

    let mut h2 = Harness::new("cam_rt_load");
    h2.app.open_doc_at(path);
    assert!((h2.app.tab().cam.offset.x - 120.0).abs() < 1e-3);
    assert!((h2.app.tab().cam.offset.y + 40.0).abs() < 1e-3);
    assert!((h2.app.tab().cam.z - 1.6).abs() < 1e-3);
}

#[test]
fn grid_layout_is_cached_across_unchanged_paints() {
    let mut h = Harness::new("layout_cache");
    h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Grid;
    h.app.layout_cache = None;
    h.app.layout_builds = 0;
    h.frame();
    h.frame();
    assert_eq!(
        h.app.layout_builds, 1,
        "an unchanged doc + size must not rebuild Grid/Venn layout"
    );
}

#[test]
fn board_paint_culls_offscreen_nodes() {
    let mut h = Harness::new("board_cull");
    h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    for i in 0..200 {
        add_rect(&mut h.app, (i as f32) * 400.0, 0.0);
    }
    h.app.canvas_rect = ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0));
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.app.tab_mut().cam.z = 1.0;
    let painted = h.app.board_paint_nodes(h.app.canvas_rect);
    assert!(
        painted.len() < 200,
        "viewport cull must drop off-screen nodes, got {}",
        painted.len()
    );
    assert!(
        !painted.is_empty(),
        "the camera must still see nearby nodes"
    );
}

#[test]
fn slate_thumb_lru_caps_resident_textures() {
    let mut h = Harness::new("thumb_lru");
    let cap = atlas_core::display::SLATE_TEXTURES.resident_cap;
    for i in 0..(cap + 100) {
        let k = format!("k{i}");
        h.app.textures.insert(k.clone(), ThumbState::Failed);
        h.app
            .thumb_pixels
            .insert(k.clone(), egui::ColorImage::example());
        h.app.thumb_used.insert(k, i as u64);
    }
    h.app.evict_thumbs();
    assert!(h.app.textures.len() <= cap);
    assert_eq!(h.app.textures.len(), h.app.thumb_pixels.len());
}

/// Wire grips preview only at the handle under the pointer — not the whole edge.
#[test]
fn wire_grips_preview_only_the_handle_under_the_pointer() {
    let mut h = web_board("wire_grip_prox");
    add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.nodes[0].clone();
    // Midway along the top edge, well away from the side-midpoint grip.
    let between = xf.w2s(Pos2::new(n.rect.x + 12.0, n.rect.y));
    h.app.update_wire_grips(Some(between), &xf);
    assert!(
        h.app.wire_grips.is_none(),
        "an edge between grips must not preview a wire handle"
    );

    let grip = xf.w2s(board_wire::grip_point(n.rect, slate_doc::scene::Side::Top));
    h.app.update_wire_grips(Some(grip), &xf);
    assert_eq!(
        h.app.wire_grips.map(|g| (g.node, g.hovered)),
        Some((n.id, Some(slate_doc::scene::Side::Top))),
        "only the grip under the pointer previews"
    );
}

/// A press on the displayed wire handle starts a wire, even when that
/// point is also on the resize band and the live hover cache has cleared
/// (egui's drag threshold often leaves the 8 px dot before drag_started).
#[test]
fn wire_grip_press_beats_edge_resize() {
    let mut h = web_board("wire_grip_beats_resize");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip = xf.w2s(board_wire::grip_point(rect, slate_doc::scene::Side::Top));
    let world = xf.s2w(grip);

    // Simulate the live pointer having already left the dot.
    h.app.wire_grips = None;
    let mods = egui::Modifiers::default();
    let drag = h.app.begin_gesture_for_test(grip, world, mods);
    assert!(
        matches!(drag, Some(board::BoardDrag::Wire(_))),
        "press on the wire handle must start a wire"
    );
    assert!(
        h.app.begin_transform_drag(grip, world).is_none(),
        "resize must stay suppressed on the wire handle"
    );

    let edge = xf.w2s(Pos2::new(rect.x + 12.0, rect.y));
    let edge_world = xf.s2w(edge);
    let edge_drag = h.app.begin_gesture_for_test(edge, edge_world, mods);
    assert!(
        matches!(edge_drag, Some(board::BoardDrag::Resize { .. })),
        "the rest of the edge must still resize"
    );
}

/// Bounding-box chrome is live on hover — no prior selection (P1.node.transform).
#[test]
fn bbox_chrome_is_live_without_selection() {
    let mut h = web_board("bbox_chrome");
    add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.nodes[0].clone();
    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);

    let hit = h.app.transform_hit_at(geom.edges[1]);
    assert!(
        matches!(
            hit,
            Some((
                Some(_),
                board_handles::BoardHitTarget::Resize(board_handles::ResizeHandle::E)
            ))
        ),
        "right-edge hover must resize without a selection, got {hit:?}"
    );

    let hit = h.app.transform_hit_at(geom.rotate_points[0]);
    assert!(
        matches!(hit, Some((_, board_handles::BoardHitTarget::Rotate(_)))),
        "outside-corner hover must rotate a shape, got {hit:?}"
    );

    // Press the edge away from the wire-grip midpoint so resize, not a
    // wire, is the gesture (P1.node.transform).
    let edge = xf.w2s(Pos2::new(n.rect.x + n.rect.w, n.rect.y + 12.0));
    let world = xf.s2w(edge);
    let drag = h.app.begin_transform_drag(edge, world);
    assert!(
        matches!(drag, Some(board::BoardDrag::Resize { .. })),
        "pressing an edge on an unselected node starts resize"
    );
    assert!(
        h.app.board_sel.contains(&n.id),
        "starting a resize selects the node"
    );
}

/// Edge hover changes the cursor target only — no body highlight (and
/// therefore no selection-look tab / handle chrome).
#[test]
fn edge_hover_does_not_arm_a_body_highlight() {
    let mut h = web_board("edge_hover_cursor");
    add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.nodes[0].clone();
    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
    let ctx = h.ctx.clone();
    h.app
        .hover_transform_chrome(Some(geom.edges[1]), &xf, &ctx, false);
    assert!(
        matches!(
            h.app.board_hover_hit,
            Some(board_handles::BoardHitTarget::Resize(_))
        ),
        "edge hover must still be a resize hit"
    );
    let world = xf.s2w(geom.edges[1]);
    assert!(
        h.app.hover_preview_target(Some(world)).is_none(),
        "edge hover must not highlight the node like a selection"
    );
}

/// Interior hover is the eased preview target; the node is still unselected.
#[test]
fn body_hover_targets_the_unselected_node() {
    let mut h = web_board("body_hover_preview");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap();
    let center = xf.w2s(Pos2::new(
        n.rect.x + n.rect.w * 0.5,
        n.rect.y + n.rect.h * 0.5,
    ));
    let ctx = h.ctx.clone();
    h.app.hover_transform_chrome(Some(center), &xf, &ctx, false);
    assert!(
        h.app.board_hover_hit.is_none(),
        "the interior is not resize chrome"
    );
    let world = xf.s2w(center);
    assert_eq!(h.app.hover_preview_target(Some(world)), Some(id));
    assert!(h.app.board_sel.is_empty(), "hover never selects");
}

/// Selection outline follows painted geometry — fillets and ellipses, not the AABB.
#[test]
fn selection_outline_follows_silhouette() {
    let mut h = web_board("sel_silhouette");
    let square = add_rect(&mut h.app, 0.0, 0.0);
    let rounded = {
        use slate_doc::scene::{ShapeKind, ShapeNode};
        let rect = slate_doc::scene::WorldRect::new(100.0, 0.0, 80.0, 60.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(slate_doc::scene::Rgba::WHITE),
                stroke: slate_doc::scene::Stroke::none(),
                corner: slate_doc::scene::Corner::Rounded { radius: 12.0 },
                flip: false,
                path: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    };
    let ellipse = {
        use slate_doc::scene::{ShapeKind, ShapeNode};
        let rect = slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 60.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Ellipse,
                fill: Some(slate_doc::scene::Rgba::WHITE),
                stroke: slate_doc::scene::Stroke::none(),
                corner: slate_doc::scene::Corner::Square,
                flip: false,
                path: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    };
    h.app.place_web_portal_at(Pos2::new(400.0, 40.0));
    h.frame();
    let xf = h.app.board_xf();
    let sq = h.app.doc().scene.node(square).unwrap();
    assert_eq!(h.app.node_screen_outline(&xf, sq).len(), 4);

    let rd = h.app.doc().scene.node(rounded).unwrap();
    let rd_pts = h.app.node_screen_outline(&xf, rd);
    assert!(
        rd_pts.len() > 4,
        "a filleted rect must not highlight as a sharp box"
    );
    let srect = xf.rect_w2s(rd.rect);
    let sharp = srect.left_top();
    assert!(
        rd_pts.iter().all(|p| p.distance(sharp) > 2.0),
        "the highlight must leave the sharp corner empty"
    );

    let el = h.app.doc().scene.node(ellipse).unwrap();
    assert!(
        h.app.node_screen_outline(&xf, el).len() > 4,
        "an ellipse highlight must be circular, not a box"
    );

    let portal = h.app.doc().scene.nodes.iter().rev().next().unwrap();
    let p_pts = h.app.node_screen_outline(&xf, portal);
    assert!(
        p_pts.len() > 4,
        "a portal highlight must follow the shared fillet"
    );

    let strip = add_dock_strip(&mut h.app, &["shape.rect"]);
    h.frame();
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let sn = h.app.doc().scene.node(strip).unwrap();
    let s_pts = h.app.node_screen_outline(&xf, sn);
    assert!(
        s_pts.len() > 4,
        "a dock-strip highlight must follow the shared fillet"
    );
    let r = super::board_dock_embed::dock_strip_corner_radius(xf.z);
    let sharp = xf.rect_w2s(sn.rect).left_top();
    assert!(
        s_pts.iter().all(|p| p.distance(sharp) > r * 0.3),
        "the dock-strip highlight must leave the sharp corner empty"
    );
}

fn add_dock_strip(app: &mut SlateApp, tools: &[&str]) -> NodeId {
    let n = tools.len().max(1) as f32;
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, n * 40.0 + 16.0, 48.0);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::DockStrip(slate_doc::scene::DockStripNode {
            palette_id: "tool.shapes".into(),
            visible: tools.iter().map(|s| (*s).to_string()).collect(),
        }),
    );
    app.add_nodes(vec![node])[0]
}

/// A click on a dropped-toolbar icon arms the command and does not place.
#[test]
fn dock_strip_click_arms_without_placing() {
    let mut h = web_board("dock_arm");
    let id = add_dock_strip(&mut h.app, &["text.block"]);
    h.frame();
    let n = h.app.doc().scene.node(id).unwrap();
    let world = Pos2::new(n.rect.x + n.rect.w * 0.5, n.rect.y + n.rect.h * 0.5);
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.try_dock_embed_click(&h.ctx, world));
    assert_eq!(h.app.board_tool, board::BoardTool::Text);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        before,
        "arming from a dropped toolbar must not place"
    );
}

/// Press-drag on a dropped toolbar moves it even when a create tool is armed.
#[test]
fn dock_strip_drag_moves_regardless_of_armed_tool() {
    let mut h = web_board("dock_move");
    let id = add_dock_strip(&mut h.app, &["shape.rect"]);
    h.app.set_board_tool(board::BoardTool::RectShape);
    h.frame();
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap();
    let world = Pos2::new(n.rect.x + n.rect.w * 0.5, n.rect.y + n.rect.h * 0.5);
    let screen = xf.w2s(world);
    let drag = h
        .app
        .begin_gesture_for_test(screen, world, egui::Modifiers::NONE);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "press-drag on a dropped toolbar must move it, not draw"
    );
}

/// Portals stay axis-aligned: no rotate chrome, but edges still resize.
#[test]
fn portals_do_not_rotate() {
    let mut h = web_board("portal_norot");
    h.app.place_web_portal_at(Pos2::new(0.0, 0.0));
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let n = h.app.doc().scene.nodes[0].clone();
    assert!(
        !SlateApp::node_allows_rotation(&n),
        "portals must not offer rotation"
    );

    let xf = h.app.board_xf();
    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);

    let hit = h.app.transform_hit_at(geom.rotate_points[0]);
    assert!(
        !matches!(hit, Some((_, board_handles::BoardHitTarget::Rotate(_)))),
        "outside-corner hover must not rotate a portal, got {hit:?}"
    );

    let hit = h.app.transform_hit_at(geom.edges[1]);
    assert!(
        matches!(
            hit,
            Some((
                Some(_),
                board_handles::BoardHitTarget::Resize(board_handles::ResizeHandle::E)
            ))
        ),
        "portal edges still resize, got {hit:?}"
    );
}

// ---------- Align widget golden paths (contracts/align-widget.md GP1–GP5) ----------

fn align_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    h
}

fn select_ids(app: &mut SlateApp, ids: &[NodeId]) {
    app.board_sel = ids.iter().copied().collect();
}

/// GP1 — two offset rects · align left → both share x = 0.
#[test]
fn align_gp1_align_left() {
    let mut h = align_board("align_gp1");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 40.0, 30.0);
    select_ids(&mut h.app, &[a, b]);
    assert!(h.app.align_board_selection(board::BoardAlign::Left));
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    assert!((ra.x - 0.0).abs() < 1e-4, "left datum, got {}", ra.x);
    assert!((rb.x - 0.0).abs() < 1e-4, "left datum, got {}", rb.x);
    assert!((ra.y - 0.0).abs() < 1e-4, "y must not change");
    assert!((rb.y - 30.0).abs() < 1e-4, "y must not change");
}

/// GP2 — same rects · align bottom → both bottom edges share y+h = 90.
#[test]
fn align_gp2_align_bottom() {
    let mut h = align_board("align_gp2");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 40.0, 30.0);
    select_ids(&mut h.app, &[a, b]);
    assert!(h.app.align_board_selection(board::BoardAlign::Bottom));
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    assert!(((ra.y + ra.h) - 90.0).abs() < 1e-4);
    assert!(((rb.y + rb.h) - 90.0).abs() < 1e-4);
}

/// GP3 — three rects · distribute horizontal → ends stay, equal gaps.
#[test]
fn align_gp3_distribute_horizontal() {
    let mut h = align_board("align_gp3");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 100.0, 0.0);
    let c = add_rect(&mut h.app, 400.0, 0.0);
    select_ids(&mut h.app, &[a, b, c]);
    assert!(h
        .app
        .distribute_board_selection(board::DistributeAxis::Horizontal));
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    let rc = h.app.doc().scene.node(c).unwrap().rect;
    assert!((ra.x - 0.0).abs() < 1e-3);
    assert!((rc.x - 400.0).abs() < 1e-3);
    let g0 = rb.x - (ra.x + ra.w);
    let g1 = rc.x - (rb.x + rb.w);
    assert!((g0 - g1).abs() < 1e-3, "gaps {g0} vs {g1}");
}

/// GP4 — align is one undo step.
#[test]
fn align_gp4_one_undo() {
    let mut h = align_board("align_gp4");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 40.0, 30.0);
    select_ids(&mut h.app, &[a, b]);
    assert!(h.app.align_board_selection(board::BoardAlign::Left));
    h.app.board_undo();
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    assert!((ra.x - 0.0).abs() < 1e-4 && (ra.y - 0.0).abs() < 1e-4);
    assert!((rb.x - 40.0).abs() < 1e-4 && (rb.y - 30.0).abs() < 1e-4);
}

/// GP5 — icons sit outside the group box; the inner bottom edge is not an align hit.
#[test]
fn align_gp5_widget_hit_misses_the_box() {
    let mut h = align_board("align_gp5");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 120.0, 40.0);
    select_ids(&mut h.app, &[a, b]);
    h.frame();
    let xf = h.app.board_xf();
    let gb = h.app.board_group_bounds().expect("group bounds");
    let bbox = xf.rect_w2s(gb);
    let inner_bottom = Pos2::new(bbox.center().x, bbox.bottom());
    assert_eq!(
        h.app.align_action_at(inner_bottom),
        None,
        "inner bottom edge is resize, not align"
    );
    let cluster = 4.0 * board_align::ICON_PX + 3.0 * board_align::ICON_GAP_PX;
    let icon = Pos2::new(
        bbox.center().x - cluster * 0.5 + board_align::ICON_PX * 0.5,
        bbox.bottom() + board_align::FRAME_OUTSET_PX,
    );
    assert_eq!(
        h.app.align_action_at(icon),
        Some(board_align::AlignAction::Align(board::BoardAlign::Left))
    );
    let old_top = Pos2::new(
        bbox.center().x - cluster * 0.5 + board_align::ICON_PX * 0.5,
        bbox.top() - board_align::FRAME_OUTSET_PX,
    );
    assert_eq!(h.app.align_action_at(old_top), None, "top cluster is gone");
}

/// Ctrl+Alt+Shift on a group edge: members keep size and only translate.
#[test]
fn group_reposition_keeps_member_size() {
    let mut h = align_board("group_reposition");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 120.0, 0.0);
    select_ids(&mut h.app, &[a, b]);
    let gb = h.app.board_group_bounds().expect("group bounds");
    let before: Vec<_> = [a, b]
        .iter()
        .map(|id| h.app.doc().scene.node(*id).unwrap().clone())
        .collect();
    h.app.board_drag = Some(board::BoardDrag::GroupResize {
        ids: vec![a, b],
        before,
        group_before: gb,
        handle: board_handles::ResizeHandle::E as u8,
    });
    let mut mods = egui::Modifiers::default();
    mods.ctrl = true;
    mods.alt = true;
    mods.shift = true;
    h.app.update_gesture_for_test(Pos2::new(300.0, 30.0), mods);
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    assert!((ra.w - 80.0).abs() < 1e-3 && (ra.h - 60.0).abs() < 1e-3);
    assert!((rb.w - 80.0).abs() < 1e-3 && (rb.h - 60.0).abs() < 1e-3);
    assert!((ra.x - 0.0).abs() < 1e-2, "left item stays, got {}", ra.x);
    assert!(
        (rb.x - 180.0).abs() < 1e-2,
        "right item translates, got {}",
        rb.x
    );
}

/// Every group-box handle × (scale | Ctrl+Alt+Shift): opposite union
/// handle stays put, and keep-size never changes member w/h. Nw / Ne / Sw
/// used to translate the whole stack; some corners looked one-axis-locked.
#[test]
fn group_every_handle_scale_and_reposition() {
    use super::board_snap;
    use board_handles::ResizeHandle;
    use slate_doc::scene::WorldRect;

    fn union_of(app: &SlateApp, ids: &[NodeId]) -> WorldRect {
        let rects: Vec<WorldRect> = ids
            .iter()
            .map(|id| app.doc().scene.node(*id).unwrap().rect)
            .collect();
        board_snap::union_rect(&rects).unwrap()
    }

    fn almost(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05
    }

    // Pointers that double a 200×100 group box uniformly (corners) or on
    // one axis (edges). The second member is offset on both axes so every
    // handle has layout to scale — a flat row made N/S look like a no-op.
    let cases: [(u8, Pos2); 8] = [
        (ResizeHandle::Nw as u8, Pos2::new(-200.0, -100.0)),
        (ResizeHandle::N as u8, Pos2::new(100.0, -100.0)),
        (ResizeHandle::Ne as u8, Pos2::new(400.0, -100.0)),
        (ResizeHandle::E as u8, Pos2::new(400.0, 50.0)),
        (ResizeHandle::Se as u8, Pos2::new(400.0, 200.0)),
        (ResizeHandle::S as u8, Pos2::new(100.0, 200.0)),
        (ResizeHandle::Sw as u8, Pos2::new(-200.0, 200.0)),
        (ResizeHandle::W as u8, Pos2::new(-200.0, 50.0)),
    ];

    for (handle, pointer) in cases {
        for reposition in [false, true] {
            let mut h = align_board(&format!("grp_{handle}_{reposition}"));
            let a = add_rect(&mut h.app, 0.0, 0.0);
            let b = add_rect(&mut h.app, 120.0, 40.0);
            select_ids(&mut h.app, &[a, b]);
            let gb = h.app.board_group_bounds().expect("group bounds");
            let before: Vec<_> = [a, b]
                .iter()
                .map(|id| h.app.doc().scene.node(*id).unwrap().clone())
                .collect();
            h.app.board_drag = Some(board::BoardDrag::GroupResize {
                ids: vec![a, b],
                before,
                group_before: gb,
                handle,
            });
            let mut mods = egui::Modifiers::default();
            if reposition {
                mods.ctrl = true;
                mods.alt = true;
                mods.shift = true;
            }
            h.app.update_gesture_for_test(pointer, mods);
            let ra = h.app.doc().scene.node(a).unwrap().rect;
            let rb = h.app.doc().scene.node(b).unwrap().rect;
            if reposition {
                assert!(
                    (ra.w - 80.0).abs() < 1e-3 && (ra.h - 60.0).abs() < 1e-3,
                    "handle {handle} keep-size: A sized {}×{}",
                    ra.w,
                    ra.h
                );
                assert!(
                    (rb.w - 80.0).abs() < 1e-3 && (rb.h - 60.0).abs() < 1e-3,
                    "handle {handle} keep-size: B sized {}×{}",
                    rb.w,
                    rb.h
                );
            } else {
                assert!(
                    ra.w > 80.0 + 1.0 || ra.h > 60.0 + 1.0,
                    "handle {handle} scale: members did not grow"
                );
            }
            let union = union_of(&h.app, &[a, b]);
            let old_a = board_snap::resize_anchor(gb, handle, false);
            let new_a = board_snap::resize_anchor(union, handle, false);
            assert!(
                almost(old_a, new_a),
                "handle {handle} reposition={reposition}: opposite walked {old_a:?} → {new_a:?}"
            );
            let old_g = board_snap::handle_local(gb, handle);
            let new_g = board_snap::handle_local(union, handle);
            let grabbed = (old_g.0 - new_g.0).abs() + (old_g.1 - new_g.1).abs();
            assert!(
                grabbed > 1.0,
                "handle {handle} reposition={reposition}: grabbed handle did not move"
            );
        }
    }
}

/// After a 180° rotate, grabbing the visual top edge must move that edge
/// — not the far (visual bottom) edge. Rotation is about the live center,
/// so local AABB math alone walks the opposite world edge.
#[test]
fn rotated_180_resize_moves_the_grabbed_edge() {
    let mut h = web_board("rot180_resize");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    if let Some(n) = h.app.doc_mut().scene.node_mut(id) {
        n.rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 80.0);
        n.rotation_deg = 180.0;
    }
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
    // After 180°, corners[2]→[3] (local S) is the visual top. Stay off the
    // midpoint so a wire grip does not swallow the press.
    let screen = geom.corners[2] + (geom.corners[3] - geom.corners[2]) * 0.25;
    let world = xf.s2w(screen);
    let drag = h.app.begin_transform_drag(screen, world);
    let ok = matches!(&drag, Some(board::BoardDrag::Resize { handle: 5, .. }));
    assert!(ok, "visual top after 180° is local S");
    h.app.board_drag = drag;

    let top0 = n
        .rect
        .corners_rotated(n.rotation_deg)
        .into_iter()
        .map(|c| c.1)
        .fold(f32::INFINITY, f32::min);
    let bot0 = n
        .rect
        .corners_rotated(n.rotation_deg)
        .into_iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let (cx, _) = n.rect.center();
    let mut mods = egui::Modifiers::default();
    mods.alt = true; // skip object-snap so the pin is the only translation
    h.app
        .update_gesture_for_test(Pos2::new(cx, top0 - 20.0), mods);

    let after = h.app.doc().scene.node(id).unwrap();
    let top1 = after
        .rect
        .corners_rotated(after.rotation_deg)
        .into_iter()
        .map(|c| c.1)
        .fold(f32::INFINITY, f32::min);
    let bot1 = after
        .rect
        .corners_rotated(after.rotation_deg)
        .into_iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (bot1 - bot0).abs() < 0.05,
        "visual bottom walked: {bot0} → {bot1}"
    );
    assert!(
        (top1 - (top0 - 20.0)).abs() < 0.05,
        "visual top should follow the pointer: {top0} → {top1}"
    );
}

/// A wide 2+ group box still offers a 45° corner-resize cursor (P1.node.transform).
#[test]
fn group_box_corner_is_diagonal_resize() {
    let mut h = web_board("group_corner_cursor");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 160.0, 0.0);
    select_ids(&mut h.app, &[a, b]);
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let gb = h.app.board_group_bounds().expect("group bounds");
    let geom = board_handles::selection_geom(&xf, gb, 0.0);
    let hit = h.app.transform_hit_at(geom.corners[0]);
    assert!(
        matches!(
            hit,
            Some((
                None,
                board_handles::BoardHitTarget::Resize(board_handles::ResizeHandle::Nw)
            ))
        ),
        "group-box corner must be a corner resize, got {hit:?}"
    );
    assert_eq!(
        board_handles::cursor_for_resize(board_handles::ResizeHandle::Nw, &geom),
        egui::CursorIcon::ResizeNorthWest
    );
}

// ---------- Trim golden paths (contracts/trim.md GP1–GP6) ----------

fn trim_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h
}

fn add_seg(app: &mut SlateApp, a: Pos2, b: Pos2) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let (rect, path) = board_path::points_to_path_data(&[a, b], false);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: Some(path),
        }),
    );
    app.add_nodes(vec![node])[0]
}

fn add_filled_rect(app: &mut SlateApp, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let rect = slate_doc::scene::WorldRect::new(x, y, w, h);
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
    app.add_nodes(vec![node])[0]
}

fn add_filled_ellipse(app: &mut SlateApp, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let rect = slate_doc::scene::WorldRect::new(x, y, w, h);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Ellipse,
            fill: Some(slate_doc::scene::Rgba::WHITE),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: None,
        }),
    );
    app.add_nodes(vec![node])[0]
}

fn select_trim(app: &mut SlateApp, ids: &[NodeId]) {
    app.board_sel.clear();
    for id in ids {
        app.board_sel.insert(*id);
    }
}

/// GP1 — two crossing lines, preselect, click one half of the horizontal.
#[test]
fn trim_gp1_crossing_lines() {
    let mut h = trim_board("trim_gp1");
    let horiz = add_seg(&mut h.app, Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0));
    let vert = add_seg(&mut h.app, Pos2::new(50.0, 0.0), Pos2::new(50.0, 100.0));
    select_trim(&mut h.app, &[horiz, vert]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert_eq!(h.app.board_tool, board::BoardTool::Trim);
    assert!(h.app.trim.as_ref().is_some_and(|s| s.cutters.len() == 2));
    assert!(h.app.trim_click(Pos2::new(25.0, 50.0), false));
    let n = h.app.doc().scene.node(horiz).expect("horizontal remains");
    match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => {
            assert_eq!(s.shape, slate_doc::scene::ShapeKind::Path);
            let path = s.path.as_ref().unwrap();
            assert!(!path.closed);
            // Remaining half should live on the right of the crossing.
            assert!(n.rect.x + n.rect.w > 50.0);
            assert!(n.rect.x >= 49.0);
        }
        _ => panic!("expected a path"),
    }
    h.app.board_undo();
    let restored = h.app.doc().scene.node(horiz).unwrap();
    assert!(restored.rect.x < 1.0);
    h.frame();
}

/// GP2 — rect cut by a vertical line; click the left half.
#[test]
fn trim_gp2_rect_cut_by_line() {
    let mut h = trim_board("trim_gp2");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let line = add_seg(&mut h.app, Pos2::new(50.0, -10.0), Pos2::new(50.0, 90.0));
    select_trim(&mut h.app, &[rect, line]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(20.0, 40.0), false));
    let n = h.app.doc().scene.node(rect).expect("rect remains");
    match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => {
            assert_eq!(s.shape, slate_doc::scene::ShapeKind::Path);
            assert!(s.path.as_ref().is_some_and(|p| p.closed));
        }
        _ => panic!("expected a path"),
    }
    assert!(
        n.rect.x >= 49.0,
        "left half should be gone, rect={:?}",
        n.rect
    );
    assert_axis_aligned_world(&node_world_contours(&h.app, rect));
    h.frame();
}

/// GP3 — circle inside a rect; click the circle punches a hole.
#[test]
fn trim_gp3_hole_punch() {
    let mut h = trim_board("trim_gp3");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    let circle = add_filled_ellipse(&mut h.app, 30.0, 30.0, 40.0, 40.0);
    select_trim(&mut h.app, &[rect, circle]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(50.0, 50.0), false));
    let n = h.app.doc().scene.node(rect).expect("rect remains");
    match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => {
            let path = s.path.as_ref().expect("rewritten path");
            assert!(
                !path.extra.is_empty()
                    || matches!(path.fill_rule, slate_doc::scene::PathFillRule::EvenOdd),
                "hole punch must be a compound even-odd path"
            );
            let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
            let contours = vector_ink::flatten_contours(&bez, 0.35);
            assert!(
                vector_ink::point_in_polygon(&contours, [5.0, 5.0]),
                "rect corner must remain"
            );
            assert!(
                !vector_ink::point_in_polygon(&contours, [50.0, 50.0]),
                "circle centre must be a hole"
            );
        }
        _ => panic!("expected a path"),
    }
    h.frame();
}

/// GP4 — Ctrl+T arms Trim; Ctrl+N still opens a tab.
#[test]
fn trim_gp4_ctrl_n_still_new_tab() {
    let mut h = trim_board("trim_gp4");
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.tool.trim"), None);
    assert_eq!(h.app.board_tool, board::BoardTool::Trim);
    let before = h.app.tabs.len();
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.new_tab"), None);
    assert_eq!(h.app.tabs.len(), before + 1);
    h.frame();
}

/// GP5 — two clicks, one undo restores only the last.
#[test]
fn trim_gp5_per_click_undo() {
    let mut h = trim_board("trim_gp5");
    let a = add_seg(&mut h.app, Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0));
    let c1 = add_seg(&mut h.app, Pos2::new(30.0, -10.0), Pos2::new(30.0, 10.0));
    let c2 = add_seg(&mut h.app, Pos2::new(70.0, -10.0), Pos2::new(70.0, 10.0));
    select_trim(&mut h.app, &[a, c1, c2]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(15.0, 0.0), false));
    let after_first = h.app.doc().scene.node(a).unwrap().rect;
    assert!(h.app.trim_click(Pos2::new(85.0, 0.0), false));
    let after_second = h.app.doc().scene.node(a).map(|n| n.rect);
    h.app.board_undo();
    let undone = h.app.doc().scene.node(a).unwrap().rect;
    assert_eq!(undone, after_first, "one undo peels one click");
    let _ = after_second;
    h.frame();
}

/// GP6 — Esc peels TrimParts → PickCutters → Select.
#[test]
fn trim_gp6_esc_stack() {
    let mut h = trim_board("trim_gp6");
    let cutter = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::board_trim::TrimPhase::PickCutters
    );
    assert!(h.app.trim_click(Pos2::new(20.0, 20.0), false));
    assert!(h.app.trim.as_ref().unwrap().cutters.contains(&cutter));
    h.app.trim_enter();
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::board_trim::TrimPhase::TrimParts
    );
    h.app.trim_cancel_step();
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::board_trim::TrimPhase::PickCutters
    );
    h.app.trim_cancel_step();
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(h.app.trim.is_none());
    h.frame();
}

/// Text + circle: the circle chops a hole in the text's clip (D11).
#[test]
fn trim_text_clip_punches_a_hole() {
    use slate_doc::scene::{FontChoice, TextAlign, TextNode};
    let mut h = trim_board("trim_text_clip");
    let text = {
        let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 40.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Text(TextNode {
                text: "HELLO".into(),
                family: FontChoice::Sans,
                size: 24.0,
                color: slate_doc::scene::Rgba::BLACK,
                align: TextAlign::Left,
                fill: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    };
    let circle = add_filled_ellipse(&mut h.app, 30.0, 0.0, 40.0, 40.0);
    select_trim(&mut h.app, &[text, circle]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(50.0, 20.0), false));
    let n = h.app.doc().scene.node(text).expect("text remains");
    let clip = n.clip.as_ref().expect("text clip after punch");
    let bez = board_path::path_data_to_world_bez(clip, n.rect, n.rotation_deg);
    let contours = vector_ink::flatten_contours(&bez, 0.35);
    assert!(vector_ink::point_in_polygon(&contours, [5.0, 20.0]));
    assert!(!vector_ink::point_in_polygon(&contours, [50.0, 20.0]));
    h.frame();
}

// ---------- Join golden paths (contracts/join.md GP1–GP6) ----------

fn join_board(tag: &str) -> Harness {
    trim_board(tag)
}

fn assert_axis_aligned_world(contours: &[Vec<[f32; 2]>]) {
    for ring in contours {
        let n = ring.len();
        assert!(n >= 3, "ring too short: {ring:?}");
        for i in 0..n {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let dx = (b[0] - a[0]).abs();
            let dy = (b[1] - a[1]).abs();
            assert!(
                dx < 0.02 || dy < 0.02,
                "edge {a:?} → {b:?} is not axis-aligned"
            );
        }
    }
}

fn node_world_contours(app: &SlateApp, id: slate_doc::scene::NodeId) -> Vec<Vec<[f32; 2]>> {
    let n = app.doc().scene.node(id).expect("node");
    let path = match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.path.as_ref().expect("path"),
        _ => panic!("expected a shape"),
    };
    let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
    vector_ink::flatten_contours(&bez, 0.35)
}

fn joined_contours(app: &SlateApp) -> (slate_doc::scene::WorldRect, Vec<Vec<[f32; 2]>>) {
    assert_eq!(app.doc().scene.nodes.len(), 1, "join should leave one node");
    let n = &app.doc().scene.nodes[0];
    let path = match &n.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.path.as_ref().expect("joined path"),
        _ => panic!("joined node must be a shape"),
    };
    let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
    (n.rect, vector_ink::flatten_contours(&bez, 0.35))
}

/// GP1 — two open segments join at nearest ends (existing style rule).
#[test]
fn join_gp1_open_paths() {
    join_two_open_paths_keeps_first_style();
}

/// GP2 — overlapping filled rects union into one region.
#[test]
fn join_gp2_closed_union() {
    let mut h = join_board("join_gp2");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 80.0, 80.0);
    let b = add_filled_rect(&mut h.app, 40.0, 40.0, 80.0, 80.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.cmd_join());
    let (_, contours) = joined_contours(&h.app);
    assert!(vector_ink::point_in_polygon(&contours, [10.0, 10.0]));
    assert!(vector_ink::point_in_polygon(&contours, [100.0, 100.0]));
    assert!(vector_ink::point_in_polygon(&contours, [50.0, 50.0]));
    assert!(!vector_ink::point_in_polygon(&contours, [10.0, 100.0]));
    assert_axis_aligned_world(&contours);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    h.frame();
}

/// Joined concave union must earcut both lobes (egui PathShape fans tear this).
#[test]
fn join_fill_mesh_covers_concave_union() {
    let mut h = join_board("join_fill_mesh");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 80.0, 80.0);
    let b = add_filled_rect(&mut h.app, 40.0, 40.0, 80.0, 80.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.cmd_join());
    let (_, contours) = joined_contours(&h.app);
    let (verts, idx) = vector_ink::fill_triangles(&contours);
    assert!(vector_ink::point_in_mesh(&verts, &idx, [10.0, 10.0]));
    assert!(vector_ink::point_in_mesh(&verts, &idx, [100.0, 100.0]));
    assert!(vector_ink::point_in_mesh(&verts, &idx, [50.0, 50.0]));
    assert!(!vector_ink::point_in_mesh(&verts, &idx, [10.0, 100.0]));
    h.frame();
}

/// GP3 — open stroke + filled rect: the ribbon outside the rect is kept.
#[test]
fn join_gp3_open_plus_closed() {
    let mut h = join_board("join_gp3");
    let rect = add_filled_rect(&mut h.app, 40.0, 20.0, 40.0, 40.0);
    let line = add_seg(&mut h.app, Pos2::new(0.0, 40.0), Pos2::new(140.0, 40.0));
    h.app.board_sel = [rect, line].into_iter().collect();
    assert!(h.app.cmd_join());
    let (_, contours) = joined_contours(&h.app);
    assert!(
        vector_ink::point_in_polygon(&contours, [10.0, 40.0]),
        "stroke ribbon left of the rect must remain"
    );
    assert!(vector_ink::point_in_polygon(&contours, [60.0, 40.0]));
    h.frame();
}

/// GP4 — nested rects union to the outer (no hole).
#[test]
fn join_gp4_nested_union() {
    let mut h = join_board("join_gp4");
    let outer = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    let inner = add_filled_rect(&mut h.app, 30.0, 30.0, 20.0, 20.0);
    h.app.board_sel = [outer, inner].into_iter().collect();
    assert!(h.app.cmd_join());
    let (_, contours) = joined_contours(&h.app);
    assert!(vector_ink::point_in_polygon(&contours, [10.0, 10.0]));
    assert!(
        vector_ink::point_in_polygon(&contours, [40.0, 40.0]),
        "inner rect must not become a hole"
    );
    h.frame();
}

/// GP5 — disjoint rects are a no-op (Rhino; Group is Ctrl+G).
#[test]
fn join_gp5_disjoint_is_noop() {
    let mut h = join_board("join_gp5");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 20.0, 20.0);
    let b = add_filled_rect(&mut h.app, 80.0, 80.0, 20.0, 20.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(!h.app.cmd_join());
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    assert!(h.app.toasts.iter().any(|(m, _)| m.contains("do not touch")));
    h.frame();
}

/// A touching pair unions; a far object stays its own node.
#[test]
fn join_leaves_a_far_object() {
    let mut h = join_board("join_far");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    let b = add_filled_rect(&mut h.app, 20.0, 20.0, 40.0, 40.0);
    let c = add_filled_rect(&mut h.app, 200.0, 200.0, 20.0, 20.0);
    h.app.board_sel = [a, b, c].into_iter().collect();
    assert!(h.app.cmd_join());
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    assert!(
        h.app.doc().scene.node(c).is_some(),
        "far rect must stay editable"
    );
    h.frame();
}

/// GP6 — typed / dock command is the same as Ctrl+J.
#[test]
fn join_gp6_command_id() {
    let mut h = join_board("join_gp6");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    let b = add_filled_rect(&mut h.app, 20.0, 20.0, 40.0, 40.0);
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.path.join"), None));
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.frame();
}

/// One closed shape alone is a no-op (inferred D11).
#[test]
fn join_one_closed_is_noop() {
    let mut h = join_board("join_one_closed");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    h.app.board_sel = [a].into_iter().collect();
    assert!(!h.app.cmd_join());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.frame();
}

/// Text is skipped; two rects still union (P2.RhinoJoin.skip).
#[test]
fn join_skips_text() {
    use slate_doc::scene::{FontChoice, TextAlign, TextNode};
    let mut h = join_board("join_skips_text");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    let b = add_filled_rect(&mut h.app, 20.0, 20.0, 40.0, 40.0);
    let text = {
        let rect = slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 24.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Text(TextNode {
                text: "keep".into(),
                family: FontChoice::Sans,
                size: 14.0,
                color: slate_doc::scene::Rgba::BLACK,
                align: TextAlign::Left,
                fill: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    };
    h.app.board_sel = [a, b, text].into_iter().collect();
    assert!(h.app.cmd_join());
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        2,
        "text remains, rects union"
    );
    assert!(h.app.doc().scene.node(text).is_some());
    h.frame();
}

/// A click on a host selects the host, not the wire painted underneath it.
#[test]
fn wire_under_node_does_not_steal_pick() {
    use slate_doc::scene::{ConnectorEnd, Side};
    let mut h = Harness::new("wire_under");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 200.0, 0.0);
    h.app
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
        .expect("wire");
    let hit = board_path::board_pick_node_routed(
        &h.app.doc().scene,
        40.0,
        30.0,
        1.0,
        false,
        h.app.board_wire_routing,
    );
    assert_eq!(hit, Some(a), "host under the pointer beats the wire");
}

#[test]
fn wire_routing_toggle_is_session_not_journaled() {
    let mut h = Harness::new("wire_routing");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    assert_eq!(h.app.board_wire_routing, slate_doc::WireRouting::Bezier);
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.wire.orthogonal"),
        None
    ));
    assert_eq!(h.app.board_wire_routing, slate_doc::WireRouting::Orthogonal);
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.wire.routing"),
        None
    ));
    assert_eq!(h.app.board_wire_routing, slate_doc::WireRouting::Bezier);
}

// ---------- Split golden paths (contracts/split.md GP1–GP6) ----------

fn closed_shape_contains(app: &SlateApp, p: [f32; 2]) -> bool {
    app.doc().scene.nodes.iter().any(|n| {
        let slate_doc::scene::NodeKind::Shape(s) = &n.kind else {
            return false;
        };
        let Some(path) = s.path.as_ref() else {
            return false;
        };
        if !path.closed {
            return false;
        }
        let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
        let contours = vector_ink::flatten_contours(&bez, 0.35);
        vector_ink::point_in_polygon(&contours, p)
    })
}

/// GP1 — crossing lines: click the horizontal; both halves stay.
#[test]
fn split_gp1_crossing_lines() {
    let mut h = trim_board("split_gp1");
    let horiz = add_seg(&mut h.app, Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0));
    let vert = add_seg(&mut h.app, Pos2::new(50.0, 0.0), Pos2::new(50.0, 100.0));
    select_trim(&mut h.app, &[horiz, vert]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert_eq!(h.app.board_tool, board::BoardTool::Split);
    assert!(h.app.trim.as_ref().is_some_and(|s| {
        s.mode == super::board_trim::SliceMode::Split && s.cutters.len() == 2
    }));
    assert!(h.app.trim_click(Pos2::new(25.0, 50.0), false));
    assert_eq!(h.app.doc().scene.nodes.len(), 3, "two halves + vertical");
    assert!(h.app.doc().scene.node(vert).is_some());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    h.frame();
}

/// GP2 — rect + vertical line: click the rect; both halves stay filled.
#[test]
fn split_gp2_rect_cut_by_line() {
    let mut h = trim_board("split_gp2");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let line = add_seg(&mut h.app, Pos2::new(50.0, -10.0), Pos2::new(50.0, 90.0));
    select_trim(&mut h.app, &[rect, line]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert!(h.app.trim_click(Pos2::new(20.0, 40.0), false));
    assert_eq!(h.app.doc().scene.nodes.len(), 3, "two halves + line");
    assert!(closed_shape_contains(&h.app, [20.0, 40.0]));
    assert!(closed_shape_contains(&h.app, [80.0, 40.0]));
    let closed_paths: Vec<_> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                &n.kind,
                slate_doc::scene::NodeKind::Shape(s)
                    if s.shape == slate_doc::scene::ShapeKind::Path
                        && s.path.as_ref().is_some_and(|p| p.closed)
            )
        })
        .map(|n| n.id)
        .collect();
    for id in closed_paths {
        assert_axis_aligned_world(&node_world_contours(&h.app, id));
    }
    h.frame();
}

/// GP3 — circle inside a rect: click the rect; disk and ring both remain.
#[test]
fn split_gp3_hole_keeps_disk_and_ring() {
    let mut h = trim_board("split_gp3");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    let circle = add_filled_ellipse(&mut h.app, 30.0, 30.0, 40.0, 40.0);
    select_trim(&mut h.app, &[rect, circle]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert!(h.app.trim_click(Pos2::new(10.0, 10.0), false));
    assert!(h.app.doc().scene.node(circle).is_some(), "cutter remains");
    assert!(
        closed_shape_contains(&h.app, [50.0, 50.0]),
        "disk must remain"
    );
    assert!(
        closed_shape_contains(&h.app, [5.0, 5.0]),
        "outer ring must remain"
    );
    h.frame();
}

/// GP4 — Ctrl+Shift+T arms Split; Ctrl+N still opens a tab.
#[test]
fn split_gp4_ctrl_n_still_new_tab() {
    let mut h = trim_board("split_gp4");
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.tool.split"), None);
    assert_eq!(h.app.board_tool, board::BoardTool::Split);
    let before = h.app.tabs.len();
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.new_tab"), None);
    assert_eq!(h.app.tabs.len(), before + 1);
    h.frame();
}

/// GP5 — two clicks, one undo restores only the last.
#[test]
fn split_gp5_per_click_undo() {
    let mut h = trim_board("split_gp5");
    let a = add_seg(&mut h.app, Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0));
    let b = add_seg(&mut h.app, Pos2::new(0.0, 40.0), Pos2::new(100.0, 40.0));
    let c = add_seg(&mut h.app, Pos2::new(50.0, -10.0), Pos2::new(50.0, 50.0));
    select_trim(&mut h.app, &[a, b, c]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert!(h.app.trim_click(Pos2::new(25.0, 0.0), false));
    let after_first = h.app.doc().scene.nodes.len();
    assert!(h.app.trim_click(Pos2::new(25.0, 40.0), false));
    assert!(h.app.doc().scene.nodes.len() > after_first);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), after_first);
    h.frame();
}

/// GP6 — Esc peels TrimParts → PickCutters → Select.
#[test]
fn split_gp6_esc_stack() {
    let mut h = trim_board("split_gp6");
    let cutter = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    h.app.set_board_tool(board::BoardTool::Split);
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::board_trim::TrimPhase::PickCutters
    );
    assert!(h.app.trim_click(Pos2::new(20.0, 20.0), false));
    assert!(h.app.trim.as_ref().unwrap().cutters.contains(&cutter));
    h.app.trim_enter();
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::board_trim::TrimPhase::TrimParts
    );
    h.app.trim_cancel_step();
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::board_trim::TrimPhase::PickCutters
    );
    h.app.trim_cancel_step();
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(h.app.trim.is_none());
    h.frame();
}
