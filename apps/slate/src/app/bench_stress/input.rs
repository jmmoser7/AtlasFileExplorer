//! Real navigation events: retain input routing and egui wheel smoothing cost.

use super::*;

pub(super) fn navigation(h: &mut Harness, rows: &mut Vec<ResultRow>, count: usize, frames: usize) {
    let original = h.app.tab().cam;
    let center = h.app.canvas_rect.center();
    let middle = |p, pressed| {
        vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Middle,
                pressed,
                modifiers: Default::default(),
            },
        ]
    };
    let mut samples = vec![frame(h, middle(center, true))];
    let mut moved = false;
    for i in 0..frames {
        let p = center
            + Vec2::new(
                (i as f32 * 0.09).sin() * 170.0,
                (i as f32 * 0.07).cos() * 100.0,
            );
        samples.push(frame(h, vec![egui::Event::PointerMoved(p)]));
        moved |= (h.app.tab().cam.offset - original.offset).length() > 0.01;
    }
    samples.push(frame(h, middle(center, false)));
    assert!(moved, "middle drag must move the real camera");
    report(rows, count, "middle_drag_overview", samples, 0.0);
    h.app.tab_mut().cam = original;
    let mut changed = false;
    let mut samples = Vec::with_capacity(frames);
    for i in 0..frames {
        samples.push(frame(
            h,
            vec![
                egui::Event::PointerMoved(center),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, if (i / 20) % 2 == 0 { 12.0 } else { -12.0 }),
                    modifiers: Default::default(),
                },
            ],
        ));
        changed |= (h.app.tab().cam.z - original.z).abs() > original.z * 0.01;
    }
    assert!(changed, "wheel input must zoom the real camera");
    report(rows, count, "wheel_zoom_overview", samples, 0.0);
    report(
        rows,
        count,
        "wheel_release_tail",
        (0..30).map(|_| frame(h, Vec::new())).collect(),
        0.0,
    );
    h.app.tab_mut().cam = original;
}
