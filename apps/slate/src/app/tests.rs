//! Headless Slate stability tests: drive the real frame loop through a plain
//! `egui::Context` (no eframe window) with the real thumbnail pool, exercising
//! the tag model, both presentations, tabs, and workbook save/load.

use super::{board_align, board_handles, board_place, board_wire, *};
use eframe::egui::{Pos2, Rect as ERect, Vec2 as EVec2};
use slate_doc::{NodeId, ViewKind};

pub(super) struct Harness {
    pub(super) ctx: egui::Context,
    pub(super) app: SlateApp,
    pub(super) base: PathBuf,
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

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
    let (_tx, rx) = crossbeam_channel::unbounded();
    h.app.picker_rx = Some(rx);
    assert!(h.app.update_close_blocked().is_some());
    h.app.picker_rx = None;
    assert!(h.app.update_close_blocked().is_none());
}

#[test]
fn media_menu_has_three_registered_families() {
    let mut h = Harness::new("media_menu");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let items = ui::tools::palette_strip_items(&h.app, "tool.media", &[]);
    assert_eq!(
        items.iter().map(|i| i.label).collect::<Vec<_>>(),
        vec!["Image", "3D", "Video"]
    );
    for id in [
        "board.media.image",
        "board.media.model",
        "board.media.video",
        "board.media.page",
    ] {
        assert!(h
            .app
            .registry
            .by_id(atlas_commands::CommandId(id))
            .is_some());
    }
    assert_eq!(
        slate_doc::media_kind(std::path::Path::new("model.3dm")),
        slate_doc::MediaKind::Model
    );
    assert_eq!(
        slate_doc::media_kind(std::path::Path::new("clip.mp4")),
        slate_doc::MediaKind::Video
    );
    // Keep a picker pending so this routing test never opens a native dialog.
    let (_tx, rx) = crossbeam_channel::unbounded();
    h.app.picker_rx = Some(rx);
    for (icon, command) in [
        ("media.image", "board.media.image"),
        ("media.model", "board.media.model"),
        ("media.video", "board.media.video"),
    ] {
        ui::tools::activate_flyout_id(&mut h.app, &h.ctx, icon);
        assert_eq!(h.app.cmd_history.iter().last().unwrap().id.0, command);
    }
}

#[test]
fn link_health_follows_relink_removal_and_tab_switches() {
    let mut h = Harness::new("link_health");
    h.app.ensure_work_tab();
    h.app.leave_home();
    let exists = h.base.join("exists.txt");
    let absent = h.base.join("absent.txt");
    std::fs::write(&exists, "local test fixture").unwrap();
    let id = h
        .app
        .doc_mut()
        .add_item(exists.clone(), "exists.txt", 0, 0, "");
    let wait_for = |app: &mut SlateApp, path: &Path, expected| {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            app.link_health_frame(&h.ctx);
            if app.tab().link_health.status(path) == expected {
                break;
            }
            assert!(Instant::now() < deadline, "link health did not settle");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait_for(&mut h.app, &exists, slate_doc::LinkStatus::Ok);
    h.app.doc_mut().relink(id, absent.clone());
    wait_for(&mut h.app, &absent, slate_doc::LinkStatus::Missing);
    assert_eq!(h.app.tab().link_health.counts().missing, 1);
    assert_eq!(
        h.app.tab().link_health.status(&exists),
        slate_doc::LinkStatus::Unknown
    );
    let first = h.app.active_tab;
    h.app.new_tab();
    h.app
        .doc_mut()
        .add_item(exists.clone(), "exists.txt", 0, 0, "");
    wait_for(&mut h.app, &exists, slate_doc::LinkStatus::Ok);
    assert_eq!(h.app.tab().link_health.counts().missing, 0);
    h.app.active_tab = first;
    assert_eq!(h.app.tab().link_health.counts().missing, 1);
    h.app.doc_mut().remove_item(id);
    h.app.link_health_frame(&h.ctx);
    assert_eq!(h.app.tab().link_health.counts().missing, 0);
    assert_eq!(
        h.app.tab().link_health.status(&absent),
        slate_doc::LinkStatus::Unknown
    );
}

#[test]
fn media_picker_places_one_undo_group_and_ignores_late_or_cancelled_results() {
    let mut h = Harness::new("media_picker");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let tab_id = h.app.tab().id;
    let path = h.base.join("picture.png");
    image::RgbaImage::from_pixel(16, 16, image::Rgba([80, 150, 220, 255]))
        .save(&path)
        .unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker_rx = Some(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id,
        at: Pos2::new(80.0, 100.0),
        paths: Some(vec![path.clone()]),
    })
    .unwrap();
    h.app.drain_pickers();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.new_tab();
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker_rx = Some(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id,
        at: Pos2::ZERO,
        paths: Some(vec![path]),
    })
    .unwrap();
    h.app.drain_pickers();
    assert!(h.app.doc().items.is_empty());
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker_rx = Some(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id: h.app.tab().id,
        at: Pos2::ZERO,
        paths: None,
    })
    .unwrap();
    h.app.drain_pickers();
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn media_powerpoint_page_choice_is_undoable_and_keeps_the_source_link() {
    let mut h = Harness::new("media_page");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let source = h.base.join("deck.pptx");
    std::fs::write(&source, b"source remains linked").unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&source));
    h.app.place_items_on_board(&ids, Pos2::new(120.0, 100.0));
    h.app.documents.seed(
        source.clone(),
        pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "test-deck".into(),
            pages: 3,
            bytes: 120,
        },
    );
    let node = h.app.doc().scene.nodes[0].id;
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.page"),
        Some(format!("{}:2", ids[0].0))
    ));
    let page_item = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(i) => i.item,
        _ => panic!(),
    };
    assert_ne!(page_item, ids[0]);
    assert_eq!(h.app.doc().item(page_item).unwrap().pdf_page, 2);
    assert_eq!(h.app.doc().item(page_item).unwrap().path, source);
    h.app.board_undo();
    assert!(
        matches!(&h.app.doc().scene.node(node).unwrap().kind,slate_doc::NodeKind::Image(i) if i.item==ids[0])
    );
    h.app.board_redo();
    assert!(
        matches!(&h.app.doc().scene.node(node).unwrap().kind,slate_doc::NodeKind::Image(i) if i.item==page_item)
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
    h.app.tab_mut().read_only = true;
    h.app.set_pdf_poster_page(page_item, 0);
    assert!(
        matches!(&h.app.doc().scene.node(node).unwrap().kind,slate_doc::NodeKind::Image(i) if i.item==page_item)
    );
}

#[test]
fn media_picker_hover_bridge_survives_moving_off_the_slide() {
    let mut h = Harness::new("media_hover");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let source = h.base.join("deck.pptx");
    std::fs::write(&source, b"deck").unwrap();
    let ids = h.app.add_paths(&[source]);
    h.app.place_items_on_board(&ids, Pos2::ZERO);
    let card = ERect::from_min_size(Pos2::new(20.0, 20.0), EVec2::new(100.0, 100.0));
    let popup = ERect::from_min_size(Pos2::new(20.0, 128.0), EVec2::new(200.0, 90.0));
    h.app.documents.picker = Some((h.app.tab().id, ids[0], card, popup));
    let screen = popup.center();
    assert!(h.app.document_picker_contains(Some(screen)));
    assert_eq!(
        h.app
            .board_hovered_pdf(h.app.board_xf().s2w(screen))
            .unwrap()
            .0,
        ids[0]
    );
    h.app.board_undo();
    assert!(!h.app.document_picker_contains(Some(screen)));
}

