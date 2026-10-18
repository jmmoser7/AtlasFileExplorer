//! Actual controls/completion and real elapsed retirement; never a pose model.
use super::super::*;
use super::management::{click_text, required, seek};
use super::support::{Fixture, Scenario};
use eframe::egui::{self, Id};
use serde_json::json;
use std::time::Duration;

pub(super) fn run() {
    for (style, label, portal) in [
        ("bow", "Bow & puff", false),
        ("sunset", "Into the sunset", false),
        ("edge", "Walk off the edge", false),
        ("portal", "Under a portal", true),
        ("portal", "Under a portal", false),
    ] {
        let mut f = Fixture::for_scenario(Scenario::Departure);
        let who = f.who.clone();
        brief(&mut f);
        if portal {
            let prepared=f.h.app.prepare_targeted_command("board.objects.create_train",&[],
                &json!({"x":160,"y":40,"width":240,"height":190,"provider":"local","title":"Real visible fixture portal"})).unwrap();
            f.h.app.commit_scene(prepared.cmds);
        }
        f.settle();
        f.click(Id::new(("presence-chat", who.as_str())));
        let out = f.click(Id::new(("conversation-more", who.as_str())));
        let out = click_text(&mut f, &out, "Retire agent");
        let out = click_text(&mut f, &out, label);
        let out = click_text(&mut f, &out, "Retire agent");
        assert!(f
            .h
            .app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap()
            .grant
            .is_none());
        let suffix = if portal {
            "real-portal"
        } else if style == "portal" {
            "fallback"
        } else {
            style
        };
        farewell(&mut f, &who, &format!("departure-{suffix}"), out);
        if chrome::system_reduced_motion(&f.h.ctx) {
            f.note_omission(
                "OS reduced motion yields a real fade; flourish motion is not claimed.",
            );
        }
        f.finish_scenario();
    }
    helper();
}

fn farewell(f: &mut Fixture, who: &str, prefix: &str, first: egui::FullOutput) {
    let a =
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .unwrap();
    let (started, style, _) = a.departure.as_ref().unwrap();
    let started = *started;
    let style = style.clone();
    let duration = chrome::departure_duration(&style);
    let observation = |f: &Fixture| {
        json!({"evidence":"actual app output at real lifecycle Instant elapsed",
        "participant":who,"style":style,"elapsedMs":started.elapsed().as_secs_f64()*1000.,
        "durationMs":duration*1000.,"reducedMotion":f.h.app.settings.presence_reduced_motion,
        "systemReducedMotion":chrome::system_reduced_motion(&f.h.ctx),"clockReset":false})
    };
    let mut retained = vec![(format!("{prefix}-early"), f.retain(first, observation(f)))];
    for (name, ratio) in [("middle", 0.36), ("late", 0.76)] {
        while started.elapsed().as_secs_f64() < duration * ratio {
            f.step(|_| {});
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            started.elapsed().as_secs_f64() < duration,
            "capture must not manufacture a missed lifecycle phase"
        );
        let out = f.step(|_| {});
        assert!(f.h.app.presence.agents.iter().any(|a| a.profile.id == who));
        retained.push((format!("{prefix}-{name}"), f.retain(out, observation(f))));
    }
    let deadline = Instant::now() + Duration::from_secs_f64(duration + 2.0);
    while f.h.app.presence.agents.iter().any(|a| a.profile.id == who) {
        assert!(Instant::now() < deadline, "real departure must finish");
        f.step(|_| {});
        std::thread::sleep(Duration::from_millis(5));
    }
    for (phase, frame) in retained {
        f.save_retained(&phase, frame);
    }
    let gone = f.step(|_| {});
    f.save(
        &format!("{prefix}-gone"),
        gone,
        json!({"evidence":"same actual lifecycle removed the participant", "participant":who}),
    );
}

fn helper() {
    let mut f = Fixture::for_scenario(Scenario::Departure);
    let parent = f.who.clone();
    f.settle();
    brief(&mut f);
    f.settle();
    f.click(Id::new(("presence-chat", parent.as_str())));
    let out = f.click(Id::new(("conversation-more", parent.as_str())));
    let out = click_text(&mut f, &out, "Access settings");
    let access = f.enclosure(&out, required(&out, "Board access"), 80.0);
    let out = seek(&mut f, "Add temporary helper", access);
    let previous: Vec<_> =
        f.h.app
            .presence
            .agents
            .iter()
            .map(|a| a.profile.id.clone())
            .collect();
    click_text(&mut f, &out, "Add temporary helper");
    let child =
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| !previous.contains(&a.profile.id))
            .unwrap()
            .profile
            .id
            .clone();
    assert_eq!(
        f.h.app
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == child)
            .unwrap()
            .parent
            .as_deref(),
        Some(parent.as_str())
    );
    f.declare_task(&child, "Synthetic helper visual result", false);
    let progress = f.synthetic_session(
        &child,
        atlas_agent::AgentStatus::Thinking,
        "Synthetic helper report: the test-owned brief has been reviewed.",
    );
    f.observe(&child, &progress);
    let out = f.click(Id::new(("presence-chat", child.as_str())));
    super::support::has(&out, "Synthetic helper report");
    f.save("helper-public-report",out,json!({"evidence":"synthetic visible turn through actual poller and chat", "helper":child,"parent":parent,"terminal":false}));
    f.permit_terminal(&child);
    let completed = f.synthetic_session(
        &child,
        atlas_agent::AgentStatus::Idle,
        "Synthetic helper report: the test-owned brief has been reviewed.",
    );
    f.observe(&child, &completed);
    let result =
        f.h.app
            .presence
            .helper_results
            .iter()
            .find(|r| r.id == child)
            .unwrap();
    assert_eq!(result.parent_id, parent);
    assert_eq!(result.outcome, "success");
    assert!(!result.expired);
    assert!(result.result.contains("Synthetic helper report"));
    let out = f.step(|_| {});
    farewell(&mut f, &child, "helper-completion", out);
    assert!(f
        .h
        .app
        .presence
        .agents
        .iter()
        .any(|a| a.profile.id == parent && a.departure.is_none()));
    f.finish_scenario();
}

fn brief(f: &mut Fixture) {
    let prepared=f.h.app.prepare_targeted_command("board.objects.create_text",&[],
        &json!({"x":-260,"y":-140,"width":220,"height":120,"text":"Test-owned work board\nVisible farewell fixture."})).unwrap();
    f.h.app.commit_scene(prepared.cmds);
}
