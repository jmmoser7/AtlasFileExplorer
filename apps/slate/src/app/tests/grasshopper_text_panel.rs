//! Exercise the canvas input route, native solver, and workbook restoration together.
use super::*;
use crate::app::tests::{painted, pointer_to, press_key, press_key_with, primary_button, Harness};
use egui::{Key, Pos2};
use slate_doc::scene::{
    grasshopper_wire, GrasshopperPort, GrasshopperPortAccess, GrasshopperPortDirection,
    GrasshopperPortType, Side,
};

const DEFINITION: &str = "../../design/agent-interaction-studio/native-implementation/examples/linked-grasshopper/01 Program.ghx";
const OUT_IDS: [&str; 3] = [
    "541a0000-1000-4000-8000-010000000005",
    "541a0000-1000-4000-8000-010000000007",
    "541a0000-1000-4000-8000-010000000009",
];

struct Study {
    h: Harness,
    source: NodeId,
    targets: [NodeId; 3],
    note: PathBuf,
    workbook: PathBuf,
}

impl Study {
    fn new(tag: &str) -> Self {
        let mut h = Harness::new(tag);
        h.app.home_new_workspace();
        let workbook = h.base.join("Panel study.slate");
        h.app.save_doc_to(h.app.tab().id, workbook.clone());
        let definition = h.base.join("01 Program.ghx");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(DEFINITION),
            &definition,
        )
        .unwrap();
        let file = h.app.add_paths(&[definition])[0];
        h.app.place_items_on_board(&[file], Pos2::ZERO);
        let source = *h.app.board_sel.iter().next().unwrap();
        let mut inline = |words: &str| {
            h.app.place_text_at(Pos2::ZERO);
            h.app.text_box_draft.as_mut().unwrap().buffer = words.into();
            h.app.commit_text_box_draft();
            *h.app.board_sel.iter().next().unwrap()
        };
        let width = inline("Authored width note");
        let profiles = inline("Authored profiles note");
        let note = h.base.join("Samples note.txt");
        std::fs::write(&note, "Authored samples note").unwrap();
        let item = h.app.add_paths(std::slice::from_ref(&note))[0];
        h.app.place_items_on_board(&[item], Pos2::ZERO);
        let samples = *h.app.board_sel.iter().next().unwrap();
        for (id, rect) in [
            (source, WorldRect::new(-540., -160., 300., 255.)),
            (width, WorldRect::new(30., -270., 335., 132.)),
            (samples, WorldRect::new(30., -93., 335., 145.)),
            (profiles, WorldRect::new(30., 98., 335., 210.)),
        ] {
            h.app.patch_nodes(&[id], |n| n.rect = rect);
        }
        h.app.board_sel.clear();
        h.app.tab_mut().cam.z = 1.0;
        h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
        h.app.request_grasshopper(source, &h.ctx);
        Self {
            h,
            source,
            targets: [width, samples, profiles],
            note,
            workbook,
        }
    }

    fn drag(&mut self, index: usize, drop_on_edge: bool) -> NodeId {
        let h = &mut self.h;
        let source_ports: Vec<_> = h
            .app
            .grasshopper_ports(self.source)
            .iter()
            .filter(|p| p.direction == GrasshopperPortDirection::Output)
            .collect();
        let row = source_ports
            .iter()
            .position(|p| p.id.as_str() == OUT_IDS[index])
            .expect("the actual definition exposes the expected named output");
        let target = self.targets[index];
        let grip = |id, side, row| {
            let n = h.app.doc().scene.node(id).unwrap();
            let world =
                h.app
                    .wire_port_point(n, side, slate_doc::wire_host::grasshopper_port_t(row));
            h.app.world_to_screen(world)
        };
        let start = grip(self.source, Side::Right, row);
        let node = h.app.doc().scene.node(target).unwrap();
        let end = if drop_on_edge {
            h.app.world_to_screen(h.app.wire_port_point(node, Side::Left, 0.5))
        } else {
            h.app.world_to_screen(Pos2::from(node.rect.center()))
        };
        let before = h.app.tab().journal.undo_depth();
        h.frame_with(pointer_to(start, false));
        h.frame_with(primary_button(start, true, false));
        h.frame_with(pointer_to(start.lerp(end, 0.5), false));
        h.frame_with(pointer_to(end, false));
        h.frame_with(primary_button(end, false, false));
        h.frame();
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            before + 1,
            "one wire for {start:?} → {end:?}"
        );
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .find_map(|n| {
                let NodeKind::Connector(c) = &n.kind else {
                    return None;
                };
                let w = grasshopper_wire(c)?;
                (w.source == self.source
                    && w.target == target
                    && w.source_port.as_str() == OUT_IDS[index])
                    .then_some(n.id)
            })
            .expect("gesture did not bind the requested output")
    }

    fn value(&self, index: usize) -> Option<std::sync::Arc<str>> {
        self.h.app.grasshopper_panel_text(self.targets[index])
    }

    fn wait(&mut self, predicate: impl Fn(&Self) -> bool) {
        let start = Instant::now();
        loop {
            self.h.app.grasshopper_frame(&self.h.ctx);
            if predicate(self) {
                break;
            }
            let panel = self.h.app.grasshopper_panel(self.source);
            assert!(
                !panel.as_ref().is_some_and(|p| p.can_retry),
                "native worker stopped: {:?}",
                panel.and_then(|p| p.status)
            );
            assert!(
                start.elapsed() < Duration::from_secs(850),
                "native panel solve timed out: {:?}",
                self.h.app.grasshopper_status(self.source)
            );
            std::thread::sleep(Duration::from_millis(40));
        }
    }
}