#[test]
fn media_page_command_preserves_grid_and_venn_item_selection() {
    let mut h = Harness::new("media_grid_page");
    h.app.ensure_work_tab();
    h.app.leave_home();
    let source = h.base.join("pages.pdf");
    std::fs::write(&source, b"page fixture").unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&source));
    h.app.place_items_on_board(&ids, Pos2::ZERO);
    h.app.documents.seed(
        source,
        pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "grid-pages".into(),
            pages: 3,
            bytes: 120,
        },
    );
    for (view, page) in [(ViewKind::Grid, 1), (ViewKind::Venn, 2)] {
        h.app.doc_mut().view.active_view = view;
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("board.media.page"),
            Some(format!("{}:{page}", ids[0].0))
        ));
        assert_eq!(h.app.doc().item(ids[0]).unwrap().pdf_page, page);
        assert_eq!(h.app.doc().items.len(), 1);
    }
}

impl Harness {
    fn wait_for_export(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while self.app.export_rx.is_some() {
            self.app.poll_artifact_export(&self.ctx);
            assert!(
                std::time::Instant::now() < deadline,
                "export worker did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    pub(super) fn new(tag: &str) -> Harness {
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

    pub(super) fn frame(&mut self) {
        self.frame_with(|_| {});
    }

    /// One frame with real input, which is the only way to test what the board
    /// and a focused page each do with the same wheel notch or keystroke.
    pub(super) fn frame_with(&mut self, prepare: impl FnOnce(&mut egui::RawInput)) {
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
    h.wait_for_export();
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
    h.wait_for_export();
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

#[test]
fn shape_drawing_moving_polyline_presses_are_placed_once_at_event_positions() {
    let mut h = line_board("moving_polyline");
    h.app.set_board_tool(board::BoardTool::Polyline);
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.frame();
    h.frame();
    let xf = h.app.board_xf();
    for w in [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 30.0),
        Pos2::new(50.0, 120.0),
    ] {
        let p = xf.w2s(w);
        let after = p + EVec2::new(25.0, 12.0);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
        h.frame_with(|i| {
            i.events = vec![
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(after),
                egui::Event::PointerButton {
                    pos: after,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        });
    }
    let Some(board_path::BoardPathDraft::Polyline { points }) = &h.app.board_path_draft else {
        panic!("polyline draft survives moving clicks")
    };
    assert_eq!(points.len(), 3);
    for (got, want) in points.iter().zip([
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 30.0),
        Pos2::new(50.0, 120.0),
    ]) {
        assert!((*got - want).length() < 0.01, "{got:?} != {want:?}");
    }
}

#[test]
fn shape_drawing_polyline_snaps_to_draft_and_commits_closed_without_duplicate_vertex() {
    let mut h = line_board("closed_draft");
    h.app.set_board_tool(board::BoardTool::Polyline);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap = Default::default();
    h.app.board_osnap.near = true;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    for p in [Pos2::ZERO, Pos2::new(100.0, 0.0), Pos2::new(100.0, 100.0)] {
        h.app.path_tool_click(p);
    }
    let near = h.app.resolve_point_snap(
        Pos2::new(70.0, 3.0),
        &[],
        Some(Pos2::new(100.0, 100.0)),
        false,
        true,
    );
    assert!((near - Pos2::new(70.0, 0.0)).length() < 0.01);
    h.app.path_tool_click(Pos2::new(2.0, 1.0));
    assert!(h.app.board_path_draft.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!()
    };
    let path = s.path.as_ref().unwrap();
    assert!(path.closed);
    assert_eq!(path.segs.len(), 2);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn shape_drawing_stationary_line_click_is_not_a_drag_when_first_point_snaps() {
    let mut h = line_board("line_raw_press");
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_snap_grid = true;
    let p = Pos2::new(7.0, 0.0);
    assert!(h.app.line_begin(p, false));
    assert_eq!(h.app.line_draft.as_ref().unwrap().start, Pos2::ZERO);
    h.app.line_release(p, true, false);
    assert!(h.app.line_draft.is_some());
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn shape_drawing_line_second_endpoint_is_latched_on_press_not_later_motion() {
    let mut h = line_board("line_press_endpoint");
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.frame();
    h.frame();
    let xf = h.app.board_xf();
    let a = xf.w2s(Pos2::ZERO);
    let b = xf.w2s(Pos2::new(120.0, 0.0));
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(a)));
    for (p, moved) in [(a, a), (b, b + EVec2::new(35.0, 25.0))] {
        h.frame_with(|i| {
            i.events = vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(moved),
                egui::Event::PointerButton {
                    pos: moved,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        });
    }
    assert_endpoints(&h.app, Pos2::ZERO, Pos2::new(120.0, 0.0));
}

#[test]
fn shape_drawing_pen_keeps_all_frame_motion_samples_and_final_release() {
    let mut h = line_board("pen_ordered");
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    h.frame();
    let xf = h.app.board_xf();
    let a = xf.w2s(Pos2::ZERO);
    let points = [
        Pos2::new(20.0, 20.0),
        Pos2::new(40.0, 0.0),
        Pos2::new(60.0, 20.0),
    ];
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(a)));
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        for p in points {
            i.events.push(egui::Event::PointerMoved(xf.w2s(p)));
        }
    });
    let Some(board::BoardDrag::FreehandPen { points, .. }) = &h.app.board_drag else {
        panic!("pen owns the press")
    };
    assert_eq!(points.len(), 4);
    let end = Pos2::new(61.0, 21.0);
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: xf.w2s(end),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Shape(s) = &node.kind else {
        panic!()
    };
    let bez =
        board_path::path_data_to_world_bez(s.path.as_ref().unwrap(), node.rect, node.rotation_deg);
    let last = bez.elements().last().unwrap();
    let point = match last {
        vector_ink::kurbo::PathEl::CurveTo(_, _, p) | vector_ink::kurbo::PathEl::LineTo(p) => p,
        _ => panic!("expected endpoint"),
    };
    assert!((point.x - 61.0).abs() < 0.01 && (point.y - 21.0).abs() < 0.01);
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

/// Closed unfilled polylines pick on the stroke, not the AABB or interior.
#[test]
fn closed_polyline_pick_and_near_ignore_empty_bbox() {
    let mut h = line_board("closed_pick");
    h.app.tab_mut().cam.z = 1.0;
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(80.0, 0.0),
        Pos2::new(0.0, 80.0),
    ];
    let (rect, data) = board_path::points_to_path_data(&pts, true);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Path,
            fill: None,
            stroke: board_path::default_curve_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: Some(data),
        }),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let scene = &h.app.doc().scene;
    assert_eq!(
        board_path::board_pick_node(scene, 40.0, 0.0, 1.0),
        Some(id),
        "base stroke"
    );
    assert!(
        board_path::board_pick_node(scene, 20.0, 20.0, 1.0).is_none(),
        "unfilled interior must not hover-select"
    );
    assert!(
        board_path::board_pick_node(scene, 80.0, 80.0, 1.0).is_none(),
        "empty AABB corner must not hover-select"
    );
    h.app.board_osnap = Default::default();
    h.app.board_osnap.near = true;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    let ghost = Pos2::new(80.0, 80.0);
    assert!(
        board_osnap::pick(
            &h.app.doc().scene,
            ghost,
            8.0,
            h.app.board_osnap,
            &[],
            None,
            slate_doc::WireRouting::Bezier,
        )
        .is_none(),
        "Near must not fire on empty AABB space of a closed polyline"
    );
    h.frame();
}

