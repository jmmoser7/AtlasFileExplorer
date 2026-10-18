//! Ignored machine-local command-cost probe, not a renderer or frame-rate claim.
//! Image locators are metadata only: no image file is created, opened or decoded.
use super::*;
use crate::app::dispatch::PreparedBoardCommand;
use serde_json::Value;
use slate_doc::scene::{CmdAuthor, ImageNode, NodeId, NodeKind, Scene, SceneJournal, WorldRect};
use std::{hint::black_box, time::Instant};

const REPEATS: usize = 21;

#[derive(Clone, Copy)]
enum Case {
    Move(usize),
    Place,
    Frame,
    Train,
}
impl Case {
    fn command(self) -> &'static str {
        match self {
            Self::Move(_) => "board.objects.move",
            Self::Place => "board.objects.place_images",
            Self::Frame => "board.objects.create_frame",
            Self::Train => "board.objects.create_train",
        }
    }
    fn count(self) -> usize {
        match self {
            Self::Move(n) => n,
            Self::Place => 64,
            _ => 0,
        }
    }
    fn args(self) -> Value {
        match self {
            Self::Move(_) => json!({"dx":73,"dy":29}),
            Self::Place => json!({"x":100000,"y":100000}),
            _ => json!({"x":100000,"y":100000,"width":960,"height":540}),
        }
    }
}

struct Fixture {
    h: Harness,
    scene: Scene,
    authored: slate_doc::SlateDoc,
    serialized: Value,
    ids: Vec<NodeId>,
}
impl Fixture {
    fn new(count: usize) -> Self {
        let mut h = host();
        h.app.ensure_work_tab();
        add(&mut h, "Probe");
        let items: Vec<_> = (0..count.min(128))
            .map(|n| {
                h.app.doc_mut().add_item(
                    h.base.join(format!("metadata-only-{n}.png")),
                    format!("metadata-only-{n}.png"),
                    1024,
                    42,
                    format!("metadata-only-{n}"),
                )
            })
            .collect();
        let mut allocator = h.app.doc().scene.node_preparation();
        let nodes = (0..count)
            .map(|n| {
                allocator.build_node(
                    WorldRect::new(
                        (n % 100) as f32 * 200.0,
                        (n / 100) as f32 * 150.0,
                        120.0,
                        80.0,
                    ),
                    NodeKind::Image(ImageNode::new(items[n % items.len()])),
                )
            })
            .collect();
        let ids = h.app.add_nodes(nodes);
        assert_eq!(ids.len(), count);
        let serialized = serde_json::to_value(&h.app.doc().scene).unwrap();
        let scene = serde_json::from_value(serialized.clone()).unwrap();
        let authored = h.app.doc().clone();
        Self {
            h,
            scene,
            authored,
            serialized,
            ids,
        }
    }
    fn reset(&mut self, warm: bool) {
        // Deserialization resets derived caches without copying or emulating their owners.
        let scene: Scene = serde_json::from_value(self.serialized.clone()).unwrap();
        assert_eq!(scene.spatial_rebuilds(), 0);
        if warm {
            black_box(scene.query_rect(scene.visible_bounds().unwrap()));
            black_box(scene.node(self.ids[0]));
            assert_eq!(scene.spatial_rebuilds(), 1);
        }
        let tab = self.h.app.tab_mut();
        tab.doc = self.authored.clone();
        tab.doc.scene = scene;
        tab.journal = SceneJournal::default();
        tab.edits.clear();
        tab.edit_redo.clear();
        self.h.app.board_sel.clear();
        self.h.app.cmd_history = atlas_commands::History::new();
    }
    fn targets(&self, case: Case) -> Vec<String> {
        self.ids
            .iter()
            .take(case.count())
            .map(|id| id.0.to_string())
            .collect()
    }
    fn prepare(&self, case: Case, targets: &[String], args: &Value) -> PreparedBoardCommand {
        self.h
            .app
            .prepare_targeted_command(case.command(), targets, args)
            .unwrap()
    }
    fn assert_unmodified(&self, items: &[slate_doc::SlateItem]) {
        assert_eq!(self.h.app.doc().scene, self.scene);
        assert_eq!(self.h.app.doc(), &self.authored);
        assert_eq!(self.h.app.doc().items, items);
        assert!(self.h.app.board_sel.is_empty());
        assert_eq!(self.h.app.tab().journal.undo_depth(), 0);
    }
}

