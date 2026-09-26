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
fn a_drop_while_the_picker_is_open_lands_and_dismisses_the_dialog() {
    let mut h = Harness::new("drop_dismisses_picker");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.frame();
    let dropped = h.base.join("dragged-from-dialog.png");
    let picked = h.base.join("picked-in-dialog.png");
    for path in [&dropped, &picked] {
        image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
            .save(path)
            .unwrap();
    }
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker.adopt(rx);
    let mut raw = egui::RawInput::default();
    raw.events.push(egui::Event::PointerButton {
        pos: Pos2::new(400.0, 400.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    assert!(h.app.dialogs.gate_input(&mut raw), "the window is gated");
    assert_eq!(raw.events, vec![egui::Event::PointerGone]);

    h.app
        .external_drop
        .push_test(super::external_drop::DropEvent {
            payload: super::external_drop::Payload::Files(vec![dropped.clone()]),
            at: h.app.canvas_rect.center(),
            alt: false,
        });
    h.frame();
    let linked = |h: &Harness| -> Vec<PathBuf> {
        h.app.doc().items.iter().map(|i| i.path.clone()).collect()
    };
    assert_eq!(linked(&h), vec![dropped.clone()], "drop is a normal drop");
    assert!(h.app.picker.is_open(), "gated until the dialog reports");

    // The cancelled dialog (or a pick that raced the cancel) is discarded.
    tx.send(PickerMsg::AddFiles(Some(vec![picked]))).unwrap();
    h.frame();
    assert!(!h.app.picker.is_open());
    assert_eq!(linked(&h), vec![dropped]);
}

#[test]
fn media_menu_has_four_registered_families() {
    let mut h = Harness::new("media_menu");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let items = ui::tools::palette_strip_items(&h.app, "tool.media", &[]);
    assert_eq!(
        items.iter().map(|i| i.label).collect::<Vec<_>>(),
        vec!["Image", "3D", "Video", "Text"]
    );
    for id in [
        "board.media.image",
        "board.media.model",
        "board.media.video",
        "board.media.text",
        "board.media.page",
        "board.media.unbundle",
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
    h.app.picker.adopt(rx);
    for (icon, command) in [
        ("media.image", "board.media.image"),
        ("media.model", "board.media.model"),
        ("media.video", "board.media.video"),
        ("media.text", "board.media.text"),
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
    h.app.picker.adopt(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id,
        at: Pos2::new(80.0, 100.0),
        paths: Some(vec![path.clone()]),
    })
    .unwrap();
    h.app.drain_pickers(&h.ctx);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.new_tab();
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker.adopt(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id,
        at: Pos2::ZERO,
        paths: Some(vec![path]),
    })
    .unwrap();
    h.app.drain_pickers(&h.ctx);
    assert!(h.app.doc().items.is_empty());
    let (tx, rx) = crossbeam_channel::unbounded();
    h.app.picker.adopt(rx);
    tx.send(PickerMsg::AddMedia {
        tab_id: h.app.tab().id,
        at: Pos2::ZERO,
        paths: None,
    })
    .unwrap();
    h.app.drain_pickers(&h.ctx);
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
fn media_unbundle_places_a_selected_page_grid_and_undoes() {
    let mut h = Harness::new("media_unbundle");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let source = h.base.join("deck.pdf");
    std::fs::write(&source, b"source remains linked").unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&source));
    h.app.place_items_on_board(&ids, Pos2::new(120.0, 100.0));
    h.app.documents.seed(
        source.clone(),
        pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "unbundle-deck".into(),
            pages: 3,
            bytes: 120,
        },
    );
    let node = h.app.doc().scene.nodes[0].id;
    let before = h.app.doc().scene.node(node).unwrap().rect;
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(format!("{}:1", node.0))
    ));
    assert_eq!(h.app.doc().scene.nodes.len(), 3);
    assert_eq!(h.app.board_sel.len(), 3);
    assert!(h.app.board_sel.contains(&node));
    let page_item = match &h.app.doc().scene.node(node).unwrap().kind {
        slate_doc::NodeKind::Image(i) => i.item,
        _ => panic!(),
    };
    assert_eq!(h.app.doc().item(page_item).unwrap().pdf_page, 1);
    let xs: Vec<f32> = h.app.doc().scene.nodes.iter().map(|n| n.rect.x).collect();
    assert!(
        xs.iter().any(|x| (*x - xs[0]).abs() > 1.0),
        "unbundled pages must spread into a grid, not stack"
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"source remains linked");
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.doc().scene.nodes[0].id, node);
    assert_eq!(h.app.doc().scene.node(node).unwrap().rect, before);
    h.app.documents.seed(
        source,
        pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "unbundle-deck".into(),
            pages: 1,
            bytes: 120,
        },
    );
    assert!(!h.app.unbundle_paged_media(node, None));
    h.app.tab_mut().read_only = true;
    assert!(!h.app.unbundle_paged_media(node, Some(0)));
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
        let _ = self.frame_output(prepare);
    }

    /// [`Self::frame_with`], returning what the frame painted and what it
    /// asked of the platform (the cursor icon, for one).
    pub(super) fn frame_output(
        &mut self,
        prepare: impl FnOnce(&mut egui::RawInput),
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
            ..Default::default()
        };
        prepare(&mut input);
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let out = ctx.run(input, |c| app.update_app(c));
        assert_invariants(&self.app);
        out
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
                fill_authored: false,
                assignments: std::collections::BTreeMap::new(),
                stroke: slate_doc::scene::Stroke::none(),
                corner: slate_doc::scene::Corner::Square,
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
fn a_blank_board_opens_a_dropped_workbook_without_asking() {
    let mut h = Harness::new("wb_blank_drop");
    let wb_path = h.base.join("other.slate");
    SlateDoc::new("Other").save_to(&wb_path).unwrap();
    h.app
        .accept_workbook_drop(wb_path.clone(), Pos2::new(40.0, 40.0));
    assert_eq!(h.app.pending_workbook_drops(), 0);
    assert_eq!(h.app.pending_workbooks, vec![wb_path.clone()]);
    h.frame();
    assert!(h.app.pending_workbooks.is_empty());
    assert_eq!(h.app.tabs.len(), 1);
    assert_eq!(h.app.tab().doc.name, "Other");
    assert!(h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .all(|n| !matches!(&n.kind, slate_doc::scene::NodeKind::Portal(_))));
}

#[test]
fn an_occupied_board_can_insert_a_dropped_workbook() {
    let mut h = Harness::new("wb_insert");
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 40.0),
        slate_doc::scene::NodeKind::Text(slate_doc::scene::TextNode {
            text: "keep".into(),
            family: slate_doc::scene::Typeface::Sans,
            size: 24.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: slate_doc::scene::TextAlign::Left,
            fill: None,
            agent: None,
        }),
    );
    h.app.add_nodes(vec![node]);
    let wb_path = h.base.join("child.slate");
    let mut child = SlateDoc::new("Child");
    let mark = child.scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 120.0, 40.0),
        slate_doc::scene::NodeKind::Text(slate_doc::scene::TextNode {
            text: "NESTED-MARK".into(),
            family: slate_doc::scene::Typeface::Sans,
            size: 24.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: slate_doc::scene::TextAlign::Left,
            fill: None,
            agent: None,
        }),
    );
    child.scene.nodes.push(mark);
    child.save_to(&wb_path).unwrap();
    h.app.tab_mut().path = Some(h.base.join("parent.slate"));

    h.app
        .accept_workbook_drop(wb_path.clone(), Pos2::new(200.0, 200.0));
    assert!(h.app.pending_workbooks.is_empty());
    assert_eq!(h.app.pending_workbook_drops(), 1);
    let (path, at) = h.app.pop_workbook_drop().unwrap();
    h.app
        .apply_workbook_drop(&h.ctx, board_slate::WorkbookDropChoice::Insert, path, at);
    let portal = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            slate_doc::scene::NodeKind::Portal(p)
                if p.kind == slate_doc::scene::PortalKind::Slate =>
            {
                Some(p)
            }
            _ => None,
        })
        .expect("inserted portal");
    assert_eq!(portal.class, slate_doc::scene::PortalClass::Document);
    assert_eq!(
        portal.source.as_ref().map(|s| s.locator.as_str()),
        Some("child.slate")
    );

    h.app.do_export(h.base.join("export"));
    h.wait_for_export();
    let html = std::fs::read_to_string(
        h.base
            .join("export")
            .join("Untitled-slides")
            .join("index.html"),
    )
    .unwrap();
    assert!(html.contains("NESTED-MARK"), "{html}");
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
fn video_pan_scrubs_the_full_trim_and_does_not_journal() {
    use slate_doc::scene::VideoOpts;

    let mut h = Harness::new("video_pan");
    let clip = h.base.join("clip.mp4");
    std::fs::write(&clip, b"not really mp4").unwrap();
    let ids = h.app.add_paths(&[clip]);
    h.app
        .place_items_on_board(&ids, eframe::egui::Pos2::new(400.0, 300.0));
    let node_id = h.app.doc().scene.nodes[0].id;
    h.app.patch_nodes(&[node_id], |n| {
        if let NodeKind::Image(i) = &mut n.kind {
            i.video = VideoOpts {
                start: 3.0,
                end: Some(11.0),
                ..VideoOpts::default()
            };
        }
    });
    h.app.video_probe_for_test(node_id, 20.0);
    let before = h.app.doc().scene.nodes.clone();
    assert!(h.app.video_shown_for_test(node_id).is_none());

    let rect = h.app.doc().scene.node(node_id).unwrap().rect;
    h.app
        .video_pointer(Pos2::new(rect.x + 0.5, rect.y + rect.h * 0.5));
    let left = h.app.video_shown_for_test(node_id).expect("in-point");
    assert!((left - 3.0).abs() < 0.05, "left pan {left}");

    h.app
        .video_pointer(Pos2::new(rect.x + rect.w - 0.5, rect.y + rect.h * 0.5));
    let right = h.app.video_shown_for_test(node_id).expect("out-point");
    assert!((right - 11.0).abs() < 0.05, "right pan {right}");

    h.app.video_clear_hover();
    let held = h.app.video_shown_for_test(node_id).expect("held frame");
    assert!(
        (held - right).abs() < 0.001,
        "pan should leave the playhead {held}"
    );

    h.app.video_click(node_id);
    assert!(h.app.video_playing_for_test(node_id));
    assert_eq!(
        before,
        h.app.doc().scene.nodes,
        "scrub and play are derived, not journaled"
    );
    h.app.video_click(node_id);
    assert!(!h.app.video_playing_for_test(node_id));
    let paused = h.app.video_shown_for_test(node_id).expect("paused");
    assert!(
        (paused - right).abs() < 0.2,
        "pause keeps the panned frame {paused}"
    );
    assert_eq!(before, h.app.doc().scene.nodes);
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.0, 0.5],
                segs: vec![PathSeg::Line { to: [1.0, 0.5] }],
                closed: false,
                ..Default::default()
            })),

            text: None,
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.0, 0.5],
                segs: vec![PathSeg::Line { to: [1.0, 0.5] }],
                closed: false,
                ..Default::default()
            })),

            text: None,
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,

            text: None,
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

    h.app
        .finish_erase(ids.clone(), vec![Pos2::new(50.0, 0.5)], Vec::new());
    assert!(h.app.doc().scene.nodes.is_empty());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 3, "one undo restores all");
    h.frame();
}

#[test]
fn smooth_pass_increases_stamp_blur_and_undo_restores() {
    let mut h = Harness::new("smooth_blur");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app
        .finish_freehand_brush(vec![Pos2::new(10.0, 10.0), Pos2::new(80.0, 40.0)]);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Smooth);
    h.app.smooth_width = 40.0;
    h.app.smooth_strength = 1.0;
    let drag = h.app.begin_smooth(Pos2::new(40.0, 25.0), false);
    h.app.finish_smooth(drag);
    let blur_after = h.app.doc().scene.node(id).unwrap();
    let slate_doc::scene::NodeKind::Shape(after) = &blur_after.kind else {
        panic!("shape");
    };
    assert!(after.stroke.gaussian_blur > 0.0, "blur should increase");
    h.app.board_undo();
    let blur_before = h.app.doc().scene.node(id).unwrap();
    let slate_doc::scene::NodeKind::Shape(before) = &blur_before.kind else {
        panic!("shape");
    };
    assert_eq!(before.stroke.gaussian_blur, 0.0);
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

/// Place is one-shot: Select returns, text is center-aligned, the caret is
/// open, and the text/color capsule stays closed.
#[test]
fn sticky_place_opens_centered_edit_without_the_color_capsule() {
    use slate_doc::scene::{NodeKind, TextAlign};
    let mut h = Harness::new("sticky-place");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Sticky);

    h.app.place_sticky_at(Pos2::new(0.0, 0.0));
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    let id = *h.app.board_sel.iter().next().expect("sticky selected");
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Text(t) => {
            assert_eq!(t.align, TextAlign::Center);
            assert_eq!(t.fill, Some(board_color::STICKY_FILL));
            assert_eq!(board_color::STICKY_FILL.0, [255, 255, 255, 255]);
            assert!(t.text.is_empty());
        }
        _ => panic!("sticky is a text node"),
    }
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(edit, _)| *edit == id));
    h.app.sync_shape_properties();
    assert!(h.app.shape_properties.panel.is_none());

    h.app.commit_text_edit();
    assert!(h.app.text_edit.is_none());
    assert_eq!(h.app.board_tool, board::BoardTool::Select);

    h.app.board_double_click_for_test(Pos2::new(0.0, 0.0));
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(edit, _)| *edit == id));
    h.app.sync_shape_properties();
    assert!(h.app.shape_properties.panel.is_none());
    h.frame();
}

#[test]
fn text_box_click_starts_draft_without_journal() {
    let mut h = Harness::new("text-draft");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let before = h.app.tab().journal.undo_depth();
    h.app.place_text_at(Pos2::new(40.0, 40.0));
    assert!(h.app.text_box_draft.is_some());
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    let draft = h.app.text_box_draft.as_ref().unwrap();
    assert!(draft.buffer.is_empty());
    assert_eq!(draft.color, board::to_rgba(h.app.palette().ink));
    h.frame();
}

#[test]
fn text_box_commit_on_click_away_is_one_undo_step() {
    use slate_doc::scene::NodeKind;
    let mut h = Harness::new("text-commit");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.place_text_at(Pos2::ZERO);
    h.app.text_box_draft.as_mut().unwrap().buffer = "Hello".into();
    let before = h.app.tab().journal.undo_depth();
    h.app.commit_text_box_draft();
    assert!(h.app.text_box_draft.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
    match &h.app.doc().scene.nodes[0].kind {
        NodeKind::Text(t) => assert_eq!(t.text, "Hello"),
        _ => panic!("text node"),
    }
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
    h.frame();
}

#[test]
fn text_box_empty_click_away_discards_without_journal() {
    let mut h = Harness::new("text-discard");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.place_text_at(Pos2::ZERO);
    let before = h.app.tab().journal.undo_depth();
    h.app.cancel_text_box_draft();
    assert!(h.app.text_box_draft.is_none());
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    h.frame();
}

#[test]
fn text_box_draft_uses_theme_ink_in_dark_mode() {
    let mut h = Harness::new("text-ink-dark");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.dark_mode = true;
    h.app.place_text_at(Pos2::ZERO);
    assert_eq!(
        h.app.text_box_draft.as_ref().unwrap().color,
        board::to_rgba(h.app.palette().ink)
    );
    h.app.dark_mode = false;
    h.app.cancel_text_box_draft();
    h.app.place_text_at(Pos2::ZERO);
    assert_eq!(
        h.app.text_box_draft.as_ref().unwrap().color,
        board::to_rgba(h.app.palette().ink)
    );
    h.frame();
}

fn text_draft_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.frame();
    h
}

/// Real keystrokes into the focused draft, then a frame with the pointer over
/// the board so egui hit-tests every registered widget rect.
fn type_into_draft(h: &mut Harness, text: &str) {
    h.frame();
    let text = text.to_string();
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(720.0, 450.0)));
        input.events.push(egui::Event::Text(text));
    });
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(724.0, 452.0)));
    });
}

fn assert_draft_rect_sane(h: &Harness) -> slate_doc::scene::WorldRect {
    let r = h
        .app
        .text_box_draft
        .as_ref()
        .expect("draft still open")
        .rect;
    for v in [r.x, r.y, r.w, r.h] {
        assert!(v.is_finite(), "draft rect must stay finite: {r:?}");
    }
    assert!(
        r.w > 0.0 && r.h > 0.0,
        "draft rect must be non-empty: {r:?}"
    );
    r
}

/// Crash regression: typing into a click-created text box made its rect
/// infinitely wide and egui's hit test panicked on the next frame.
#[test]
fn text_box_click_draft_grows_to_typed_width_only() {
    let mut h = text_draft_board("text-autowidth");
    let world = h.app.board_xf().s2w(Pos2::new(600.0, 400.0));
    h.app.place_text_at(world);
    type_into_draft(&mut h, "hello world");
    assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer, "hello world");
    let r = assert_draft_rect_sane(&h);
    let z = h.app.board_xf().z;
    let text_w = h.ctx.fonts(|f| {
        f.layout_no_wrap(
            "hello world".into(),
            egui::FontId::proportional(24.0 * z),
            egui::Color32::WHITE,
        )
        .size()
        .x
    }) / z;
    assert!(
        (r.w - text_w).abs() <= 8.0,
        "auto width {} should fit the text ({text_w})",
        r.w
    );
}

#[test]
fn text_box_click_draft_stays_finite_for_long_text() {
    let mut h = text_draft_board("text-autowidth-long");
    let world = h.app.board_xf().s2w(Pos2::new(600.0, 400.0));
    h.app.place_text_at(world);
    type_into_draft(&mut h, &"x".repeat(500));
    assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer.len(), 500);
    assert_draft_rect_sane(&h);
}

#[test]
fn text_box_click_draft_stays_finite_at_extreme_zoom() {
    for z in [0.05_f32, 20.0] {
        let mut h = text_draft_board("text-autowidth-zoom");
        h.app.tab_mut().cam.z = z;
        let world = h.app.board_xf().s2w(Pos2::new(600.0, 400.0));
        h.app.place_text_at(world);
        type_into_draft(&mut h, "hello world");
        assert_eq!(h.app.board_xf().z, z, "typed at the requested zoom");
        assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer, "hello world");
        assert_draft_rect_sane(&h);
    }
}

fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// A saved workbook, so new text documents have a folder beside it.
fn quote_board(tag: &str) -> Harness {
    let mut h = text_draft_board(tag);
    h.app.tab_mut().path = Some(h.base.join("quote.slate"));
    h
}

fn press_quote(h: &mut Harness, quote: char) {
    let modifiers = if quote == '"' {
        egui::Modifiers::SHIFT
    } else {
        egui::Modifiers::NONE
    };
    h.frame_with(|input| {
        input.modifiers = modifiers;
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(700.0, 420.0)));
        input.events.push(key_event(egui::Key::Quote, modifiers));
        input.events.push(egui::Event::Text(quote.to_string()));
    });
}

fn press_key(h: &mut Harness, key: egui::Key) {
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(700.0, 420.0)));
        input.events.push(key_event(key, egui::Modifiers::NONE));
    });
}

