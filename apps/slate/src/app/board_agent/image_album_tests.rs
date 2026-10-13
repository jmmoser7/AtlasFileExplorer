//! Generated albums retain their resting pick and include arriving results.
use super::*;
use crate::app::tests::Harness;

pub(super) fn append_result(h: &mut Harness, id: NodeId, name: &str) {
    let path = h.base.join(format!("{name}.png"));
    image::RgbImage::new(20, 10).save(&path).unwrap();
    let mut session = h
        .app
        .agents
        .sessions
        .get(&id)
        .map(|s| (**s).clone())
        .unwrap_or_else(|| atlas_agent::AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: AgentStatus::Idle,
            provider: "comfy".into(),
            turns: vec![],
            updated_at: 0,
            bundle: Default::default(),
            request: String::new(),
            usage: None,
        });
    session.bundle.images.push(atlas_agent::ImageOutput {
        id: name.into(),
        source: path.to_string_lossy().into_owned(),
        request: name.into(),
        ..Default::default()
    });
    h.app
        .agents
        .sessions
        .insert(id, std::sync::Arc::new(session));
    h.app.agents.output_epoch += 1;
}

fn frame(h: &mut Harness, id: NodeId, wheel: bool) {
    let node = h.app.doc().scene.node(id).unwrap();
    let pointer = h.app.board_xf().rect_w2s(node.rect).center();
    let time = h.ctx.input(|i| i.time) + 1.0 / 60.0;
    h.frame_with(|input| {
        input.time = Some(time);
        input.events.push(egui::Event::PointerMoved(pointer));
        if wheel {
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -3.0),
                modifiers: egui::Modifiers::NONE,
            });
        }
    });
}

fn settle(h: &mut Harness, id: NodeId) {
    for _ in 0..180 {
        frame(h, id, false);
    }
    assert!(atlas_shell::home::album_browsing(&h.ctx, Id::new(("agent-picture", id.0))) < 0.001);
}

#[test]
fn generated_album_resumes_at_its_resting_pick_and_reaches_new_results() {
    let (mut h, id) = generator_with_output("album-rest-and-arrival");
    append_result(&mut h, id, "second");
    append_result(&mut h, id, "third");
    h.app.pick_agent_result(id, 1);
    h.app.board_sel = std::iter::once(id).collect();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    h.app.tab_mut().cam.z = 1.0;
    settle(&mut h, id);
    let zoom = h.app.tab().cam.z;
    for expected in [2, 0, 1, 2] {
        frame(&mut h, id, true);
        settle(&mut h, id);
        assert_eq!(h.app.agent_shown_index(id), Some(expected));
        assert_eq!(h.app.tab().cam.z, zoom, "album owns the wheel");
    }
    append_result(&mut h, id, "fourth");
    frame(&mut h, id, true);
    settle(&mut h, id);
    assert_eq!(h.app.agent_active_output(id).as_deref(), Some("fourth"));
    frame(&mut h, id, true);
    settle(&mut h, id);
    assert_eq!(h.app.agent_shown_index(id), Some(0));
}