#[derive(Default)]
struct Samples {
    ns: Vec<u128>,
    spatial_rebuilds: Vec<u64>,
}
impl Samples {
    fn measure<T>(&mut self, scene: &Scene, op: impl FnOnce() -> T) -> T {
        let rebuilds = scene.spatial_rebuilds();
        let start = Instant::now();
        let result = black_box(op());
        self.ns.push(start.elapsed().as_nanos());
        self.spatial_rebuilds
            .push(scene.spatial_rebuilds() - rebuilds);
        result
    }
    fn record(&mut self, ns: u128, rebuilds: u64) {
        self.ns.push(ns);
        self.spatial_rebuilds.push(rebuilds);
    }
    fn print(&self, count: usize, case: Case, warm: bool, phase: &str, expanded: usize) {
        assert_eq!(self.ns.len(), REPEATS);
        let mut sorted = self.ns.clone();
        sorted.sort_unstable();
        println!(
            "{}",
            json!({
                "probe":"presence-command-machine-local-v1", "command":case.command(),
                "scene_nodes":count, "image_nodes":count, "input_targets":case.count(),
                "prepared_targets":expanded, "phase":phase, "samples":self.ns.len(),
                "cache_condition":if warm { "scene_spatial_and_id_warm" } else { "scene_derived_cold" },
                "unit":"ns", "first":self.ns[0], "median":sorted[sorted.len()/2],
                "p95":sorted[(sorted.len()*95).div_ceil(100)-1], "max":sorted[sorted.len()-1],
                "raw":self.ns, "spatial_rebuilds":self.spatial_rebuilds,
                "timing_scope":"CPU command owners; fixture/reset/assertions/undo excluded"
            })
        );
    }
}

fn benchmark(f: &mut Fixture, case: Case, warm: bool) {
    let targets = f.targets(case);
    let args = case.args();
    let items = f.h.app.doc().items.clone();
    let mut prepare = Samples::default();
    let mut applicable = Samples::default();
    let mut expected = Samples::default();
    let mut finish = Samples::default();
    let mut expanded = 0;
    for _ in 0..REPEATS {
        f.reset(warm);
        let prepared = prepare.measure(&f.h.app.doc().scene, || f.prepare(case, &targets, &args));
        expanded = prepared.targets.len();
        f.assert_unmodified(&items);

        f.reset(warm);
        applicable
            .measure(&f.h.app.doc().scene, || {
                prepared.applicable(&f.h.app.doc().scene)
            })
            .unwrap();
        f.assert_unmodified(&items);

        f.reset(warm);
        expected
            .measure(&f.h.app.doc().scene, || {
                prepared.expected_matches(&f.h.app.doc().scene)
            })
            .unwrap();
        f.assert_unmodified(&items);

        let mut after = f.scene.clone();
        assert!(after.apply_all(&prepared.cmds));
        let mut after_doc = f.authored.clone();
        after_doc.scene = after.clone();
        let created = prepared.created_ids();
        f.reset(warm);
        let rebuilds = f.h.app.doc().scene.spatial_rebuilds();
        let start = Instant::now();
        let outcome =
            black_box(
                f.h.app
                    .finish_targeted_command(prepared, "Probe", true, "perf-session"),
            );
        let ns = start.elapsed().as_nanos();
        finish.record(ns, f.h.app.doc().scene.spatial_rebuilds() - rebuilds);
        assert!(outcome.unwrap().starts_with("applied:"));
        assert_eq!(f.h.app.doc().scene, after);
        assert_eq!(f.h.app.doc(), &after_doc);
        assert_authored_result(f, case, &created);
        assert_eq!(f.h.app.doc().items, items);
        assert_eq!(f.h.app.tab().journal.undo_depth(), 1);
        assert_eq!(
            f.h.app.tab().journal.last_author(),
            Some(&CmdAuthor::Agent("Probe".into()))
        );
        let tab = f.h.app.tab_mut();
        assert!(tab.journal.undo(&mut tab.doc.scene));
        assert_eq!(tab.doc.scene.nodes, f.scene.nodes);
        assert_eq!(tab.journal.undo_depth(), 0);
    }
    for (phase, samples) in [
        ("prepare", prepare),
        ("applicable", applicable),
        ("expected_matches", expected),
        ("finish", finish),
    ] {
        samples.print(f.scene.nodes.len(), case, warm, phase, expanded);
    }
    if case.count() > 0 {
        stale_refusal(f, case, &targets, &args, warm);
    }
}

