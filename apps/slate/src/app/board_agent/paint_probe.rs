//! Paint, menu, and stop-square probes shared by the agent session tests.

use super::train_setup::{center_on, press};
use super::*;

/// Every string one real frame paints, with the pointer at `pointer`.
pub(super) fn painted_text(
    h: &mut super::super::tests::Harness,
    pointer: Option<Pos2>,
) -> Vec<String> {
    fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
        ..Default::default()
    };
    input.events.push(egui::Event::PointerMoved(
        pointer.unwrap_or(Pos2::new(2.0, 890.0)),
    ));
    let ctx = h.ctx.clone();
    let app = &mut h.app;
    let output = ctx.run(input, |c| app.update_app(c));
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut texts);
    }
    texts
}

/// Every string one frame painted, with where it was painted.
pub(super) fn painted_at(output: &egui::FullOutput) -> Vec<(String, Rect)> {
    painted_clipped(output)
        .into_iter()
        .map(|(t, r, _)| (t, r))
        .collect()
}

/// [`painted_at`] with each string's clip rect.
pub(super) fn painted_clipped(output: &egui::FullOutput) -> Vec<(String, Rect, Rect)> {
    fn walk(shape: &egui::Shape, clip: Rect, out: &mut Vec<(String, Rect, Rect)>) {
        match shape {
            egui::Shape::Text(t) => {
                out.push((t.galley.text().to_string(), t.visual_bounding_rect(), clip))
            }
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, clipped.clip_rect, &mut texts);
    }
    texts
}

/// Open the card's model dropdown with a real click on its name.
pub(super) fn open_model_menu(h: &mut super::super::tests::Harness, card: NodeId, z: f32) {
    // A fresh draft sizes itself on its first frame; center on that size.
    h.frame();
    center_on(h, card);
    h.app.tab_mut().cam.z = z;
    h.frame();
    let out = h.frame_output(|_| {});
    let label = h.app.model_menu_label(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(card).unwrap()).unwrap(),
    );
    let texts = painted_at(&out);
    let at = texts
        .iter()
        .find(|(t, _)| *t == label)
        .map(|(_, r)| r.center())
        .unwrap_or_else(|| panic!("the tail shows its model name: {texts:?}"));
    press(h, at);
    assert!(
        menu_items_painted(&h.frame_output(|_| {})),
        "the click opened the model list"
    );
}

/// Every row of the open model list is painted whole: none clipped away
/// or scrolled out. The title reads "Astra", so the list's rows are
/// Default, Astra, Terra and, last, Luna.
pub(super) fn menu_items_painted(output: &egui::FullOutput) -> bool {
    let texts = painted_clipped(output);
    ["Default", "Terra", "Luna"].iter().all(|item| {
        texts
            .iter()
            .any(|(t, r, clip)| t == item && clip.expand(0.5).contains_rect(*r))
    })
}

/// Wheel notches at `at`: `steps` in, then as many out.
pub(super) fn wheel_sweep(
    h: &mut super::super::tests::Harness,
    at: Pos2,
    steps: usize,
    mut check: impl FnMut(&mut super::super::tests::Harness, &egui::FullOutput, usize, f32),
) {
    for step in 0..steps * 2 {
        let delta = if step < steps { 40.0 } else { -40.0 };
        let out = h.frame_output(|i| {
            i.events.push(egui::Event::PointerMoved(at));
            i.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, delta),
                modifiers: Default::default(),
            });
        });
        let z = h.app.tab().cam.z;
        check(h, &out, step, z);
    }
}

/// Where the card's top output circle sits on screen.
pub(super) fn output_circle(h: &super::super::tests::Harness, id: NodeId) -> Pos2 {
    let xf = h.app.board_xf();
    xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .right_top()
        + egui::vec2(
            -slate_doc::agent_chat::PORT_INSET,
            slate_doc::agent_chat::RAIL_INSET,
        ) * xf.z
}

/// The Stop square painted at `at`, if any.
pub(super) fn stop_square_at(output: &egui::FullOutput, at: Pos2) -> Option<Rect> {
    fn walk(shape: &egui::Shape, at: Pos2, found: &mut Option<Rect>) {
        match shape {
            egui::Shape::Rect(r) if r.rect.center().distance(at) < 1.5 => *found = Some(r.rect),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, at, found)),
            _ => {}
        }
    }
    let mut found = None;
    for clipped in &output.shapes {
        walk(&clipped.shape, at, &mut found);
    }
    found
}

/// The cancel request lands on a worker thread.
pub(super) fn stop_requested(cancel: &std::path::Path) -> bool {
    (0..200).any(|_| {
        if cancel.is_file() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
        false
    })
}