#[test]
fn closed_polyline_pick_and_near_ignore_points_outside_the_bbox() {
    let mut h = line_board("closed_outside");
    h.app.tab_mut().cam.z = 1.0;
    let pts = [
        Pos2::new(400.0, 300.0),
        Pos2::new(480.0, 300.0),
        Pos2::new(400.0, 380.0),
    ];
    let (rect, data) = board_path::points_to_path_data(&pts, true);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Path,
            fill: None,
            stroke: board_path::default_curve_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: Some(data),
        }),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let scene = &h.app.doc().scene;
    assert_eq!(
        board_path::board_pick_node(scene, 440.0, 300.0, 1.0),
        Some(id)
    );
    for (x, y, why) in [
        (0.0, 0.0, "world origin"),
        (200.0, 150.0, "line toward origin"),
        (200.0, 300.0, "infinite base extension"),
        (500.0, 400.0, "outside AABB"),
    ] {
        assert!(
            board_path::board_pick_node(scene, x, y, 1.0).is_none(),
            "pick at ({x},{y}) {why}"
        );
    }
    h.app.board_osnap = Default::default();
    h.app.board_osnap.end = true;
    h.app.board_osnap.mid = true;
    h.app.board_osnap.center = true;
    h.app.board_osnap.near = true;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    for ghost in [
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 150.0),
        Pos2::new(200.0, 300.0),
        Pos2::new(500.0, 400.0),
    ] {
        assert!(
            board_osnap::pick(
                &h.app.doc().scene,
                ghost,
                8.0,
                h.app.board_osnap,
                &[],
                None,
                slate_doc::WireRouting::Bezier,
            )
            .is_none(),
            "snap at {ghost:?} must not bind to a far-away polyline"
        );
    }
    h.frame();
}

// ---------- Object snaps (contracts/object-snap.md GP1–GP4) ----------

fn osnap_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.board_snap_grid = false;
    // These fixtures exercise object snaps independently of alignment guides.
    h.app.board_smart_guides = false;
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
    let mods = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
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

#[test]
fn f3_opens_and_closes_the_selection_inspector_in_icon_strip_mode() {
    use super::ui::tools::{DOCK_ID, SELECTION_PANEL_ID};
    let mut h = kit_board("selection_f3", board::BoardTool::Select);
    h.app.dock_pins.clear();
    h.app.dock_icon_strips = vec!["*".into()];
    h.app
        .chrome_mut()
        .set_tool(chrome::ToolPanel::Selection, true);
    h.frame();
    assert!(!atlas_shell::dock::panel_is_open(
        &h.ctx,
        DOCK_ID,
        SELECTION_PANEL_ID
    ));
    for expected_open in [true, false, true] {
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::F3,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            })
        });
        assert_eq!(
            atlas_shell::dock::panel_is_open(&h.ctx, DOCK_ID, SELECTION_PANEL_ID),
            expected_open
        );
        assert_eq!(
            h.app.chrome().tool(chrome::ToolPanel::Selection),
            expected_open
        );
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::F3,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            })
        });
    }
    // A visible dock icon is not the same thing as an open body: after its
    // minimize action, F3 must reopen immediately rather than hide the icon.
    atlas_shell::dock::set_panel_open(&h.ctx, DOCK_ID, SELECTION_PANEL_ID, false);
    h.frame();
    assert!(h.app.chrome().tool(chrome::ToolPanel::Selection));
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.properties"), None));
    h.frame();
    assert!(atlas_shell::dock::panel_is_open(
        &h.ctx,
        DOCK_ID,
        SELECTION_PANEL_ID
    ));
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

/// P1.shape.style — a single-node fill/stroke edit seeds the next inherit create.
#[test]
fn a_drawn_rectangle_inherits_last_fill_and_stroke() {
    let mut h = kit_board("kit_rect_last_style", board::BoardTool::RectShape);
    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 120.0),
    );
    let id = h.app.doc().scene.nodes[0].id;
    let fill = slate_doc::scene::Rgba([10, 20, 30, 200]);
    let stroke = slate_doc::scene::Stroke {
        width: 5.0,
        color: slate_doc::scene::Rgba([200, 40, 40, 255]),
        dash: slate_doc::scene::Dash::Solid,
        cap: slate_doc::scene::StrokeCap::Butt,
        join: slate_doc::scene::StrokeJoin::Miter,
        profile: slate_doc::scene::WidthProfile::Uniform,
    };
    h.app.patch_nodes(&[id], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.fill = Some(fill);
            s.stroke = stroke;
        }
    });
    h.app.set_board_tool(board::BoardTool::RectShape);
    drag(
        &mut h,
        board::BoardTool::RectShape,
        Pos2::new(0.0, 200.0),
        Pos2::new(80.0, 260.0),
    );
    let second = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| n.id != id)
        .expect("second rectangle");
    let slate_doc::scene::NodeKind::Shape(s) = &second.kind else {
        panic!("expected a shape node");
    };
    assert_eq!(s.fill, Some(fill));
    assert_eq!(s.stroke, stroke);
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

    // Tools the user did not override are untouched. Stroke and fill cannot
    // show that: create-style inheritance (P1.shape.style) carries the
    // rectangle's stroke onto the next shape whatever recipe produced it.
    // Corner is recipe-owned, so it is what distinguishes the two.
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
    assert_eq!(e.corner, slate_doc::scene::Corner::Square);
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

/// A click-placed File Atlas lens is host-class, unbound, and 960×540.
#[test]
fn a_placed_file_atlas_lens_is_unbound_at_the_recipe_size() {
    let mut h = kit_board("kit_atlas_portal", board::BoardTool::AtlasPortal);
    h.app.place_atlas_portal_at(Pos2::new(0.0, 0.0));

    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Portal(p) = &node.kind else {
        panic!("expected a portal node");
    };
    assert_eq!(p.class, slate_doc::scene::PortalClass::Host);
    assert_eq!(p.kind, slate_doc::scene::PortalKind::FileAtlas);
    assert!(
        p.source.is_none(),
        "unbound until the user chooses a folder"
    );
    assert_eq!(
        (node.rect.w, node.rect.h),
        (
            slate_doc::scene::PORTAL_DEFAULT_W,
            slate_doc::scene::PORTAL_DEFAULT_H
        )
    );
    h.frame();
}

/// Dropping a folder queues a chooser; applying File Atlas binds a host portal.
#[test]
fn gp2_dropping_a_folder_binds_a_file_atlas_lens() {
    let mut h = kit_board("atlas_drop_folder", board::BoardTool::Select);
    let folder = h.base.join("shots");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("a.png"), [0u8; 8]).unwrap();
    let rest = h
        .app
        .queue_folder_drop_choosers(std::slice::from_ref(&folder), Pos2::ZERO);
    assert!(rest.is_empty());
    assert_eq!(h.app.atlas_lenses.pending_drops.len(), 1);
    h.app.apply_folder_drop(
        super::board_atlas::FolderDropKind::AtlasLens,
        folder.clone(),
        Pos2::ZERO,
    );
    let node = &h.app.doc().scene.nodes[0];
    let NodeKind::Portal(p) = &node.kind else {
        panic!("expected a portal");
    };
    assert_eq!(p.kind, slate_doc::scene::PortalKind::FileAtlas);
    assert!(p.source.is_some());
    h.frame();
}

/// Place-contents dumps the folder's files onto the board, not a portal.
#[test]
fn gp2_place_folder_contents_makes_image_nodes() {
    let mut h = kit_board("atlas_place_contents", board::BoardTool::Select);
    let folder = h.base.join("dump");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("one.png"), [0u8; 8]).unwrap();
    std::fs::write(folder.join("two.png"), [0u8; 8]).unwrap();
    h.app.apply_folder_drop(
        super::board_atlas::FolderDropKind::PlaceContents,
        folder,
        Pos2::ZERO,
    );
    assert!(
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .all(|n| matches!(n.kind, NodeKind::Image(_))),
        "place contents is not a lens"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    h.frame();
}

