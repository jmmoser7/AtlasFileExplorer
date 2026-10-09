//! Harness, frame stepping, and basic shape fixtures.

use super::*;

pub(crate) struct Harness {
    pub(crate) ctx: egui::Context,
    pub(crate) app: SlateApp,
    pub(crate) base: PathBuf,
}

pub(crate) fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

impl Harness {
    pub(crate) fn wait_for_export(&mut self) {
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

    pub(crate) fn new(tag: &str) -> Harness {
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
        // The developer's AI workspace is real, often a synced folder: a
        // frame with an agent card would publish its link folder there.
        app.ai.config.workspace_dir = None;
        Harness { ctx, app, base }
    }

    pub(crate) fn frame(&mut self) {
        self.frame_with(|_| {});
    }

    /// One frame with real input, which is the only way to test what the board
    /// and a focused page each do with the same wheel notch or keystroke.
    pub(crate) fn frame_with(&mut self, prepare: impl FnOnce(&mut egui::RawInput)) {
        let _ = self.frame_output(prepare);
    }

    /// [`Self::frame_with`], returning what the frame painted and what it
    /// asked of the platform (the cursor icon, for one).
    pub(crate) fn frame_output(
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
    pub(crate) fn seed(&mut self) -> (TagId, TagId, TagId) {
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

pub(crate) fn assert_invariants(app: &SlateApp) {
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

// ----- board (authored canvas) ---------------------------------------------------

impl Harness {
    /// A frame at (0,0)-(800,450) tagged with the given tag, via the same
    /// journaled path the UI uses.
    pub(crate) fn seed_frame(&mut self, tag: Option<TagId>) -> NodeId {
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

// ---------- keymap wave 2b ----------

/// A horizontal open path stroke at (x, y)..(x+100, y).
pub(crate) fn add_stroke(app: &mut SlateApp, x: f32, y: f32) -> NodeId {
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
            phase_deg: 0.0,
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

pub(crate) fn add_rect(app: &mut SlateApp, x: f32, y: f32) -> NodeId {
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
            phase_deg: 0.0,
            flip: false,
            path: None,

            text: None,
        }),
    );
    let ids = app.add_nodes(vec![node]);
    ids[0]
}

pub(crate) fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

pub(crate) fn press_key(h: &mut Harness, key: egui::Key) {
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(700.0, 420.0)));
        input.events.push(key_event(key, egui::Modifiers::NONE));
    });
}

/// Press and release the primary button at `screen`, one frame each.
pub(crate) fn click_board(h: &mut Harness, screen: Pos2) {
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(screen));
            input.events.push(egui::Event::PointerButton {
                pos: screen,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
}

/// Let a second pass, as it does while a person types. Harness frames are
/// 1/60 s apart, so two clicks a dozen frames apart would read as a
/// double-click (the canvas palette on empty board).
pub(crate) fn pause(h: &mut Harness) {
    let t = h.ctx.input(|i| i.time);
    h.frame_with(|i| i.time = Some(t + 1.0));
}