/// Grasshopper panel entry: `"` then Enter places one blank linked text
/// document at the pointer, as one undo step, with the caret in it.
#[test]
fn quote_then_enter_places_a_blank_text_document_ready_to_type() {
    use slate_doc::scene::NodeKind;
    let mut h = quote_board("quote-enter");
    let before = h.app.tab().journal.undo_depth();
    press_quote(&mut h, '"');
    assert!(
        h.app.palette_state.open,
        "the quote opens the canvas search"
    );
    assert_eq!(h.app.palette_state.query, "\"");
    assert_eq!(
        h.app
            .palette_items
            .iter()
            .map(|it| it.id.0)
            .collect::<Vec<_>>(),
        vec!["board.media.text_new"]
    );
    assert!(h.app.doc().scene.nodes.is_empty());
    h.frame();
    press_key(&mut h, egui::Key::Enter);

    assert!(!h.app.palette_state.open);
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "exactly one node");
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        before + 1,
        "one undo step"
    );
    let node = h.app.doc().scene.nodes[0].clone();
    let NodeKind::Image(img) = &node.kind else {
        panic!("a text media card is a linked image node");
    };
    let path = h.app.doc().item(img.item).unwrap().path.clone();
    assert_eq!(slate_doc::media_kind(&path), slate_doc::MediaKind::Text);
    assert_eq!(
        path.parent().unwrap(),
        h.base.join("slate-outputs").join("quote").join("text")
    );
    let pointer = h.app.board_xf().s2w(Pos2::new(700.0, 420.0));
    let center = Pos2::new(
        node.rect.x + node.rect.w / 2.0,
        node.rect.y + node.rect.h / 2.0,
    );
    assert!((center - pointer).length() < 1.0, "placed at the pointer");
    assert!(h
        .app
        .text_doc_edit
        .as_ref()
        .is_some_and(|e| e.node == node.id && e.buffer.is_empty()));

    h.frame();
    assert!(h.ctx.wants_keyboard_input(), "the caret holds the keyboard");
    h.frame_with(|input| input.events.push(egui::Event::Text("hi".into())));
    assert_eq!(h.app.text_doc_edit.as_ref().unwrap().buffer, "hi");
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "typing adds no nodes");
    press_key(&mut h, egui::Key::Escape);
    assert!(h.app.text_doc_edit.is_none());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_to_string(&path).ok().as_deref() != Some("hi") {
        assert!(
            std::time::Instant::now() < deadline,
            "typed words reach the linked file"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn single_quote_then_escape_creates_nothing() {
    let mut h = quote_board("quote-escape");
    let before = h.app.tab().journal.undo_depth();
    press_quote(&mut h, '\'');
    assert!(h.app.palette_state.open);
    assert_eq!(h.app.palette_state.query, "'");
    h.frame();
    press_key(&mut h, egui::Key::Escape);
    assert!(!h.app.palette_state.open);
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    assert!(h.app.text_doc_edit.is_none());
}

#[test]
fn text_box_drag_draft_keeps_its_wrap_width() {
    let mut h = text_draft_board("text-fixed-width");
    let world = h.app.board_xf().s2w(Pos2::new(600.0, 400.0));
    let rect = slate_doc::scene::WorldRect::new(world.x, world.y, 120.0, 48.0);
    h.app.begin_text_box_draft(world, rect, true);
    let long = "the quick brown fox jumps over the lazy dog";
    type_into_draft(&mut h, long);
    assert_eq!(h.app.text_box_draft.as_ref().unwrap().buffer, long);
    let r = assert_draft_rect_sane(&h);
    assert_eq!(r.w, 120.0, "drag-created width is the wrap width");
    h.app.commit_text_box_draft();
    let node = h.app.doc().scene.nodes.last().expect("committed");
    assert_eq!(node.rect.w, 120.0);
}

/// Double-click anywhere on a closed shape opens center-justified text editing
/// and the text configuration. A line does not.
#[test]
fn double_click_closed_shape_opens_text_editor() {
    use slate_doc::scene::{NodeKind, ShapeKind, ShapeNode, TextAlign};
    let mut h = Harness::new("shape-text");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let rect = slate_doc::scene::WorldRect::new(10.0, 20.0, 180.0, 90.0);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,
            text: None,
        }),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_double_click_for_test(egui::pos2(40.0, 50.0));
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_some_and(|(edit_id, body)| *edit_id == id && body.is_empty()));
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Text)
    );
    h.app.text_edit = Some((id, "Hello".into()));
    h.app.commit_text_edit();
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Shape(s) => {
            let text = s.text.as_ref().expect("hosted text");
            assert_eq!(text.body, "Hello");
            assert_eq!(text.align, TextAlign::Center);
        }
        _ => panic!("rectangle"),
    }
    h.app.board_undo();
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Shape(s) => assert!(s.text.is_none()),
        _ => panic!("rectangle"),
    }

    let line = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(300.0, 20.0, 80.0, 40.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Line,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,
            text: None,
        }),
    );
    let line_id = line.id;
    h.app.add_nodes(vec![line]);
    h.app.board_double_click_for_test(egui::pos2(340.0, 40.0));
    assert!(h
        .app
        .text_edit
        .as_ref()
        .is_none_or(|(edit_id, _)| *edit_id != line_id));
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
        softness: 0.0,
        stamp: false,
        tween_from: None,
        gaussian_blur: 0.0,
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: Some(data.into()),

            text: None,
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: Some(data.into()),

            text: None,
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,

            text: None,
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
        let r = h.app.resolve_draw_rect(
            Pos2::new(0.0, 0.0),
            Pos2::new(197.0, 280.0),
            tool,
            false,
            false,
        );
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
fn selection_properties_are_not_a_bottom_dock_panel() {
    use super::ui::tools::DOCK_ID;
    let mut h = kit_board("selection_strip", board::BoardTool::Select);
    h.app.dock_pins = vec![
        "selection".into(),
        "object.properties".into(),
        "document.settings".into(),
    ];
    h.frame();
    for id in ["selection", "object.properties", "document.settings"] {
        assert!(
            !atlas_shell::dock::panel_is_open(&h.ctx, DOCK_ID, id),
            "{id} must not return as a bottom-dock panel"
        );
    }
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.properties"), None));
    h.frame();
    assert!(!atlas_shell::dock::panel_is_open(
        &h.ctx,
        DOCK_ID,
        "selection"
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
        softness: 0.0,
        stamp: false,
        tween_from: None,
        gaussian_blur: 0.0,
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
    let NodeKind::Frame(first) = &h.app.doc().scene.nodes[0].kind else {
        panic!("expected a frame");
    };
    assert_eq!(
        first.corner,
        slate_doc::scene::Corner::Rounded {
            radius: slate_doc::media::TEXT_CARD_FILLET
        }
    );
    assert!(first.stroke.is_none(), "a new frame has no border");
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

fn deck_frame(h: &mut Harness, rect: slate_doc::scene::WorldRect, order: u32) -> NodeId {
    let node = h.app.doc_mut().scene.build_node(
        rect,
        NodeKind::Frame(slate_doc::scene::FrameNode {
            title: format!("S{order}"),
            order,
            fill: slate_doc::scene::Rgba::WHITE,
            fill_authored: false,
            assignments: Default::default(),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
        }),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    id
}

fn visible_frames(h: &Harness) -> Vec<NodeId> {
    h.app
        .doc()
        .scene
        .frames_in_order()
        .iter()
        .filter(|n| !n.hidden)
        .map(|n| n.id)
        .collect()
}

fn deck_release(h: &mut Harness, press: Pos2, release_world: Pos2, release_screen: Pos2) {
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(Pos2::new(0.0, 0.0), press, egui::Modifiers::default());
    h.app.end_gesture_for_test(
        release_world,
        Some(release_screen),
        egui::Modifiers::default(),
    );
}

/// GP1–GP6 for the Deck tool (`docs/keymap/contracts/frame-deck.md`).
#[test]
fn deck_clicks_build_a_prefix_and_undo_one_gesture_at_a_time() {
    let mut h = kit_board("deck_gp1", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 60.0),
        0,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 60.0),
        1,
    );
    let c = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(400.0, 0.0, 80.0, 60.0),
        2,
    );
    let before = h.app.tab().journal.undo_depth();
    deck_release(
        &mut h,
        Pos2::new(40.0, 30.0),
        Pos2::new(40.0, 30.0),
        Pos2::new(0.0, 0.0),
    );
    // A was already first, so the click journals nothing but stays in the session.
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    deck_release(
        &mut h,
        Pos2::new(440.0, 30.0),
        Pos2::new(440.0, 30.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![a, c, b]);
    deck_release(
        &mut h,
        Pos2::new(240.0, 30.0),
        Pos2::new(240.0, 30.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![a, c, b]);
    // C is already in the session, so this click moves it to the end.
    deck_release(
        &mut h,
        Pos2::new(440.0, 30.0),
        Pos2::new(440.0, 30.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![a, b, c]);
    assert_eq!(h.app.board_tool, board::BoardTool::Deck);
    assert!(h.app.presenting.is_none());
    h.app.board_undo();
    assert_eq!(visible_frames(&h), vec![a, c, b]);
    h.app.board_undo();
    assert_eq!(visible_frames(&h), vec![a, b, c]);
    assert_eq!(h.app.tab().journal.undo_depth(), before);
}

#[test]
fn deck_stroke_orders_frames_along_the_path_without_a_node() {
    let mut h = kit_board("deck_gp2", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 80.0),
        2,
    );
    let c = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 100.0, 80.0),
        0,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(400.0, 0.0, 100.0, 80.0),
        1,
    );
    let before_nodes = h.app.doc().scene.nodes.len();
    h.app.board_drag = h.app.begin_gesture_for_test(
        Pos2::new(0.0, 0.0),
        Pos2::new(50.0, 40.0),
        egui::Modifiers::default(),
    );
    h.app
        .update_gesture_for_test(Pos2::new(250.0, 40.0), egui::Modifiers::default());
    h.app.end_gesture_for_test(
        Pos2::new(450.0, 40.0),
        Some(Pos2::new(40.0, 0.0)),
        egui::Modifiers::default(),
    );
    assert_eq!(visible_frames(&h), vec![a, c, b]);
    assert_eq!(h.app.doc().scene.nodes.len(), before_nodes);
    assert!(h.app.board_drag.is_none());
}

#[test]
fn deck_travel_under_four_pixels_is_a_click() {
    let mut h = kit_board("deck_gp3", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 80.0),
        1,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(300.0, 0.0, 100.0, 80.0),
        0,
    );
    deck_release(
        &mut h,
        Pos2::new(50.0, 40.0),
        Pos2::new(350.0, 40.0),
        Pos2::new(3.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![a, b]);
}

#[test]
fn deck_escape_drops_the_stroke_then_disarms() {
    let mut h = kit_board("deck_gp4", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 60.0),
        0,
    );
    let _b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 60.0),
        1,
    );
    let before = h.app.tab().journal.undo_depth();
    h.app.board_drag = h.app.begin_gesture_for_test(
        Pos2::new(0.0, 0.0),
        Pos2::new(40.0, 30.0),
        egui::Modifiers::default(),
    );
    h.app
        .update_gesture_for_test(Pos2::new(240.0, 30.0), egui::Modifiers::default());
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None));
    assert!(h.app.board_drag.is_none());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    assert_eq!(visible_frames(&h)[0], a);
    assert_eq!(h.app.board_tool, board::BoardTool::Deck);
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None));
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
}

#[test]
fn deck_stroke_skips_a_hidden_frame() {
    let mut h = kit_board("deck_gp5", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 60.0),
        0,
    );
    let hidden = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(100.0, 0.0, 80.0, 60.0),
        1,
    );
    h.app.patch_nodes(&[hidden], |n| n.hidden = true);
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 60.0),
        2,
    );
    h.app.board_drag = h.app.begin_gesture_for_test(
        Pos2::new(0.0, 0.0),
        Pos2::new(20.0, 20.0),
        egui::Modifiers::default(),
    );
    h.app.end_gesture_for_test(
        Pos2::new(240.0, 20.0),
        Some(Pos2::new(30.0, 0.0)),
        egui::Modifiers::default(),
    );
    assert!(h.app.doc().scene.node(hidden).unwrap().hidden);
    assert_eq!(visible_frames(&h), vec![a, b]);
}

#[test]
fn deck_does_not_start_presentation_and_present_follows_the_new_order() {
    let mut h = kit_board("deck_gp6", board::BoardTool::Deck);
    let _a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 40.0),
        0,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 40.0),
        1,
    );
    deck_release(
        &mut h,
        Pos2::new(240.0, 20.0),
        Pos2::new(240.0, 20.0),
        Pos2::new(0.0, 0.0),
    );
    assert!(h.app.presenting.is_none());
    h.app.start_present(None);
    let first = h
        .app
        .doc()
        .scene
        .frames_in_order()
        .iter()
        .find(|n| !n.hidden)
        .map(|n| n.id);
    assert_eq!(first, Some(b));
    assert!(h.app.presenting.is_some());
}

#[test]
fn deck_rearm_appends_and_a_second_click_moves_to_the_end() {
    let mut h = kit_board("deck_session", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 40.0, 40.0),
        0,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(80.0, 0.0, 40.0, 40.0),
        1,
    );
    let c = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(160.0, 0.0, 40.0, 40.0),
        2,
    );
    deck_release(
        &mut h,
        Pos2::new(180.0, 20.0),
        Pos2::new(180.0, 20.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![c, a, b]);
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.set_board_tool(board::BoardTool::Deck);
    deck_release(
        &mut h,
        Pos2::new(100.0, 20.0),
        Pos2::new(100.0, 20.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![c, b, a]);
    deck_release(
        &mut h,
        Pos2::new(180.0, 20.0),
        Pos2::new(180.0, 20.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![b, c, a]);
}
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

/// Closed paths a frame painted (convex fills and closed strokes).
fn painted_closed_paths(out: &egui::FullOutput) -> Vec<Vec<Pos2>> {
    fn walk(shape: &egui::Shape, acc: &mut Vec<Vec<Pos2>>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
            egui::Shape::Path(p) if p.closed => acc.push(p.points.clone()),
            _ => {}
        }
    }
    let mut acc = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut acc);
    }
    acc
}

fn screen_bounds(pts: &[Pos2]) -> ERect {
    ERect::from_points(pts)
}

#[test]
fn armed_polygon_ghost_is_a_small_polygon_beside_the_pointer() {
    let mut h = arming_board("polygon_ghost", board::BoardTool::Polygon);
    h.frame();
    let p = h.app.canvas_rect.center();
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(p)));
    let sides = usize::from(slate_doc::scene::default_regular_sides());
    let size = board_place::place_tokens::GHOST_SIZE;
    let ghost = painted_closed_paths(&out).into_iter().find(|pts| {
        let b = screen_bounds(pts);
        pts.len() == sides && (b.height() - size).abs() < 1.0 && b.center().distance(p) < size * 3.0
    });
    assert!(
        ghost.is_some(),
        "armed Polygon must paint a {sides}-gon GhostFollow glyph at the pointer"
    );
}

#[test]
fn polygon_drag_preview_is_the_polygon_not_its_bounding_box() {
    let mut h = arming_board("polygon_preview", board::BoardTool::Polygon);
    h.frame();
    let a = h.app.canvas_rect.center();
    let b = a + EVec2::new(120.0, 90.0);
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(a));
        i.events.push(egui::Event::PointerButton {
            pos: a,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
    });
    let out = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(b)));
    let sides = usize::from(slate_doc::scene::default_regular_sides());
    let preview = painted_closed_paths(&out).into_iter().find(|pts| {
        let bb = screen_bounds(pts);
        pts.len() == sides && bb.height() > 60.0
    });
    let preview = preview.expect("the live preview is the polygon with its current sides");
    let bb = screen_bounds(&preview);
    let top = preview
        .iter()
        .min_by(|p, q| p.y.total_cmp(&q.y))
        .copied()
        .unwrap();
    assert!(
        (top.x - bb.center().x).abs() < 0.5,
        "first vertex at top center like the committed polygon: {preview:?}"
    );
}

#[test]
fn filleted_polygon_stroke_never_spikes() {
    use egui::epaint::{tessellator::Path, Mesh, PathStroke};
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 160.0, 160.0);
    let limit = ERect::from_min_size(Pos2::ZERO, EVec2::splat(160.0)).expand(24.0);
    for sides in 3..=12u8 {
        for radius in [4.0, 20.0, 45.0, 80.0, 1.0e4] {
            let outline = slate_doc::geom::regular_polygon_world_outline(
                rect,
                0.0,
                sides,
                slate_doc::scene::Corner::Rounded { radius },
                0.25,
            );
            let pts: Vec<Pos2> = outline.iter().map(|p| Pos2::new(p[0], p[1])).collect();
            let mut path = Path::default();
            path.add_line_loop(&pts);
            let mut mesh = Mesh::default();
            path.stroke_closed(
                1.0,
                &PathStroke::new(4.0_f32, egui::Color32::WHITE),
                &mut mesh,
            );
            for v in &mesh.vertices {
                assert!(
                    v.pos.x.is_finite() && v.pos.y.is_finite() && limit.contains(v.pos),
                    "sides={sides} radius={radius}: stroke vertex {:?} spikes out",
                    v.pos
                );
            }
        }
    }
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

/// Ctrl during a rectangle or ellipse drag keeps the press point as the center.
#[test]
fn ctrl_drag_draws_rect_and_circle_from_center() {
    let mut h = arming_board("draw_from_center_rect", board::BoardTool::RectShape);
    let mods = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    h.app.finish_draw(
        Pos2::new(40.0, 30.0),
        Pos2::new(140.0, 70.0),
        board::BoardTool::RectShape,
        mods,
    );
    let r = h.app.doc().scene.nodes[0].rect;
    let (cx, cy) = r.center();
    assert!((cx - 40.0).abs() < 0.01 && (cy - 30.0).abs() < 0.01);
    assert!((r.w - 200.0).abs() < 0.01 && (r.h - 80.0).abs() < 0.01);

    let mut h = arming_board("draw_from_center_circle", board::BoardTool::Ellipse);
    let mods = egui::Modifiers {
        ctrl: true,
        shift: true,
        ..Default::default()
    };
    h.app.finish_draw(
        Pos2::new(0.0, 0.0),
        Pos2::new(50.0, 20.0),
        board::BoardTool::Ellipse,
        mods,
    );
    let r = h.app.doc().scene.nodes[0].rect;
    assert!((r.w - r.h).abs() < 0.01 && (r.w - 100.0).abs() < 0.01);
    let (cx, cy) = r.center();
    assert!(cx.abs() < 0.01 && cy.abs() < 0.01);
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
    admit_profiles: Vec<(slate_doc::NodeId, String)>,
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
        log.admit_profiles.push((id, req.profile.clone()));
        log.current_urls
            .entry(id)
            .or_insert_with(|| req.target.clone());
    }
    fn evict(&mut self, id: slate_doc::NodeId) {
        self.0.borrow_mut().admitted.remove(&id);
    }
    fn take_frame(&mut self, id: slate_doc::NodeId) -> Option<board_web::WebFrame> {
        self.0
            .borrow()
            .admitted
            .contains(&id)
            .then(|| egui::ColorImage::new([8, 8], egui::Color32::from_rgb(30, 90, 160)).into())
    }
    fn capture_poster(&mut self, _id: slate_doc::NodeId) -> Option<board_web::WebFrame> {
        Some(egui::ColorImage::new([8, 8], egui::Color32::from_rgb(30, 90, 160)).into())
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

/// A sign-in redirect or a followed link to another origin keeps the portal's
/// own cookie jar; only the authored locator picks the profile (D15, D32).
#[test]
fn a_page_on_another_origin_keeps_the_authored_profile() {
    let mut h = web_board("web_profile_follows_locator");
    let host = with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    web_settle(&mut h, &[(id, 540.0)], 2);
    host.set_current_url(id, "https://accounts.google.com/signin");
    web_settle(&mut h, &[(id, 540.0)], 2);

    let authored = slate_doc::scene::web_profile_name(board_web::WEB_START_LOCATOR);
    let last = host.0.borrow().admit_profiles.last().cloned();
    assert_eq!(
        last,
        Some((id, authored)),
        "the page's current origin must not choose the cookie jar"
    );
    assert_eq!(
        host.admit_targets().last().map(|(_, t)| t.clone()),
        Some("https://accounts.google.com/signin".to_string()),
        "the page itself still resumes where it is"
    );
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

fn assert_send_shows_thinking(h: &Harness) {
    let awaiting = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .any(|n| h.app.agent_is_awaiting(n.id));
    assert!(awaiting, "Send must enter Thinking — never a blank wait");
    assert!(h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .all(|n| h.app.agent_failure_reason(n.id).is_none()));
}

/// Hermetic: `local` writes `request.json` only; it never starts Ollama/Codex runtimes.
fn send_thinking_harness(tag: &str) -> (Harness, slate_doc::NodeId) {
    let mut h = agent_board(tag);
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    let ws = h.base.join("ai-ws");
    std::fs::create_dir_all(&ws).unwrap();
    h.app.ai.config.workspace_dir = Some(ws);
    *h.app.agents.prompt_mut(id) = "what is 2+2?".into();
    (h, id)
}

/// Train cards default to message-pair presentation (`bind_program` since agent-pairs).
#[test]
fn sending_a_prompt_shows_thinking_not_silence_in_pair_mode() {
    let (mut h, id) = send_thinking_harness("agent_thinking_pair");
    assert!(matches!(
        h.app.doc().scene.node(id).and_then(slate_doc::agent_chat::agent),
        Some(a) if a.chat.detail == slate_doc::agent_chat::Detail::Pair
    ));
    h.app.send_agent_prompt(id);
    assert_send_shows_thinking(&h);
    // First pair send on a non-linear provider spawns the tail card (draft is false).
    assert!(
        h.app.doc().scene.nodes.iter().any(|n| {
            slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id))
        }),
        "pair presentation still trains on the first send"
    );
}

/// Summary presentation spawns the next train card; awaiting follows the tail.
#[test]
fn sending_a_prompt_shows_thinking_not_silence_in_summary_mode() {
    use slate_doc::scene::NodeKind;
    let (mut h, id) = send_thinking_harness("agent_thinking_summary");
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.detail = slate_doc::agent_chat::Detail::Summary;
        }
    });
    h.app.send_agent_prompt(id);
    assert_send_shows_thinking(&h);
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

/// The restore glyph on a maximized portal is the click that leaves maximize.
/// It has to land for every kind, including a press and release in one frame.
#[test]
fn maximized_restore_glyph_click_leaves_maximize() {
    fn click_restore(h: &mut Harness, kind: slate_doc::scene::PortalKind, id: NodeId) {
        h.app.portal_maximize(id);
        h.frame();
        let screen = h.app.canvas_rect;
        let node = h.app.doc().scene.node(id).unwrap().rect;
        let host = board_portal_chrome::maximized_host_rect(kind, node, screen);
        let layout = board_portal_chrome::layout_for_portal(
            kind,
            host,
            false,
            true,
            slate_doc::scene::Corner::Rounded {
                radius: slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET,
            },
            1.0,
        );
        let at = layout.maximize.center();
        assert!(
            layout.maximize.width() > 8.0 && layout.maximize.contains(at),
            "{kind:?} restore hit {:?} does not contain its center",
            layout.maximize
        );
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(at));
        });
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(at));
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
        });
        assert_eq!(
            h.app.portal_chrome.maximized, None,
            "{kind:?} press on the restore glyph did not leave maximize"
        );
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(at));
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
        });
        assert_eq!(
            h.app.portal_chrome.maximized, None,
            "{kind:?} restore click at {at:?} in {:?} did not leave maximize",
            layout.maximize
        );
    }

    let mut web = web_board("restore_web");
    with_fake_host(&mut web);
    web.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&web);
    click_restore(&mut web, slate_doc::scene::PortalKind::Web, id);

    // D33: agent cards do not offer maximize, so the restore glyph is absent.
    // D34: Esc still leaves maximize.
    let mut agent = web_board("restore_agent");
    agent.app.place_agent_portal_at(Pos2::ZERO);
    let id = agent.app.doc().scene.nodes[0].id;
    agent.app.portal_maximize(id);
    agent.frame();
    let screen = agent.app.canvas_rect;
    let node = agent.app.doc().scene.node(id).unwrap().rect;
    let host =
        board_portal_chrome::maximized_host_rect(slate_doc::scene::PortalKind::Agent, node, screen);
    let layout = board_portal_chrome::layout_for_portal(
        slate_doc::scene::PortalKind::Agent,
        host,
        false,
        true,
        slate_doc::scene::Corner::Rounded {
            radius: slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET,
        },
        1.0,
    );
    assert_eq!(
        layout.maximize,
        egui::Rect::NOTHING,
        "agent cards omit the maximize glyph"
    );
    agent.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert_eq!(
        agent.app.portal_chrome.maximized, None,
        "Esc restores a maximized agent portal"
    );

    let mut atlas = web_board("restore_atlas");
    atlas.app.place_atlas_portal_at(Pos2::ZERO);
    let id = atlas.app.doc().scene.nodes[0].id;
    click_restore(&mut atlas, slate_doc::scene::PortalKind::FileAtlas, id);
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

/// The enlarged grip hit reaches outside the node and stops at the edge.
#[test]
fn wire_grip_hit_reaches_outside_the_node_only() {
    let mut h = web_board("wire_grip_outward");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip = xf.w2s(board_wire::grip_point(rect, slate_doc::scene::Side::Top));
    let outside = grip + EVec2::new(0.0, -30.0);
    let inside = grip + EVec2::new(0.0, 30.0);

    assert_eq!(
        h.app.wire_grip_at(outside, &xf).map(|(_, side, _)| side),
        Some(slate_doc::scene::Side::Top),
        "30 px outside the edge is a grip"
    );
    assert!(
        h.app.wire_grip_at(inside, &xf).is_none(),
        "the same distance inside the node is not a grip"
    );

    let mods = egui::Modifiers::default();
    let drag = h.app.begin_gesture_for_test(outside, xf.s2w(outside), mods);
    assert!(
        matches!(drag, Some(board::BoardDrag::Wire(_))),
        "a press in the outward hit starts a wire"
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

/// Dropping a new wire on empty canvas commits a free end there.
/// The tool-search palette stays closed. Undo removes that wire.
#[test]
fn wire_drop_on_empty_canvas_keeps_a_free_end() {
    let mut h = web_board("wire_drop_empty");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip = xf.w2s(board_wire::grip_point(rect, slate_doc::scene::Side::Right));
    let mods = egui::Modifiers::default();
    let Some(board::BoardDrag::Wire(mut wd)) =
        h.app.begin_gesture_for_test(grip, xf.s2w(grip), mods)
    else {
        panic!("press on the grip starts a wire");
    };

    let drop = Pos2::new(rect.x + rect.w + 240.0, rect.y + rect.h * 0.5);
    h.app.wire_drag_update(&mut wd, drop, false);
    assert!(wd.snap.is_none(), "blank canvas is not a snap target");
    let end = [wd.cursor.x, wd.cursor.y];
    h.app.finish_wire_drag(wd);

    assert!(
        !h.app.palette_state.open,
        "empty release must not open the tool search"
    );
    let (a, b) = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            slate_doc::scene::NodeKind::Connector(c) => Some((c.a, c.b)),
            _ => None,
        })
        .expect("a wire is committed");
    assert!(matches!(
        a,
        slate_doc::scene::ConnectorEnd::Anchored { node, .. } if node == id
    ));
    match b {
        slate_doc::scene::ConnectorEnd::Free { point } => assert_eq!(point, end),
        other => panic!("free end at the drop, got {other:?}"),
    }
    assert!(
        slate_doc::wire::connector_route_in_scene(
            &h.app.doc().scene,
            None,
            &a,
            &b,
            h.app.board_wire_routing,
        )
        .is_some(),
        "the free end draws"
    );

    h.app.board_undo();
    assert!(
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .all(|n| !matches!(n.kind, slate_doc::scene::NodeKind::Connector(_))),
        "undo removes the wire"
    );
}

