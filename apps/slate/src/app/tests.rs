//! Headless Slate stability tests: drive the real frame loop through a plain
//! `egui::Context` (no eframe window) with the real thumbnail pool, exercising
//! the tag model, both presentations, tabs, and workbook save/load.

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
        let app = SlateApp::with_ctx(&ctx, None);
        Harness { ctx, app, base }
    }

    fn frame(&mut self) {
        let input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let _ = ctx.run(input, |c| app.update_app(c));
        assert_invariants(&self.app);
    }

    /// A workbook with two facet groups, three tags, and three linked files
    /// (one uncategorized, one single-tagged, one cross-group tagged).
    fn seed(&mut self) -> (TagId, TagId, TagId) {
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
    assert!(!app.tabs.is_empty(), "at least one tab must exist");
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
