//! Regression coverage for Atlas's real settings menus and their filter controls.

use super::*;
use atlas_core::{
    index::Db,
    types::{Family, FileEntry},
};
use atlas_shell::{
    dock::{current_body_layout, DockBodyLayout, DockSide},
    file_picker::DialogGate,
};
use egui::{epaint::Shape, Event, FullOutput, PointerButton, Pos2, Rect, Vec2};
use std::time::{SystemTime, UNIX_EPOCH};

struct Harness {
    ctx: egui::Context,
    app: AtlasApp,
    time: f64,
}

impl Harness {
    fn new(side: DockSide, panel: &'static str, saved_strips: &[&str]) -> Self {
        let base = std::env::temp_dir().join(format!(
            "atlas_dock_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&base).unwrap();
        let ctx = egui::Context::default();
        let mut app = AtlasApp::with_db(
            &ctx,
            Db::open_at(base.join("index.db")),
            None,
            DialogGate::new(),
        );
        app.canvas_rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 1200.0));
        app.dock_side = side;
        app.dock_pins = vec![panel.into()];
        app.dock_strip_hidden.clear();
        app.dock_strip_order.clear();
        app.dock_bar_collapsed = false;
        app.family_on = [true; 10];
        app.ext_group_on.clear();
        app.auto_zoom_matches = false;
        app.map_view = MapView::Tree;
        for tool in ToolPanel::ALL {
            app.active_chrome_mut().set_tool(tool, true);
        }
        app.entries = ["photo.jpg", "diagram.png", "notes.pdf"]
            .into_iter()
            .map(|name| FileEntry::from_rel(&base, name.into(), 10, 1, 1, "Ada".into()))
            .collect();
        app.all_owners.clear();
        app.all_owners.insert("Ada".into(), 3);

        // Restore legacy IDs through the shared API before the real app's
        // persistence observer runs. Synchronizing normalization here avoids
        // writing the user's chrome preferences during a headless test.
        let seed = [DockItem {
            id: panel,
            label: "Restore saved menu",
            description: "",
            icon: DockIcon::Filters,
            kind: DockItemKind::Dashboard,
            active: false,
            visible: true,
            gap_before: false,
        }];
        let saved: Vec<String> = saved_strips.iter().map(|s| (*s).into()).collect();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(app.canvas_rect),
                time: Some(0.0),
                ..Default::default()
            },
            |ctx| {
                floating_dock(
                    ctx,
                    "file_atlas_tools",
                    app.canvas_rect,
                    &app.palette(),
                    side,
                    &seed,
                    &app.dock_pins,
                    &saved,
                    &[],
                    &[],
                    false,
                    |ui, _| {
                        ui.label("Restore saved menu");
                    },
                );
            },
        );
        app.dock_icon_strips = atlas_shell::dock::icon_strip_ids(&ctx, "file_atlas_tools").unwrap();
        atlas_shell::dock::set_panel_open(&ctx, "file_atlas_tools", panel, true);
        Self {
            ctx,
            app,
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> FullOutput {
        self.time += 0.05;
        self.app.frame_no += 1;
        let ctx = self.ctx.clone();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(self.app.canvas_rect),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ctx| floating_tools_dock(&mut self.app, ctx),
        )
    }

    fn settle(&mut self) -> Snapshot {
        for _ in 0..8 {
            self.frame(vec![]);
        }
        Snapshot::from_output(self.frame(vec![]))
    }

    fn click(&mut self, pos: Pos2) -> Snapshot {
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.settle()
    }

    fn click_label(&mut self, shot: &Snapshot, label: &str) -> Snapshot {
        self.click(shot.text_rect(label).center())
    }
}

#[derive(Default)]
struct Snapshot {
    text: Vec<(String, Rect)>,
    rects: Vec<Rect>,
}

impl Snapshot {
    fn from_output(output: FullOutput) -> Self {
        let mut snapshot = Self::default();
        for painted in output.shapes {
            snapshot.record(&painted.shape, painted.clip_rect);
        }
        snapshot
    }