/// Alt on a scale handle copies, then scales the copy. The original stays.
#[test]
fn alt_edge_scale_copies_and_keeps_the_original() {
    let mut h = web_board("alt_scale_copy");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let xf = h.app.board_xf();
    // Off the edge midpoint so a wire grip does not steal the press.
    let edge = xf.w2s(Pos2::new(rect.x + rect.w, rect.y + 12.0));
    let edge_world = xf.s2w(edge);
    h.app.alt_down = true;
    let drag = h.app.begin_gesture_for_test(
        edge,
        edge_world,
        egui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    assert!(
        matches!(drag, Some(board::BoardDrag::Resize { dup: true, .. })),
        "Alt on an edge must scale a copy"
    );
    h.app.board_drag = drag;
    let grown = Pos2::new(rect.x + rect.w + 50.0, rect.y + 12.0);
    h.app.update_gesture_for_test(
        grown,
        egui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    h.app.end_gesture_for_test(
        grown,
        Some(xf.w2s(grown)),
        egui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );

    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    let original = h.app.doc().scene.node(id).unwrap().rect;
    assert!(
        (original.w - 80.0).abs() < 0.01 && (original.h - 60.0).abs() < 0.01,
        "original must stay, got {}×{}",
        original.w,
        original.h
    );
    let copy = h.app.doc().scene.nodes.iter().find(|n| n.id != id).unwrap();
    assert!(
        copy.rect.w > original.w + 20.0,
        "copy must grow, got {}",
        copy.rect.w
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(h.app.doc().scene.node(id).is_some());
}

/// Ctrl on an edge scales about the center.
#[test]
fn ctrl_edge_scale_keeps_the_center() {
    let mut h = web_board("ctrl_scale_center");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.insert(id);
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().rect;
    let (cx, cy) = before.center();
    let xf = h.app.board_xf();
    let edge = xf.w2s(Pos2::new(before.x + before.w, before.y + 12.0));
    let mods = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    h.app.ctrl_down = true;
    h.app.board_drag = h.app.begin_gesture_for_test(edge, xf.s2w(edge), mods);
    let grown = Pos2::new(before.x + before.w + 40.0, before.y + 12.0);
    h.app.update_gesture_for_test(grown, mods);
    h.app.end_gesture_for_test(grown, Some(xf.w2s(grown)), mods);
    let after = h.app.doc().scene.node(id).unwrap().rect;
    let (nx, ny) = after.center();
    assert!(
        (nx - cx).abs() < 0.5 && (ny - cy).abs() < 0.5,
        "center walked to {nx},{ny}"
    );
    assert!(after.w > before.w + 20.0, "width {}", after.w);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
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

#[test]
fn fillet_grip_press_beats_edge_band_but_nw_corner_still_resizes() {
    let mut h = web_board("fillet_press_order");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.patch_nodes(&[id], |node| {
        slate_doc::scene::set_corner(node, slate_doc::scene::Corner::Rounded { radius: 12.0 });
    });
    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    h.app.tab_mut().cam.z = 1.0;

    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let geom = board_handles::selection_geom(&xf, node.rect, node.rotation_deg);
    let grip = h.app.fillet_grip_at(&node, &xf).expect("visible grip");
    let overlap = Pos2::new(
        grip.x,
        geom.corners[0].y
            + atlas_shell::canvas_scale::hit_px(board_handles::EDGE_BAND_PX, geom.zoom),
    );
    assert!(board_handles::hit_test_fillet_grip(overlap, &geom, grip));
    assert!(board_handles::hit_test_resize_bands(overlap, &geom).is_some());
    assert!(matches!(
        h.app
            .begin_gesture(overlap, xf.s2w(overlap), egui::Modifiers::NONE),
        Some(board::BoardDrag::FilletRadius { id: hit, .. }) if hit == id
    ));

    let nw = geom.corners[0];
    assert!(matches!(
        h.app
            .begin_gesture(nw, xf.s2w(nw), egui::Modifiers::NONE),
        Some(board::BoardDrag::Resize { id: hit, handle: 0, .. }) if hit == id
    ));
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
                sides: slate_doc::scene::default_regular_sides(),
                flip: false,
                path: None,

                text: None,
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
                sides: slate_doc::scene::default_regular_sides(),
                flip: false,
                path: None,

                text: None,
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
    let mut tokens = atlas_shell::tokens::current().dock.clone();
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

fn sweep(h: &mut Harness, from: Pos2, to: Pos2, mods: egui::Modifiers) {
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::vec2(0.0, 0.0);
    let xf = h.app.board_xf();
    let drag = h.app.begin_gesture_for_test(xf.w2s(from), from, mods);
    let kind = match &drag {
        None => "none",
        Some(board::BoardDrag::Marquee { .. }) => "marquee",
        Some(board::BoardDrag::Move { .. }) => "move",
        Some(board::BoardDrag::LineGrip { .. }) => "line-grip",
        Some(board::BoardDrag::Wire(_)) => "wire",
        Some(board::BoardDrag::Resize { .. }) => "resize",
        Some(board::BoardDrag::GroupResize { .. }) => "group-resize",
        Some(board::BoardDrag::Draw { .. }) => "draw",
        Some(board::BoardDrag::Direct(_)) => "direct",
        Some(_) => "other",
    };
    let hit = h.app.board_pick_node(from.x, from.y);
    let rect = hit.and_then(|id| h.app.doc().scene.node(id).map(|n| (n.id, n.rect)));
    assert!(
        matches!(drag, Some(board::BoardDrag::Marquee { .. })),
        "sweep must start on empty board, got {kind} pick={rect:?} at {from:?} z={}",
        h.app.tab().cam.z
    );
    h.app.board_drag = drag;
    h.app.end_gesture_for_test(to, Some(xf.w2s(to)), mods);
}

/// GP1 — left-to-right over the middle of a line misses it.
#[test]
fn sweep_gp1_window_misses_a_crossing_line() {
    let mut h = align_board("sweep_gp1");
    let line = add_stroke(&mut h.app, 0.0, 50.0);
    sweep(
        &mut h,
        Pos2::new(-80.0, 20.0),
        Pos2::new(40.0, 80.0),
        egui::Modifiers::NONE,
    );
    assert!(!h.app.board_sel.contains(&line));
}

/// GP2 — right-to-left over that middle selects the line.
#[test]
fn sweep_gp2_crossing_hits_the_line() {
    let mut h = align_board("sweep_gp2");
    let line = add_stroke(&mut h.app, 0.0, 50.0);
    sweep(
        &mut h,
        Pos2::new(40.0, 120.0),
        Pos2::new(-80.0, 0.0),
        egui::Modifiers::NONE,
    );
    assert!(h.app.board_sel.contains(&line));
}

/// GP3 — window takes a fully inside rect and skips an edge overlap.
#[test]
fn sweep_gp3_window_takes_only_the_contained_rect() {
    let mut h = align_board("sweep_gp3");
    let inside = add_rect(&mut h.app, 10.0, 10.0);
    let overlap = add_rect(&mut h.app, 90.0, 10.0);
    sweep(
        &mut h,
        Pos2::new(-40.0, -40.0),
        Pos2::new(120.0, 100.0),
        egui::Modifiers::NONE,
    );
    assert!(h.app.board_sel.contains(&inside));
    assert!(!h.app.board_sel.contains(&overlap));
}

/// GP4 — Shift crossing adds without dropping the current selection.
#[test]
fn sweep_gp4_shift_adds() {
    let mut h = align_board("sweep_gp4");
    let kept = add_rect(&mut h.app, 400.0, 400.0);
    let line = add_stroke(&mut h.app, 0.0, 50.0);
    h.app.board_sel.insert(kept);
    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    sweep(&mut h, Pos2::new(40.0, 120.0), Pos2::new(-80.0, 0.0), shift);
    assert!(h.app.board_sel.contains(&kept));
    assert!(h.app.board_sel.contains(&line));
}

/// GP5 — a window with no modifier replaces the selection.
#[test]
fn sweep_gp5_window_replaces() {
    let mut h = align_board("sweep_gp5");
    let inside = add_rect(&mut h.app, 10.0, 10.0);
    let other = add_rect(&mut h.app, 400.0, 400.0);
    h.app.board_sel.insert(inside);
    h.app.board_sel.insert(other);
    sweep(
        &mut h,
        Pos2::new(-40.0, -40.0),
        Pos2::new(120.0, 100.0),
        egui::Modifiers::NONE,
    );
    assert!(h.app.board_sel.contains(&inside));
    assert!(!h.app.board_sel.contains(&other));
}

/// A single-click brush dab answers a left-to-right window sweep, a
/// right-to-left crossing sweep that only grazes its rim, and a click on its
/// ink away from the center.
#[test]
fn a_brush_dab_answers_sweeps_both_ways_and_a_click() {
    let mut h = align_board("brush_dab_sweep");
    h.app.brush_width = 20.0;
    h.app.finish_freehand_brush(vec![Pos2::new(100.0, 100.0)]);
    let dab = h.app.doc().scene.nodes.last().unwrap().id;
    h.frame();
    let none = egui::Modifiers::NONE;
    let swept = |h: &mut Harness, from: Pos2, to: Pos2| {
        h.app.board_sel.clear();
        sweep(h, from, to, none);
        h.app.board_sel.contains(&dab)
    };
    assert!(
        swept(&mut h, Pos2::new(50.0, 50.0), Pos2::new(125.0, 125.0)),
        "left-to-right window around the dab"
    );
    assert!(
        swept(&mut h, Pos2::new(150.0, 150.0), Pos2::new(104.0, 104.0)),
        "right-to-left crossing through the dab's rim"
    );
    assert!(
        !swept(&mut h, Pos2::new(50.0, 50.0), Pos2::new(104.0, 104.0)),
        "a window that cuts the dab leaves it"
    );
    h.app.board_sel.clear();
    h.app.board_click_for_test(Pos2::new(106.0, 100.0), none);
    assert!(h.app.board_sel.contains(&dab), "a click on the dab's ink");
}

/// A frame that covers the viewport selects its members on drag. A smaller
/// frame still moves.
#[test]
fn frame_drag_moves_until_the_frame_covers_the_viewport() {
    let mut h = align_board("frame_cover_select");
    assert_eq!(board::FramePreset::Tabloid.size(), (1224.0, 792.0));
    let frame = h.seed_frame(None);
    let inside = add_rect(&mut h.app, 100.0, 80.0);
    let outside = add_rect(&mut h.app, 2000.0, 80.0);
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::vec2(400.0, 225.0);
    let press = Pos2::new(400.0, 300.0);
    let drag =
        h.app
            .begin_gesture_for_test(h.app.board_xf().w2s(press), press, egui::Modifiers::NONE);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "a frame smaller than the viewport moves"
    );

    h.app.board_drag = None;
    h.app.tab_mut().cam.z = 2.0;
    let drag =
        h.app
            .begin_gesture_for_test(h.app.board_xf().w2s(press), press, egui::Modifiers::NONE);
    assert!(
        matches!(
            drag,
            Some(board::BoardDrag::Marquee { frame: Some(id), .. }) if id == frame
        ),
        "a frame covering the viewport selects inside"
    );
    h.app.board_drag = drag;
    let end = Pos2::new(120.0, 90.0);
    h.app
        .end_gesture_for_test(end, Some(h.app.board_xf().w2s(end)), egui::Modifiers::NONE);
    assert!(h.app.board_sel.contains(&inside));
    assert!(!h.app.board_sel.contains(&frame));
    assert!(!h.app.board_sel.contains(&outside));
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
        dup: false,
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
                dup: false,
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: Some(path.into()),

            text: None,
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,

            text: None,
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
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,

            text: None,
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
    use slate_doc::scene::{TextAlign, TextNode, Typeface};
    let mut h = trim_board("trim_text_clip");
    let text = {
        let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 40.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Text(TextNode {
                text: "HELLO".into(),
                family: Typeface::Sans,
                size: 24.0,
                color: slate_doc::scene::Rgba::BLACK,
                align: TextAlign::Left,
                fill: None,
                agent: None,
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
    use slate_doc::scene::{TextAlign, TextNode, Typeface};
    let mut h = join_board("join_skips_text");
    let a = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    let b = add_filled_rect(&mut h.app, 20.0, 20.0, 40.0, 40.0);
    let text = {
        let rect = slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 24.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Text(TextNode {
                text: "keep".into(),
                family: Typeface::Sans,
                size: 14.0,
                color: slate_doc::scene::Rgba::BLACK,
                align: TextAlign::Left,
                fill: None,
                agent: None,
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

    let wire = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| n.id != a && n.id != b)
        .unwrap()
        .id;
    let beside = board_path::board_pick_node_routed(
        &h.app.doc().scene,
        84.0,
        30.0,
        1.0,
        false,
        h.app.board_wire_routing,
    );
    assert_ne!(
        beside,
        Some(wire),
        "the pick slop on the neighboring node must not select the wire"
    );
    let mid = board_path::board_pick_node_routed(
        &h.app.doc().scene,
        140.0,
        30.0,
        1.0,
        false,
        h.app.board_wire_routing,
    );
    assert_eq!(mid, Some(wire), "the open span of the wire still hits");
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
        slate_doc::agent_chat::Detail::Pair
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
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .train,
        "train is the default presentation"
    );
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("portal.agent.chat"), None);
    assert!(
        !slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .train
    );
    h.app
        .agent_set_detail(slate_doc::agent_chat::Detail::Identity);
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .detail,
        slate_doc::agent_chat::Detail::Full
    );
    h.app.board_undo();
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
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
fn contents_focus_suppresses_frame_selection_chrome_for_every_host() {
    let mut h = agent_board("portal_chrome_suppress");
    let portals = [
        slate_doc::PortalNode::unbound_file_atlas("File Atlas"),
        slate_doc::PortalNode::unbound_web("Web"),
        slate_doc::PortalNode::unbound_agent("Agent portal", ""),
    ];
    let mut ids = Vec::new();
    for (i, portal) in portals.into_iter().enumerate() {
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(i as f32 * 400.0, 0.0, 320.0, 200.0),
            NodeKind::Portal(portal),
        );
        ids.push(node.id);
        h.app.add_nodes(vec![node]);
    }
    for id in ids {
        assert!(!h.app.portal_frame_chrome_suppressed(id));
        assert!(!h.app.selection_stringers_suppressed());
        h.app.portal_enter_interactive(id);
        assert!(h.app.portal_frame_chrome_suppressed(id));
        assert!(h.app.frame_chrome_suppressed(id));
        assert!(h.app.selection_stringers_suppressed());
        h.app.contents_blur();
        assert!(!h.app.portal_frame_chrome_suppressed(id));
        assert!(!h.app.frame_chrome_suppressed(id));
        assert!(!h.app.selection_stringers_suppressed());
    }
}

/// Double-click into a text frame, a shape's text, or a sheet cell drops the
/// single-click selection cast. The frame stays selected. Leaving the edit
/// brings the cast back.
#[test]
fn entered_media_suppresses_the_selection_cast() {
    use slate_doc::scene::{NodeKind, ShapeKind, ShapeNode, TextAlign, TextNode, Typeface};
    let mut h = agent_board("media_cast");
    let text = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 80.0),
        NodeKind::Text(TextNode {
            text: "Note".into(),
            family: Typeface::Sans,
            size: 18.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: TextAlign::Left,
            fill: Some(slate_doc::scene::Rgba::WHITE),
            agent: None,
        }),
    );
    let text_id = text.id;
    h.app.add_nodes(vec![text]);
    let shape = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(300.0, 0.0, 180.0, 90.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,
            text: None,
        }),
    );
    let shape_id = shape.id;
    h.app.add_nodes(vec![shape]);

    h.app.board_sel.clear();
    h.app.board_sel.insert(text_id);
    assert!(!h.app.frame_chrome_suppressed(text_id));
    assert!(!h.app.selection_stringers_suppressed());
    h.app.text_edit = Some((text_id, "Note".into()));
    assert!(h.app.frame_chrome_suppressed(text_id));
    assert!(h.app.selection_stringers_suppressed());
    h.app.commit_text_edit();
    assert!(!h.app.frame_chrome_suppressed(text_id));

    h.app.board_sel.clear();
    h.app.board_sel.insert(shape_id);
    h.app.text_edit = Some((shape_id, String::new()));
    assert!(h.app.frame_chrome_suppressed(shape_id));
    assert!(h.app.selection_stringers_suppressed());
    h.app.commit_text_edit();
    assert!(!h.app.frame_chrome_suppressed(shape_id));

    h.app.sheet_edit = Some(board::SheetEdit {
        node: text_id,
        item: slate_doc::ItemId(1),
        row: 0,
        col: 0,
        buf: "a".into(),
        origin: "a".into(),
        fresh: false,
        screen: egui::Rect::NOTHING,
        font_px: 12.0,
    });
    h.app.board_sel.clear();
    h.app.board_sel.insert(text_id);
    assert!(h.app.frame_chrome_suppressed(text_id));
    assert!(h.app.selection_stringers_suppressed());
    h.app.sheet_edit = None;
    assert!(!h.app.frame_chrome_suppressed(text_id));
    assert!(!h.app.selection_stringers_suppressed());
}

#[test]
fn sheet_enter_keeps_the_typed_number_until_save() {
    let mut h = Harness::new("sheet_enter");
    let path = h.base.join("rows.csv");
    std::fs::write(&path, "A,B\n1,2\n").unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(path.clone(), "rows.csv", 8, 0, "rows");
    h.app
        .sheets
        .insert(item, atlas_core::table::read_sheet_card(&path));
    h.app.sheet_edit = Some(board::SheetEdit {
        node: NodeId(1),
        item,
        row: 1,
        col: 1,
        buf: "9".into(),
        origin: "2".into(),
        fresh: false,
        screen: egui::Rect::NOTHING,
        font_px: 12.0,
    });
    h.app.commit_sheet_edit();
    let shown = h
        .app
        .sheets
        .get(&item)
        .and_then(|g| g.as_ref())
        .and_then(|rows| rows.get(1))
        .and_then(|row| row.get(1))
        .map(|cell| cell.text.as_str());
    assert_eq!(shown, Some("9"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "A,B\n1,2\n");
    assert!(h.app.sheet_dirty);
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

#[test]
fn pasted_chat_train_forks_source_and_undo_removes_cards() {
    let mut h = agent_board("train_fork_paste");
    h.app.ai.config.workspace_dir = Some(h.base.join("ai-ws"));
    std::fs::create_dir_all(h.app.ai.config.workspace_dir.as_ref().unwrap()).unwrap();
    h.app.place_agent_portal_at(Pos2::ZERO);
    let root = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(root, "local");
    let session = slate_doc::scene::new_agent_session_id();
    let dir = atlas_ai::agent::agent_dir(h.app.ai.config.workspace_dir.as_ref().unwrap(), &session);
    std::fs::create_dir_all(&dir).unwrap();
    let turns = vec![
        atlas_ai::agent::AgentTurn {
            role: "user".into(),
            text: "hello".into(),
            at: 0,
        },
        atlas_ai::agent::AgentTurn {
            role: "assistant".into(),
            text: "hi".into(),
            at: 1,
        },
    ];
    let state = atlas_ai::agent::AgentSession {
        approval: None,
        conversation: "provider-1".into(),
        artifacts: vec![],
        status: atlas_ai::agent::AgentStatus::Idle,
        provider: "local".into(),
        turns: turns.clone(),
        updated_at: 0,
        bundle: Default::default(),
        request: String::new(),
    };
    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &state).unwrap();
    let manifest = slate_doc::SourceUri {
        locator: super::board_portal::source_locator(None, &dir.join("session.json")),
    };
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.session = session.clone();
            a.channel = Some("provider-1".into());
            a.bundle = Some(manifest.clone());
            a.chat.train = true;
            a.chat.detail = slate_doc::agent_chat::Detail::Pair;
            p.title = "Courtyard".into();
        }
    });
    let original_session = session;
    h.app.board_sel.clear();
    h.app.board_sel.insert(root);
    assert_eq!(h.app.board_copy(&h.ctx), 1);
    h.app.board_sel.clear();
    let before = h.app.doc().scene.nodes.len();
    assert_eq!(
        h.app.board_paste(None, Some(egui::Pos2::new(900.0, 0.0))),
        1
    );
    assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
    let pasted = *h.app.board_sel.iter().next().unwrap();
    let forked = slate_doc::agent_chat::agent(h.app.doc().scene.node(pasted).unwrap()).unwrap();
    assert_ne!(forked.session, original_session);
    assert!(forked.channel.is_none());
    assert!(forked
        .bundle
        .as_ref()
        .is_some_and(|b| b.locator.contains("session.json")));
    let NodeKind::Portal(p) = &h.app.doc().scene.node(pasted).unwrap().kind else {
        panic!("portal")
    };
    assert!(p.title.contains("Forked from Courtyard"));
    let original_turns = std::fs::read_to_string(dir.join("session.json")).unwrap();
    assert!(original_turns.contains("provider-1"));
    assert_ne!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(root).unwrap())
            .unwrap()
            .session,
        forked.session
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), before);
}

#[test]
fn an_agent_place_file_spawns_a_file_atlas_portal() {
    let mut h = agent_board("atlas_place");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    let folder = h.base.join("penn-station-images");
    std::fs::create_dir_all(&folder).unwrap();
    let link = h.base.join("link");
    std::fs::create_dir_all(&link).unwrap();
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    std::fs::write(
        link.join("place.json"),
        r#"{"id":"penn","kind":"file_atlas","path":"penn-station-images"}"#,
    )
    .unwrap();
    assert!(h.app.consume_atlas_place(id, &link));
    assert!(
        !link.join("place.json").exists(),
        "the request is consumed once the portal exists"
    );
    let atlas = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| {
            matches!(
                &n.kind,
                NodeKind::Portal(p) if p.kind == slate_doc::scene::PortalKind::FileAtlas
            )
        })
        .expect("a File Atlas portal");
    let host = h.app.doc().scene.node(id).unwrap();
    assert!(atlas.rect.x >= host.rect.x + host.rect.w);
    assert!(!h.app.consume_atlas_place(id, &link));
}