fn atlas_screen_outside(h: &Harness, id: slate_doc::NodeId) -> Pos2 {
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap();
    let frame = xf.rect_w2s(node.rect);
    if frame.left() > 24.0 {
        Pos2::new(frame.left() - 12.0, frame.center().y)
    } else {
        Pos2::new(frame.right() + 12.0, frame.center().y)
    }
}

/// P1.portal.contents-focus — a primary click outside the body peels the
/// File Atlas lens so the board owns the wheel again.
#[test]
fn primary_click_outside_a_focused_atlas_lens_releases_focus() {
    let mut h = kit_board("atlas_click_out", board::BoardTool::Select);
    h.app.place_atlas_portal_at(Pos2::new(0.0, 0.0));
    let id = h.app.doc().scene.nodes[0].id;
    h.app.atlas_focus(id);
    h.frame();
    assert_eq!(h.app.atlas_lenses.focused, Some(id));
    let outside = atlas_screen_outside(&h, id);
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(outside));
        input.events.push(egui::Event::PointerButton {
            pos: outside,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    assert_eq!(
        h.app.atlas_lenses.focused, None,
        "outside click must peel File Atlas contents-focus"
    );
}

/// After peel, a wheel over the (still-large) frame must not change the
/// inner FolderCam — that was the stuck-zoom defect.
#[test]
fn wheel_after_atlas_blur_does_not_change_inner_zoom() {
    let mut h = kit_board("atlas_wheel_after_blur", board::BoardTool::Select);
    let folder = h.base.join("shots");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("a.png"), [0u8; 8]).unwrap();
    h.app.apply_folder_drop(
        super::board_atlas::FolderDropKind::AtlasLens,
        folder,
        Pos2::ZERO,
    );
    h.frame();
    let id = h.app.doc().scene.nodes[0].id;
    assert!(
        h.app.atlas_has_view(id),
        "a bound lens has a per-portal view"
    );
    h.app.atlas_focus(id);
    h.app.atlas_force_inner_zoom(id, 2.0);
    h.app.contents_blur();
    assert_eq!(h.app.atlas_lenses.focused, None);
    let z_before = h.app.atlas_inner_zoom(id).unwrap();
    let xf = h.app.board_xf();
    let body = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(body.center()));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: EVec2::new(0.0, -80.0),
            modifiers: egui::Modifiers::default(),
        });
    });
    assert_eq!(
        h.app.atlas_inner_zoom(id).unwrap(),
        z_before,
        "an unfocused lens must not eat the wheel"
    );
}

/// Clicking another portal peels File Atlas focus (one logical slot).
#[test]
fn clicking_another_portal_peels_atlas_focus() {
    let mut h = kit_board("atlas_click_other", board::BoardTool::Select);
    h.app.place_atlas_portal_at(Pos2::new(0.0, 0.0));
    h.app.place_atlas_portal_at(Pos2::new(2200.0, 0.0));
    let a = h.app.doc().scene.nodes[0].id;
    let b = h.app.doc().scene.nodes[1].id;
    h.app.atlas_focus(a);
    h.frame();
    let xf = h.app.board_xf();
    let on_b = xf
        .rect_w2s(h.app.doc().scene.node(b).unwrap().rect)
        .center();
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(on_b));
        input.events.push(egui::Event::PointerButton {
            pos: on_b,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    assert_eq!(
        h.app.atlas_lenses.focused, None,
        "a click on another node peels the previous contents-focus"
    );
}

/// Entering File Atlas peels a focused web page — four slots, one focus.
#[test]
fn entering_atlas_peels_web_contents_focus() {
    let mut h = web_board("atlas_peels_web");
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (web_id, _) = only_portal(&h);
    h.app.place_atlas_portal_at(Pos2::new(2200.0, 0.0));
    let atlas_id = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == slate_doc::scene::PortalKind::FileAtlas))
        .map(|n| n.id)
        .expect("atlas portal");
    h.app.web_focus(web_id);
    assert_eq!(h.app.web.focused, Some(web_id));
    h.app.atlas_focus(atlas_id);
    assert_eq!(h.app.atlas_lenses.focused, Some(atlas_id));
    assert_eq!(
        h.app.web.focused, None,
        "only one host portal may be focused"
    );
}

fn bound_atlas_lens(tag: &str) -> (Harness, slate_doc::NodeId, PathBuf) {
    let mut h = kit_board(tag, board::BoardTool::Select);
    let folder = h.base.join("shots");
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("a.png");
    std::fs::write(&file, [0u8; 8]).unwrap();
    h.app.apply_folder_drop(
        super::board_atlas::FolderDropKind::AtlasLens,
        folder,
        Pos2::ZERO,
    );
    let id = h.app.doc().scene.nodes[0].id;
    for _ in 0..24 {
        h.frame();
        if h.app.atlas_file_count(id) > 0 {
            break;
        }
    }
    (h, id, file)
}

/// Contents-focus left-drag hands the same path File Explorer would (D22).
#[test]
fn a_focused_atlas_lens_drags_the_hovered_file_as_a_shell_path() {
    let (mut h, id, file) = bound_atlas_lens("atlas_shell_drag");
    assert!(
        h.app.atlas_file_count(id) > 0,
        "scan must have produced the dropped file"
    );
    h.app.atlas_focus(id);
    h.app.atlas_hover_file(id, 0);
    h.app.atlas_queue_shell_drag(id);
    h.frame();
    let paths = h
        .app
        .atlas_lenses
        .last_shell_drag
        .clone()
        .expect("the card must become a shell drag");
    assert_eq!(paths.len(), 1, "one hovered file becomes one shell item");
    assert_eq!(
        paths[0].file_name(),
        file.file_name(),
        "CF_HDROP must name the file under the cursor"
    );
}

