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

    /// A frame without the invariant sweep, which is itself O(entries) and
    /// would dominate any measurement of the frame loop.
    fn timed_frame(&mut self, events: Vec<egui::Event>) -> f64 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            events,
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let t = Instant::now();
        let _ = ctx.run(input, |c| app.update_app(c));
        t.elapsed().as_secs_f64() * 1000.0
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
    if app.at_home && app.tabs.is_empty() {
        return;
    }
    if !app.tabs.is_empty() {
        assert!(
            app.active_tab < app.tabs.len(),
            "active_tab {} out of bounds ({} tabs)",
            app.active_tab,
            app.tabs.len()
        );
    }
    assert_eq!(app.entries.len(), app.thumb_state.len());
    assert_eq!(app.entries.len(), app.avg_color.len());
    assert!(app.file_match.len() <= app.entries.len());
    if let Some(t) = &app.tree {
        // A tree lags a streaming scan: entries arrive every batch, the canvas
        // is rebuilt on a slower cadence. Trailing is fine — what would crash is
        // a tree holding positions for files that no longer exist, which is the
        // stale-tab case this guards.
        assert!(
            t.file_pos.len() <= app.entries.len(),
            "tree holds {} positions for {} entries",
            t.file_pos.len(),
            app.entries.len()
        );
        if !app.tree_dirty && app.scan_ui.is_none() && app.tree_build_rx.is_none() {
            assert_eq!(
                t.file_pos.len(),
                app.entries.len(),
                "settled tree does not cover every entry"
            );
        }
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

    // Switching back restores the parked workspace (quiet refresh may run).
    h.app.switch_tab(0);
    h.pump_until_idle();
    assert_eq!(h.app.root.as_ref(), Some(&root));
    assert_eq!(h.app.entries.len(), 12);
}

/// Warm jobs deliberately carry no pixels. When one answers for a card that an
/// on-demand request is already waiting on, nothing else will ever ask again —
/// the paint pass only re-requests `NotAsked`/`HasColor` — so the card has to be
/// released here or it stays blank for the life of the tab.
#[test]
fn a_warm_result_releases_a_card_that_was_waiting_on_pixels() {
    let mut h = Harness::new("warm_release");
    let root = make_tree(&h._base.join("warm_proj"), 3);
    h.app.set_root(root);
    h.pump_until_idle();
    assert!(!h.app.entries.is_empty());

    // Let the app's own request for this card resolve first, so the warm job is
    // the only result still in flight when we set the scenario up.
    let settle = Instant::now() + Duration::from_secs(10);
    while !matches!(
        h.app.thumb_state[0],
        ThumbState::Failed | ThumbState::Loaded
    ) && Instant::now() < settle
    {
        h.frame();
        std::thread::sleep(Duration::from_millis(5));
    }

    let e = &h.app.entries[0];
    let req = ThumbRequest {
        id: 0,
        generation: h.app.generation,
        path: e.path.clone(),
        key: h.app.entry_key(e),
        color_only: false,
        shared_dir: None,
        src_bytes: e.size,
        pdf_page: None,
    };
    // The stranding case: waiting on pixels, with nothing on screen to draw.
    h.app.textures.remove(&0);
    h.app.thumb_state[0] = ThumbState::AskedFull;
    h.app.thumbs.request_warm(req);

    // Released, the card re-requests and resolves on its own; stranded, it stays
    // in `AskedFull` with no texture no matter how long the loop runs.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !h.app.textures.contains_key(&0) && Instant::now() < deadline {
        h.frame();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        h.app.textures.contains_key(&0),
        "a warm result left the card stranded in {:?} with nothing to draw",
        h.app.thumb_state[0]
    );
}

/// Downloading cloud files is opt-in, and the opt-in must not be reachable by
/// accident: asking for the plan may only ever *count*, and on a tree of ordinary
/// local files there is nothing to count, so no window should appear.
#[test]
fn planning_a_cloud_download_finds_nothing_in_a_local_folder() {
    let mut h = Harness::new("cloud_plan");
    let root = make_tree(&h._base.join("cloud_proj"), 5);
    h.app.set_root(root);
    h.pump_until_idle();
    assert_eq!(h.app.entries.len(), 5);

    h.app.plan_cloud_download();
    assert!(
        h.app.cloud_plan.is_none(),
        "local files were offered up as a download"
    );
    assert!(h.app.cloud_dl.is_none(), "nothing should be fetching");
    assert_eq!(h.app.cloud_remaining(), 0);

    // And the confirmation window stays away.
    h.frame();
    assert!(h.app.cloud_plan.is_none());
}