#[test]
fn train_composer_grows_with_the_message_instead_of_a_tall_empty_card() {
    let mut h = agent_board("train_composer_fit");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "local");
    h.frame();
    *h.app.agents.prompt_mut(id) = "Hello".into();
    h.app.agents.prompt_epoch = h.app.agents.prompt_epoch.wrapping_add(1);
    h.app.fit_agent_cards(&h.ctx);
    let short = h.app.doc().scene.node(id).unwrap().rect.h;
    assert!(
        short < 120.0,
        "a short train message should sit in a short card, height {short}"
    );
    *h.app.agents.prompt_mut(id) = (0..24)
        .map(|i| format!("line {i} wraps across the card"))
        .collect::<Vec<_>>()
        .join("\n");
    h.app.agents.prompt_epoch = h.app.agents.prompt_epoch.wrapping_add(1);
    h.app.fit_agent_cards(&h.ctx);
    let long = h.app.doc().scene.node(id).unwrap().rect.h;
    assert!(
        long > short + 80.0,
        "the card should grow with the message ({short} -> {long})"
    );
}

#[test]
fn brush_hud_scrubs_size_and_softness_and_escape_restores() {
    let mut h = Harness::new("brush_hud");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.alt_down = true;
    h.app.brush_width = 10.0;
    h.app.brush_softness = 0.0;
    h.app.tab_mut().cam.z = 1.0;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(40.0, -50.0)), true, false));
    assert!((h.app.brush_width - 50.0).abs() < 0.01);
    assert!((h.app.brush_softness - 0.5).abs() < 0.01);
    h.app.cancel_brush_hud();
    assert!((h.app.brush_width - 10.0).abs() < 0.01);
    assert!(h.app.brush_softness.abs() < 0.01);
    assert!(h.app.brush_hud.is_none());

    h.app.alt_down = false;
    h.app.ctrl_down = true;
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(20.0, 20.0)), true, true));
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Wheel { .. })
    ));
    h.app.cancel_brush_hud();
}

#[test]
fn brush_size_hud_opens_when_alt_is_already_held() {
    let mut h = Harness::new("brush_hold");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.alt_down = true;
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(4.0, 4.0)), true, false));
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Size { .. })
    ));
    h.app.cancel_brush_hud();
}

#[test]
fn shift_right_drag_scrubs_opacity_and_escape_restores() {
    let mut h = Harness::new("brush_opacity");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.shift_down = true;
    h.app.brush_opacity = 1.0;
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(10.0, 10.0)), true, false));
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Opacity { .. })
    ));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(10.0, 60.0)), true, false));
    assert!((h.app.brush_opacity - 0.5).abs() < 0.02);
    h.app.cancel_brush_hud();
    assert!((h.app.brush_opacity - 1.0).abs() < 1e-4);
}

#[test]
fn a_color_dot_moves_the_pointer_and_leaves_the_wheel() {
    let mut h = Harness::new("brush_dot");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.tab_mut().doc.view.recent_colors = Some(vec![[255, 0, 0]]);
    h.app.ctrl_down = true;
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(400.0, 300.0)), true, true));
    let center = match h.app.brush_hud {
        Some(board_color::BrushHud::Wheel { center, .. }) => center,
        other => panic!("wheel opened, got {other:?}"),
    };
    let slot = board_color::wheel_slot_offset(0, 24, board_color::WHEEL_SLOT_RADIUS);
    let dot = center + egui::vec2(slot[0], slot[1]);
    let approach = dot + egui::vec2(12.0, 0.0);
    assert!(h.app.drive_brush_hud(Some(approach), true, false));
    match h.app.brush_hud {
        Some(board_color::BrushHud::Wheel { center: now, .. }) => {
            assert_eq!(now, center);
        }
        other => panic!("wheel stayed open, got {other:?}"),
    }
    assert_eq!(h.app.board_colors.fg.0[0], 255);
    assert_eq!(h.app.brush_cursor_warp, Some((approach, dot)));
}

#[test]
fn brush_stroke_keeps_softness_and_remembers_its_color() {
    let mut h = Harness::new("brush_soft");
    h.app.brush_softness = 0.5;
    h.app.brush_width = 8.0;
    h.app.finish_freehand_brush(vec![
        Pos2::new(0.0, 0.0),
        Pos2::new(30.0, 12.0),
        Pos2::new(60.0, 0.0),
    ]);
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("brush commits a path");
    };
    assert!((shape.stroke.softness - 0.5).abs() < 1e-4);
    assert!(shape.stroke.paints_as_stamp());
    assert!(shape.stroke.stamp);
    let rgb = [
        shape.stroke.color.0[0],
        shape.stroke.color.0[1],
        shape.stroke.color.0[2],
    ];
    assert_eq!(
        h.app
            .doc()
            .view
            .recent_colors
            .as_ref()
            .unwrap()
            .first()
            .copied(),
        Some(rgb)
    );
}

#[test]
fn a_brush_click_commits_one_round_dab() {
    let mut h = Harness::new("brush_dab");
    h.app.finish_freehand_brush(vec![Pos2::new(12.0, 8.0)]);
    let dab = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &dab.kind else {
        panic!("a click commits a dab");
    };
    assert!(shape.path.as_ref().is_some_and(|p| p.segs.is_empty()));
    assert!(shape.stroke.stamp);
}

/// The stroke of every node the vector tools commit, in commit order.
fn committed_vector_strokes(h: &mut Harness) -> Vec<(board::BoardTool, slate_doc::scene::Stroke)> {
    use board::BoardTool;
    let mut out = Vec::new();
    let mut last = |h: &mut Harness, tool: BoardTool| {
        let node = h.app.doc().scene.nodes.last().unwrap();
        let slate_doc::scene::NodeKind::Shape(s) = &node.kind else {
            panic!("{tool:?} commits a shape");
        };
        out.push((tool, s.stroke));
    };
    h.app.set_board_tool(BoardTool::Pen);
    h.app.finish_freehand_pen(vec![
        Pos2::new(0.0, 100.0),
        Pos2::new(40.0, 120.0),
        Pos2::new(80.0, 100.0),
    ]);
    last(h, BoardTool::Pen);
    h.app.set_board_tool(BoardTool::Line);
    h.app
        .commit_line(Pos2::new(0.0, 200.0), Pos2::new(90.0, 200.0));
    last(h, BoardTool::Line);
    h.app.set_board_tool(BoardTool::Arc);
    for p in [(0.0, 300.0), (100.0, 300.0), (50.0, 260.0)] {
        h.app.path_tool_click(Pos2::new(p.0, p.1));
    }
    last(h, BoardTool::Arc);
    h.app.set_board_tool(BoardTool::Polyline);
    for p in [(0.0, 400.0), (60.0, 430.0), (120.0, 400.0)] {
        h.app.path_tool_click(Pos2::new(p.0, p.1));
    }
    assert!(h.app.finish_path_draft());
    last(h, BoardTool::Polyline);
    h.app.set_board_tool(BoardTool::BezierSpan);
    for (press, release) in [
        ((0.0, 500.0), (30.0, 480.0)),
        ((120.0, 500.0), (150.0, 520.0)),
    ] {
        let press = Pos2::new(press.0, press.1);
        h.app.bezier_anchor_press(press);
        h.app
            .bezier_anchor_release(press, Pos2::new(release.0, release.1), false);
    }
    assert!(h.app.finish_path_draft());
    last(h, BoardTool::BezierSpan);
    out
}

/// Pen, line, arc, polyline, and Bézier strokes are hard vector strokes: a
/// soft, blurred brush stroke that became the last edited style must not
/// leak its softness, stamp, or blur into them.
#[test]
fn vector_tools_never_inherit_brush_softness_or_blur() {
    let mut h = line_board("vector_no_soft");
    h.app.brush_softness = 0.5;
    h.app.brush_width = 40.0;
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.finish_freehand_brush(vec![
        Pos2::new(0.0, 0.0),
        Pos2::new(30.0, 12.0),
        Pos2::new(60.0, 0.0),
    ]);
    let brush = h.app.doc().scene.nodes.last().unwrap().id;
    // A Shift chain or an inspector edit patches the brush stroke by itself.
    h.app.patch_nodes(&[brush], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.gaussian_blur = 3.0;
        }
    });
    for (tool, stroke) in committed_vector_strokes(&mut h) {
        assert_eq!(stroke.softness, 0.0, "{tool:?} inherited brush softness");
        assert_eq!(stroke.gaussian_blur, 0.0, "{tool:?} inherited blur");
        assert!(!stroke.stamp, "{tool:?} became a raster stamp");
        assert!(stroke.tween_from.is_none(), "{tool:?} inherited a tween");
        assert!(!stroke.paints_as_stamp(), "{tool:?} paints as a stamp");
        assert!(stroke.width > 0.0, "{tool:?} has no width");
    }
}

fn last_stroke(h: &Harness) -> (NodeId, slate_doc::scene::Stroke) {
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(s) = &node.kind else {
        panic!("expected a shape");
    };
    (node.id, s.stroke)
}

fn restyle(h: &mut Harness, id: NodeId, width: f32, rgb: [u8; 3]) {
    h.app.patch_nodes(&[id], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.width = width;
            s.stroke.color = Rgba([rgb[0], rgb[1], rgb[2], 255]);
        }
    });
}

fn draw_pen(h: &mut Harness, y: f32) {
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.finish_freehand_pen(vec![
        Pos2::new(0.0, y),
        Pos2::new(40.0, y + 20.0),
        Pos2::new(80.0, y),
    ]);
}

fn draw_arc(h: &mut Harness, y: f32) {
    h.app.set_board_tool(board::BoardTool::Arc);
    for p in [(0.0, y), (100.0, y), (50.0, y - 40.0)] {
        h.app.path_tool_click(Pos2::new(p.0, p.1));
    }
}

/// Stated intent: brush color, size, and blur never reach the Pen. The Pen
/// draws with its own last color and width, and a hard edge.
#[test]
fn pen_keeps_its_own_style_when_the_brush_changes() {
    let mut h = line_board("pen_own_style");
    draw_pen(&mut h, 0.0);
    let (pen, _) = last_stroke(&h);
    restyle(&mut h, pen, 5.0, [10, 200, 30]);

    h.app.board_colors.fg = Rgba([250, 20, 20, 255]);
    h.app.brush_width = 40.0;
    h.app.brush_softness = 0.6;
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.finish_freehand_brush(vec![
        Pos2::new(0.0, 100.0),
        Pos2::new(30.0, 112.0),
        Pos2::new(60.0, 100.0),
    ]);
    let (brush, _) = last_stroke(&h);
    h.app.patch_nodes(&[brush], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.gaussian_blur = 4.0;
        }
    });

    draw_pen(&mut h, 200.0);
    let (_, stroke) = last_stroke(&h);
    assert_eq!(stroke.width, 5.0, "pen width is its own");
    assert_eq!(
        stroke.color,
        Rgba([10, 200, 30, 255]),
        "pen color is its own"
    );
    assert_eq!(stroke.softness, 0.0);
    assert_eq!(stroke.gaussian_blur, 0.0);
    assert!(!stroke.stamp);
}

/// A Pen that has never drawn does not start from the brush color either.
#[test]
fn a_fresh_pen_does_not_take_the_brush_color_or_size() {
    let mut h = line_board("pen_fresh_style");
    h.app.board_colors.fg = Rgba([250, 20, 20, 255]);
    h.app.brush_width = 40.0;
    draw_pen(&mut h, 0.0);
    let (_, stroke) = last_stroke(&h);
    assert_ne!(stroke.color, Rgba([250, 20, 20, 255]));
    assert_ne!(stroke.width, 40.0);
    assert!(stroke.width > 0.0);
}

/// Stated intent: each curve tool remembers its own width.
#[test]
fn changing_the_line_width_leaves_the_arc_alone() {
    let mut h = line_board("line_arc_style");
    draw_arc(&mut h, 300.0);
    let (_, arc_before) = last_stroke(&h);
    let line = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(50.0, 0.0))
        .unwrap();
    restyle(&mut h, line, 9.0, [1, 2, 3]);
    draw_arc(&mut h, 400.0);
    let (_, arc) = last_stroke(&h);
    assert_eq!(arc.width, arc_before.width, "arc kept its own width");
    assert_eq!(arc.color, arc_before.color, "arc kept its own color");
    h.app.set_board_tool(board::BoardTool::Line);
    h.app
        .commit_line(Pos2::new(0.0, 50.0), Pos2::new(50.0, 50.0))
        .unwrap();
    let (_, line_again) = last_stroke(&h);
    assert_eq!(line_again.width, 9.0, "line remembers its own width");
    assert_eq!(line_again.color, Rgba([1, 2, 3, 255]));
}

/// Stated intent: per-tool memory is saved with the workbook.
#[test]
fn per_tool_style_memory_survives_save_and_reopen() {
    let mut h = line_board("tool_style_save");
    let line = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(50.0, 0.0))
        .unwrap();
    restyle(&mut h, line, 6.0, [40, 50, 60]);
    draw_pen(&mut h, 100.0);
    let (pen, _) = last_stroke(&h);
    restyle(&mut h, pen, 3.0, [70, 80, 90]);
    let path = h.base.join("styles.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());
    drop(h);

    let mut h2 = Harness::new("tool_style_reopen");
    h2.app.open_doc_at(path);
    h2.frame();
    assert!(!h2.app.tab().read_only, "the reopened workbook is editable");
    h2.app.doc_mut().view.active_view = ViewKind::Board;
    h2.app.set_board_tool(board::BoardTool::Line);
    h2.app
        .commit_line(Pos2::new(0.0, 200.0), Pos2::new(50.0, 200.0))
        .unwrap();
    let (_, line_stroke) = last_stroke(&h2);
    assert_eq!(line_stroke.width, 6.0);
    assert_eq!(line_stroke.color, Rgba([40, 50, 60, 255]));
    draw_pen(&mut h2, 300.0);
    let (_, pen_stroke) = last_stroke(&h2);
    assert_eq!(pen_stroke.width, 3.0);
    assert_eq!(pen_stroke.color, Rgba([70, 80, 90, 255]));
}

/// Closed shapes keep one shared memory, and curve edits stay out of it.
#[test]
fn closed_shapes_still_share_style_memory() {
    let mut h = line_board("closed_style_shared");
    h.app
        .place_default_at(board::BoardTool::RectShape, Pos2::new(0.0, 0.0));
    let (rect, _) = last_stroke(&h);
    h.app.patch_nodes(&[rect], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.width = 4.0;
            s.stroke.color = Rgba([200, 100, 0, 255]);
            s.fill = Some(Rgba([0, 90, 180, 255]));
        }
    });
    let line = h
        .app
        .commit_line(Pos2::new(0.0, 300.0), Pos2::new(50.0, 300.0))
        .unwrap();
    restyle(&mut h, line, 11.0, [9, 9, 9]);
    h.app
        .place_default_at(board::BoardTool::Ellipse, Pos2::new(300.0, 0.0));
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(s) = &node.kind else {
        panic!("ellipse");
    };
    assert_eq!(s.stroke.width, 4.0);
    assert_eq!(s.stroke.color, Rgba([200, 100, 0, 255]));
    assert_eq!(s.fill, Some(Rgba([0, 90, 180, 255])));
}

const STROKE_TOOLS: [(board::BoardTool, slate_doc::StrokeTool); 5] = [
    (board::BoardTool::Pen, slate_doc::StrokeTool::Pen),
    (board::BoardTool::Line, slate_doc::StrokeTool::Line),
    (board::BoardTool::Arc, slate_doc::StrokeTool::Arc),
    (board::BoardTool::Polyline, slate_doc::StrokeTool::Polyline),
    (board::BoardTool::BezierSpan, slate_doc::StrokeTool::Bezier),
];

/// Open the brush's size HUD with Alt+right-drag and scrub right by `dx`.
fn width_chord(h: &mut Harness, dx: f32) {
    h.app.alt_down = true;
    assert!(
        h.app
            .drive_brush_hud(Some(Pos2::new(500.0, 500.0)), true, true),
        "{:?} opens the size HUD",
        h.app.board_tool
    );
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Size { .. })
    ));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(500.0 + dx, 450.0)), true, false));
}

fn release_chord(h: &mut Harness) {
    assert!(h.app.drive_brush_hud(None, false, false));
    assert!(h.app.brush_hud.is_none());
    h.app.alt_down = false;
}

/// Stated: Alt+right-drag sizes the pen, line, arc, polyline, and Bézier
/// through the brush's size chord. Each changes only its own width, the
/// vertical drag offers no softness, and the HUD reads the width.
#[test]
fn alt_right_drag_sizes_every_stroke_tool() {
    let mut h = line_board("stroke_size_chord");
    h.app.tab_mut().cam.z = 1.0;
    h.app.brush_width = 10.0;
    for (tool, slot) in STROKE_TOOLS {
        h.app.set_board_tool(tool);
        let before = h.app.stroke_for_tool(slot);
        assert!(h.app.board_tool_takes_width_chord(), "{tool:?}");
        width_chord(&mut h, 40.0);
        let during = h.app.stroke_for_tool(slot);
        assert!(during.width > before.width + 1.0, "{tool:?} got wider");
        assert_eq!(during.softness, 0.0, "{tool:?} stays hard");
        let label = h.app.size_hud_label().unwrap_or_default();
        assert!(label.ends_with(" px"), "{tool:?} HUD reads {label:?}");
        release_chord(&mut h);
        assert_eq!(h.app.stroke_for_tool(slot).width, during.width);
        let saved = h.app.doc().view.create_style.clone().unwrap_or_default();
        assert_eq!(
            saved.tool(slot).stroke.map(|s| s.width),
            Some(during.width),
            "{tool:?} width is saved with the workbook"
        );
    }
    assert_eq!(h.app.brush_width, 10.0, "the brush size is untouched");

    h.app.set_board_tool(board::BoardTool::Line);
    let kept = h.app.stroke_for_tool(slate_doc::StrokeTool::Line).width;
    width_chord(&mut h, 60.0);
    h.app.cancel_brush_hud();
    h.app.alt_down = false;
    assert_eq!(
        h.app.stroke_for_tool(slate_doc::StrokeTool::Line).width,
        kept,
        "Escape restores the width"
    );
}

/// Stated: the chord during a line draw changes the line being drawn.
#[test]
fn the_width_chord_mid_draw_sets_the_line_being_drawn() {
    let mut h = line_board("line_chord_mid");
    h.app.tab_mut().cam.z = 1.0;
    h.app.set_board_tool(board::BoardTool::Line);
    assert!(h.app.line_begin(Pos2::new(0.0, 0.0), false));
    h.app.line_hover(Pos2::new(80.0, 0.0), false);
    width_chord(&mut h, 40.0);
    release_chord(&mut h);
    assert!(h.app.line_draft.is_some(), "the chord keeps the draft");
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Line).width;
    h.app.line_release(Pos2::new(100.0, 0.0), false, false);
    let (_, stroke) = last_stroke(&h);
    assert_eq!(stroke.width, wide);
}

/// Stated: the chord mid-draw sets the arc, polyline, or Bézier being drawn.
#[test]
fn the_width_chord_mid_draw_sets_the_path_being_drawn() {
    let mut h = line_board("path_chord_mid");
    h.app.tab_mut().cam.z = 1.0;
    h.app.set_board_tool(board::BoardTool::Polyline);
    h.app.path_tool_click(Pos2::new(0.0, 0.0));
    h.app.path_tool_click(Pos2::new(60.0, 30.0));
    width_chord(&mut h, 40.0);
    release_chord(&mut h);
    assert!(
        h.app.board_path_draft.is_some(),
        "the chord keeps the draft"
    );
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Polyline).width;
    h.app.path_tool_click(Pos2::new(120.0, 0.0));
    assert!(h.app.finish_path_draft());
    assert_eq!(last_stroke(&h).1.width, wide);
}

fn pen_point_count(h: &Harness) -> usize {
    match &h.app.board_drag {
        Some(board::BoardDrag::FreehandPen { points, .. }) => points.len(),
        _ => panic!("the pen stroke is live"),
    }
}

/// Stated: the chord mid-stroke changes the pen's width from that point on,
/// stored as variable width (per-vertex tips). The scrub itself draws nothing.
#[test]
fn the_width_chord_mid_stroke_widens_the_rest_of_the_pen_stroke() {
    let mut h = line_board("pen_chord_mid");
    h.app.tab_mut().cam.z = 1.0;
    h.app.set_board_tool(board::BoardTool::Pen);
    let mods = egui::Modifiers::NONE;
    let narrow = h.app.stroke_for_tool(slate_doc::StrokeTool::Pen).width;
    h.app.board_drag = h.app.begin_gesture_for_test(Pos2::ZERO, Pos2::ZERO, mods);
    for i in 1..=10 {
        h.app
            .update_gesture_for_test(Pos2::new(i as f32 * 10.0, 0.0), mods);
    }
    width_chord(&mut h, 40.0);
    let count = pen_point_count(&h);
    h.app.update_gesture_for_test(Pos2::new(100.0, 60.0), mods);
    assert_eq!(pen_point_count(&h), count, "the scrub does not draw");
    release_chord(&mut h);
    let wide = h.app.stroke_for_tool(slate_doc::StrokeTool::Pen).width;
    assert!(wide > narrow + 1.0);
    for i in 11..=20 {
        h.app
            .update_gesture_for_test(Pos2::new(i as f32 * 10.0, 0.0), mods);
    }
    h.app
        .end_gesture_for_test(Pos2::new(200.0, 0.0), None, mods);

    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("the pen commits a path");
    };
    let path = shape.path.as_ref().unwrap();
    assert_eq!(path.tips.len(), path.segs.len() + 1, "one tip per vertex");
    assert!((path.tips.first().unwrap().width - narrow).abs() < 1e-3);
    assert!((path.tips.last().unwrap().width - wide).abs() < 1e-3);
    assert!(path.tips.iter().all(|t| t.softness == 0.0));
    assert!((shape.stroke.width - wide).abs() < 1e-3);
    assert!(
        !shape.stroke.paints_as_stamp(),
        "still a hard vector stroke"
    );
    let widths = path.vector_widths(&shape.stroke).expect("varying width");
    assert!((widths[0] - narrow).abs() < 1e-3);

    draw_pen(&mut h, 300.0);
    let (_, plain) = last_stroke(&h);
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        unreachable!()
    };
    assert!(shape.path.as_ref().unwrap().tips.is_empty());
    assert_eq!(plain.width, wide, "the next stroke starts at the new width");
}

