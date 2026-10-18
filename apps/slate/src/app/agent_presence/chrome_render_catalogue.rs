//! Direct-model painter catalogue, separate from observed-event or UI evidence.
use super::super::*;
use super::support::{rect_json, Fixture, Scenario};
use eframe::egui::{self, Rect};
use serde_json::json;

pub(super) fn run() {
    let mut f = Fixture::for_scenario(Scenario::Catalogue);
    let who = f.who.clone();
    let p =
        f.h.app
            .presence
            .roster
            .session(&f.h.app.presence.agents[0].motion)
            .unwrap()
            .sample()
            .position;
    let prepared =
        f.h.app
            .prepare_targeted_command(
                "board.objects.create_shape",
                &[],
                &json!({"x":p.x-10.0,"y":p.y-10.0,"width":20,"height":20}),
            )
            .unwrap();
    let target = prepared.targets[0];
    f.h.app.commit_scene(prepared.cmds);
    if !chrome::system_reduced_motion(&f.h.ctx) {
        transitions(&mut f, target);
    } else {
        f.note_omission("OS reduced motion disables normal transition evidence; no independent-speed visual pass is claimed.");
    }
    // Representative coverage only: three distinct crown shapes plus None.
    // This is deliberately not an exhaustive 26-hat visual-fit claim.
    f.h.app.settings.presence_reduced_motion = true;
    for hat in ["none", "cap", "beanie", "wizard"] {
        f.h.app
            .apply_presence_action(
                &f.h.ctx,
                chrome::PresenceAction::SetHat {
                    id: who.clone(),
                    hat: hat.into(),
                },
            )
            .unwrap();
        for (name, dx, dy) in [
            ("forward", 0., 0.),
            ("left", -160., 0.),
            ("right", 160., 0.),
            ("up", 0., -160.),
            ("down", 0., 160.),
            ("up-left", -160., -160.),
            ("up-right", 160., -160.),
            ("down-left", -160., 160.),
            ("down-right", 160., 160.),
        ] {
            set_pose(&mut f, target, "reading", dx, dy);
            let out = f.settle();
            let center = f.board_avatar_center(&who);
            let head = super::world_opaque_head(&out, center, 1.0);
            let crown = Rect::from_min_max(
                center + egui::vec2(-32., -44.),
                center + egui::vec2(32., -8.),
            );
            let vertices = Fixture::mesh_vertices(&out, crown, false);
            assert!(!vertices.is_empty());
            f.save(
                &format!("catalogue-{hat}-{name}"),
                out,
                json!({"evidence":"direct-model catalogue through actual board painter",
                "hat":hat,"direction":name,"worldHead":rect_json(head),"crown":rect_json(crown),
                "representativeOnly":true,"reducedMotionEndpoint":true,"traceIntegration":false}),
            );
        }
    }
    // Direct-model neutral/forward fit evidence for all manually calibrated
    // hats. The canonical activity map, rather than a guessed activity name,
    // establishes the exact neutral expression with no prop or target gaze.
    let hat_source = include_str!("../../../../../crates/atlas-shell/assets/agent-hats.json");
    let pose_source =
        include_str!("../../../../../crates/atlas-agent/assets/agent-activities.json");
    let rig_source = include_str!("../../../../../crates/atlas-shell/assets/agent-avatar.json");
    let hats: serde_json::Value = serde_json::from_str(hat_source).unwrap();
    let poses: serde_json::Value = serde_json::from_str(pose_source).unwrap();
    let neutral = poses["activities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pose| pose["id"] == "idle")
        .unwrap();
    assert_eq!(neutral["expression"], "neutral");
    assert_eq!(neutral["orientation"], "forward");
    assert_eq!(neutral["prop"], "none");
    let order = hats["order"].as_array().unwrap();
    assert_eq!(order.len(), 27);
    assert_eq!(hats["hats"].as_object().unwrap().len(), 27);
    let unique: std::collections::HashSet<_> =
        order.iter().map(|value| value.as_str().unwrap()).collect();
    assert_eq!(unique.len(), 27);
    assert!(unique.contains("none"));
    let input_hashes = json!({
        "agent-hats.json":atlas_ai::versions::hash_artifact_input(hat_source.as_bytes()),
        "agent-activities.json":atlas_ai::versions::hash_artifact_input(pose_source.as_bytes()),
        "agent-avatar.json":atlas_ai::versions::hash_artifact_input(rig_source.as_bytes())});
    for value in order {
        let hat = value.as_str().unwrap();
        let definition = &hats["hats"][hat];
        if hat == "none" {
            assert!(definition["fit"].is_null());
        } else {
            assert_eq!(definition["fit"].as_object().unwrap().len(), 5);
        }
        let a = &mut f.h.app.presence.agents[0];
        a.activity = neutral["id"].as_str().unwrap().into();
        a.status = "Direct-model neutral hat-fit catalogue".into();
        a.observed_target = None;
        a.last_active = Instant::now();
        f.h.app
            .apply_presence_action(
                &f.h.ctx,
                chrome::PresenceAction::SetHat {
                    id: who.clone(),
                    hat: hat.into(),
                },
            )
            .unwrap();
        let out = f.settle();
        let a = &f.h.app.presence.agents[0];
        assert_eq!(f.h.app.presence.avatar_activity(a), "idle");
        assert_eq!(a.profile.hat, hat);
        assert!(a.observed_target.is_none());
        let center = f.board_avatar_center(&who);
        let head = super::world_opaque_head(&out, center, 1.0);
        f.save(&format!("catalogue-neutral-forward-{hat}"),out,
            json!({"evidence":"direct-model neutral-forward fit catalogue through actual app painter",
                "hat":hat,"fit":definition["fit"],"pose":neutral,
                "expression":"neutral","orientation":"forward","observedTarget":null,
                "reducedMotionEndpoint":true,"allManualHatsPlusNone":27,
                "worldHead":rect_json(head),"traceIntegration":false,"inputHashes":input_hashes,
                "acceptedFitEqualityEvidence":{"path":"target/presence-accepted-neutral-hat-fit-equality-20261018.json",
                    "sha256":"d89b4182a97858f1bcf80865a5986872cca6267b05db024ec356920bc6c136c4",
                    "reexecutedByThisScenario":false}}));
    }
    f.finish_scenario();
}

fn set_pose(f: &mut Fixture, target: slate_doc::NodeId, activity: &str, dx: f32, dy: f32) {
    let a = &mut f.h.app.presence.agents[0];
    let p =
        f.h.app
            .presence
            .roster
            .session(&a.motion)
            .unwrap()
            .sample()
            .position;
    a.activity = activity.into();
    a.status = "Direct-model visual catalogue".into();
    a.observed_target = Some(target);
    a.last_active = Instant::now();
    f.h.app.patch_nodes(&[target], |node| {
        node.rect.x = p.x as f32 + dx - 10.;
        node.rect.y = p.y as f32 + dy - 10.;
    });
}

fn steps(f: &mut Fixture, count: usize) -> egui::FullOutput {
    assert!(count > 0);
    let mut out = f.step(|_| {});
    for _ in 1..count {
        out = f.step(|_| {});
    }
    out
}
fn eyes(f: &Fixture, out: &egui::FullOutput) -> Vec<[u32; 3]> {
    let center = f.board_avatar_center(&f.who);
    let vertices = Fixture::mesh_vertices(
        out,
        // Keep the measurement on black eyes/bridge, clear of the animated
        // props below the head and the participant-name glyphs.
        Rect::from_center_size(center, egui::vec2(35., 20.)),
        true,
    );
    assert!(
        vertices.len() > 20,
        "measure actual opaque eye/bridge mesh vertices"
    );
    vertices
}

fn transitions(f: &mut Fixture, target: slate_doc::NodeId) {
    // First hold gaze fixed: expression must finish by 180ms and remain stable.
    f.h.app.settings.presence_reduced_motion = true;
    set_pose(f, target, "reading", 0., 0.);
    f.settle();
    f.h.app.settings.presence_reduced_motion = false;
    set_pose(f, target, "writing", 0., 0.);
    let start = f.step(|_| {});
    let e0 = eyes(f, &start);
    f.save(
        "catalogue-expression-start",
        start,
        json!({"evidence":"direct-model expression change, fixed gaze","relativeMs":0}),
    );
    let at180 = steps(f, 11);
    let e180 = eyes(f, &at180);
    f.save(
        "catalogue-expression-183ms",
        at180,
        json!({"evidence":"actual eye mesh after 11 existing 60Hz frames","relativeMs":183.333}),
    );
    let at475 = steps(f, 18);
    let e475 = eyes(f, &at475);
    assert_ne!(e0, e180);
    assert_eq!(
        e180, e475,
        "fixed-gaze expression is already settled while a look could still move"
    );
    f.save(
        "catalogue-expression-483ms",
        at475,
        json!({"evidence":"actual settled expression mesh","relativeMs":483.333}),
    );

    // Then hold expression fixed: gaze must still move after 180ms.
    f.h.app.settings.presence_reduced_motion = true;
    set_pose(f, target, "writing", -160., 0.);
    f.settle();
    f.h.app.settings.presence_reduced_motion = false;
    set_pose(f, target, "writing", 160., 0.);
    let start = f.step(|_| {});
    let g0 = eyes(f, &start);
    f.save(
        "catalogue-look-start",
        start,
        json!({"evidence":"direct-model look change, fixed expression","relativeMs":0}),
    );
    let at180 = steps(f, 11);
    let g180 = eyes(f, &at180);
    f.save(
        "catalogue-look-183ms",
        at180,
        json!({"evidence":"actual intermediate gaze mesh","relativeMs":183.333}),
    );
    let at475 = steps(f, 18);
    let g475 = eyes(f, &at475);
    assert_ne!(g0, g180);
    assert_ne!(g180, g475, "look continues after expression duration");
    f.save(
        "catalogue-look-483ms",
        at475,
        json!({"evidence":"actual final gaze mesh","relativeMs":483.333}),
    );
    let stable = steps(f, 3);
    assert_eq!(g475, eyes(f, &stable));

    // Interrupt a look during motion. The actual output must retain a visible
    // intermediate, not jump immediately to either endpoint.
    set_pose(f, target, "writing", -160., 0.);
    f.step(|_| {});
    let middle = steps(f, 11);
    let before = eyes(f, &middle);
    f.save(
        "catalogue-look-before-interruption",
        middle,
        json!({"evidence":"actual in-progress gaze before retarget"}),
    );
    set_pose(f, target, "writing", 160., 0.);
    let changed = f.step(|_| {});
    let after = eyes(f, &changed);
    assert_ne!(after, g0);
    assert_ne!(after, g475);
    assert_ne!(after, before);
    f.save(
        "catalogue-look-interrupted",
        changed,
        json!({"evidence":"retarget at current sampled mesh; no animation model recreated"}),
    );
    let settled = steps(f, 30);
    f.save(
        "catalogue-look-interruption-finished",
        settled,
        json!({"evidence":"actual retargeted final app output"}),
    );
}