/// Right-drag stays a pan. It must not start a Windows drag (File Atlas rule).
#[test]
fn a_right_drag_inside_a_focused_atlas_lens_does_not_start_a_shell_drag() {
    let (mut h, id, _) = bound_atlas_lens("atlas_rmb_no_drag");
    h.app.atlas_focus(id);
    h.app.atlas_hover_file(id, 0);
    h.frame();
    let xf = h.app.board_xf();
    let on = xf
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .center();
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(on));
        input.events.push(egui::Event::PointerButton {
            pos: on,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(on + EVec2::new(24.0, 12.0)));
    });
    assert!(
        h.app.atlas_lenses.last_shell_drag.is_none(),
        "right-drag must not hand the card to Windows"
    );
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
    let mods = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
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
    escape: bool,
    admitted: std::collections::HashSet<slate_doc::NodeId>,
    admit_targets: Vec<(slate_doc::NodeId, String)>,
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
    fn admit_targets(&self) -> Vec<(slate_doc::NodeId, String)> {
        self.0.borrow().admit_targets.clone()
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
    fn take_escape(&mut self) -> bool {
        std::mem::take(&mut self.0.borrow_mut().escape)
    }
    fn available(&self) -> bool {
        true
    }
    fn admit(&mut self, id: slate_doc::NodeId, req: &board_web::WebRequest) {
        let mut log = self.0.borrow_mut();
        log.admitted.insert(id);
        log.admit_targets.push((id, req.target.clone()));
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
            h.app.note_web_geometry(*id, r, clip, 1.0);
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

/// Zooming out retains the live webview. Coming back must preserve the page
/// the human had reached, not the authored Google home (D15 / D31).
#[test]
fn zooming_out_does_not_reset_a_visited_page_to_home() {
    let mut h = web_board("web_resume_after_zoom");
    let host = with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    web_settle(&mut h, &[(id, 540.0)], 2);
    host.set_current_url(id, "https://example.com/result");
    web_settle(&mut h, &[(id, 540.0)], 1);

    web_settle(&mut h, &[(id, 40.0)], 2);
    assert_eq!(
        h.app.web.live_count(),
        1,
        "zoom keeps the visible browser alive"
    );

    web_settle(&mut h, &[(id, 540.0)], 2);
    assert!(h.app.web.is_live(id), "zooming back in readmits");
    let last = host
        .admit_targets()
        .into_iter()
        .rev()
        .find(|(admit_id, _)| *admit_id == id)
        .map(|(_, target)| target)
        .expect("a re-admit target");
    assert_eq!(
        last, "https://example.com/result",
        "eviction must not send the page back to the authored home"
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
fn binding_a_project_with_agents_opens_the_picker() {
    let mut h = agent_board("agent_pick_list");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    let folder = h.base.join("climate-grid");
    std::fs::create_dir_all(&folder).unwrap();
    h.app.bind_agent_project(id, folder);
    h.app.present_agent_picker(
        id,
        vec![
            atlas_ai::cursor_chats::CursorChat {
                id: "one".into(),
                title: "Climate grid".into(),
                updated_at: 2,
            },
            atlas_ai::cursor_chats::CursorChat {
                id: "two".into(),
                title: "Older".into(),
                updated_at: 1,
            },
        ],
    );
    assert_eq!(
        h.app.agent_picker_titles(),
        Some(vec!["Climate grid".into(), "Older".into()]),
        "existing agents must be offered, not auto-bound"
    );
    let NodeKind::Portal(p) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal");
    };
    assert!(
        p.agent
            .as_ref()
            .and_then(|a| a.channel.as_deref())
            .is_none(),
        "picking is the user's; do not silently attach the first agent"
    );
    assert!(h.app.pick_agent_from_list(id, "two"));
    assert!(h.app.agent_picker_titles().is_none());
    let NodeKind::Portal(p) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a portal");
    };
    assert_eq!(
        p.agent.as_ref().and_then(|a| a.channel.as_deref()),
        Some("two")
    );
}

#[test]
fn a_single_existing_agent_is_still_a_choice() {
    let mut h = agent_board("agent_pick_one");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.present_agent_picker(
        id,
        vec![atlas_ai::cursor_chats::CursorChat {
            id: "only".into(),
            title: "The one agent".into(),
            updated_at: 1,
        }],
    );
    assert_eq!(
        h.app.agent_picker_titles(),
        Some(vec!["The one agent".into()]),
        "one agent is still a list — do not auto-select"
    );
}

#[test]
fn a_failed_agent_send_names_the_failure() {
    let mut h = agent_board("agent_named_fail");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
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
    h.app.set_agent_program(id, "local");
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
        .divert_web_drops(&[with_entry.clone(), plain.clone()], Pos2::ZERO);
    assert_eq!(
        rest,
        vec![with_entry, plain],
        "folders go to the lens chooser, not auto-web"
    );
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
    h.app
        .apply_folder_drop(super::board_atlas::FolderDropKind::Web, dash, Pos2::ZERO);
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

#[test]
fn web_url_drop_uses_os_position_and_is_one_undo_step() {
    let mut h = web_board("web_url_drop_position");
    h.frame();
    h.app.tab_mut().cam.z = 0.75;
    h.app.tab_mut().cam.offset = egui::vec2(80.0, -30.0);
    let at = h.app.canvas_rect.center() + egui::vec2(70.0, 40.0);
    let world = h.app.board_xf().s2w(at);
    let before = h.app.tab().journal.undo_depth();
    h.app
        .external_drop
        .push_test(super::external_drop::DropEvent {
            payload: super::external_drop::Payload::Url(
                " https://example.com/drop?q=1#anchor ".into(),
            ),
            at,
            alt: false,
        });
    h.frame();
    let (id, p) = only_portal(&h);
    assert_eq!(
        p.source.as_ref().unwrap().locator,
        "https://example.com/drop?q=1#anchor"
    );
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    assert!((rect.x + rect.w / 2.0 - world.x).abs() < 0.01);
    assert!((rect.y + rect.h / 2.0 - world.y).abs() < 0.01);
    assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
    assert!(h.app.web.has_consent("https://example.com"));
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn web_url_drop_rebinds_the_hit_portal_and_undo_restores_it() {
    let mut h = web_board("web_url_drop_rebind");
    h.frame();
    let at = h.app.canvas_rect.center();
    assert!(h.app.drop_web_url("https://example.com/first", at));
    let (id, _) = only_portal(&h);
    assert!(h.app.drop_web_url("https://example.org/second", at));
    assert_eq!(only_portal(&h).0, id);
    assert_eq!(
        only_portal(&h).1.source.as_ref().unwrap().locator,
        "https://example.org/second"
    );
    h.app.board_undo();
    assert_eq!(
        only_portal(&h).1.source.as_ref().unwrap().locator,
        "https://example.com/first"
    );
    h.app.doc_mut().scene.nodes[0].locked = true;
    let before = h.app.tab().journal.undo_depth();
    assert!(!h.app.drop_web_url("https://example.org/locked", at));
    assert_eq!(h.app.tab().journal.undo_depth(), before);
}

#[test]
fn web_url_drop_rejects_non_urls_and_non_editable_targets() {
    let mut h = web_board("web_url_drop_reject");
    h.frame();
    let at = h.app.canvas_rect.center();
    for text in [
        "hello",
        "https://",
        "javascript:alert(1)",
        "data:text/html,test",
        "file:///x.html",
        "https://example.com more text",
        "https://a.test\nhttps://b.test",
    ] {
        assert!(!h.app.drop_web_url(text, at), "accepted {text}");
    }
    assert!(!h.app.drop_web_url(
        "https://example.com",
        h.app.canvas_rect.min - egui::vec2(1.0, 1.0)
    ));
    h.app.tab_mut().read_only = true;
    assert!(!h.app.drop_web_url("https://example.com", at));
    h.app.tab_mut().read_only = false;
    h.app.doc_mut().view.active_view = ViewKind::Grid;
    assert!(!h.app.drop_web_url("https://example.com", at));
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn web_url_drop_native_file_payload_keeps_html_and_alt_behavior() {
    for alt in [false, true] {
        let mut h = web_board(if alt {
            "web_url_drop_alt"
        } else {
            "web_url_drop_html"
        });
        h.frame();
        let path = h.base.join("dashboard.html");
        std::fs::write(&path, "<h1>local test</h1>").unwrap();
        h.app
            .external_drop
            .push_test(super::external_drop::DropEvent {
                payload: super::external_drop::Payload::Files(vec![path]),
                at: h.app.canvas_rect.center(),
                alt,
            });
        h.frame();
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
        assert_eq!(
            matches!(&h.app.doc().scene.nodes[0].kind, NodeKind::Portal(_)),
            !alt
        );
    }
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

#[test]
fn native_escape_restores_maximize_without_also_blurring_the_page() {
    let (mut h, id, host) = focused_page("web_native_escape");
    h.app.portal_maximize(id);
    let before = h.app.doc().scene.clone();
    host.0.borrow_mut().escape = true;
    h.frame();
    assert_eq!(h.app.portal_chrome.maximized, None);
    assert_eq!(h.app.web.focused, Some(id));
    assert_eq!(h.app.doc().scene, before);
    host.0.borrow_mut().escape = true;
    h.frame();
    assert_eq!(h.app.web.focused, None);
}

#[test]
fn escape_restores_maximize_even_with_a_retained_palette() {
    let (mut h, id, _) = focused_page("web_escape_palette");
    h.app.portal_maximize(id);
    h.app.palette_state.open = true;
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(h.app.portal_chrome.maximized, None);
}

#[test]
fn hover_preferences_toggle_one_kind_without_changing_selection() {
    let mut h = web_board("hover_preferences");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    assert_eq!(
        h.app.hover_preview_target(Some(Pos2::new(20.0, 20.0))),
        Some(id)
    );
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.hover_highlight"),
        Some("shape".into())
    ));
    assert_eq!(
        h.app.hover_preview_target(Some(Pos2::new(20.0, 20.0))),
        None
    );
    assert!(h.app.settings.hover_highlight("image"));
    assert!(h.app.board_sel.is_empty());
    let saved = serde_json::to_string(&h.app.settings).unwrap();
    let restored: settings::SlateSettings = serde_json::from_str(&saved).unwrap();
    assert!(!restored.hover_highlight("shape"));
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.hover_highlight"),
        Some("shape".into())
    ));
    assert_eq!(
        h.app.hover_preview_target(Some(Pos2::new(20.0, 20.0))),
        Some(id)
    );
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
    // Folder drops open the lens chooser; bind the dashboard explicitly so
    // this export test does not depend on that separate interaction.
    let local = h.app.add_web_portal(
        WorldRect::new(20.0, 20.0, 320.0, 180.0),
        None,
        "test dashboard",
    );
    assert!(h.app.bind_web_path(local, dash));
    assert!(matches!(
        &h.app.doc().scene.node(local).unwrap().kind,
        NodeKind::Portal(portal) if portal.kind == slate_doc::scene::PortalKind::Web
    ));
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
    h.wait_for_export();
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
    assert_eq!(h.app.node_screen_outline(&h.ctx, &xf, sq).len(), 4);

    let rd = h.app.doc().scene.node(rounded).unwrap();
    let rd_pts = h.app.node_screen_outline(&h.ctx, &xf, rd);
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
        h.app.node_screen_outline(&h.ctx, &xf, el).len() > 4,
        "an ellipse highlight must be circular, not a box"
    );

    let portal = h.app.doc().scene.nodes.last().unwrap();
    let p_pts = h.app.node_screen_outline(&h.ctx, &xf, portal);
    assert!(
        p_pts.len() > 4,
        "a portal highlight must follow the shared fillet"
    );

    let strip = add_dock_strip(&mut h.app, "tool.shapes", &["shape.rect"]);
    h.frame();
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let sn = h.app.doc().scene.node(strip).unwrap().clone();
    let s_pts = h.app.node_screen_outline(&h.ctx, &xf, &sn);
    assert!(
        s_pts.len() > 4,
        "a dock-strip highlight must follow the shared fillet"
    );
    let slate_doc::scene::NodeKind::DockStrip(strip_data) = &sn.kind else {
        panic!("expected a dock strip");
    };
    let (card, r) = h.app.dock_strip_screen_card(&h.ctx, &xf, &sn, strip_data);
    let sharp = card.left_top();
    assert!(
        s_pts.iter().all(|p| p.distance(sharp) > r * 0.3),
        "the dock-strip highlight must leave the sharp corner empty"
    );
}

fn add_dock_strip(app: &mut SlateApp, palette_id: &str, tools: &[&str]) -> NodeId {
    let n = tools.len().max(1) as f32;
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, n * 160.0 + 80.0, 72.0);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::DockStrip(slate_doc::scene::DockStripNode {
            palette_id: palette_id.into(),
            visible: tools.iter().map(|s| (*s).to_string()).collect(),
        }),
    );
    app.add_nodes(vec![node])[0]
}