/// Stated: every armed drawing tool shows a crosshair or a circle cursor.
/// The match is exhaustive, so a new tool cannot ship without a choice.
#[test]
fn every_armed_tool_names_its_cursor() {
    use board::BoardTool as T;
    use board_place::ArmedCursor as C;
    for tool in T::ALL {
        let want = match tool {
            T::Brush | T::Eraser | T::Smooth | T::Pen => C::TipCircle,
            T::Line
            | T::Arc
            | T::Polyline
            | T::BezierSpan
            | T::RectShape
            | T::Ellipse
            | T::Polygon
            | T::Trim
            | T::Split
            | T::Deck => C::Crosshair,
            T::Frame
            | T::Text
            | T::Sticky
            | T::AgentPortal
            | T::WebPortal
            | T::AtlasPortal
            | T::SlatePortal => C::Ghost,
            T::Select | T::Pan | T::DirectSelect | T::Eyedropper => C::Own,
        };
        assert_eq!(board_place::armed_cursor(tool), want, "{tool:?}");
    }
}

/// Stated: line, arc, polyline, Bézier, and the shape tools hover with a
/// crosshair; brush, eraser, smooth, and pen hide the arrow under their tip.
#[test]
fn armed_drawing_tools_hover_with_a_crosshair_or_tip_circle() {
    use board::BoardTool as T;
    let mut h = line_board("armed_cursors");
    h.frame();
    let c = h.app.canvas_rect.center();
    let hover = |h: &mut Harness, tool: T| {
        h.app.set_board_tool(tool);
        h.frame_with(pointer_to(c, false));
        h.frame_output(pointer_to(c + EVec2::new(4.0, 0.0), false))
            .platform_output
            .cursor_icon
    };
    for tool in [
        T::Line,
        T::Arc,
        T::Polyline,
        T::BezierSpan,
        T::RectShape,
        T::Ellipse,
        T::Polygon,
    ] {
        assert_eq!(hover(&mut h, tool), egui::CursorIcon::Crosshair, "{tool:?}");
    }
    for tool in [T::Brush, T::Eraser, T::Smooth, T::Pen] {
        assert_eq!(hover(&mut h, tool), egui::CursorIcon::None, "{tool:?}");
    }
}

/// Chosen: the pen's cursor is a hard circle of the pen's own width and
/// color, the same disc the brush shows for its tip.
#[test]
fn the_pen_cursor_is_a_hard_circle_of_its_width() {
    let mut h = line_board("pen_cursor");
    h.app.tab_mut().cam.z = 2.0;
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.set_tool_width(slate_doc::StrokeTool::Pen, 8.0);
    let pen = h.app.stroke_for_tool(slate_doc::StrokeTool::Pen);
    let (r, softness, ink) = h.app.width_cursor_disc().expect("the pen shows a disc");
    assert_eq!(r, 8.0, "radius is half the width, zoomed");
    assert_eq!(softness, 0.0, "the pen is hard");
    assert_eq!(ink, board::rgba32(pen.color));
    h.app.set_board_tool(board::BoardTool::Line);
    assert!(
        h.app.width_cursor_disc().is_none(),
        "the line uses a crosshair"
    );
}

#[test]
fn ctrl_z_reverts_a_brush_size_change_until_another_action() {
    let mut h = Harness::new("brush_undo_size");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 10.0;
    let before = h.app.brush_setting_snapshot();
    h.app.brush_width = 40.0;
    h.app.push_brush_setting_undo(before);
    h.app.board_undo();
    assert!((h.app.brush_width - 10.0).abs() < 1e-3);
    h.app.brush_width = 10.0;
    let before = h.app.brush_setting_snapshot();
    h.app.brush_width = 40.0;
    h.app.push_brush_setting_undo(before);
    h.app
        .finish_freehand_brush(vec![Pos2::new(0.0, 0.0), Pos2::new(30.0, 0.0)]);
    h.app.board_undo();
    assert!((h.app.brush_width - 40.0).abs() < 1e-3);
}

#[test]
fn a_shift_line_tweens_the_tip_from_the_start_click() {
    let mut h = Harness::new("brush_tween");
    h.app.brush_width = 4.0;
    let start = h.app.tip_now();
    h.app.brush_width = 20.0;
    h.app
        .commit_tween_line(Pos2::new(0.0, 0.0), Pos2::new(80.0, 0.0), start, None);
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("line");
    };
    assert!((shape.stroke.width - 20.0).abs() < 1e-3);
    let tips = &shape.path.as_ref().unwrap().tips;
    assert_eq!(tips.len(), 2);
    assert!((tips[0].width - 4.0).abs() < 1e-3);
    assert!((tips[1].width - 20.0).abs() < 1e-3);
}

#[test]
fn a_shift_chain_extends_one_stroke_so_joints_do_not_stack() {
    let mut h = Harness::new("brush_chain");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 12.0;
    h.app.brush_opacity = 0.5;
    h.app
        .finish_freehand_brush(vec![Pos2::new(0.0, 0.0), Pos2::new(60.0, 0.0)]);
    let before = h.app.doc().scene.nodes.len();
    let anchor = h.app.brush_line_anchor.expect("anchor after a stroke");
    let first = anchor.node.expect("anchor names the stroke");
    h.app
        .commit_tween_line(anchor.pos, Pos2::new(60.0, 50.0), anchor.tip, anchor.node);
    let anchor = h.app.brush_line_anchor.unwrap();
    h.app
        .commit_tween_line(anchor.pos, Pos2::new(10.0, 10.0), anchor.tip, anchor.node);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        before,
        "segments extend the stroke instead of stacking new nodes"
    );
    let node = h.app.doc().scene.node(first).unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    let path = shape.path.clone().unwrap();
    assert_eq!(path.tips.len(), path.segs.len() + 1);
    // One stamp for the whole chain: its opacity never exceeds the brush's.
    let contours = board_path::stamped_contours(node, shape, &path, 0.25);
    let img = vector_ink::stamp_tipped(&contours, 1.0).unwrap();
    let top = img.rgba.iter().skip(3).step_by(4).copied().max().unwrap();
    assert_eq!(top, shape.stroke.color.0[3]);
    // Undo removes only the last segment.
    h.app.board_undo();
    let node = h.app.doc().scene.node(first).unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    assert_eq!(shape.path.as_ref().unwrap().tips.len(), path.tips.len() - 1);
}

#[test]
fn space_repeats_the_latest_tool_not_a_brush_stroke() {
    let mut h = Harness::new("repeat_tool");
    h.app.set_board_tool(board::BoardTool::Line);
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.finish_freehand_brush(vec![
        Pos2::new(0.0, 0.0),
        Pos2::new(20.0, 8.0),
        Pos2::new(40.0, 0.0),
    ]);
    h.app.set_board_tool(board::BoardTool::RectShape);
    let last = h
        .app
        .cmd_history
        .last_repeatable(&h.app.registry)
        .expect("a tool was armed");
    assert_eq!(last.0, "board.tool.rect");
}

/// Renders the committed brush pipeline to `target/brush-validate/*.png` so
/// joints, self-overlaps, tweens, and dabs can be inspected by eye.
#[test]
#[ignore]
fn brush_validation_images() {
    fn composite(img: &mut image::RgbaImage, stamp: &vector_ink::StampImage, offset: [f32; 2]) {
        for y in 0..stamp.height {
            for x in 0..stamp.width {
                let i = ((y * stamp.width + x) * 4) as usize;
                let a = stamp.rgba[i + 3] as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let wx = stamp.origin[0] + (x as f32 + 0.5) * stamp.pixel - offset[0];
                let wy = stamp.origin[1] + (y as f32 + 0.5) * stamp.pixel - offset[1];
                if wx < 0.0 || wy < 0.0 || wx >= img.width() as f32 || wy >= img.height() as f32 {
                    continue;
                }
                let p = img.get_pixel_mut(wx as u32, wy as u32);
                for c in 0..3 {
                    p.0[c] =
                        (stamp.rgba[i + c] as f32 * a + p.0[c] as f32 * (1.0 - a)).round() as u8;
                }
            }
        }
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate");
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::new("brush_validate");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg.0 = [150, 255, 170, 255];

    // 1. Soft freehand wave, 60% opacity.
    h.app.brush_width = 40.0;
    h.app.brush_softness = 0.6;
    h.app.brush_opacity = 0.6;
    let wave: Vec<Pos2> = (0..=60)
        .map(|i| {
            let t = i as f32 / 60.0;
            Pos2::new(40.0 + t * 520.0, 120.0 + (t * 9.0).sin() * 60.0)
        })
        .collect();
    h.app.finish_freehand_brush(wave);

    // 2. Semi-transparent Shift zigzag with acute joints.
    h.app.brush_width = 36.0;
    h.app.brush_softness = 0.4;
    h.app.brush_opacity = 0.5;
    h.app.finish_freehand_brush(vec![Pos2::new(60.0, 300.0)]);
    for p in [
        Pos2::new(300.0, 300.0),
        Pos2::new(90.0, 340.0),
        Pos2::new(330.0, 380.0),
        Pos2::new(120.0, 430.0),
    ] {
        let a = h.app.brush_line_anchor.unwrap();
        h.app.commit_tween_line(a.pos, p, a.tip, a.node);
    }

    // 3. Tween chain: size and color change between Shift clicks.
    h.app.brush_softness = 0.3;
    h.app.brush_opacity = 1.0;
    h.app.brush_width = 8.0;
    h.app.finish_freehand_brush(vec![Pos2::new(380.0, 460.0)]);
    for (i, p) in [
        Pos2::new(470.0, 300.0),
        Pos2::new(560.0, 460.0),
        Pos2::new(560.0, 250.0),
    ]
    .into_iter()
    .enumerate()
    {
        h.app.brush_width = 8.0 + 22.0 * (i + 1) as f32;
        h.app.board_colors.fg.0 = [150, 255 - 60 * i as u8, 170 + 25 * i as u8, 255];
        let a = h.app.brush_line_anchor.unwrap();
        h.app.commit_tween_line(a.pos, p, a.tip, a.node);
    }

    // 4. Dabs, hard and soft.
    h.app.board_colors.fg.0 = [255, 120, 200, 255];
    h.app.brush_opacity = 0.7;
    h.app.brush_width = 50.0;
    h.app.brush_softness = 0.0;
    h.app.finish_freehand_brush(vec![Pos2::new(90.0, 520.0)]);
    h.app.brush_softness = 1.0;
    h.app.finish_freehand_brush(vec![Pos2::new(170.0, 520.0)]);

    // 5. Self-crossing loop at 50%.
    h.app.board_colors.fg.0 = [120, 200, 255, 255];
    h.app.brush_opacity = 0.5;
    h.app.brush_softness = 0.5;
    h.app.brush_width = 30.0;
    let loop_pts: Vec<Pos2> = (0..=80)
        .map(|i| {
            let t = i as f32 / 80.0 * std::f32::consts::TAU * 1.25;
            Pos2::new(300.0 + t.cos() * 60.0 + t * 12.0, 520.0 + t.sin() * 45.0)
        })
        .collect();
    h.app.finish_freehand_brush(loop_pts);

    let mut img = image::RgbaImage::from_pixel(620, 600, image::Rgba([16, 17, 20, 255]));
    let mut tops = Vec::new();
    for node in &h.app.doc().scene.nodes {
        let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
            continue;
        };
        let Some(path) = shape.path.as_ref() else {
            continue;
        };
        let contours = board_path::stamped_contours(node, shape, path, 0.25);
        let stamp = vector_ink::stamp_tipped(&contours, 1.0).unwrap();
        let top = stamp.rgba.iter().skip(3).step_by(4).copied().max().unwrap();
        let widest = path
            .paint_tips(&shape.stroke)
            .iter()
            .map(|t| t.color.0[3])
            .chain([shape.stroke.color.0[3]])
            .max()
            .unwrap();
        tops.push((node.id, top, widest));
        composite(&mut img, &stamp, [0.0, 0.0]);
    }
    img.save(dir.join("brush.png")).unwrap();
    for (id, top, cap) in tops {
        assert!(top <= cap, "node {id:?} peaks at {top}, opacity is {cap}");
    }
}

#[test]
fn image_paint_brush_commits_into_active_layer_not_scene() {
    let mut h = Harness::new("image_paint_brush");
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.sync_image_paint_for_tool();
    h.app
        .finish_freehand_brush(vec![Pos2::new(50.0, 50.0), Pos2::new(150.0, 50.0)]);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "stroke stays on the layer"
    );
    let host = h.app.doc().scene.node(image_id).unwrap();
    let slate_doc::scene::NodeKind::Image(img) = &host.kind else {
        panic!("image");
    };
    assert_eq!(img.paint_layers.len(), 1);
    assert_eq!(img.paint_layers[0].nodes.len(), 1);
}

#[test]
fn image_paint_eraser_spot_hits_layer_strokes() {
    let mut h = Harness::new("image_paint_eraser");
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.sync_image_paint_for_tool();
    h.app
        .finish_freehand_brush(vec![Pos2::new(50.0, 50.0), Pos2::new(150.0, 50.0)]);
    let stroke_id = h.app.doc().scene.node(image_id).unwrap();
    let slate_doc::scene::NodeKind::Image(img) = &stroke_id.kind else {
        panic!("image");
    };
    let stroke_id = img.paint_layers[0].nodes[0].id;
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.sync_image_paint_for_tool();
    h.app.eraser_width = 30.0;
    h.app.board_drag = Some(h.app.begin_erase(Pos2::new(100.0, 50.0), false));
    h.app.update_erase(Pos2::new(100.0, 50.0));
    let Some(board::BoardDrag::Erase { spot, .. }) = h.app.board_drag.take() else {
        panic!("erase drag");
    };
    assert!(spot.contains(&stroke_id));
}

#[test]
fn image_paint_eraser_removes_vector_stroke_and_undo_restores_it() {
    let mut h = Harness::new("image_paint_vector_eraser");
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.sync_image_paint_for_tool();
    h.app
        .finish_freehand_pen(vec![Pos2::new(40.0, 50.0), Pos2::new(160.0, 50.0)]);
    let stroke_id = {
        let host = h.app.doc().scene.node(image_id).unwrap();
        let slate_doc::scene::NodeKind::Image(img) = &host.kind else {
            panic!("image");
        };
        img.paint_layers[0].nodes[0].id
    };
    h.app
        .finish_erase(vec![stroke_id], vec![Pos2::new(100.0, 50.0)], Vec::new());
    assert!(slate_doc::image_paint::find_layer_node(&h.app.doc().scene, stroke_id).is_none());
    h.app.board_undo();
    assert!(slate_doc::image_paint::find_layer_node(&h.app.doc().scene, stroke_id).is_some());
}

#[test]
fn image_paint_session_clears_before_drawing_off_another_selection() {
    let mut h = Harness::new("image_paint_session_clear");
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = image.id;
    h.app.add_nodes(vec![image]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    assert!(h.app.image_paint_session().is_some());

    let other = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(300.0, 0.0, 20.0, 20.0),
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            sides: 6,
            fill: Some(slate_doc::scene::Rgba::opaque(0, 0, 0)),
            stroke: slate_doc::scene::Stroke::none(),
            corner: Default::default(),
            flip: false,
            path: None,
            text: None,
        }),
    );
    let other_id = other.id;
    h.app.add_nodes(vec![other]);
    h.app.board_sel = std::iter::once(other_id).collect();
    h.app.set_board_tool(board::BoardTool::RectShape);
    h.app.finish_draw(
        Pos2::new(400.0, 200.0),
        Pos2::new(500.0, 300.0),
        board::BoardTool::RectShape,
        egui::Modifiers::NONE,
    );
    assert!(h.app.image_paint_session().is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 3);
    let host = h.app.doc().scene.node(image_id).unwrap();
    let slate_doc::scene::NodeKind::Image(img) = &host.kind else {
        panic!("image");
    };
    assert!(img.paint_layers.is_empty());
}

#[test]
fn image_drop_replace_is_one_undo_group() {
    let mut h = Harness::new("image_drop_replace_undo");
    h.app.ensure_work_tab();
    let a = h.base.join("a.png");
    let b = h.base.join("b.png");
    std::fs::write(&a, b"a").unwrap();
    std::fs::write(&b, b"b").unwrap();
    let items = h.app.add_paths(&[a, b]);
    let target = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(items[0])),
    );
    let source = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(150.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(items[1])),
    );
    let (target_id, source_id) = (target.id, source.id);
    h.app.add_nodes(vec![target, source]);
    assert!(h
        .app
        .replace_image_item_preserve_layers(target_id, items[1], Some(source_id)));
    assert!(h.app.doc().scene.node(source_id).is_none());
    h.app.board_undo();
    assert!(h.app.doc().scene.node(source_id).is_some());
    let slate_doc::scene::NodeKind::Image(img) = &h.app.doc().scene.node(target_id).unwrap().kind
    else {
        panic!("image");
    };
    assert_eq!(img.item, items[0]);
}

#[test]
fn image_drop_layer_on_rotated_host_is_one_undo_group() {
    let mut h = Harness::new("image_drop_layer_rotated_undo");
    h.app.ensure_work_tab();
    let a = h.base.join("a.png");
    let b = h.base.join("b.png");
    std::fs::write(&a, b"a").unwrap();
    std::fs::write(&b, b"b").unwrap();
    let items = h.app.add_paths(&[a, b]);
    let mut target = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(items[0])),
    );
    target.rotation_deg = 37.0;
    let source = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(250.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(items[1])),
    );
    let (target_id, source_id) = (target.id, source.id);
    h.app.add_nodes(vec![target, source]);
    assert!(h
        .app
        .add_dropped_image_as_layer(target_id, items[1], Some(source_id)));
    assert!(h.app.doc().scene.node(source_id).is_none());
    let slate_doc::scene::NodeKind::Image(img) = &h.app.doc().scene.node(target_id).unwrap().kind
    else {
        panic!("image");
    };
    assert_eq!(img.paint_layers.len(), 1);
    assert!(img.paint_layers[0].nodes[0].rotation_deg.abs() < 1e-4);
    h.app.board_undo();
    assert!(h.app.doc().scene.node(source_id).is_some());
    let slate_doc::scene::NodeKind::Image(img) = &h.app.doc().scene.node(target_id).unwrap().kind
    else {
        panic!("image");
    };
    assert!(img.paint_layers.is_empty());
}

#[test]
fn the_eraser_spot_erases_painted_ink_and_keeps_the_stroke() {
    let mut h = Harness::new("eraser_spot");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 20.0;
    h.app
        .finish_freehand_brush(vec![Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0)]);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 30.0;
    h.app.board_drag = Some(h.app.begin_erase(Pos2::new(100.0, -20.0), false));
    h.app.update_erase(Pos2::new(100.0, 20.0));
    let Some(board::BoardDrag::Erase {
        touched,
        points,
        spot,
        ..
    }) = h.app.board_drag.take()
    else {
        panic!("erase drag");
    };
    assert!(touched.is_empty(), "painted strokes are not removed whole");
    assert_eq!(spot, vec![id]);
    h.app.finish_erase(touched, points, spot);
    let node = h.app.doc().scene.node(id).expect("stroke survives");
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    let path = shape.path.as_ref().unwrap();
    assert_eq!(path.erase.len(), 1);
    // The gap no longer picks; the ink either side still does.
    let z = h.app.tab().cam.z;
    assert!(!board_path::hit_path_node(node, shape, 100.0, 0.0, z));
    assert!(board_path::hit_path_node(node, shape, 20.0, 0.0, z));
    // One undo restores the ink.
    h.app.board_undo();
    let node = h.app.doc().scene.node(id).unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("path");
    };
    assert!(shape.path.as_ref().unwrap().erase.is_empty());
}

#[test]
fn erasing_all_of_a_painted_stroke_removes_it() {
    let mut h = Harness::new("eraser_all");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 10.0;
    h.app.finish_freehand_brush(vec![Pos2::new(40.0, 40.0)]);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 60.0;
    h.app.board_drag = Some(h.app.begin_erase(Pos2::new(40.0, 40.0), false));
    let Some(board::BoardDrag::Erase {
        touched,
        points,
        spot,
        ..
    }) = h.app.board_drag.take()
    else {
        panic!("erase drag");
    };
    h.app.finish_erase(touched, points, spot);
    assert!(h.app.doc().scene.node(id).is_none());
}

#[test]
fn the_wheel_gap_keeps_the_color_and_only_outside_samples() {
    use board_color::{sample_wheel, WheelHit, WHEEL_BACKDROP_RADIUS, WHEEL_SV_RADIUS};
    let hsv = [0.3, 0.5, 0.5];
    // Just past the saturation/value disc: the hue ring, never the eyedropper.
    assert!(matches!(
        sample_wheel([WHEEL_SV_RADIUS + 1.0, 0.0], hsv, &[]),
        WheelHit::Field(..)
    ));
    // Between the ring and the swatches: keep the value.
    assert!(matches!(
        sample_wheel([0.0, -(WHEEL_BACKDROP_RADIUS - 4.0)], hsv, &[]),
        WheelHit::Keep
    ));
    assert!(matches!(
        sample_wheel([0.0, -(WHEEL_BACKDROP_RADIUS + 4.0)], hsv, &[]),
        WheelHit::Outside
    ));
}

#[test]
fn eraser_settings_ride_the_same_hud_and_undo() {
    let mut h = Harness::new("eraser_hud");
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 10.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    h.app.tab_mut().cam.z = 1.0;
    h.app.alt_down = true;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(40.0, -50.0)), true, false));
    assert!((h.app.eraser_width - 50.0).abs() < 0.01);
    assert!((h.app.eraser_softness - 0.5).abs() < 0.01);
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(40.0, -50.0)), false, false));
    h.app.alt_down = false;
    h.app.shift_down = true;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 50.0)), true, false));
    assert!((h.app.eraser_opacity - 0.5).abs() < 0.02);
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 50.0)), false, false));
    assert!((h.app.eraser_tip().rgba[3] as f32 - 127.5).abs() < 3.0);
    h.app.board_undo();
    assert!((h.app.eraser_opacity - 1.0).abs() < 1e-3);
    h.app.board_undo();
    assert!((h.app.eraser_width - 10.0).abs() < 1e-3);
}

