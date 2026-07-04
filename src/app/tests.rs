//! Headless multi-tab stability tests.
//!
//! These drive the real `AtlasApp` frame loop through a plain `egui::Context`
//! (no eframe window), with real scans, the real SQLite index, and the real
//! thumbnail pool — the exact code paths the desktop build runs. Every test
//! checks the workspace invariants that keep tab switching crash-free.

use super::*;
use std::path::Path;

struct Harness {
    ctx: egui::Context,
    app: AtlasApp,
    _base: PathBuf,
}

impl Harness {
    fn new(tag: &str) -> Harness {
        let base = std::env::temp_dir().join(format!(
            "nfa_tab_test_{}_{}_{}",
            tag,
            std::process::id(),
            now_nanos()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let ctx = egui::Context::default();
        let app = AtlasApp::with_db(&ctx, Db::open_at(base.join("index.db")), None);
        Harness {
            ctx,
            app,
            _base: base,
        }
    }

    fn frame(&mut self) {
        self.frame_with_events(Vec::new());
    }

    fn frame_with_events(&mut self, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            events,
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let _ = ctx.run(input, |c| app.update_app(c));
        assert_workspace_invariants(&self.app);
    }

    /// Pump frames until the active tab has finished loading + scanning.
    fn pump_until_idle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.frame();
            let idle = self.app.scan_ui.is_none() && self.app.pending_load.is_none();
            if idle {
                // One extra frame so filter recompute / tree rebuild settle.
                self.frame();
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for scan/load to finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// The invariants that make tab switching safe: `active_tab` in bounds, the
/// per-entry parallel vectors in lockstep, and no id anywhere pointing past
/// the entries vec.
fn assert_workspace_invariants(app: &AtlasApp) {
    assert!(
        !app.tabs.is_empty(),
        "there must always be at least one tab"
    );
    assert!(
        app.active_tab < app.tabs.len(),
        "active_tab {} out of bounds ({} tabs)",
        app.active_tab,
        app.tabs.len()
    );
    assert_eq!(app.entries.len(), app.thumb_state.len());
    assert_eq!(app.entries.len(), app.avg_color.len());
    assert!(app.file_match.len() <= app.entries.len());
    if let Some(t) = &app.tree {
        assert_eq!(
            t.file_pos.len(),
            app.entries.len(),
            "tree built against a different entries vec"
        );
        for d in &t.dirs {
            for &f in &d.files {
                assert!((f as usize) < app.entries.len());
            }
        }
    }
    for &f in &app.selection {
        assert!(
            (f as usize) < app.entries.len(),
            "selection id out of range"
        );
    }
    if let Some(f) = app.hovered_file {
        assert!((f as usize) < app.entries.len(), "hovered id out of range");
    }
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// A small folder tree with a nested dir and a few file families.
fn make_tree(dir: &Path, files: usize) -> PathBuf {
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    for i in 0..files {
        let name = match i % 3 {
            0 => format!("photo_{i}.jpg"),
            1 => format!("doc_{i}.pdf"),
            _ => format!("nested/clip_{i}.mp4"),
        };
        std::fs::write(dir.join(name), vec![b'x'; 10 + i]).unwrap();
    }
    dir.to_path_buf()
}

#[test]
fn second_tab_from_top_bar_while_first_is_loaded() {
    let mut h = Harness::new("second_tab");
    let root = make_tree(&h._base.join("proj_a"), 12);

    h.app.set_root(root.clone());
    h.pump_until_idle();
    assert_eq!(h.app.entries.len(), 12);
    assert!(h.app.tree.is_some());

    // The reported crash: "+" on the top bar with a folder already open.
    h.app.new_tab();
    h.frame();
    assert_eq!(h.app.tabs.len(), 2);
    assert_eq!(h.app.active_tab, 1);
    assert!(h.app.root.is_none(), "new tab must start empty");
    assert!(h.app.entries.is_empty());

    // Several idle frames on the welcome screen must be stable too.
    for _ in 0..5 {
        h.frame();
    }

    // Switching back re-loads the first tab from the index.
    h.app.switch_tab(0);
    h.pump_until_idle();
    assert_eq!(h.app.root.as_ref(), Some(&root));
    assert_eq!(h.app.entries.len(), 12);
}

#[test]
fn second_tab_mid_scan_cancels_cleanly() {
    let mut h = Harness::new("mid_scan");
    let root = make_tree(&h._base.join("proj_a"), 400);

    // Open and immediately punch "+" before the scan can possibly finish.
    h.app.set_root(root.clone());
    h.frame();
    h.app.new_tab();
    // Stale scan batches / thumb results must be discarded by generation.
    for _ in 0..20 {
        h.frame();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.app.root.is_none());
    assert!(h.app.entries.is_empty());

    // Back to tab 0: a fresh load must produce the full folder again.
    h.app.switch_tab(0);
    h.pump_until_idle();
    assert_eq!(h.app.entries.len(), 400);
}

#[test]
fn ten_plus_tabs_switch_and_close_stress() {
    let mut h = Harness::new("stress");
    let roots: Vec<PathBuf> = (0..4)
        .map(|i| make_tree(&h._base.join(format!("proj_{i}")), 6 + i * 3))
        .collect();

    // Tab 0 gets the first root.
    h.app.set_root(roots[0].clone());
    h.pump_until_idle();

    // Open 11 more tabs, alternating between empty tabs and mapped folders.
    for t in 1..12usize {
        h.app.new_tab();
        h.frame();
        if t % 2 == 0 {
            h.app.set_root(roots[t % roots.len()].clone());
            h.pump_until_idle();
        }
    }
    assert_eq!(h.app.tabs.len(), 12);

    // Walk every tab twice, including mid-scan switches (no pump in between).
    for round in 0..2 {
        for i in 0..h.app.tabs.len() {
            h.app.switch_tab(i);
            h.frame();
            if round == 1 {
                h.pump_until_idle();
                let expected = h.app.tabs[i].root.clone();
                assert_eq!(h.app.root, expected);
            }
        }
    }

    // Close tabs in awkward orders: an inactive one, the active one, then
    // everything else down to a single empty tab.
    h.app.switch_tab(5);
    h.frame();
    h.app.close_tab(2); // inactive, before the active index
    h.frame();
    assert_eq!(h.app.active_tab, 4, "active index shifts left");
    h.app.close_tab(h.app.active_tab); // active
    h.frame();
    while h.app.tabs.len() > 1 {
        h.app.close_tab(0);
        h.frame();
    }
    h.app.close_tab(0); // closing the last tab resets it to empty
    h.pump_until_idle();
    assert_eq!(h.app.tabs.len(), 1);
    assert!(h.app.root.is_none());
}

#[test]
fn same_folder_in_two_tabs_keeps_independent_cameras() {
    let mut h = Harness::new("same_root");
    let root = make_tree(&h._base.join("proj_a"), 9);

    h.app.set_root(root.clone());
    h.pump_until_idle();
    h.app.cam = Camera {
        offset: Vec2::new(111.0, 22.0),
        z: 1.25,
    };

    // Second tab onto the same folder.
    h.app.new_tab();
    h.frame();
    h.app.set_root(root.clone());
    h.pump_until_idle();
    h.app.cam = Camera {
        offset: Vec2::new(-300.0, 40.0),
        z: 0.5,
    };
    h.frame();

    // Hopping between them only jumps the camera — no reload, no crash.
    h.app.switch_tab(0);
    h.frame();
    assert_eq!(h.app.cam.offset, Vec2::new(111.0, 22.0));
    assert_eq!(h.app.cam.z, 1.25);
    assert_eq!(h.app.entries.len(), 9, "same-root switch must not clear");

    h.app.switch_tab(1);
    h.frame();
    assert_eq!(h.app.cam.offset, Vec2::new(-300.0, 40.0));
    assert_eq!(h.app.cam.z, 0.5);
}

#[test]
fn tab_switch_restores_camera_after_reload() {
    let mut h = Harness::new("cam_restore");
    let root_a = make_tree(&h._base.join("proj_a"), 8);
    let root_b = make_tree(&h._base.join("proj_b"), 5);

    h.app.set_root(root_a.clone());
    h.pump_until_idle();
    h.app.cam = Camera {
        offset: Vec2::new(77.0, -13.0),
        z: 0.8,
    };

    h.app.new_tab();
    h.frame();
    h.app.set_root(root_b);
    h.pump_until_idle();

    h.app.switch_tab(0);
    h.pump_until_idle();
    assert_eq!(h.app.root.as_ref(), Some(&root_a));
    assert_eq!(h.app.cam.offset, Vec2::new(77.0, -13.0));
    assert_eq!(h.app.cam.z, 0.8);
}

#[test]
fn picker_result_lands_on_the_tab_that_asked() {
    let mut h = Harness::new("picker_routing");
    let root_a = make_tree(&h._base.join("proj_a"), 4);
    let root_b = make_tree(&h._base.join("proj_b"), 7);

    h.app.set_root(root_a.clone());
    h.pump_until_idle();

    // Tab 1 opens the picker, then the user switches back to tab 0 while
    // the dialog is still up.
    h.app.new_tab();
    h.frame();
    let tab1_id = h.app.tabs[1].id;
    let (tx, rx) = unbounded();
    h.app.picker_rx = Some((tab1_id, rx));
    h.app.switch_tab(0);
    h.pump_until_idle();

    // The pick arrives late: it must bind to tab 1, not the active tab 0.
    tx.send(Some(root_b.clone())).unwrap();
    h.frame();
    assert_eq!(h.app.root.as_ref(), Some(&root_a), "active tab untouched");
    assert_eq!(h.app.tabs[1].root.as_ref(), Some(&root_b));

    // Activating tab 1 loads the picked folder.
    h.app.switch_tab(1);
    h.pump_until_idle();
    assert_eq!(h.app.root.as_ref(), Some(&root_b));
    assert_eq!(h.app.entries.len(), 7);
}

#[test]
fn picker_result_for_a_closed_tab_is_dropped() {
    let mut h = Harness::new("picker_closed");
    let root_a = make_tree(&h._base.join("proj_a"), 4);
    let root_b = make_tree(&h._base.join("proj_b"), 3);

    h.app.set_root(root_a.clone());
    h.pump_until_idle();
    h.app.new_tab();
    h.frame();
    let tab1_id = h.app.tabs[1].id;
    let (tx, rx) = unbounded();
    h.app.picker_rx = Some((tab1_id, rx));

    // Close the requesting tab before the dialog resolves.
    h.app.close_tab(1);
    h.pump_until_idle();
    tx.send(Some(root_b)).unwrap();
    h.frame();
    assert_eq!(h.app.tabs.len(), 1);
    assert_eq!(h.app.root.as_ref(), Some(&root_a), "pick must be dropped");
}

#[test]
fn late_index_reply_for_another_root_is_ignored() {
    let mut h = Harness::new("late_reply");
    let root_a = make_tree(&h._base.join("proj_a"), 6);
    let root_b = h._base.join("proj_b");

    h.app.set_root(root_a.clone());
    h.pump_until_idle();
    let entries_before = h.app.entries.len();

    // Hand-craft a stale in-flight load for a root we are no longer showing.
    let (tx, rx) = unbounded();
    h.app.pending_load = Some((root_b.clone(), rx));
    tx.send(LoadedRoot {
        snapshot: Some(vec![FileEntry::from_rel(
            &root_b,
            "ghost.jpg".into(),
            10,
            1_700_000_000,
            1_700_000_000,
            String::new(),
        )]),
        last_scan: 0,
        tag_state: TagState {
            tags: HashMap::new(),
            assigns: HashMap::new(),
        },
        journal_json: None,
    })
    .unwrap();
    h.frame();
    // A scan for root_b must NOT have been started, and entries stay root_a's.
    assert_eq!(h.app.root.as_ref(), Some(&root_a));
    assert_eq!(h.app.entries.len(), entries_before);
    assert!(!h.app.entries.iter().any(|e| e.rel == "ghost.jpg"));
}

#[test]
fn pointer_torture_across_tab_switches() {
    let mut h = Harness::new("pointer");
    let root_a = make_tree(&h._base.join("proj_a"), 30);
    let root_b = make_tree(&h._base.join("proj_b"), 10);

    h.app.set_root(root_a);
    h.pump_until_idle();

    // Hover + click around the canvas, switch tabs mid-gesture, keep
    // clicking: stale hover/selection state must never index out of bounds.
    let spots = [
        Pos2::new(720.0, 450.0),
        Pos2::new(400.0, 300.0),
        Pos2::new(1000.0, 700.0),
    ];
    for (i, p) in spots.iter().enumerate() {
        h.frame_with_events(vec![egui::Event::PointerMoved(*p)]);
        h.frame_with_events(vec![
            egui::Event::PointerButton {
                pos: *p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: *p,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        match i {
            0 => {
                h.app.new_tab();
                h.frame();
            }
            1 => {
                h.app.set_root(root_b.clone());
                // No pump: keep interacting mid-scan.
            }
            _ => {
                h.app.switch_tab(0);
                h.frame();
            }
        }
    }
    h.pump_until_idle();
}