/// A click on a dropped-toolbar icon arms the command and does not place.
#[test]
fn dock_strip_click_arms_without_placing() {
    let mut h = web_board("dock_arm");
    let id = add_dock_strip(&mut h.app, "tool.text", &["text.block"]);
    h.frame();
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let world = h
        .app
        .dock_embed_first_tool_world(&h.ctx, &xf, &n)
        .expect("dropped toolbar must expose a tool slot");
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.try_dock_embed_click(&h.ctx, world));
    assert_eq!(h.app.board_tool, board::BoardTool::Text);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        before,
        "arming from a dropped toolbar must not place"
    );
}

/// Widening a dropped toolbar must not shrink icons; uniform grow enlarges them.
#[test]
fn dock_strip_resize_contains_without_shrinking_icons() {
    let mut h = web_board("dock_scale");
    let id = add_dock_strip(&mut h.app, "tool.shapes", &["shape.rect", "shape.ellipse"]);
    h.frame();
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let slate_doc::scene::NodeKind::DockStrip(strip) = &n.kind else {
        panic!("expected a dock strip");
    };
    let items = super::ui::tools::palette_strip_items(&h.app, &strip.palette_id, &strip.visible);
    let mut tokens = atlas_shell::tokens::current().dock;
    tokens.normalize();
    let layout = atlas_shell::dock::measure_icon_strip(
        &h.ctx,
        &items,
        &tokens,
        atlas_shell::dock::DockSide::BottomCenter,
        16_384.0,
    );
    let slot = layout.first_slot_id().expect("strip has a slot");
    let dest = xf.rect_w2s(n.rect);
    let a = atlas_shell::dock::icon_strip_slot_rect(dest, &layout, &tokens, slot)
        .expect("slot at dest");
    let wide =
        ERect::from_center_size(dest.center(), EVec2::new(dest.width() * 2.5, dest.height()));
    let b = atlas_shell::dock::icon_strip_slot_rect(wide, &layout, &tokens, slot)
        .expect("slot at wide dest");
    assert!(
        (b.height() - a.height()).abs() < 0.5,
        "widening the node must not shrink icons ({} vs {})",
        b.height(),
        a.height()
    );
    let both = ERect::from_center_size(dest.center(), dest.size() * 2.0);
    let c = atlas_shell::dock::icon_strip_slot_rect(both, &layout, &tokens, slot)
        .expect("slot at uniform dest");
    assert!(
        c.height() > a.height() * 1.8,
        "uniform grow must enlarge icons ({} vs {})",
        c.height(),
        a.height()
    );
}

/// Press-drag on a dropped toolbar moves it even when a create tool is armed.
#[test]
fn dock_strip_drag_moves_regardless_of_armed_tool() {
    let mut h = web_board("dock_move");
    let id = add_dock_strip(&mut h.app, "tool.shapes", &["shape.rect"]);
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
        bbox.bottom() + board_align::bottom_outset_px(xf.z),
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

/// P1.node.select — Shift+click adds a rectangle instead of replacing.
#[test]
fn shift_click_adds_rectangles_to_the_selection() {
    let mut h = align_board("shift_select_rects");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 120.0, 0.0);
    h.app
        .board_click_for_test(Pos2::new(40.0, 30.0), egui::Modifiers::NONE);
    assert_eq!(h.app.board_sel.len(), 1);
    assert!(h.app.board_sel.contains(&a));
    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    h.app.board_click_for_test(Pos2::new(160.0, 30.0), shift);
    assert!(h.app.board_sel.contains(&a), "first rect stays");
    assert!(h.app.board_sel.contains(&b), "second rect is added");
}

/// Hover-resize on an unselected rectangle must not steal Shift+select.
#[test]
fn shift_press_on_unselected_rect_edge_adds_instead_of_resize() {
    let mut h = align_board("shift_rect_edge");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 200.0, 0.0);
    h.app.board_sel = [a].into_iter().collect();
    h.frame();
    h.app.shift_down = true;
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(b).unwrap().clone();
    let edge = xf.w2s(Pos2::new(n.rect.x + n.rect.w, n.rect.y + 12.0));
    let edge_world = xf.s2w(edge);
    assert!(
        h.app.begin_transform_drag(edge, edge_world).is_none(),
        "shift on an unselected rect must not start resize"
    );
    let mut mods = egui::Modifiers::NONE;
    mods.shift = true;
    let body = Pos2::new(n.rect.x + 40.0, n.rect.y + 30.0);
    let drag = h.app.begin_gesture_for_test(xf.w2s(body), body, mods);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "shift+press on the next rect moves the additive set"
    );
    assert!(h.app.board_sel.contains(&a));
    assert!(h.app.board_sel.contains(&b));
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
    let mods = egui::Modifiers {
        ctrl: true,
        alt: true,
        shift: true,
        ..Default::default()
    };
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
    let mods = egui::Modifiers {
        alt: true,
        ..Default::default()
    }; // skip object-snap so the pin is the only translation
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