#[test]
#[ignore]
fn eraser_validation_image() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate");
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::new("eraser_validate");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg.0 = [150, 255, 170, 255];
    h.app.brush_width = 44.0;
    h.app.brush_softness = 0.3;
    for y in [100.0, 220.0, 340.0] {
        let pts: Vec<Pos2> = (0..=40)
            .map(|i| Pos2::new(40.0 + i as f32 * 13.0, y + (i as f32 * 0.4).sin() * 20.0))
            .collect();
        h.app.finish_freehand_brush(pts);
    }
    h.app.set_board_tool(board::BoardTool::Eraser);
    let mut pass = |app: &mut SlateApp, pts: &[Pos2], shift: bool| {
        app.board_drag = Some(app.begin_erase(pts[0], shift));
        for p in &pts[1..] {
            app.update_erase(*p);
        }
        let Some(board::BoardDrag::Erase {
            touched,
            points,
            spot,
            ..
        }) = app.board_drag.take()
        else {
            panic!("erase drag");
        };
        app.finish_erase(touched, points, spot);
    };
    // Hard full-strength dabs.
    h.app.eraser_width = 36.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    pass(&mut h.app, &[Pos2::new(120.0, 100.0)], false);
    pass(&mut h.app, &[Pos2::new(200.0, 100.0)], false);
    // Soft, half-strength freehand that crosses itself over all three.
    h.app.eraser_width = 50.0;
    h.app.eraser_softness = 0.8;
    h.app.eraser_opacity = 0.5;
    let zig: Vec<Pos2> = (0..=60)
        .map(|i| {
            let t = i as f32 / 60.0;
            Pos2::new(330.0 + (t * 18.0).sin() * 50.0, 60.0 + t * 320.0)
        })
        .collect();
    pass(&mut h.app, &zig, false);
    // Hard straight Shift pass, from the last pass's end.
    h.app.eraser_width = 16.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    h.app.eraser_anchor = Some(Pos2::new(470.0, 60.0));
    pass(
        &mut h.app,
        &[Pos2::new(470.0, 60.0), Pos2::new(520.0, 380.0)],
        true,
    );

    let mut img = image::RgbaImage::from_pixel(620, 440, image::Rgba([16, 17, 20, 255]));
    for node in &h.app.doc().scene.nodes {
        let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
            continue;
        };
        let Some(path) = shape.path.as_ref() else {
            continue;
        };
        let contours = board_path::stamped_contours(node, shape, path, 0.25);
        let mut stamp = vector_ink::stamp_tipped(&contours, 1.0).unwrap();
        vector_ink::apply_erase(
            &mut stamp,
            &board_path::stamped_erase_marks(node, shape, path),
        );
        for y in 0..stamp.height {
            for x in 0..stamp.width {
                let i = ((y * stamp.width + x) * 4) as usize;
                let a = stamp.rgba[i + 3] as f32 / 255.0;
                let wx = stamp.origin[0] + x as f32 + 0.5;
                let wy = stamp.origin[1] + y as f32 + 0.5;
                if a <= 0.0 || wx < 0.0 || wy < 0.0 || wx >= 620.0 || wy >= 440.0 {
                    continue;
                }
                let p = img.get_pixel_mut(wx as u32, wy as u32);
                for c in 0..3 {
                    p.0[c] =
                        (stamp.rgba[i + c] as f32 * a + p.0[c] as f32 * (1.0 - a)).round() as u8;
                }
            }
        }
    }
    img.save(dir.join("eraser.png")).unwrap();
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        3,
        "strokes survive spot erasing"
    );
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

#[test]
fn fillet_drag_outward_grows_authored_radius() {
    let mut h = Harness::new("fillet_outward");
    h.app.ensure_work_tab();
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 80.0);
    let before = slate_doc::scene::Node {
        id: NodeId(1),
        rect,
        rotation_deg: 0.0,
        hidden: false,
        locked: false,
        opacity: 1.0,
        group: None,
        clip: None,
        bumper: None,
        kind: slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            sides: 6,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::Rounded { radius: 10.0 },
            flip: false,
            path: None,
            text: None,
        }),
    };
    h.app.doc_mut().scene.nodes.push(before.clone());
    let mut mid = before.clone();
    h.app
        .apply_fillet_radius_from_drag(&mut mid, &before, 20.0, false);
    let (_, r1) = match &mid.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.corner.effective(rect.w, rect.h),
        _ => panic!(),
    };
    let mut out = before.clone();
    h.app
        .apply_fillet_radius_from_drag(&mut out, &before, 35.0, false);
    let (_, r2) = match &out.kind {
        slate_doc::scene::NodeKind::Shape(s) => s.corner.effective(rect.w, rect.h),
        _ => panic!(),
    };
    assert!(r1 > 10.0 && r1 < rect.w.min(rect.h) * 0.5);
    assert!(r2 > r1 && r2 < rect.w.min(rect.h) * 0.5);
}

#[test]
fn fillet_drag_on_default_square_portal_keeps_designed_fillet() {
    let mut h = Harness::new("fillet_portal_default");
    h.app.ensure_work_tab();
    let mut portal = slate_doc::scene::PortalNode::unbound_web("page");
    portal.corner = slate_doc::scene::Corner::Square;
    let before = slate_doc::scene::Node {
        id: NodeId(2),
        rect: slate_doc::scene::WorldRect::new(0.0, 0.0, 320.0, 240.0),
        rotation_deg: 0.0,
        hidden: false,
        locked: false,
        opacity: 1.0,
        group: None,
        clip: None,
        bumper: None,
        kind: slate_doc::scene::NodeKind::Portal(portal),
    };
    h.app.doc_mut().scene.nodes.push(before.clone());
    let mut n = before.clone();
    h.app.apply_fillet_radius_from_drag(
        &mut n,
        &before,
        slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET,
        false,
    );
    assert!(
        !matches!(
            slate_doc::scene::corner_of(&n),
            Some(slate_doc::scene::Corner::Rounded { radius: 0.0 })
        ),
        "default portal must not become Rounded{{0}} when dragged to the designed fillet"
    );
}

#[test]
fn fillet_grip_click_on_default_portal_is_a_noop() {
    let mut h = Harness::new("fillet_portal_click");
    h.app.ensure_work_tab();
    let before = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 320.0, 240.0),
        slate_doc::scene::NodeKind::Portal(slate_doc::scene::PortalNode::unbound_web("page")),
    );
    let id = before.id;
    h.app.doc_mut().scene.nodes.push(before.clone());
    h.app.board_sel.insert(id);
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let grip = h
        .app
        .fillet_grip_at(&before, &xf)
        .expect("default portal grip");
    let world = xf.s2w(grip);
    h.app.board_drag = h.app.begin_fillet_drag(grip, world);
    h.app.update_gesture_for_test(world, egui::Modifiers::NONE);
    h.app
        .end_gesture_for_test(world, Some(grip), egui::Modifiers::NONE);
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert!(
        !h.app.tab().journal.can_undo(),
        "a press/release without motion must not add a Patch"
    );
}

#[test]
fn fillet_grip_drag_from_square_starts_at_zero_radius() {
    let mut h = Harness::new("fillet_square_drag");
    h.app.ensure_work_tab();
    let before = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 80.0),
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            sides: 6,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::Square,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let id = before.id;
    h.app.doc_mut().scene.nodes.push(before.clone());
    h.app.board_sel.insert(id);
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let grip = h.app.fillet_grip_at(&before, &xf).expect("square grip");
    let press = xf.s2w(grip);
    h.app.board_drag = h.app.begin_fillet_drag(grip, press);
    let moved = Pos2::new(press.x + 3.0, press.y + 3.0);
    h.app.update_gesture_for_test(moved, egui::Modifiers::NONE);
    h.app
        .end_gesture_for_test(moved, Some(xf.w2s(moved)), egui::Modifiers::NONE);
    let after = h.app.doc().scene.node(id).unwrap();
    let radius = slate_doc::scene::resolved_corner_effective(after, None).1;
    assert!((radius - 3.0).abs() < 0.05, "radius={radius}");
}

#[test]
fn fillet_drag_percent_mode_roundtrip() {
    let mut h = Harness::new("fillet_percent");
    h.app.ensure_work_tab();
    let rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0);
    let before = slate_doc::scene::Node {
        id: NodeId(3),
        rect,
        rotation_deg: 0.0,
        hidden: false,
        locked: false,
        opacity: 1.0,
        group: None,
        clip: None,
        bumper: None,
        kind: slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            sides: 6,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::RoundedPercent { percent: 50.0 },
            flip: false,
            path: None,
            text: None,
        }),
    };
    h.app.doc_mut().scene.nodes.push(before.clone());
    let mut n = before.clone();
    h.app
        .apply_fillet_radius_from_drag(&mut n, &before, 30.0, false);
    assert!(matches!(
        slate_doc::scene::corner_of(&n),
        Some(slate_doc::scene::Corner::RoundedPercent { .. })
    ));
    let (_, r) = slate_doc::scene::resolved_corner_effective(&n, None);
    assert!((r - 30.0).abs() < 0.01);
}

fn primary_button(pos: Pos2, pressed: bool, alt: bool) -> impl FnOnce(&mut egui::RawInput) {
    move |input: &mut egui::RawInput| {
        let modifiers = egui::Modifiers {
            alt,
            ..Default::default()
        };
        input.modifiers = modifiers;
        input.events.push(egui::Event::PointerMoved(pos));
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
}

fn pointer_to(pos: Pos2, alt: bool) -> impl FnOnce(&mut egui::RawInput) {
    move |input: &mut egui::RawInput| {
        input.modifiers.alt = alt;
        input.events.push(egui::Event::PointerMoved(pos));
    }
}

/// Alt+left-click with the Brush samples instead of painting; releasing Alt
/// paints again.
#[test]
fn brush_alt_click_samples_and_releasing_alt_paints_again() {
    let mut h = web_board("brush_alt_sample");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.frame();
    let c = h.app.canvas_rect.center();
    h.frame_with(pointer_to(c, true));
    assert!(h.app.eyedropper_active(), "Alt spring-loads the eyedropper");
    h.frame_with(primary_button(c, true, true));
    h.frame_with(primary_button(c, false, true));
    assert!(
        h.app.doc().scene.nodes.is_empty(),
        "Alt+click painted a dab"
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Brush);

    h.frame_with(pointer_to(c, false));
    assert!(!h.app.eyedropper_active());
    h.frame_with(primary_button(c, true, false));
    for i in 1..=6 {
        h.frame_with(pointer_to(c + EVec2::new(i as f32 * 12.0, 0.0), false));
    }
    h.frame_with(primary_button(c + EVec2::new(80.0, 0.0), false, false));
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "plain drag paints again");
}

/// The Alt sample reads the screen pixel under the hotspot, so the sampling
/// cursor must leave that pixel showing the canvas, not the current color.
#[test]
fn brush_alt_sampling_cursor_leaves_the_hotspot_clear() {
    use eframe::epaint::{Shape, Vertex};
    fn covers(shape: &Shape, p: Pos2, fg: egui::Color32) -> bool {
        match shape {
            Shape::Vec(list) => list.iter().any(|s| covers(s, p, fg)),
            Shape::Circle(c) => {
                let d = c.center.distance(p);
                (c.fill == fg && d <= c.radius)
                    || (c.stroke.color == fg && (d - c.radius).abs() <= c.stroke.width * 0.5)
            }
            Shape::Mesh(m) => m.indices.chunks(3).any(|t| {
                let v: Vec<&Vertex> = t.iter().map(|i| &m.vertices[*i as usize]).collect();
                v.iter().all(|v| v.color == fg) && {
                    let s =
                        |a: Pos2, b: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                    let (d0, d1, d2) = (
                        s(v[0].pos, v[1].pos),
                        s(v[1].pos, v[2].pos),
                        s(v[2].pos, v[0].pos),
                    );
                    (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
                }
            }),
            _ => false,
        }
    }
    let mut h = web_board("brush_alt_cursor");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg = slate_doc::scene::Rgba([12, 200, 34, 255]);
    let fg = board::rgba32(h.app.board_colors.fg);
    h.frame();
    let c = h.app.canvas_rect.center();
    h.frame_with(pointer_to(c, true));
    let mut input = egui::RawInput {
        screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
        ..Default::default()
    };
    pointer_to(c, true)(&mut input);
    let ctx = h.ctx.clone();
    let app = &mut h.app;
    let out = ctx.run(input, |c| app.update_app(c));
    assert!(h.app.eyedropper_active());
    let hits = out
        .shapes
        .iter()
        .filter(|s| covers(&s.shape, c, fg))
        .count();
    assert_eq!(
        hits, 0,
        "the sampling cursor paints the foreground on the hotspot"
    );
}

#[test]
fn a_brush_sample_sets_the_foreground_and_recent_colors() {
    let mut h = web_board("brush_sample_apply");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg = slate_doc::scene::Rgba([1, 2, 3, 200]);
    h.app.adopt_sampled_brush_color([90, 140, 210]);
    assert_eq!(h.app.board_colors.fg.0, [90, 140, 210, 200]);
    let recent = h.app.doc().view.recent_colors.clone().unwrap_or_default();
    assert_eq!(recent.first().copied(), Some([90, 140, 210]));
}

/// A two-point diagonal line crossing `(x, x)` at a right angle.
fn crossing_line(app: &mut SlateApp, x: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let (rect, data) = board_path::points_to_path_data(
        &[Pos2::new(x - 12.0, x + 12.0), Pos2::new(x + 12.0, x - 12.0)],
        false,
    );
    let node = app.doc_mut().scene.build_node(
        rect,
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: board_path::default_curve_stroke(Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: Some(data.into()),
            text: None,
        }),
    );
    let id = node.id;
    app.add_nodes(vec![node]);
    id
}

/// One fast pointer move is one long segment. Everything under that segment
/// is erased, not only what sits under its two ends.
#[test]
fn a_fast_eraser_segment_erases_along_its_whole_length() {
    let mut h = web_board("eraser_fast_segment");
    let lines: Vec<NodeId> = (1..=7)
        .map(|i| crossing_line(&mut h.app, i as f32 * 50.0))
        .collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 10.0;
    let dots: Vec<NodeId> = (1..=6)
        .map(|i| {
            let p = Pos2::new(i as f32 * 50.0 + 25.0, i as f32 * 50.0 + 25.0);
            h.app.finish_freehand_brush(vec![p]);
            h.app.doc().scene.nodes.last().unwrap().id
        })
        .collect();
    {
        let cam = &mut h.app.tab_mut().cam;
        cam.z = 1.0;
        cam.offset = EVec2::new(200.0, 200.0);
    }
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_width = 30.0;
    h.app.eraser_softness = 0.0;
    h.app.eraser_opacity = 1.0;
    h.frame();
    let xf = h.app.board_xf();
    let (a, b) = (xf.w2s(Pos2::new(0.0, 0.0)), xf.w2s(Pos2::new(400.0, 400.0)));
    assert!(h.app.canvas_rect.contains(a) && h.app.canvas_rect.contains(b));
    h.frame_with(pointer_to(a, false));
    h.frame_with(primary_button(a, true, false));
    h.frame_with(pointer_to(b, false));
    h.frame_with(primary_button(b, false, false));
    let left: Vec<NodeId> = lines
        .iter()
        .chain(&dots)
        .copied()
        .filter(|id| h.app.doc().scene.node(*id).is_some())
        .collect();
    assert!(
        left.is_empty(),
        "{} of {} marks under the segment survived",
        left.len(),
        lines.len() + dots.len()
    );
}

// ---------- Bézier span drafting and editing (contracts/bezier-span.md) ----------

fn bezier_board(tag: &str) -> Harness {
    let mut h = line_board(tag);
    h.app.set_board_tool(board::BoardTool::BezierSpan);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.frame();
    h.frame();
    h
}

fn bezier_draft(h: &Harness) -> Vec<(Pos2, board_path::BezierHandles)> {
    match &h.app.board_path_draft {
        Some(board_path::BoardPathDraft::Bezier { anchors, .. }) => anchors.clone(),
        other => panic!("expected a Bézier draft, got {other:?}"),
    }
}

/// One frame of ordered pointer events: press at the first world point, move
/// through the rest, release at the last.
fn press_drag_release(h: &mut Harness, world: &[Pos2], modifiers: egui::Modifiers) {
    let xf = h.app.board_xf();
    let pts: Vec<Pos2> = world.iter().map(|w| xf.w2s(*w)).collect();
    let (first, last) = (pts[0], *pts.last().unwrap());
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(first)));
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        for p in &pts[1..] {
            i.events.push(egui::Event::PointerMoved(*p));
        }
        i.events.push(egui::Event::PointerButton {
            pos: last,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
    });
}

fn bezier_click(h: &mut Harness, world: Pos2) {
    press_drag_release(h, &[world], egui::Modifiers::NONE);
}

/// Place one draft anchor without pointer routing (press, then release).
fn bezier_place(h: &mut Harness, press: Pos2, release: Pos2) {
    h.app.bezier_anchor_press(press);
    h.app.bezier_anchor_release(press, release, false);
}

/// A Select-tool drag through the board's gesture entry points: begin at the
/// press origin, one live update, end at the release.
fn select_drag(h: &mut Harness, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    let xf = h.app.board_xf();
    h.app.board_drag = h.app.begin_gesture_for_test(xf.w2s(from), from, modifiers);
    h.app.update_gesture_for_test(to, modifiers);
    h.app.end_gesture_for_test(to, Some(xf.w2s(to)), modifiers);
    h.frame();
}

fn press_key_with(h: &mut Harness, key: egui::Key, modifiers: egui::Modifiers) {
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
    });
    h.frame();
}

fn near(a: Pos2, b: Pos2) -> bool {
    (a - b).length() < 0.01
}

fn kpt(p: vector_ink::kurbo::Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

/// Bézier D04 / GP1: a stationary click places a corner anchor with zero
/// handles, so a span can hold straight segments and sharp corners.
#[test]
fn bezier_stationary_click_places_a_corner_anchor_without_handles() {
    let mut h = bezier_board("bezier_click_corner");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(120.0, 90.0),
    ];
    for p in pts {
        bezier_click(&mut h, p);
    }
    let anchors = bezier_draft(&h);
    assert_eq!(anchors.len(), 3, "each stationary click places one anchor");
    for ((p, handles), want) in anchors.iter().zip(pts) {
        assert!(near(*p, want), "{p:?} != {want:?}");
        assert_eq!(
            *handles,
            board_path::BezierHandles::default(),
            "a click has no weight"
        );
    }
    assert!(h.app.path_tool_try_finish());
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("a shape")
    };
    let path = s.path.as_ref().unwrap();
    assert!(
        path.segs
            .iter()
            .all(|seg| matches!(seg, slate_doc::scene::PathSeg::Line { .. })),
        "handle-less anchors commit straight segments: {:?}",
        path.segs
    );
}

/// Bézier D04: two quick clicks in different places are two anchors (the
/// stationary-click test above); a double-click on the anchor it just placed
/// finishes the span.
#[test]
fn bezier_double_click_on_the_placed_anchor_finishes_the_span() {
    let mut h = bezier_board("bezier_double_click_finish");
    let depth = h.app.tab().journal.undo_depth();
    let (a, b) = (Pos2::new(0.0, 0.0), Pos2::new(120.0, 40.0));
    bezier_click(&mut h, a);
    let t = h.ctx.input(|i| i.time);
    h.frame_with(|i| i.time = Some(t + 1.0));
    bezier_click(&mut h, b);
    assert_eq!(bezier_draft(&h).len(), 2);
    bezier_click(&mut h, b);
    assert!(
        h.app.board_path_draft.is_none(),
        "the double-click finished"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("a shape")
    };
    assert_eq!(
        s.path.as_ref().unwrap().segs.len(),
        1,
        "no duplicate anchor"
    );
}

/// Bézier GP2 through real pointer events: press-drag still authors
/// symmetric handles.
#[test]
fn bezier_press_drag_still_authors_symmetric_handles() {
    let mut h = bezier_board("bezier_drag_handles");
    press_drag_release(
        &mut h,
        &[
            Pos2::new(0.0, 0.0),
            Pos2::new(20.0, 0.0),
            Pos2::new(40.0, 0.0),
        ],
        egui::Modifiers::NONE,
    );
    let anchors = bezier_draft(&h);
    assert_eq!(anchors.len(), 1);
    assert!(near(anchors[0].0, Pos2::ZERO));
    assert!((anchors[0].1.handle_out - EVec2::new(40.0, 0.0)).length() < 0.01);
    assert!((anchors[0].1.handle_in - EVec2::new(-40.0, 0.0)).length() < 0.01);
}

/// Bézier D12: Ctrl+Z while drawing removes the last placed anchor and
/// Ctrl+Y / Ctrl+Shift+Z re-adds it, without touching the document journal.
/// With no anchors left, Ctrl+Z exits drawing; document undo then resumes.
#[test]
fn bezier_ctrl_z_while_drawing_removes_anchors_without_journaling() {
    let mut h = bezier_board("bezier_draft_undo");
    let rect = add_rect(&mut h.app, 400.0, 300.0);
    let depth = h.app.tab().journal.undo_depth();
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(120.0, 90.0),
    ];
    for p in pts {
        bezier_place(&mut h, p, p);
    }
    assert_eq!(bezier_draft(&h).len(), 3);
    let ctrl = egui::Modifiers::CTRL;
    let ctrl_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;

    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert_eq!(bezier_draft(&h).len(), 2, "Ctrl+Z removes the last anchor");
    press_key_with(&mut h, egui::Key::Y, ctrl);
    let anchors = bezier_draft(&h);
    assert_eq!(anchors.len(), 3, "Ctrl+Y re-adds it");
    assert!(near(anchors[2].0, pts[2]));
    press_key_with(&mut h, egui::Key::Z, ctrl);
    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert_eq!(bezier_draft(&h).len(), 1);
    press_key_with(&mut h, egui::Key::Z, ctrl_shift);
    assert_eq!(bezier_draft(&h).len(), 2, "Ctrl+Shift+Z re-adds too");
    press_key_with(&mut h, egui::Key::Z, ctrl);
    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert!(
        bezier_draft(&h).is_empty(),
        "drawing continues at zero anchors"
    );
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth,
        "draft undo never journals"
    );
    assert!(h.app.doc().scene.node(rect).is_some());

    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert!(h.app.board_path_draft.is_none(), "Ctrl+Z at zero exits");
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.doc().scene.node(rect).is_some());

    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert!(
        h.app.doc().scene.node(rect).is_none(),
        "document undo resumes once drawing ends"
    );
}

/// Bézier D17: while drawing, a press on an existing anchor or handle knob
/// drags it instead of placing an anchor. Alt on a knob breaks symmetry.
#[test]
fn bezier_press_on_a_draft_anchor_or_handle_edits_it_instead_of_placing() {
    let mut h = bezier_board("bezier_draft_edit");
    let depth = h.app.tab().journal.undo_depth();
    bezier_place(&mut h, Pos2::ZERO, Pos2::new(40.0, 0.0));
    bezier_place(&mut h, Pos2::new(200.0, 0.0), Pos2::new(200.0, 0.0));
    assert_eq!(bezier_draft(&h).len(), 2);

    press_drag_release(
        &mut h,
        &[
            Pos2::new(200.0, 0.0),
            Pos2::new(200.0, 40.0),
            Pos2::new(200.0, 80.0),
        ],
        egui::Modifiers::NONE,
    );
    let a = bezier_draft(&h);
    assert_eq!(a.len(), 2, "pressing an anchor edits it; no third anchor");
    assert!(near(a[1].0, Pos2::new(200.0, 80.0)), "{:?}", a[1].0);

    press_drag_release(
        &mut h,
        &[
            Pos2::new(40.0, 0.0),
            Pos2::new(40.0, 30.0),
            Pos2::new(40.0, 60.0),
        ],
        egui::Modifiers::NONE,
    );
    let a = bezier_draft(&h);
    assert_eq!(a.len(), 2);
    assert!(near(a[0].0, Pos2::ZERO), "a handle drag leaves its anchor");
    assert!((a[0].1.handle_out - EVec2::new(40.0, 60.0)).length() < 0.01);
    let mirrored = -EVec2::new(40.0, 60.0).normalized() * 40.0;
    assert!(
        (a[0].1.handle_in - mirrored).length() < 0.01,
        "the opposite handle stays collinear at its own length: {:?}",
        a[0].1.handle_in
    );

    let in_knob = a[0].0 + a[0].1.handle_in;
    press_drag_release(
        &mut h,
        &[in_knob, Pos2::new(-10.0, -15.0), Pos2::new(-10.0, -30.0)],
        egui::Modifiers::ALT,
    );
    let a = bezier_draft(&h);
    assert_eq!(a.len(), 2);
    assert!((a[0].1.handle_in - EVec2::new(-10.0, -30.0)).length() < 0.01);
    assert!(
        (a[0].1.handle_out - EVec2::new(40.0, 60.0)).length() < 0.01,
        "Alt leaves the opposite handle alone"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.doc().scene.nodes.is_empty());
}