fn assert_authored_result(f: &Fixture, case: Case, created: &[NodeId]) {
    let scene = &f.h.app.doc().scene;
    match case {
        Case::Move(count) => {
            assert!(created.is_empty());
            for id in f.ids.iter().take(count) {
                assert_eq!(
                    scene.node(*id).unwrap().rect,
                    f.scene.node(*id).unwrap().rect.translated(73.0, 29.0)
                );
            }
        }
        Case::Place => {
            assert_eq!(created.len(), 64);
            for (new, source) in created.iter().zip(&f.ids) {
                let node = scene.node(*new).unwrap();
                let NodeKind::Image(image) = &node.kind else {
                    panic!("placed image required")
                };
                let NodeKind::Image(old) = &f.scene.node(*source).unwrap().kind else {
                    panic!("linked source required")
                };
                assert_eq!(image.item, old.item);
                assert!(node.rect.x > 50000.0 && node.rect.y > 50000.0);
                assert!((node.rect.w / node.rect.h - 1.5).abs() < 0.001);
            }
        }
        Case::Frame | Case::Train => {
            assert_eq!(created.len(), 1);
            let node = scene.node(created[0]).unwrap();
            assert_eq!(node.rect, WorldRect::new(100000.0, 100000.0, 960.0, 540.0));
            if matches!(case, Case::Frame) {
                assert!(node.is_frame());
            } else {
                let agent = slate_doc::agent_chat::agent(node).unwrap();
                assert!(agent.chat.train);
                assert_eq!(agent.provider, "codex");
                assert!(!agent.live);
            }
        }
    }
}

fn stale_refusal(f: &mut Fixture, case: Case, targets: &[String], args: &Value, warm: bool) {
    f.reset(warm);
    let prepared = f.prepare(case, targets, args);
    f.h.app.patch_nodes(&[f.ids[0]], |node| node.rect.y += 5.0);
    let human_scene = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    assert!(prepared.expected_matches(&f.h.app.doc().scene).is_err());
    assert!(prepared.applicable(&f.h.app.doc().scene).is_err());
    assert!(f
        .h
        .app
        .finish_targeted_command(prepared, "Probe", true, "perf-session")
        .is_err());
    assert_eq!(f.h.app.doc().scene, human_scene);
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
}

fn admission(f: &mut Fixture, warm: bool) {
    let case = Case::Move(64);
    let mut samples = Samples::default();
    for n in 0..REPEATS {
        f.reset(warm);
        let items = f.h.app.doc().items.clone();
        let mut agent = f.h.app.presence.agents.remove(0);
        agent.request = format!("perf-admission-{}-{warm}-{n}", f.scene.nodes.len());
        agent.calls = 0;
        agent.sequences = 0;
        agent.work = None;
        agent.grant.as_mut().unwrap().mutation_autonomy = true;
        f.h.app
            .presence
            .roster
            .session_mut(&agent.motion)
            .unwrap()
            .begin_request(id(&agent.request).unwrap())
            .unwrap();
        let call = BoardCall {
            id: format!("perf-{n}"),
            session: agent.binding.clone(),
            request_id: agent.request.clone(),
            sequence: 0,
            operation: BoardOperation::Invoke {
                command: case.command().into(),
                targets: f.targets(case),
                arguments: case.args(),
            },
        };
        let rebuilds = f.h.app.doc().scene.spatial_rebuilds();
        let start = Instant::now();
        let reply = black_box(f.h.app.presence_call(&mut agent, &call));
        let ns = start.elapsed().as_nanos();
        samples.record(ns, f.h.app.doc().scene.spatial_rebuilds() - rebuilds);
        assert!(reply.is_none(), "{reply:?}");
        assert_eq!(agent.calls, 1);
        let work = agent
            .work
            .take()
            .expect("admitted action must have its real witness");
        assert_eq!(work.prepared.targets.len(), 64);
        assert_eq!(work.steps.len(), 64);
        f.assert_unmodified(&items);
        f.h.app.presence.agents.insert(0, agent);
    }
    samples.print(
        f.scene.nodes.len(),
        case,
        warm,
        "presence_call_admission",
        64,
    );
}

#[test]
#[ignore = "machine-local release performance probe; run alone after compilation with --nocapture"]
fn witnessed_command_machine_local_performance_probe() {
    assert!(
        !cfg!(debug_assertions),
        "run the normal release test harness"
    );
    println!(
        "{}",
        json!({
            "probe":"presence-command-machine-local-v1", "repeats":REPEATS,
        "os":std::env::consts::OS, "arch":std::env::consts::ARCH,
        "pid":std::process::id(), "test_executable":std::env::current_exe().ok(),
            "fixture":"In-memory linked ImageNodes; at most 128 distinct registered image items; no source files read",
            "boundary":"Harness creates its temporary workspace. Timed production owners retain normal bookkeeping. No provider or paint/frame loop is run.",
            "frame_validation":"Native session logs and existing witnessed-motion correctness tests remain required",
            "move128_boundary":"Typed command owner accepts 128; file-mailbox targets stay limited to 64, so admission probes only move64"
        })
    );
    for count in [100, 1000, 10000] {
        let mut fixture = Fixture::new(count);
        for warm in [false, true] {
            benchmark(&mut fixture, Case::Move(64), warm);
            if count >= 128 {
                benchmark(&mut fixture, Case::Move(128), warm);
            }
            benchmark(&mut fixture, Case::Place, warm);
            benchmark(&mut fixture, Case::Frame, warm);
            benchmark(&mut fixture, Case::Train, warm);
            admission(&mut fixture, warm);
        }
    }
}