/// GP7 — a closed cutter removes a corner without changing the source stroke.
#[test]
fn trim_gp7_notch_preserves_stroke_and_undo() {
    use slate_doc::scene::{NodeKind, Rgba, ShapeKind, StrokeJoin};
    let mut h = trim_board("trim_gp7");
    let rect = add_filled_rect(&mut h.app, 40.0, 30.0, 920.0, 380.0);
    h.app.patch_nodes(&[rect], |node| {
        if let NodeKind::Shape(shape) = &mut node.kind {
            shape.stroke.width = 8.0;
            shape.stroke.color = Rgba([45, 212, 191, 255]);
            shape.stroke.join = StrokeJoin::Miter;
        }
    });
    let before = h.app.doc().scene.node(rect).unwrap().clone();
    let cutter = add_filled_rect(&mut h.app, 835.0, 0.0, 165.0, 200.0);
    select_trim(&mut h.app, &[cutter]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(900.0, 100.0), false));
    let after = h.app.doc().scene.node(rect).unwrap();
    let (NodeKind::Shape(original), NodeKind::Shape(trimmed)) = (&before.kind, &after.kind) else {
        panic!("trim must preserve a shape");
    };
    assert_eq!(trimmed.shape, ShapeKind::Path);
    assert_eq!(trimmed.stroke, original.stroke);
    assert_eq!(trimmed.fill, original.fill);
    let bez = board_path::path_data_to_world_bez(
        trimmed.path.as_ref().unwrap(),
        after.rect,
        after.rotation_deg,
    );
    let mesh = vector_ink::stroke_mesh(
        &bez,
        &board_path::stroke_style_world(&trimmed.stroke, 1.0),
        0.0,
        0.02,
    );
    let vertices: Vec<_> = mesh.vertices.iter().map(|v| v.pos).collect();
    for p in [
        [750.0, 26.2],
        [831.2, 100.0],
        [900.0, 196.2],
        [963.8, 350.0],
        [100.0, 413.8],
        [36.2, 100.0],
    ] {
        assert!(
            vector_ink::point_in_mesh(&vertices, &mesh.indices, p),
            "stroke missing at {p:?}"
        );
    }
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(rect).unwrap(), &before);
}

/// GP8 — both source and cutter keep their true corners, including after rotation.
#[test]
fn trim_gp8_true_corners_and_rotation() {
    use slate_doc::scene::{Corner, NodeKind, ShapeKind, WorldRect};
    for corner in [
        Corner::Rounded { radius: 15.0 },
        Corner::RoundedPercent { percent: 30.0 },
        Corner::Chamfer { cut: 15.0 },
        Corner::ChamferPercent { percent: 30.0 },
    ] {
        for angle in [0.0, 33.0, 90.0] {
            let mut h = trim_board("trim_gp8");
            let target_rect = WorldRect::new(0.0, 0.0, 200.0, 100.0);
            let target = add_filled_rect(&mut h.app, 0.0, 0.0, 200.0, 100.0);
            // Rotate the arrangement as a whole; each node still uses its own center.
            let cutter_center = target_rect.rotate_point([180.0, 10.0], angle);
            let cutter = add_filled_rect(
                &mut h.app,
                cutter_center[0] - 60.0,
                cutter_center[1] - 50.0,
                120.0,
                100.0,
            );
            h.app.patch_nodes(&[target, cutter], |node| {
                node.rotation_deg = angle;
                if let NodeKind::Shape(shape) = &mut node.kind {
                    shape.corner = corner;
                }
            });
            let before = h.app.doc().scene.node(target).unwrap().clone();
            let cutter_before = h.app.doc().scene.node(cutter).unwrap().clone();
            select_trim(&mut h.app, &[cutter]);
            h.app.set_board_tool(board::BoardTool::Trim);
            let click = target_rect.rotate_point([160.0, 30.0], angle);
            assert!(h.app.trim_click(Pos2::new(click[0], click[1]), false));
            let after = h.app.doc().scene.node(target).unwrap().clone();
            assert_eq!(
                after.rotation_deg, 0.0,
                "world-space result must not rotate twice"
            );
            let NodeKind::Shape(shape) = &after.kind else {
                panic!("shape");
            };
            assert_eq!(shape.shape, ShapeKind::Path);
            let result = h.app.node_closed_poly(&after).unwrap();
            for (point, inside) in [
                ([1.0, 1.0], false),    // untouched upper-left corner stays rounded/chamfered
                ([199.0, 99.0], false), // untouched lower-right corner
                ([15.0, 10.0], true),
                ([121.0, 59.0], true), // material outside the cutter's rounded/chamfered corner
                ([135.0, 55.0], false), // removed overlap
                ([160.0, 30.0], false),
                ([180.0, 80.0], true),
            ] {
                let world = target_rect.rotate_point(point, angle);
                assert_eq!(
                    vector_ink::point_in_polygon(&result, world),
                    inside,
                    "{corner:?}, rotation {angle}, local point {point:?}"
                );
            }
            assert_eq!(h.app.doc().scene.node(cutter).unwrap(), &cutter_before);
            let saved = serde_json::to_string(&after).unwrap();
            assert_eq!(
                serde_json::from_str::<slate_doc::Node>(&saved).unwrap(),
                after
            );
            h.app.board_undo();
            assert_eq!(h.app.doc().scene.node(target).unwrap(), &before);
        }
    }
}

/// GP9 — rotated legacy lines use their painted endpoints and bake rotation once.
#[test]
fn trim_gp9_rotated_line_endpoints() {
    use slate_doc::scene::{NodeKind, ShapeKind};
    let mut h = trim_board("trim_gp9");
    let target = add_filled_rect(&mut h.app, 0.0, 50.0, 100.0, 0.0);
    h.app.patch_nodes(&[target], |node| {
        node.rotation_deg = 45.0;
        if let NodeKind::Shape(shape) = &mut node.kind {
            shape.shape = ShapeKind::Line;
            shape.fill = None;
        }
    });
    let before = h.app.doc().scene.node(target).unwrap().clone();
    let cutter = add_seg(&mut h.app, Pos2::new(50.0, -20.0), Pos2::new(50.0, 120.0));
    select_trim(&mut h.app, &[cutter]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(30.0, 30.0), false));
    let after = h.app.doc().scene.node(target).unwrap();
    let points = h.app.node_open_polyline(after).unwrap();
    assert_eq!(after.rotation_deg, 0.0);
    assert!((points[0][0] - 50.0).abs() < 0.01 && (points[0][1] - 50.0).abs() < 0.01);
    let last = points.last().unwrap();
    assert!((last[0] - 85.35534).abs() < 0.01 && (last[1] - 85.35534).abs() < 0.01);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(target).unwrap(), &before);
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

    // Re-run the punch with a rotated host: clip coordinates stay host-local.
    h.app.board_undo();
    h.app.patch_nodes(&[text], |node| node.rotation_deg = 37.0);
    let before = h.app.doc().scene.node(text).unwrap().clone();
    select_trim(&mut h.app, &[circle]);
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(50.0, 20.0), false));
    let n = h.app.doc().scene.node(text).unwrap();
    assert_eq!(n.rotation_deg, 37.0);
    assert_eq!(n.rect, before.rect);
    let contours = h.app.node_closed_poly(n).unwrap();
    let kept = n.rect.rotate_point([5.0, 20.0], n.rotation_deg);
    assert!(vector_ink::point_in_polygon(&contours, kept));
    assert!(!vector_ink::point_in_polygon(&contours, [50.0, 20.0]));
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(text).unwrap(), &before);
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