/// Bézier D13 / P1.curve.grips: a single-selected Bézier path shows its
/// anchors and handles; each drag is one journaled patch; Alt on a handle
/// breaks symmetry.
#[test]
fn bezier_single_selection_grips_edit_the_curve_one_patch_per_drag() {
    let mut h = bezier_board("bezier_select_grips");
    let (a, b, c) = (
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(200.0, 80.0),
    );
    h.app.bezier_anchor_press(a);
    h.app.bezier_anchor_release(a, a, false);
    h.app.bezier_anchor_press(b);
    h.app
        .bezier_anchor_release(b, b + EVec2::new(40.0, 0.0), false);
    h.app.bezier_anchor_press(c);
    h.app.bezier_anchor_release(c, c, false);
    assert!(h.app.path_tool_try_finish());
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    let id = h.app.doc().scene.nodes[0].id;
    assert_eq!(h.app.board_sel.len(), 1);
    assert!(h.app.board_sel.contains(&id));
    h.frame();
    let depth = h.app.tab().journal.undo_depth();

    select_drag(&mut h, c, Pos2::new(200.0, 140.0), egui::Modifiers::NONE);
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    assert_eq!(anchors.len(), 3);
    assert!(
        near(kpt(anchors[0].point), a),
        "an anchor drag is not a move"
    );
    assert!(near(kpt(anchors[1].point), b));
    assert!(near(kpt(anchors[2].point), Pos2::new(200.0, 140.0)));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(h.app.board_sel.contains(&id));

    select_drag(
        &mut h,
        Pos2::new(140.0, 0.0),
        Pos2::new(140.0, 60.0),
        egui::Modifiers::NONE,
    );
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    let out = kpt(anchors[1].handle_out.unwrap());
    let inn = kpt(anchors[1].handle_in.unwrap());
    assert!(near(out, Pos2::new(140.0, 60.0)), "{out:?}");
    let mirrored = b - EVec2::new(40.0, 60.0).normalized() * 40.0;
    assert!(near(inn, mirrored), "symmetric: {inn:?} != {mirrored:?}");
    assert!(near(kpt(anchors[1].point), b));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);

    select_drag(&mut h, inn, Pos2::new(80.0, -40.0), egui::Modifiers::ALT);
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    assert!(near(
        kpt(anchors[1].handle_in.unwrap()),
        Pos2::new(80.0, -40.0)
    ));
    assert!(
        near(kpt(anchors[1].handle_out.unwrap()), Pos2::new(140.0, 60.0)),
        "Alt breaks symmetry"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 3);
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "no duplicate on Alt");

    h.app.board_undo();
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    assert!(near(kpt(anchors[1].handle_in.unwrap()), mirrored));
}

/// Bézier D12: Esc with two or more anchors commits the open curve as exactly
/// one journaled add.
#[test]
fn bezier_escape_commits_an_open_curve_as_one_journaled_add() {
    let mut h = bezier_board("bezier_escape_commit");
    let depth = h.app.tab().journal.undo_depth();
    h.app.bezier_anchor_press(Pos2::ZERO);
    h.app.bezier_anchor_release(Pos2::ZERO, Pos2::ZERO, false);
    let b = Pos2::new(120.0, 0.0);
    h.app.bezier_anchor_press(b);
    h.app
        .bezier_anchor_release(b, Pos2::new(160.0, 20.0), false);
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.app.board_path_draft.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "Esc commits the curve");
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "one journaled add"
    );
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("a shape")
    };
    assert!(!s.path.as_ref().unwrap().closed, "an open curve");
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
}

/// Bézier D12: Esc with fewer than two anchors cancels.
#[test]
fn bezier_escape_with_one_anchor_cancels() {
    let mut h = bezier_board("bezier_escape_cancel");
    let depth = h.app.tab().journal.undo_depth();
    h.app.bezier_anchor_press(Pos2::ZERO);
    h.app.bezier_anchor_release(Pos2::ZERO, Pos2::ZERO, false);
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.app.board_path_draft.is_none());
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

/// P1.wire.ports: open shapes offer no wire ports and are not wire targets;
/// a closed polyline keeps its ports.
#[test]
fn open_shapes_offer_no_wire_ports_while_a_closed_polyline_keeps_them() {
    let mut h = line_board("open_shape_ports");
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    let open_pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(120.0, 90.0),
    ];
    let (r, d) = board_path::points_to_path_data(&open_pts, false);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Polyline, r, d, false);
    let open = h.app.doc().scene.nodes.last().unwrap().id;
    let tri = [
        Pos2::new(300.0, 0.0),
        Pos2::new(420.0, 0.0),
        Pos2::new(300.0, 120.0),
    ];
    let (r, d) = board_path::points_to_path_data(&tri, true);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Polyline, r, d, true);
    let closed = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();

    let open_node = h.app.doc().scene.node(open).unwrap().clone();
    for (side, t) in [
        (slate_doc::scene::Side::Start, 0.0),
        (slate_doc::scene::Side::Mid, 0.5),
        (slate_doc::scene::Side::End, 1.0),
    ] {
        let at = slate_doc::connector_anchor_on(&open_node, side, t);
        let screen = xf.w2s(Pos2::new(at[0], at[1]));
        assert_eq!(
            h.app.wire_grip_at(screen, &xf),
            None,
            "open polyline {side:?} must not offer a port"
        );
    }

    let closed_node = h.app.doc().scene.node(closed).unwrap().clone();
    let ports = h.app.wire_host(&closed_node).ports();
    assert_eq!(ports.len(), 3, "a closed polyline keeps its ports");
    for port in &ports {
        let screen = xf.w2s(Pos2::new(port.point[0], port.point[1]));
        assert_eq!(
            h.app.wire_grip_at(screen, &xf).map(|g| g.0),
            Some(closed),
            "closed polyline {:?}",
            port.side
        );
    }

    let start = Pos2::new(ports[0].point[0], ports[0].point[1]);
    let mut wd = h
        .app
        .try_begin_wire_drag(xf.w2s(start), start, egui::Modifiers::NONE)
        .expect("a closed polyline still starts a wire");
    h.app.wire_drag_update(&mut wd, open_pts[1], false);
    assert!(
        wd.snap.is_none_or(|(id, _, _)| id != open),
        "an open shape is not a wire target: {:?}",
        wd.snap
    );
}

// ---------- parametric grips on committed curves (P1.curve.grips) ----------

fn grip_board(tag: &str) -> Harness {
    let mut h = line_board(tag);
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.frame();
    h
}

fn commit_polyline(h: &mut Harness, pts: &[Pos2], closed: bool) -> NodeId {
    let (r, d) = board_path::points_to_path_data(pts, closed);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Polyline, r, d, closed);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    assert!(
        h.app.board_sel.contains(&id),
        "a committed curve is selected"
    );
    h.frame();
    id
}

fn world_anchor_points(h: &Harness, id: NodeId) -> Vec<Pos2> {
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    anchors.iter().map(|a| kpt(a.point)).collect()
}

fn cmds_bez(cmds: &[slate_doc::wire::PathCmd]) -> vector_ink::kurbo::BezPath {
    use slate_doc::wire::PathCmd;
    use vector_ink::kurbo::Point;
    let pt = |p: [f32; 2]| Point::new(p[0] as f64, p[1] as f64);
    let mut bez = vector_ink::kurbo::BezPath::new();
    for cmd in cmds {
        match *cmd {
            PathCmd::Move(p) => bez.move_to(pt(p)),
            PathCmd::Line(p) => bez.line_to(pt(p)),
            PathCmd::Cubic { c1, c2, to } => bez.curve_to(pt(c1), pt(c2), pt(to)),
        }
    }
    bez
}

fn assert_bez_near(got: &vector_ink::kurbo::BezPath, want: &vector_ink::kurbo::BezPath) {
    use vector_ink::kurbo::PathEl;
    let points = |el: &PathEl| -> Vec<vector_ink::kurbo::Point> {
        match *el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
            PathEl::QuadTo(a, b) => vec![a, b],
            PathEl::CurveTo(a, b, c) => vec![a, b, c],
            PathEl::ClosePath => vec![],
        }
    };
    let (g, w) = (got.elements(), want.elements());
    assert_eq!(g.len(), w.len(), "{got:?} != {want:?}");
    for (a, b) in g.iter().zip(w) {
        for (p, q) in points(a).into_iter().zip(points(b)) {
            assert!((p - q).hypot() < 1e-2, "{got:?} != {want:?}");
        }
    }
}

/// The committed path of `id` is one circular arc from `s` to `e` passing
/// through `m`, within the arc tool's own fitting tolerance.
fn assert_circular_arc_through(h: &Harness, id: NodeId, s: Pos2, m: Pos2, e: Pos2) {
    use vector_ink::kurbo::{ParamCurve, PathSeg as KSeg, Point};
    let n = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Shape(shape) = &n.kind else {
        panic!("a shape")
    };
    let bez = board_path::path_data_to_world_bez(shape.path.as_ref().unwrap(), n.rect, 0.0);
    let (sx, sy, mx, my, ex, ey) = (
        s.x as f64, s.y as f64, m.x as f64, m.y as f64, e.x as f64, e.y as f64,
    );
    let d = 2.0 * (sx * (my - ey) + mx * (ey - sy) + ex * (sy - my));
    let (s2, m2, e2) = (sx * sx + sy * sy, mx * mx + my * my, ex * ex + ey * ey);
    let c = Point::new(
        (s2 * (my - ey) + m2 * (ey - sy) + e2 * (sy - my)) / d,
        (s2 * (ex - mx) + m2 * (sx - ex) + e2 * (mx - sx)) / d,
    );
    let r = (Point::new(sx, sy) - c).hypot();
    let mut samples = Vec::new();
    for seg in bez.segments() {
        for i in 0..=256 {
            samples.push(match seg {
                KSeg::Line(l) => l.eval(i as f64 / 256.0),
                KSeg::Quad(q) => q.eval(i as f64 / 256.0),
                KSeg::Cubic(k) => k.eval(i as f64 / 256.0),
            });
        }
    }
    let first = *samples.first().unwrap();
    let last = *samples.last().unwrap();
    assert!(
        (first - Point::new(sx, sy)).hypot() < 0.05,
        "starts at {s:?}: {first:?}"
    );
    assert!(
        (last - Point::new(ex, ey)).hypot() < 0.05,
        "ends at {e:?}: {last:?}"
    );
    for p in &samples {
        assert!(
            ((*p - c).hypot() - r).abs() < 0.3,
            "{p:?} is off the circle"
        );
    }
    let through = samples
        .iter()
        .map(|p| (*p - Point::new(mx, my)).hypot())
        .fold(f64::INFINITY, f64::min);
    assert!(through < 0.5, "passes through {m:?} (closest {through})");
}

/// User finding (2026-09-26), P1.curve.grips: reselecting a polyline exposes
/// its corner vertices and end points. Each drag moves that one point as one
/// journaled patch, and the path stays a line polyline.
#[test]
fn polyline_single_selection_grips_move_one_vertex_per_patch() {
    let mut h = grip_board("polyline_grips");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(120.0, 90.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let depth = h.app.tab().journal.undo_depth();

    let corner = Pos2::new(160.0, -30.0);
    select_drag(&mut h, pts[1], corner, egui::Modifiers::NONE);
    let got = world_anchor_points(&h, id);
    assert_eq!(got.len(), 3);
    assert!(near(got[0], pts[0]), "the other points stay: {got:?}");
    assert!(near(got[1], corner), "the corner vertex moved: {got:?}");
    assert!(near(got[2], pts[2]), "the other points stay: {got:?}");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);

    let end = Pos2::new(-20.0, 10.0);
    select_drag(&mut h, pts[0], end, egui::Modifiers::NONE);
    let got = world_anchor_points(&h, id);
    assert!(near(got[0], end), "an end point moves: {got:?}");
    assert!(near(got[1], corner), "{got:?}");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);
    let NodeKind::Shape(s) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!("a shape")
    };
    assert!(
        slate_doc::geom::path_is_line_polyline(s.path.as_ref().unwrap()),
        "still a line polyline, so Corners still applies"
    );
    assert!(h.app.board_sel.contains(&id));

    h.app.board_undo();
    let got = world_anchor_points(&h, id);
    assert!(near(got[0], pts[0]), "undo restores one drag: {got:?}");
    assert!(near(got[1], corner));
}

/// A filled closed polyline shows its vertices too; a vertex that sits on
/// the bounding-box corner moves as a vertex, not as a resize.
#[test]
fn closed_polyline_grips_move_a_vertex_where_the_resize_corner_sits() {
    let mut h = grip_board("closed_polyline_grips");
    let tri = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(0.0, 90.0),
    ];
    let id = commit_polyline(&mut h, &tri, true);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.fill = Some(Rgba([200, 30, 30, 255]));
        }
    });
    h.frame();
    let depth = h.app.tab().journal.undo_depth();

    let moved = Pos2::new(-30.0, -20.0);
    select_drag(&mut h, tri[0], moved, egui::Modifiers::NONE);
    let got = world_anchor_points(&h, id);
    assert_eq!(got.len(), 3, "no duplicated seam vertex: {got:?}");
    assert!(near(got[0], moved), "{got:?}");
    assert!(near(got[1], tri[1]), "{got:?}");
    assert!(near(got[2], tri[2]), "{got:?}");
    let NodeKind::Shape(s) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!("a shape")
    };
    assert!(s.path.as_ref().unwrap().closed);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
}

/// User finding (2026-09-26): editing a filleted polyline re-applies the
/// authored radius to the new geometry, clamped per corner only.
#[test]
fn polyline_vertex_drag_keeps_the_authored_fillet_radius() {
    let mut h = grip_board("polyline_fillet_grips");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 0.0),
        Pos2::new(200.0, 200.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let corner = slate_doc::scene::Corner::Rounded { radius: 30.0 };
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.corner = corner;
        }
    });
    h.frame();

    let moved = Pos2::new(400.0, 40.0);
    select_drag(&mut h, pts[2], moved, egui::Modifiers::NONE);
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape")
    };
    assert_eq!(s.corner, corner, "the authored radius is stored unchanged");
    let drawn = slate_doc::geom::path_data_to_world_bez_with_fillet(
        s.path.as_ref().unwrap(),
        n.rect,
        n.rotation_deg,
        s.corner,
    );
    let want = cmds_bez(&slate_doc::filleted_vertex_path(
        &[[0.0, 0.0], [200.0, 0.0], [400.0, 40.0]],
        30.0,
        false,
        false,
    ));
    assert_bez_near(&drawn, &want);
}

/// User finding (2026-09-26): a reselected arc exposes its start, end and
/// through point; dragging one rebuilds the circular arc through the three.
#[test]
fn arc_single_selection_grips_edit_start_end_and_through_point() {
    let mut h = grip_board("arc_grips");
    h.app.set_board_tool(board::BoardTool::Arc);
    let (s, e, m) = (
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 0.0),
        Pos2::new(100.0, 60.0),
    );
    for p in [s, e, m] {
        h.app.path_tool_click(p);
    }
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(h.app.board_sel.contains(&id));
    h.frame();
    let depth = h.app.tab().journal.undo_depth();

    let m2 = Pos2::new(100.0, 100.0);
    select_drag(&mut h, m, m2, egui::Modifiers::NONE);
    assert_circular_arc_through(&h, id, s, m2, e);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);

    let s2 = Pos2::new(-40.0, 20.0);
    select_drag(&mut h, s, s2, egui::Modifiers::NONE);
    assert_circular_arc_through(&h, id, s2, m2, e);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);

    let e2 = Pos2::new(220.0, -30.0);
    select_drag(&mut h, e, e2, egui::Modifiers::NONE);
    let n = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Shape(shape) = &n.kind else {
        panic!("a shape")
    };
    let bez = board_path::path_data_to_world_bez(shape.path.as_ref().unwrap(), n.rect, 0.0);
    let (anchors, _) = vector_ink::anchors_from_bezpath(&bez);
    assert!(near(kpt(anchors[0].point), s2), "the start stays put");
    assert!(
        near(kpt(anchors.last().unwrap().point), e2),
        "the end moved"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 3);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

/// Line endpoints are painted and picked by the shared path-edit overlay:
/// the one 7 screen px pick radius, not a line-only radius.
#[test]
fn line_endpoint_grips_follow_the_shared_path_edit_hit_rule() {
    let mut h = grip_board("line_grip_hit");
    let id = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0))
        .unwrap();
    h.frame();
    let xf = h.app.board_xf();
    let end = xf.w2s(Pos2::new(200.0, 0.0));
    let r = super::path_edit_overlay::HIT_PX;
    assert_eq!(
        h.app.line_grip_at(id, end + EVec2::new(r - 0.5, 0.0), &xf),
        Some(1)
    );
    assert_eq!(
        h.app.line_grip_at(id, end + EVec2::new(r + 0.5, 0.0), &xf),
        None,
        "the shared pick radius"
    );
}

// ----- image crop: handle hits, first grab, multi-crop, repeat -------------------

/// `n` croppable 200×150 images in a row 60 world units apart, all selected,
/// the camera at 1:1 and centered on the row so screen px equal world units.
fn crop_board(tag: &str, n: usize) -> (Harness, Vec<NodeId>) {
    let mut h = kit_board(tag, board::BoardTool::Select);
    let paths: Vec<PathBuf> = (0..n)
        .map(|i| {
            let p = h.base.join(format!("crop{i}.png"));
            image::RgbaImage::from_pixel(8, 6, image::Rgba([90, 140, 200, 255]))
                .save(&p)
                .unwrap();
            p
        })
        .collect();
    let items = h.app.add_paths(&paths);
    let nodes: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            h.app.doc_mut().scene.build_node(
                WorldRect::new(i as f32 * 260.0, 0.0, 200.0, 150.0),
                NodeKind::Image(slate_doc::scene::ImageNode::new(*item)),
            )
        })
        .collect();
    let ids = h.app.add_nodes(nodes);
    h.app.board_sel = ids.iter().copied().collect();
    let row_w = n as f32 * 260.0 - 60.0;
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = EVec2::new(row_w * 0.5, 75.0);
    h.frame();
    h.frame();
    for id in &ids {
        assert!(h.app.croppable_image(*id), "fixture images must crop");
    }
    (h, ids)
}

/// Turn crop on through the registry command (C) with no panel open.
fn crop_via_command(h: &mut Harness) {
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.crop"), None));
    assert!(h.app.board_crop.is_some());
}

fn crop_pointer(h: &mut Harness, p: Pos2, button: Option<bool>) {
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(p));
        if let Some(pressed) = button {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    });
}

fn crop_drag_handle(h: &Harness) -> Option<(NodeId, u8, usize)> {
    match &h.app.board_drag {
        Some(board::BoardDrag::CropEdge {
            id, handle, peers, ..
        }) => Some((*id, *handle, peers.len())),
        _ => None,
    }
}

/// The W handle is hit along the whole left edge, a few px outside the box,
/// not only on the 18 px bar at the midpoint.
#[test]
fn crop_edge_hits_along_its_full_length_and_just_outside() {
    let (mut h, ids) = crop_board("crop_edge_full_length", 1);
    crop_via_command(&mut h);
    let xf = h.app.board_xf();
    // 40 px below the NW corner: past the corner zone, far from the bar,
    // and 6 px outside the box.
    let p = xf.w2s(Pos2::new(-6.0, 40.0));
    crop_pointer(&mut h, p, Some(true));
    crop_pointer(&mut h, p + EVec2::new(30.0, 0.0), None);
    assert_eq!(
        crop_drag_handle(&h).map(|(id, handle, _)| (id, handle)),
        Some((ids[0], board_handles::ResizeHandle::W as u8))
    );
    crop_pointer(&mut h, p + EVec2::new(30.0, 0.0), Some(false));
    assert!(h.app.board_crop.is_some());
}

/// Hovering a handle's slop shows its resize cursor.
#[test]
fn crop_handle_hover_shows_the_resize_cursor() {
    let (mut h, _) = crop_board("crop_hover_cursor", 1);
    crop_via_command(&mut h);
    let xf = h.app.board_xf();
    let cursor_at = |h: &mut Harness, p: Pos2| {
        let input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
            events: vec![egui::Event::PointerMoved(p)],
            ..Default::default()
        };
        let ctx = h.ctx.clone();
        let app = &mut h.app;
        ctx.run(input, |c| app.update_app(c))
            .platform_output
            .cursor_icon
    };
    cursor_at(&mut h, xf.w2s(Pos2::new(100.0, 75.0)));
    assert_eq!(
        cursor_at(&mut h, xf.w2s(Pos2::new(-6.0, 40.0))),
        egui::CursorIcon::ResizeWest
    );
    assert_eq!(
        cursor_at(&mut h, xf.w2s(Pos2::new(205.0, 155.0))),
        egui::CursorIcon::ResizeSouthEast
    );
    assert_eq!(
        cursor_at(&mut h, xf.w2s(Pos2::new(120.0, -6.0))),
        egui::CursorIcon::ResizeNorth
    );
}

/// A click that stays in a handle's outside slop is not a click on empty
/// canvas: crop mode and the selection stay.
#[test]
fn crop_click_in_outside_slop_keeps_crop_mode() {
    let (mut h, ids) = crop_board("crop_click_slop", 1);
    crop_via_command(&mut h);
    let xf = h.app.board_xf();
    let p = xf.w2s(Pos2::new(-5.0, 75.0));
    crop_pointer(&mut h, p, Some(true));
    crop_pointer(&mut h, p, Some(false));
    assert_eq!(h.app.board_crop, Some(ids[0]));
    assert!(h.app.board_sel.contains(&ids[0]));
}

/// Turn crop on the way D01 describes: open the Corners squircle, then pick
/// Crop in its Off/Crop control. The panel stays open (D09).
fn crop_via_corners_panel(h: &mut Harness) {
    h.app.shape_properties.panel = Some(board_properties::Panel::Corners);
    for _ in 0..4 {
        h.frame();
    }
    let first = *h.app.board_sel.iter().min_by_key(|id| id.0).unwrap();
    h.app.enter_crop_mode(first);
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Corners)
    );
}