    fn record(&mut self, shape: &Shape, clip: Rect) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes {
                    self.record(shape, clip);
                }
            }
            Shape::Text(text) => {
                let rect = Rect::from_min_size(text.pos, text.galley.size());
                if clip.contains(rect.center()) {
                    self.text.push((text.galley.job.text.clone(), rect));
                }
            }
            Shape::Rect(rect) if clip.intersects(rect.rect) => self.rects.push(rect.rect),
            _ => {}
        }
    }

    fn text_rect(&self, label: &str) -> Rect {
        self.text
            .iter()
            .find(|(text, _)| text == label)
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("missing {label:?}; painted text: {:?}", self.text))
    }

    fn assert_labels(&self, labels: &[&str]) {
        for label in labels {
            self.text_rect(label);
        }
    }

    fn checkbox_before(&self, label: &str) -> Pos2 {
        let label = self.text_rect(label);
        self.rects
            .iter()
            .filter(|rect| {
                (rect.center().y - label.center().y).abs() < 2.0
                    && rect.right() < label.left()
                    && (8.0..=24.0).contains(&rect.width())
                    && (rect.height() - rect.width()).abs() < 0.1
            })
            .max_by(|a, b| a.right().total_cmp(&b.right()))
            .expect("family checkbox should precede its swatch and label")
            .center()
    }

    fn expand_before(&self, label: &str) -> Pos2 {
        let label = self.text_rect(label);
        self.text
            .iter()
            .find(|(text, rect)| {
                text == "+"
                    && (rect.center().y - label.center().y).abs() < 2.0
                    && rect.right() < label.left()
            })
            .expect("family should expose a subtype expand control")
            .1
            .center()
    }
}

#[test]
fn atlas_filter_menus_keep_all_controls_with_saved_capsule_preferences() {
    for side in [DockSide::LeftCenter, DockSide::BottomCenter] {
        for saved in [vec!["*"], vec!["filters", "display"]] {
            let mut h = Harness::new(side, "filters", &saved);
            let shot = h.settle();
            shot.assert_labels(&[
                "Basic filters",
                "Search names…",
                "Filter by file types",
                "Filter by owner",
                "Filter by dates",
                "Zoom to matches",
                "Duplicates",
                "Display",
            ]);
            assert_eq!(current_body_layout(&h.ctx), DockBodyLayout::List);
            assert_eq!(h.app.dock_icon_strips, ["*"]);

            let shot = h.click_label(&shot, "Filter by file types");
            shot.assert_labels(&["Images (2)", "Documents (1)", "all", "none"]);
            let shot = h.click(shot.expand_before("Images (2)"));
            shot.assert_labels(&["JPEG (1)", "PNG (1)"]);
            let jpeg = Family::Image
                .ext_groups()
                .iter()
                .find(|g| g.label == "JPEG")
                .unwrap();
            h.app.filter_dirty = false;
            let shot = h.click_label(&shot, "JPEG (1)");
            assert!(!h.app.ext_group_enabled(Family::Image, jpeg));
            assert!(h.app.filter_dirty);
            let shot = h.click(shot.checkbox_before("Images (2)"));
            assert!(!h.app.family_on[Family::Image.idx()]);
            let shot = h.click(shot.checkbox_before("Images (2)"));
            assert!(h.app.family_on[Family::Image.idx()]);
            assert!(h.app.ext_group_enabled(Family::Image, jpeg));

            h.app.filter_dirty = false;
            let shot = h.click_label(&shot, "Zoom to matches");
            assert!(h.app.auto_zoom_matches);
            assert!(h.app.filter_dirty);
            let shot = h.click_label(&shot, "Filter by owner");
            shot.assert_labels(&["Ada (3)"]);
            let shot = h.click_label(&shot, "Duplicates");
            shot.assert_labels(&["Hide duplicate files (same name & size, keep newest)"]);
            let shot = h.click_label(&shot, "Display");
            shot.assert_labels(&["ghost", "hide"]);

            h.click_label(&shot, "Search names…");
            h.app.filter_dirty = false;
            h.frame(vec![Event::Text("diagram".into())]);
            assert_eq!(h.app.search, "diagram");
            assert!(h.app.filter_dirty);
            assert_eq!(h.app.edit_mode, EditMode::View);
        }
    }
}

#[test]
fn atlas_display_menu_preserves_form_settings_and_fit_action() {
    for side in [DockSide::LeftCenter, DockSide::BottomCenter] {
        let mut h = Harness::new(side, "display", &["*"]);
        let shot = h.settle();
        shot.assert_labels(&[
            "Display settings",
            "layout",
            "tree",
            "packed",
            "Fit",
            "Dark",
            "grid columns",
            "stack threshold",
            "row spacing",
            "File Atlas layout",
        ]);
        assert_eq!(current_body_layout(&h.ctx), DockBodyLayout::List);
        h.app.pending_view = None;
        h.click_label(&shot, "Fit");
        assert!(matches!(h.app.pending_view, Some(ViewCmd::Fit)));
        assert_eq!(h.app.edit_mode, EditMode::View);
    }
}