#[test]
fn agent_program_choice_is_journaled_and_undo_restores_picker() {
    let mut h = agent_board("program_choice");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "codex");
    let NodeKind::Portal(p) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(p.agent.as_ref().unwrap().provider, "codex");
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().rect.w,
        slate_doc::agent_chat::CARD_WIDTH
    );
    assert_eq!(
        p.agent.as_ref().unwrap().chat.detail,
        slate_doc::agent_chat::Detail::Full
    );
    assert_eq!(h.app.contents_focused(), Some(id));
    h.app.board_undo();
    let NodeKind::Portal(p) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert!(p.agent.as_ref().unwrap().provider.is_empty());
}

#[test]
fn agent_presentation_is_menu_only_and_window_is_not_a_bundle() {
    let mut h = agent_board("agent_mode_menu");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "codex");
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().bundle = None;
        }
    });
    h.frame();
    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    assert!(!h.app.agent_expand_bundle());
    h.app
        .agent_set_detail(slate_doc::agent_chat::Detail::Identity);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .detail,
        slate_doc::agent_chat::Detail::Full
    );
    h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("portal.agent.train"),
        None,
    );
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .train
    );
    h.app.board_undo();
    assert!(
        !slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .train
    );
}

#[test]
fn host_focus_has_one_owner_when_switching_between_agent_and_web() {
    let mut h = agent_board("one_host_focus");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let agent = h.app.doc().scene.nodes[0].id;
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(800.0, 0.0, 300.0, 200.0),
        NodeKind::Portal(slate_doc::PortalNode::unbound_web("Web")),
    );
    let web = node.id;
    h.app.add_nodes(vec![node]);
    h.app.agent_focus(agent);
    assert_eq!(h.app.contents_focused(), Some(agent));
    h.app.web_focus(web);
    assert_eq!(h.app.contents_focused(), Some(web));
    assert_eq!(h.app.agents.focused, None);
    h.app.agent_focus(agent);
    assert_eq!(h.app.web.focused, None);
    assert_eq!(h.app.contents_focused(), Some(agent));
    h.app.contents_blur();
    assert_eq!(h.app.contents_focused(), None);
}

#[test]
fn agent_conversation_projection_rename_and_undo_preserve_forks() {
    let mut h = agent_board("train_projection");
    let mut ids = Vec::new();
    for (i, parent) in [None, Some(0), Some(1), Some(2), Some(1), Some(4)]
        .into_iter()
        .enumerate()
    {
        let mut p = slate_doc::PortalNode::unbound_agent("Courtyard", "codex");
        let a = p.agent.as_mut().unwrap();
        a.session = "projection-test".into();
        a.chat = slate_doc::agent_chat::ChatView {
            train: true,
            parent: parent.map(|j| ids[j]),
            start: i,
            end: Some(i + 1),
            ..Default::default()
        };
        let n = h.app.doc_mut().scene.build_node(
            WorldRect::new(i as f32 * 416.0, 0.0, 320.0, 200.0),
            NodeKind::Portal(p),
        );
        ids.push(n.id);
        h.app.add_nodes(vec![n]);
    }
    h.app.board_sel.clear();
    h.app.board_sel.insert(ids[3]);
    assert!(h.app.agent_rename("Garden study"));
    for id in &ids {
        let NodeKind::Portal(p) = &h.app.doc().scene.node(*id).unwrap().kind else {
            panic!()
        };
        assert_eq!(
            p.title,
            if [ids[2], ids[3]].contains(id) {
                "Garden study"
            } else {
                "Courtyard"
            }
        );
    }
    assert!(h.app.agent_show_chat());
    assert!(
        !h.app.agent_expand_bundle(),
        "collapsed windows are not bundles"
    );
    let visible: Vec<_> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| !n.hidden)
        .map(|n| n.id)
        .collect();
    assert_eq!(visible, vec![ids[1], ids[3], ids[5]]);
    let upper = h.app.doc().scene.node(ids[3]).unwrap();
    let lower = h.app.doc().scene.node(ids[5]).unwrap();
    assert!(lower.rect.y - upper.rect.y >= upper.rect.h);
    assert_eq!(
        slate_doc::agent_chat::visible_parent(&h.app.doc().scene, upper),
        Some(ids[1])
    );
    h.app.agent_show_train();
    assert_eq!(h.app.doc().scene.nodes.len(), 6);
    assert!(h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .all(|n| !n.hidden && slate_doc::agent_chat::agent(n).unwrap().chat.train));
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(ids[4]).unwrap())
            .unwrap()
            .chat
            .parent,
        Some(ids[1])
    );
    h.app.board_undo();
    assert_eq!(
        h.app.doc().scene.nodes.iter().filter(|n| !n.hidden).count(),
        3
    );
}

#[test]
fn agent_expanding_window_retains_existing_branch_checkpoint() {
    let mut h = agent_board("window_subdivision");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "codex");
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.chat.end = Some(3);
            a.bundle = None;
        }
    });
    let original = h.app.doc().scene.node(root).unwrap().clone();
    let mut child = h.app.doc_mut().scene.build_duplicate(&original, 416.0, 0.0);
    let child_id = child.id;
    if let NodeKind::Portal(p) = &mut child.kind {
        let a = p.agent.as_mut().unwrap();
        a.chat.parent = Some(root);
        a.chat.start = 3;
        a.chat.end = Some(5);
    }
    h.app.add_nodes(vec![child]);
    h.app.board_sel.clear();
    h.app.board_sel.insert(child_id);
    h.app.agent_show_train();
    assert_eq!(h.app.doc().scene.nodes.len(), 5);
    assert_eq!(
        slate_doc::agent_chat::conversation(&h.app.doc().scene, child_id).len(),
        5
    );
    let branch = slate_doc::agent_chat::agent(h.app.doc().scene.node(child_id).unwrap()).unwrap();
    assert_eq!(branch.chat.start, 4);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(branch.chat.parent.unwrap()).unwrap())
            .unwrap()
            .chat
            .parent,
        Some(root)
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
}

#[test]
fn agent_output_draft_is_consumed_without_an_extra_empty_car() {
    let mut h = agent_board("agent_draft");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "local");
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.chat.train = true;
            a.chat.detail = slate_doc::agent_chat::Detail::Summary;
            a.bundle = None;
        }
    });
    h.app.board_sel.clear();
    h.app.board_sel.insert(root);
    h.app.agent_spawn_command(Some("[480,120]"));
    let draft = *h.app.board_sel.iter().next().unwrap();
    assert_ne!(draft, root);
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    let (reply, history) = h
        .app
        .prepare_agent_train_send(draft, &std::env::temp_dir())
        .unwrap();
    assert!(history.is_empty());
    assert_eq!(h.app.doc().scene.nodes.len(), 3);
    let user = h.app.doc().scene.node(draft).unwrap();
    assert_eq!([user.rect.x, user.rect.y], [480.0, 120.0]);
    let a = slate_doc::agent_chat::agent(user).unwrap();
    assert!(!a.chat.draft);
    assert_eq!(a.chat.parent, Some(root));
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(reply).unwrap())
            .unwrap()
            .chat
            .parent,
        Some(draft)
    );
    h.app.board_undo();
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(draft).unwrap())
            .unwrap()
            .chat
            .draft
    );
}