fn crop_key(h: &mut Harness, key: egui::Key, pressed: bool) {
    h.frame_with(|i| {
        i.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
}

fn crop_of(h: &Harness, id: NodeId) -> slate_doc::scene::Crop {
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(img) => img.crop,
        _ => panic!("image node"),
    }
}

/// The very first press on a handle after turning crop on from the Corners
/// panel grabs it. No hover frame precedes the press.
#[test]
fn crop_first_press_after_corners_crop_grabs_the_handle() {
    let (mut h, ids) = crop_board("crop_first_grab_panel", 1);
    crop_via_corners_panel(&mut h);
    let xf = h.app.board_xf();
    let east = xf.w2s(Pos2::new(200.0, 75.0));
    crop_pointer(&mut h, east, Some(true));
    crop_pointer(&mut h, east + EVec2::new(-20.0, 0.0), None);
    assert_eq!(
        crop_drag_handle(&h).map(|(id, handle, _)| (id, handle)),
        Some((ids[0], board_handles::ResizeHandle::E as u8)),
        "first press on the E bar must start the crop drag"
    );
    crop_pointer(&mut h, east + EVec2::new(-40.0, 0.0), None);
    crop_pointer(&mut h, east + EVec2::new(-40.0, 0.0), Some(false));
    let c = crop_of(&h, ids[0]);
    assert!((c.w - 0.8).abs() < 0.01, "crop.w {}", c.w);
    assert!(h.app.board_crop.is_some(), "crop stays on (D02)");
}

/// C enters crop, then the next frame presses a handle with no hover.
#[test]
fn crop_first_press_after_c_key_grabs_the_handle() {
    let (mut h, ids) = crop_board("crop_first_grab_key", 1);
    crop_key(&mut h, egui::Key::C, true);
    let xf = h.app.board_xf();
    let west = xf.w2s(Pos2::new(0.0, 75.0));
    h.frame_with(|i| {
        i.events.push(egui::Event::Key {
            key: egui::Key::C,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        i.events.push(egui::Event::PointerMoved(west));
        i.events.push(egui::Event::PointerButton {
            pos: west,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
    });
    assert_eq!(h.app.board_crop, Some(ids[0]), "C entered crop mode");
    crop_pointer(&mut h, west + EVec2::new(20.0, 0.0), None);
    assert_eq!(
        crop_drag_handle(&h).map(|(_, handle, _)| handle),
        Some(board_handles::ResizeHandle::W as u8)
    );
    crop_pointer(&mut h, west + EVec2::new(20.0, 0.0), Some(false));
    assert!((crop_of(&h, ids[0]).x - 0.1).abs() < 0.01);
}

/// Crop mode, the whole selection of `n`, and the Corners squircle are all
/// still up after the frame that just ran.
fn assert_crop_holds(h: &Harness, n: usize, when: &str) {
    assert_eq!(
        h.app.shape_properties.panel,
        Some(board_properties::Panel::Corners),
        "{when}: Corners panel closed"
    );
    assert!(h.app.board_crop.is_some(), "{when}: crop mode left");
    assert_eq!(h.app.board_sel.len(), n, "{when}: {:?}", h.app.board_sel);
}

fn crop_idle_frames(h: &mut Harness, n: usize, when: &str) {
    for f in 0..6 {
        h.frame();
        assert_crop_holds(h, n, &format!("{when}, idle frame {f}"));
    }
}

/// Every selected image stays in crop mode for the whole gesture with the
/// Corners squircle up, the drag writes the same crop on all of them, and
/// one undo restores them.
#[test]
fn crop_multi_images_stay_in_crop_mode_and_commit_one_group() {
    for n in [3, 5] {
        let (mut h, ids) = crop_board(&format!("crop_multi_{n}"), n);
        crop_via_corners_panel(&mut h);
        crop_idle_frames(&mut h, n, "after entering crop");
        let xf = h.app.board_xf();
        let west_b = xf.w2s(Pos2::new(260.0, 75.0));
        crop_pointer(&mut h, west_b, Some(true));
        assert_crop_holds(&h, n, "press");
        for dx in [10.0, 20.0, 30.0] {
            crop_pointer(&mut h, west_b + EVec2::new(dx, 0.0), None);
            assert_crop_holds(&h, n, "mid-drag");
            assert_eq!(
                crop_drag_handle(&h),
                Some((ids[1], board_handles::ResizeHandle::W as u8, n - 1)),
                "the grabbed image drives every peer"
            );
        }
        crop_pointer(&mut h, west_b + EVec2::new(30.0, 0.0), Some(false));
        crop_idle_frames(&mut h, n, "after release");
        for id in &ids {
            let c = crop_of(&h, *id);
            assert!(
                (c.x - 0.15).abs() < 0.01 && (c.w - 0.85).abs() < 0.01,
                "{id:?}: {c:?}"
            );
        }
        h.app.board_undo();
        for id in &ids {
            assert!(crop_of(&h, *id).is_full(), "one undo restores {id:?}");
        }
    }
}

/// A click on each selected image in turn during crop mode keeps every
/// image in crop mode and the Corners squircle up (D17: crop mode does not
/// clear the selection).
#[test]
fn crop_click_on_each_peer_keeps_every_image_in_crop_mode() {
    for n in [3, 5] {
        let (mut h, ids) = crop_board(&format!("crop_multi_click_{n}"), n);
        crop_via_corners_panel(&mut h);
        crop_idle_frames(&mut h, n, "after entering crop");
        let xf = h.app.board_xf();
        for i in 0..n {
            let inside = xf.w2s(Pos2::new(i as f32 * 260.0 + 100.0, 75.0));
            crop_pointer(&mut h, inside, Some(true));
            assert_crop_holds(&h, n, &format!("press on image {i}"));
            crop_pointer(&mut h, inside, Some(false));
            crop_idle_frames(&mut h, n, &format!("after click on image {i}"));
        }
        for id in &ids {
            assert!(h.app.board_sel.contains(id));
        }
    }
}

/// The strip keys on which nodes are selected, not on the order the
/// selection set happens to iterate in.
#[test]
fn crop_panel_survives_selection_set_reordering() {
    for n in [3, 5] {
        let (mut h, _) = crop_board(&format!("crop_multi_reorder_{n}"), n);
        crop_via_corners_panel(&mut h);
        for round in 0..24 {
            h.app.board_sel = h.app.board_sel.iter().copied().collect();
            h.frame();
            assert_crop_holds(&h, n, &format!("reordered set, round {round}"));
        }
    }
}

/// After a crop, selecting another image and tapping the repeat key (P0.4)
/// enters crop mode on it, the same as invoking Crop.
#[test]
fn crop_repeat_last_reenters_crop_on_the_next_image() {
    for key in [egui::Key::Space, egui::Key::Enter] {
        let (mut h, ids) = crop_board("crop_repeat", 2);
        h.app.board_sel = [ids[0]].into_iter().collect();
        h.frame();
        crop_via_corners_panel(&mut h);
        let xf = h.app.board_xf();
        let east = xf.w2s(Pos2::new(200.0, 75.0));
        crop_pointer(&mut h, east, Some(true));
        crop_pointer(&mut h, east + EVec2::new(-30.0, 0.0), None);
        crop_pointer(&mut h, east + EVec2::new(-30.0, 0.0), Some(false));
        assert!(!crop_of(&h, ids[0]).is_full(), "first image cropped");
        // Esc closes the Corners panel, then leaves crop mode (P0.1).
        for _ in 0..2 {
            crop_key(&mut h, egui::Key::Escape, true);
            crop_key(&mut h, egui::Key::Escape, false);
        }
        assert!(h.app.board_crop.is_none(), "Esc left crop mode");
        h.app.board_sel = [ids[1]].into_iter().collect();
        h.frame();
        crop_key(&mut h, key, true);
        crop_key(&mut h, key, false);
        assert_eq!(h.app.board_crop, Some(ids[1]), "{key:?} repeats crop");
    }
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

fn add_picture(h: &mut Harness, rect: slate_doc::scene::WorldRect) -> NodeId {
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    h.app.add_nodes(vec![node])[0]
}

fn picture_flips(h: &Harness, id: NodeId) -> (bool, bool) {
    match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::NodeKind::Image(img) => (img.flip_x, img.flip_y),
        _ => panic!("image"),
    }
}

/// Dragging a picture's right edge past its left edge mirrors it: the width
/// stays positive, the flip is authored state, and one undo restores both.
#[test]
fn dragging_a_pictures_edge_past_its_opposite_mirrors_it() {
    let mut h = web_board("edge_cross_mirror");
    let id = add_picture(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
    );
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let xf = h.app.board_xf();
    // Off the edge midpoint so a wire grip does not steal the press.
    let edge = xf.w2s(Pos2::new(200.0, 12.0));
    let mods = egui::Modifiers::default();
    h.app.board_drag = h.app.begin_gesture_for_test(edge, xf.s2w(edge), mods);
    assert!(matches!(
        h.app.board_drag,
        Some(board::BoardDrag::Resize { handle: 3, .. })
    ));
    let undo_depth = h.app.tab().journal.undo_depth();
    // Through the far edge and back out again: the flip follows the pointer.
    h.app.update_gesture_for_test(Pos2::new(-30.0, 12.0), mods);
    assert_eq!(picture_flips(&h, id), (true, false));
    h.app.update_gesture_for_test(Pos2::new(120.0, 12.0), mods);
    assert_eq!(picture_flips(&h, id), (false, false));
    let past = Pos2::new(-60.0, 12.0);
    h.app.update_gesture_for_test(past, mods);
    h.app.end_gesture_for_test(past, Some(xf.w2s(past)), mods);

    let after = h.app.doc().scene.node(id).unwrap().clone();
    assert!(
        (after.rect.x + 60.0).abs() < 0.5 && (after.rect.w - 60.0).abs() < 0.5,
        "{:?}",
        after.rect
    );
    assert!(after.rect.w > 0.0 && after.rect.h > 0.0);
    assert_eq!(picture_flips(&h, id), (true, false));
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        undo_depth + 1,
        "one step per drag"
    );

    h.app.board_undo();
    let undone = h.app.doc().scene.node(id).unwrap();
    assert_eq!(undone.rect, before.rect);
    assert_eq!(picture_flips(&h, id), (false, false));
}

/// A shape that cannot mirror keeps the old clamp at the minimum size.
#[test]
fn dragging_a_rect_edge_past_its_opposite_still_clamps() {
    let mut h = web_board("edge_cross_rect");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let xf = h.app.board_xf();
    let edge = xf.w2s(Pos2::new(80.0, 12.0));
    let mods = egui::Modifiers::default();
    h.app.board_drag = h.app.begin_gesture_for_test(edge, xf.s2w(edge), mods);
    let past = Pos2::new(-60.0, 12.0);
    h.app.update_gesture_for_test(past, mods);
    h.app.end_gesture_for_test(past, Some(xf.w2s(past)), mods);
    let after = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!(after.x, 0.0, "{after:?}");
}

/// Mirror horizontal / vertical flip pictures and paths across the board
/// axis through each node's center; rectangles are left alone. Undo restores.
#[test]
fn mirror_commands_toggle_pictures_and_paths_and_undo() {
    use slate_doc::scene::{PathData, PathSeg, ShapeKind, ShapeNode};
    let mut h = web_board("mirror_commands");
    let pic = add_picture(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
    );
    h.app.patch_nodes(&[pic], |n| n.rotation_deg = 30.0);
    let path = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(300.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.1, 0.2],
                segs: vec![PathSeg::Line { to: [0.9, 0.7] }],
                ..PathData::default()
            })),
            text: None,
        }),
    );
    let path = h.app.add_nodes(vec![path])[0];
    let rect = add_rect(&mut h.app, 500.0, 0.0);
    let rect_before = h.app.doc().scene.node(rect).unwrap().clone();
    h.app.board_sel = [pic, path, rect].into_iter().collect();

    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.mirror.horizontal"),
        None
    ));
    assert_eq!(picture_flips(&h, pic), (true, false));
    assert_eq!(h.app.doc().scene.node(pic).unwrap().rotation_deg, -30.0);
    let start = |h: &Harness| match &h.app.doc().scene.node(path).unwrap().kind {
        slate_doc::NodeKind::Shape(s) => s.path.as_ref().unwrap().start,
        _ => panic!("path"),
    };
    assert!((start(&h)[0] - 0.9).abs() < 1e-6, "{:?}", start(&h));
    assert_eq!(h.app.doc().scene.node(rect).unwrap(), &rect_before);

    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.mirror.vertical"),
        None
    ));
    assert_eq!(picture_flips(&h, pic), (true, true));

    h.app.board_undo();
    assert_eq!(picture_flips(&h, pic), (true, false));
    h.app.board_undo();
    assert_eq!(picture_flips(&h, pic), (false, false));
    assert_eq!(h.app.doc().scene.node(pic).unwrap().rotation_deg, 30.0);
    assert!((start(&h)[0] - 0.1).abs() < 1e-6);

    h.app.board_sel = std::iter::once(rect).collect();
    assert!(!h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.mirror.horizontal"),
        None
    ));
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// A 40 x 20 PNG, red on the left half and blue on the right, placed at
/// `rect`.
fn add_two_tone_picture(
    h: &mut Harness,
    name: &str,
    rect: slate_doc::scene::WorldRect,
) -> (NodeId, PathBuf) {
    let p = h.base.join(name);
    image::RgbaImage::from_fn(40, 20, |x, _| image::Rgba(if x < 20 { RED } else { BLUE }))
        .save(&p)
        .unwrap();
    let item = h.app.add_paths(std::slice::from_ref(&p))[0];
    let node = h.app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    (h.app.add_nodes(vec![node])[0], p)
}

fn copied_bitmap(h: &Harness) -> image::RgbaImage {
    let write = h.app.os_clipboard.last_write().expect("a clipboard write");
    let png = write.png.as_ref().expect("a PNG on the clipboard");
    let from_png = image::load_from_memory(png).unwrap().to_rgba8();
    let dib = write.dibv5.as_ref().expect("a DIBV5 on the clipboard");
    let from_dib = image::load_from_memory(&clipboard::decode_clipboard_bitmap(dib).unwrap())
        .unwrap()
        .to_rgba8();
    assert_eq!(from_png, from_dib, "PNG and DIBV5 carry the same pixels");
    from_png
}

fn copy_selection(h: &mut Harness, ids: &[NodeId]) {
    h.app.board_sel = ids.iter().copied().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.copy"), None));
}

/// Copying one picture puts a real bitmap on the clipboard at the picture's
/// own resolution, plus the linked file and Slate's own format. The text
/// slot is empty, never the node JSON.
#[test]
fn copying_a_picture_puts_its_bitmap_on_the_clipboard() {
    let mut h = web_board("copy_picture_bitmap");
    let (id, path) = add_two_tone_picture(
        &mut h,
        "two.png",
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
    );
    copy_selection(&mut h, &[id]);
    let bitmap = copied_bitmap(&h);
    assert_eq!(bitmap.dimensions(), (40, 20));
    assert_eq!(bitmap.get_pixel(5, 10).0, RED);
    assert_eq!(bitmap.get_pixel(35, 10).0, BLUE);

    let write = h.app.os_clipboard.last_write().unwrap();
    assert_eq!(write.text, None, "no JSON in the text slot");
    assert_eq!(write.files, vec![path]);
    let nodes: Vec<slate_doc::scene::Node> = serde_json::from_str(&write.nodes_json).unwrap();
    assert_eq!(nodes.len(), 1);
    assert!(matches!(nodes[0].kind, slate_doc::NodeKind::Image(_)));
}

/// The bitmap is the picture as displayed: rotation, crop, mirror, filters,
/// and paint layers all land in the pixels.
#[test]
fn a_copied_bitmap_carries_rotation_crop_mirror_filters_and_ink() {
    use slate_doc::scene::{ShapeKind, ShapeNode, WorldRect};
    let mut h = web_board("copy_picture_as_shown");
    let (id, _) = add_two_tone_picture(&mut h, "two.png", WorldRect::new(0.0, 0.0, 200.0, 100.0));

    h.app.patch_nodes(&[id], |n| n.rotation_deg = 90.0);
    copy_selection(&mut h, &[id]);
    let turned = copied_bitmap(&h);
    assert_eq!(turned.dimensions(), (20, 40));
    // A clockwise quarter turn puts the red left half on top.
    assert_eq!(turned.get_pixel(10, 5).0, RED);
    assert_eq!(turned.get_pixel(10, 35).0, BLUE);

    h.app.patch_nodes(&[id], |n| {
        n.rotation_deg = 0.0;
        // The crop tool keeps the window's aspect: half the width, half the box.
        n.rect.w = 100.0;
        if let slate_doc::NodeKind::Image(img) = &mut n.kind {
            img.crop = slate_doc::scene::Crop {
                x: 0.5,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            };
        }
    });
    copy_selection(&mut h, &[id]);
    let cropped = copied_bitmap(&h);
    assert_eq!(cropped.dimensions(), (20, 20));
    assert!(cropped.pixels().all(|p| p.0 == BLUE));

    h.app.patch_nodes(&[id], |n| {
        n.rect.w = 200.0;
        if let slate_doc::NodeKind::Image(img) = &mut n.kind {
            img.crop = slate_doc::scene::Crop::full();
            img.flip_x = true;
            img.adjust.invert = 1.0;
        }
    });
    copy_selection(&mut h, &[id]);
    let flipped = copied_bitmap(&h);
    // Mirrored, the blue half is on the left; inverted, blue reads yellow.
    assert_eq!(flipped.get_pixel(5, 10).0, [255, 255, 0, 255]);
    assert_eq!(flipped.get_pixel(35, 10).0, [0, 255, 255, 255]);

    let mut scene = slate_doc::scene::Scene::default();
    let ink = scene.build_node(
        WorldRect::new(0.0, 0.0, 0.25, 1.0),
        slate_doc::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(slate_doc::scene::Rgba::opaque(0, 255, 0)),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            flip: false,
            path: None,
            text: None,
        }),
    );
    h.app.patch_nodes(&[id], |n| {
        if let slate_doc::NodeKind::Image(img) = &mut n.kind {
            img.flip_x = false;
            img.adjust.invert = 0.0;
            let mut layer = slate_doc::PaintLayer::new(slate_doc::PaintLayerId(1));
            layer.nodes.push(ink.clone());
            img.paint_layers.push(layer);
        }
    });
    copy_selection(&mut h, &[id]);
    let inked = copied_bitmap(&h);
    assert_eq!(
        inked.get_pixel(3, 10).0,
        [0, 255, 0, 255],
        "the paint layer"
    );
    assert_eq!(inked.get_pixel(15, 10).0, RED);
}

/// Several pictures copy as one bitmap laid out as they sit on the board.
#[test]
fn copying_several_pictures_composes_one_bitmap() {
    use slate_doc::scene::WorldRect;
    let mut h = web_board("copy_two_pictures");
    let (a, _) = add_two_tone_picture(&mut h, "a.png", WorldRect::new(0.0, 0.0, 200.0, 100.0));
    let (b, _) = add_two_tone_picture(&mut h, "b.png", WorldRect::new(300.0, 0.0, 200.0, 100.0));
    copy_selection(&mut h, &[a, b]);
    let bitmap = copied_bitmap(&h);
    assert_eq!(bitmap.dimensions(), (100, 20));
    assert_eq!(bitmap.get_pixel(5, 10).0, RED);
    assert_eq!(bitmap.get_pixel(50, 10).0[3], 0, "the gap is transparent");
    assert_eq!(bitmap.get_pixel(95, 10).0, BLUE);
    assert_eq!(h.app.os_clipboard.last_write().unwrap().files.len(), 2);
}

/// A mixed selection keeps Slate's own format and offers plain text, not
/// JSON. Pasting inside Slate still lands the nodes, not the fallback.
#[test]
fn a_mixed_copy_offers_plain_text_and_still_pastes_nodes() {
    use slate_doc::scene::WorldRect;
    let mut h = web_board("copy_mixed");
    let (pic, _) = add_two_tone_picture(&mut h, "two.png", WorldRect::new(0.0, 0.0, 200.0, 100.0));
    let words = h.app.doc_mut().scene.build_node(
        WorldRect::new(300.0, 0.0, 200.0, 50.0),
        slate_doc::NodeKind::Text(slate_doc::scene::TextNode {
            text: "Hello board".into(),
            family: Default::default(),
            size: 24.0,
            color: slate_doc::scene::Rgba::BLACK,
            align: Default::default(),
            fill: None,
            agent: None,
        }),
    );
    let words = h.app.add_nodes(vec![words])[0];
    copy_selection(&mut h, &[pic, words]);
    let write = h.app.os_clipboard.last_write().unwrap();
    assert_eq!(write.text.as_deref(), Some("Hello board"));
    assert!(write.png.is_none() && write.dibv5.is_none());
    let json = write.nodes_json.clone();
    assert_ne!(write.text.as_deref(), Some(json.as_str()));

    let before = h.app.doc().scene.nodes.len();
    h.app.pending_paste_text = Some("Hello board".into());
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.paste"), None));
    assert_eq!(h.app.doc().scene.nodes.len(), before + 2);
    let texts = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| matches!(&n.kind, slate_doc::NodeKind::Text(t) if t.text == "Hello board"))
        .count();
    assert_eq!(texts, 2, "the text node itself, not a pasted string");
}

/// Copy then paste a picture inside Slate: the node comes back with its
/// crop and mirror, not as a flattened new bitmap.
#[test]
fn a_copied_picture_pastes_back_as_the_same_node() {
    use slate_doc::scene::WorldRect;
    let mut h = web_board("copy_paste_picture");
    let (id, _) = add_two_tone_picture(&mut h, "two.png", WorldRect::new(0.0, 0.0, 200.0, 100.0));
    h.app.patch_nodes(&[id], |n| {
        if let slate_doc::NodeKind::Image(img) = &mut n.kind {
            img.crop.w = 0.5;
            img.flip_y = true;
        }
    });
    let original = match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::NodeKind::Image(img) => img.clone(),
        _ => unreachable!(),
    };
    copy_selection(&mut h, &[id]);
    let items = h.app.doc().items.len();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.paste"), None));
    assert_eq!(h.app.doc().items.len(), items, "no new pasted file");
    let pasted = *h.app.board_sel.iter().next().unwrap();
    assert_ne!(pasted, id);
    match &h.app.doc().scene.node(pasted).unwrap().kind {
        slate_doc::NodeKind::Image(img) => assert_eq!(img, &original),
        _ => panic!("image"),
    }
}

/// The Actions flyout offers both mirrors, and they run the commands.
#[test]
fn actions_flyout_offers_mirror() {
    let mut h = web_board("actions_mirror");
    let pic = add_picture(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
    );
    h.app.board_sel = std::iter::once(pic).collect();
    let items = ui::tools::palette_strip_items(&h.app, "tool.actions", &[]);
    let ids: Vec<_> = items.iter().map(|i| i.id).collect();
    assert!(ids.contains(&"action.mirror_h"), "{ids:?}");
    assert!(ids.contains(&"action.mirror_v"), "{ids:?}");
    for id in ["board.mirror.horizontal", "board.mirror.vertical"] {
        assert!(h
            .app
            .registry
            .by_id(atlas_commands::CommandId(id))
            .is_some());
    }
    ui::tools::activate_flyout_id(&mut h.app, &h.ctx, "action.mirror_v");
    assert_eq!(
        h.app.cmd_history.iter().last().unwrap().id.0,
        "board.mirror.vertical"
    );
    assert_eq!(picture_flips(&h, pic), (false, true));
}