#[test]
fn dragging_a_named_grasshopper_output_reads_on_text_and_undoes_through_real_input() {
    let mut s = Study::new("gh-panel-wire-input");
    let key = (s.h.app.tab().id, s.source);
    let (requests, _work) = crossbeam_channel::unbounded();
    let (_events, results) = crossbeam_channel::unbounded();
    s.h.app.grasshopper.background = Some(Background {
        requests,
        results,
        cancel: Cancellation::default(),
        busy: Some(key),
    });
    let view = s.h.app.grasshopper.views.get_mut(&key).unwrap();
    let port = GrasshopperPort {
        id: OUT_IDS[0].parse().unwrap(),
        name: "Width".into(),
        direction: GrasshopperPortDirection::Output,
        kind: GrasshopperPortType::Number,
        access: GrasshopperPortAccess::Item,
    };
    view.ports = vec![port.clone()];
    view.due = None;
    view.outputs = Some(
        [(
            port.id,
            serde_json::json!({"branches":[{"path":[0],"items":[{"kind":"number","value":6}]}]}),
        )]
        .into_iter()
        .collect(),
    );
    s.h.frame();
    let wire = s.drag(0, false);
    assert_eq!(s.value(0).as_deref(), Some("6"));
    assert!(painted(&mut s.h, Pos2::new(700., 600.))
        .iter()
        .any(|text| text == "6"));
    s.h.app.board_sel = [wire].into_iter().collect();
    press_key(&mut s.h, Key::Delete);
    s.h.frame();
    assert!(s.value(0).is_none());
    assert!(painted(&mut s.h, Pos2::new(700., 600.))
        .iter()
        .any(|t| t == "Authored width note"));
    press_key_with(&mut s.h, Key::Z, egui::Modifiers::CTRL);
    s.h.frame();
    assert_eq!(s.value(0).as_deref(), Some("6"));
}

#[test]
#[ignore = "requires installed licensed Rhino 8 and the packaged native Grasshopper worker"]
fn native_grasshopper_panel_wires_follow_the_slider_and_reopen_with_their_text() {
    let mut s = Study::new("native-gh-text-panels");
    s.wait(|s| {
        s.h.app
            .grasshopper_ports(s.source)
            .iter()
            .any(|p| p.id.as_str() == OUT_IDS[2])
    });
    s.h.frame();
    s.drag(0, false);
    let list_wire = s.drag(1, true);
    s.drag(2, false);
    assert_eq!(s.value(0).as_deref(), Some("6"));
    assert_eq!(s.value(1).as_deref(), Some("{0;0}\n0  6\n1  9\n2  12"));
    assert_eq!(
        s.value(2).as_deref(),
        Some("{0;1}\n0  6\n1  8\n{2;4}\n0  11")
    );
    let control =
        s.h.app
            .grasshopper_panel(s.source)
            .unwrap()
            .controls
            .into_iter()
            .find(|c| c.name == "Base width")
            .unwrap();
    s.h.app.grasshopper_edit(
        s.source,
        control.id,
        GrasshopperValue::Number(8.),
        EditPhase::Commit,
    );
    s.wait(|s| s.value(0).as_deref() == Some("8"));
    assert_eq!(s.value(1).as_deref(), Some("{0;0}\n0  8\n1  12\n2  16"));
    assert_eq!(
        s.value(2).as_deref(),
        Some("{0;1}\n0  8\n1  10\n{2;4}\n0  13")
    );
    s.h.app.board_sel = [list_wire].into_iter().collect();
    press_key(&mut s.h, Key::Delete);
    s.h.frame();
    assert!(s.value(1).is_none());
    press_key_with(&mut s.h, Key::Z, egui::Modifiers::CTRL);
    s.h.frame();
    assert_eq!(s.value(1).as_deref(), Some("{0;0}\n0  8\n1  12\n2  16"));
    s.h.app.save_doc_to(s.h.app.tab().id, s.workbook.clone());
    s.h.app.force_close_tab(s.h.app.active_tab);
    s.h.app.open_doc_at(s.workbook.clone());
    let start = Instant::now();
    while s
        .h
        .app
        .tabs
        .get(s.h.app.active_tab)
        .and_then(|t| t.path.as_ref())
        != Some(&s.workbook)
    {
        s.h.app.drain_pending_workbooks(&s.h.ctx);
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "workbook did not reopen"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    s.h.frame();
    s.wait(|s| s.value(0).as_deref() == Some("8"));
    assert_eq!(
        s.value(2).as_deref(),
        Some("{0;1}\n0  8\n1  10\n{2;4}\n0  13")
    );
    assert_eq!(
        std::fs::read_to_string(&s.note).unwrap(),
        "Authored samples note"
    );
    eprintln!(
        "Reopened three native panel wires in {}",
        s.workbook.display()
    );
}