/// Discovery deliberately ships entries without an owner, so the deferred pass
/// is the only thing that ever populates the owner filter facet. If this wiring
/// breaks, the facet silently stays empty forever.
#[test]
fn deferred_owner_pass_fills_the_facet_after_the_scan() {
    let mut h = Harness::new("owners");
    let root = make_tree(&h._base.join("proj_owner"), 9);

    h.app.set_root(root.clone());
    h.pump_until_idle();
    assert_eq!(h.app.entries.len(), 9);

    // Pump until the pass reports Done, which is when the facet is complete.
    let deadline = Instant::now() + Duration::from_secs(20);
    while h.app.owner_handle.is_some() {
        h.frame();
        assert!(Instant::now() < deadline, "owner pass never finished");
        std::thread::sleep(Duration::from_millis(5));
    }

    // Only Windows has owners to resolve; elsewhere the pass correctly finds
    // none, and the meaningful assertion is just that it terminated.
    #[cfg(windows)]
    {
        assert!(
            h.app.entries.iter().all(|e| !e.owner.is_empty()),
            "every entry should have an owner once the pass is done"
        );
        assert!(
            !h.app.all_owners.is_empty(),
            "the owner filter facet should have been recounted"
        );
        let counted: usize = h.app.all_owners.values().sum();
        assert_eq!(counted, 9, "facet counts must cover every entry");
    }

    // A refresh must not throw the workspace away just because the fresh walk
    // reports empty owners: owner is enrichment, not identity.
    let owners_before: Vec<String> = h.app.entries.iter().map(|e| e.owner.clone()).collect();
    h.app.new_tab();
    h.frame();
    h.app.switch_tab(0);
    h.pump_until_idle();
    assert_eq!(h.app.entries.len(), 9);
    let owners_after: Vec<String> = h.app.entries.iter().map(|e| e.owner.clone()).collect();
    assert_eq!(
        owners_before, owners_after,
        "a tab-switch refresh must preserve resolved owners"
    );
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
    h.app.close_tab(0); // last tab → home, no tabs in the strip
    h.pump_until_idle();
    assert!(h.app.tabs.is_empty());
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
    tx.send(Some(vec![root_b.clone()])).unwrap();
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
    tx.send(Some(vec![root_b])).unwrap();
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
        assign_state: AssignState {
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

/// Put the app back into "a scan is still streaming" without a real slow disk.
fn pretend_scanning(app: &mut AtlasApp) {
    app.scan_ui = Some(ScanUi {
        mode: ScanMode::Fresh,
        started: Instant::now(),
    });
}

/// A folder the user is looking at must not re-collapse itself just because the
/// scan is still bringing files in.
///
/// `Tree::build` re-derives the default collapse from counts that grow during a
/// load, so a folder that was expanded at 20 files used to slam shut on the
/// rebuild that took it past 300.
#[test]
fn a_folder_stays_expanded_while_the_scan_keeps_arriving() {
    let mut h = Harness::new("collapse_stable");
    let root = h._base.join("stable");
    std::fs::create_dir_all(root.join("deep")).unwrap();
    for i in 0..20 {
        std::fs::write(root.join(format!("deep/f{i}.jpg")), b"x").unwrap();
    }
    h.app.set_root(root.clone());
    h.pump_until_idle();

    let di = h
        .app
        .tree
        .as_ref()
        .unwrap()
        .dirs
        .iter()
        .position(|d| d.rel == "deep")
        .expect("deep folder in tree") as u32;
    if h.app.tree.as_ref().unwrap().dirs[di as usize].collapsed {
        h.app.toggle_dir(di, DirGrip::Incremental);
    }
    assert!(!h.app.tree.as_ref().unwrap().dirs[di as usize].collapsed);

    // The scan keeps going and pushes the folder well past every default rule.
    pretend_scanning(&mut h.app);
    let batch: Vec<FileEntry> = (0..500)
        .map(|i| {
            FileEntry::from_rel(
                &root,
                format!("deep\\extra_{i}.jpg"),
                10,
                1_700_000_000,
                1_700_000_000,
                String::new(),
            )
        })
        .collect();
    h.app
        .scan_tx
        .send((h.app.generation, ScanMsg::Batch(batch)))
        .unwrap();
    h.frame();
    h.app.scan_ui = None;
    h.app.rebuild_tree(false);
    h.frame();

    let t = h.app.tree.as_ref().unwrap();
    let d = t.dirs.iter().find(|d| d.rel == "deep").unwrap();
    assert!(
        !d.collapsed,
        "the folder re-collapsed itself while the scan was still running"
    );
    assert!(
        d.desc_files > 300,
        "fixture must cross the auto-collapse rule"
    );
}

/// Expanding a folder while a big root is rebuilding in the background must
/// stick. The build carried its own snapshot of collapse state, so landing it
/// used to throw away whatever the user had just done.
#[test]
fn expanding_during_a_background_build_is_not_undone() {
    let mut h = Harness::new("collapse_race");
    let root = make_tree(&h._base.join("race"), 4);
    h.app.set_root(root.clone());
    h.pump_until_idle();

    // Past the async threshold, so rebuilds go to a background thread.
    pretend_scanning(&mut h.app);
    h.app
        .scan_tx
        .send((
            h.app.generation,
            ScanMsg::Batch(synth_batch(&root, 0, ASYNC_TREE_THRESHOLD + 100, 6)),
        ))
        .unwrap();
    h.frame();
    h.app.rebuild_tree(false);
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.app.tree_build_rx.is_some() {
        h.frame();
        assert!(Instant::now() < deadline, "first build never landed");
        std::thread::sleep(Duration::from_millis(5));
    }

    // Start another build, then expand a folder while it is in flight.
    h.app.rebuild_tree(false);
    assert!(h.app.tree_build_rx.is_some(), "build should be async here");
    let di = h
        .app
        .tree
        .as_ref()
        .unwrap()
        .dirs
        .iter()
        .position(|d| d.rel == "sub_000")
        .expect("sub_000 in tree") as u32;
    if h.app.tree.as_ref().unwrap().dirs[di as usize].collapsed {
        h.app.toggle_dir(di, DirGrip::Incremental);
    }
    assert!(!h.app.tree.as_ref().unwrap().dirs[di as usize].collapsed);

    let deadline = Instant::now() + Duration::from_secs(30);
    while h.app.tree_build_rx.is_some() {
        h.frame();
        assert!(Instant::now() < deadline, "second build never landed");
        std::thread::sleep(Duration::from_millis(5));
    }
    h.app.scan_ui = None;

    let t = h.app.tree.as_ref().unwrap();
    let d = t.dirs.iter().find(|d| d.rel == "sub_000").unwrap();
    assert!(
        !d.collapsed,
        "the background build undid the folder the user just opened"
    );
}

/// Synthetic scan batches, shaped like a real folder: several families, spread
/// over subfolders, with timestamps spanning years so the activity timeline has
/// real work to do.
///
/// Injected through the app's own scan channel rather than written to disk. The
/// point is to model a *slow* scan — batches landing over hundreds of frames,
/// the way an SMB or OneDrive root arrives — which a local NVMe corpus finishes
/// too fast to reproduce.
fn synth_batch(root: &Path, start: usize, count: usize, subdirs: usize) -> Vec<FileEntry> {
    (start..start + count)
        .map(|i| {
            let ext = match i % 4 {
                0 => "jpg",
                1 => "pdf",
                2 => "mp4",
                _ => "3dm",
            };
            let rel = format!("sub_{:03}\\file_{i}.{ext}", i % subdirs);
            FileEntry::from_rel(
                root,
                rel,
                1024 + i as u64,
                1_600_000_000 + (i as i64 % 900) * SECS_PER_DAY,
                1_600_000_000 + (i as i64 % 900) * SECS_PER_DAY,
                String::new(),
            )
        })
        .collect()
}

/// The streaming path folds batches into the filter aggregates instead of
/// re-examining every file. That is only safe if it lands on exactly the answer
/// the full recompute would have given — otherwise counts drift as a folder
/// loads and nobody notices until the numbers are wrong.
#[test]
fn folding_in_a_batch_matches_a_full_recompute() {
    let mut h = Harness::new("absorb_parity");
    let root = make_tree(&h._base.join("parity"), 6);
    h.app.set_root(root.clone());
    h.pump_until_idle();

    // A live filter, so the match vector is a real mix rather than all-true.
    h.app.search = "file_1".into();
    h.app.recompute_matches();

    pretend_scanning(&mut h.app);
    for chunk in 0..4 {
        h.app
            .scan_tx
            .send((
                h.app.generation,
                ScanMsg::Batch(synth_batch(&root, chunk * 300, 300, 7)),
            ))
            .unwrap();
        h.frame();
    }
    h.app.scan_ui = None;

    assert!(
        !h.app.filter_dirty,
        "a full recompute was still pending, so this proves nothing"
    );
    assert!(h.app.shown_count > 0 && h.app.shown_count < h.app.alive_count);

    let streamed = (
        h.app.alive_count,
        h.app.shown_count,
        h.app.total_bytes,
        h.app.shown_bytes,
        h.app.date_span_lo,
        h.app.date_span_hi,
        h.app.file_match.clone(),
        h.app.all_owners.clone(),
    );
    h.app.recompute_matches();
    let recomputed = (
        h.app.alive_count,
        h.app.shown_count,
        h.app.total_bytes,
        h.app.shown_bytes,
        h.app.date_span_lo,
        h.app.date_span_hi,
        h.app.file_match.clone(),
        h.app.all_owners.clone(),
    );
    assert_eq!(streamed.0, recomputed.0, "alive count drifted");
    assert_eq!(streamed.1, recomputed.1, "shown count drifted");
    assert_eq!(streamed.2, recomputed.2, "total bytes drifted");
    assert_eq!(streamed.3, recomputed.3, "shown bytes drifted");
    assert_eq!(streamed.4, recomputed.4, "date span start drifted");
    assert_eq!(streamed.5, recomputed.5, "date span end drifted");
    assert_eq!(streamed.6, recomputed.6, "per-file matches drifted");
    assert_eq!(streamed.7, recomputed.7, "owner tallies drifted");
}

/// The readout froze during loads because the count was only recomputed a few
/// times a second on a busy frame loop. It has to move with every batch.
#[test]
fn the_file_count_keeps_up_with_every_batch() {
    let mut h = Harness::new("live_count");
    let root = make_tree(&h._base.join("counting"), 6);
    h.app.set_root(root.clone());
    h.pump_until_idle();

    pretend_scanning(&mut h.app);
    for chunk in 0..5 {
        h.app
            .scan_tx
            .send((
                h.app.generation,
                ScanMsg::Batch(synth_batch(&root, chunk * 200, 200, 5)),
            ))
            .unwrap();
        h.frame();
        assert_eq!(
            h.app.alive_count,
            h.app.entries.len(),
            "count fell behind after batch {chunk}"
        );
    }
    h.app.scan_ui = None;
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

/// Frame-time distribution while a big folder streams in — the "jittering
/// panning and zooming while loading" report, as numbers.
///
/// 60fps is a 16.7 ms budget (Constitution Art. II). What matters is not the
/// mean but the tail: one 300 ms frame in a second of panning is what the hand
/// feels, and an average hides it completely.
///
/// ```powershell
/// cargo test -p native-file-atlas --release load_jitter -- --ignored --nocapture
/// ```
#[test]
#[ignore = "benchmark"]
fn load_jitter_benchmark() {
    let files: usize = std::env::var("ATLAS_BENCH_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40_000);
    const BATCH: usize = 512;
    // ATLAS_BENCH_LEGACY=1 restores the per-batch whole-corpus work, for a
    // before/after on the same machine.
    let legacy = std::env::var("ATLAS_BENCH_LEGACY").is_ok();
    let mut h = Harness::new("jitter");

    // A real (tiny) root so the tree, index, and key prefix machinery are live.
    let root = make_tree(&h._base.join("bench"), 4);
    h.app.set_root(root.clone());
    h.pump_until_idle();

    // Now pretend the scan is still running and slow.
    h.app.scan_ui = Some(ScanUi {
        mode: ScanMode::Fresh,
        started: Instant::now(),
    });

    let mut frames: Vec<f64> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    let started = Instant::now();
    let mut sent = 0usize;
    while sent < files {
        let batch = synth_batch(&root, sent, BATCH.min(files - sent), 40);
        sent += batch.len();
        h.app
            .scan_tx
            .send((h.app.generation, ScanMsg::Batch(batch)))
            .unwrap();
        if legacy {
            // What a batch used to cost: a full filter recompute plus a rebuilt
            // timeline index and folder-heat map, every batch. Kept so the
            // regression is measurable, not just asserted.
            h.app.filter_dirty = true;
            h.app.heatmap_data_rev = h.app.heatmap_data_rev.wrapping_add(1);
        }
        // One frame per batch, panning — a hand on the canvas while it loads.
        frames.push(h.timed_frame(vec![egui::Event::PointerMoved(Pos2::new(
            400.0 + (frames.len() % 50) as f32,
            400.0,
        ))]));
        counts.push(h.app.entries.len());
        // Real frames are ~30ms apart. Without this the whole stream finishes
        // inside one 700ms tree-rebuild window, so the rebuild never fires and
        // the measurement misses the most expensive thing in the loop.
        std::thread::sleep(Duration::from_millis(25));
    }
    // Let the throttled rebuild land so the tree is full-size for what follows.
    let settle = Instant::now() + Duration::from_secs(60);
    while h.app.tree_dirty || h.app.tree_build_rx.is_some() {
        h.timed_frame(Vec::new());
        assert!(Instant::now() < settle, "tree never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
    h.app.scan_ui = None;
    let wall = started.elapsed().as_secs_f64();

    let mut sorted = frames.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let over = frames.iter().filter(|&&m| m > 16.7).count();
    println!(
        "mode: {}",
        if legacy {
            "LEGACY (per-batch full recompute)"
        } else {
            "current"
        }
    );
    println!(
        "streamed {} files in {} batches over {} frames, wall {:.1}s",
        group_digits(sent as u64),
        group_digits((sent / BATCH) as u64),
        frames.len(),
        wall
    );
    println!(
        "frame ms: p50={:.1} p95={:.1} p99={:.1} max={:.1}",
        percentile(&sorted, 0.50),
        percentile(&sorted, 0.95),
        percentile(&sorted, 0.99),
        sorted.last().copied().unwrap_or(0.0)
    );
    println!(
        "over the 16.7ms budget: {over} of {} frames ({:.0}%)",
        frames.len(),
        100.0 * over as f64 / frames.len() as f64
    );
    // Where it goes bad: cost against entry count, which is the quadratic tell.
    for q in [0.25f64, 0.5, 0.75, 1.0] {
        let i = ((frames.len() - 1) as f64 * q) as usize;
        println!(
            "  at {:>7} entries: {:.1}ms",
            group_digits(counts[i] as u64),
            frames[i]
        );
    }

    // Per-pass cost at full size, with a full tree — this is what one scan
    // batch buys you, and every batch buys it again.
    println!(
        "with {} entries in the tree:",
        group_digits(h.app.entries.len() as u64)
    );
    let t = Instant::now();
    h.app.recompute_matches();
    println!(
        "  recompute_matches:    {:.1}ms",
        t.elapsed().as_secs_f64() * 1000.0
    );

    let t = Instant::now();
    h.app.recount_owners();
    println!(
        "    recount_owners:     {:.1}ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let t = Instant::now();
    h.app.update_date_span();
    println!(
        "    update_date_span:   {:.1}ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let t = Instant::now();
    if let Some(tr) = &mut h.app.tree {
        tr.refresh_matches(&h.app.file_match);
        tr.layout_filtered(h.app.orient, false, &h.app.file_match, h.app.structure_only);
    }
    println!(
        "    tree layout:        {:.1}ms",
        t.elapsed().as_secs_f64() * 1000.0
    );

    h.app.heatmap_data_rev = h.app.heatmap_data_rev.wrapping_add(1);
    let t = Instant::now();
    h.app.ensure_activity_index();
    println!(
        "  ensure_activity_index: {:.1}ms",
        t.elapsed().as_secs_f64() * 1000.0
    );

    let t = Instant::now();
    h.app.rebuild_tree(false);
    println!(
        "  rebuild_tree:          {:.1}ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
}

#[test]
fn multi_folder_open_shares_one_canvas() {
    let mut h = Harness::new("multi_folder");
    let master = h._base.join("Master");
    let a = make_tree(&master.join("A"), 3);
    let b = make_tree(&master.join("B"), 4);
    let _c = make_tree(&master.join("C"), 5); // unselected sibling

    h.app.set_roots(vec![a.clone(), b.clone()]);
    h.pump_until_idle();

    assert_eq!(h.app.root.as_ref(), Some(&master));
    assert_eq!(h.app.scan_seeds, vec![a, b]);
    assert_eq!(h.app.entries.len(), 7, "only A + B files");
    assert!(h.app.entries.iter().all(|e| {
        e.rel.starts_with("A\\")
            || e.rel.starts_with("B\\")
            || e.rel.starts_with("A/")
            || e.rel.starts_with("B/")
    }));
    assert_eq!(h.app.tabs[0].title(), "A +1");
}
