//! Style-row picks and stamp pixels.

use super::*;

/// Pick `choice` in the armed tool's style row with Alt+right-button
/// events: press at screen `press`, drop straight into the row's band, move
/// onto the slot, and release there.
pub(crate) fn pick_style_row(h: &mut Harness, press: Pos2, choice: board_tip_hud::TipChoice) {
    let n = h.app.tip_choices().len();
    let i = h
        .app
        .tip_choices()
        .iter()
        .position(|c| *c == choice)
        .expect("a slot for the choice");
    h.frame_with(pointer_to(press, true));
    h.frame_with(right_button(press, true, true));
    let r = h.app.active_tip().0 * 0.5 * h.app.tab().cam.z;
    let band = board_tip_hud::palette_band_y(press, r);
    let slot = board_tip_hud::palette_slot(press, r, i, n);
    h.frame_with(pointer_to(Pos2::new(press.x, band + 1.0), true));
    h.frame_with(pointer_to(slot, true));
    h.frame_with(right_button(slot, false, true));
    h.frame_with(|i| i.modifiers = egui::Modifiers::NONE);
    assert_eq!(
        h.app.current_tip_choice(),
        Some(choice),
        "the style row picked"
    );
}

/// Stroke `n` with every tip, and the stroke itself, in texture `t`.
pub(crate) fn with_texture(
    n: &slate_doc::Node,
    t: slate_doc::scene::BrushTexture,
) -> slate_doc::Node {
    let mut n = n.clone();
    if let NodeKind::Shape(s) = &mut n.kind {
        s.stroke.texture = t;
        if let Some(path) = s.path.as_mut() {
            for tip in &mut std::sync::Arc::make_mut(path).tips {
                tip.texture = t;
            }
        }
    }
    n
}

/// Committed stroke `n` stamped as the tiles stamp it, one world unit per
/// pixel.
pub(crate) fn stamp_at_one(n: &slate_doc::Node) -> vector_ink::StampImage {
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape")
    };
    board_path::stroke_stamp(n, s, s.path.as_ref().expect("a path"), 1.0).expect("a stamp")
}

/// The stamp's pixel under world point `w`, clear outside it.
pub(crate) fn stamp_px(img: &vector_ink::StampImage, w: Pos2) -> [u8; 4] {
    let x = ((w.x - img.origin[0]) / img.pixel).floor();
    let y = ((w.y - img.origin[1]) / img.pixel).floor();
    if x < 0.0 || y < 0.0 || x >= img.width as f32 || y >= img.height as f32 {
        return [0; 4];
    }
    let i = (y as usize * img.width as usize + x as usize) * 4;
    [
        img.rgba[i],
        img.rgba[i + 1],
        img.rgba[i + 2],
        img.rgba[i + 3],
    ]
}

/// World points on a 1-unit grid within `half` of `w`.
pub(crate) fn around(w: Pos2, half: i32) -> Vec<Pos2> {
    (-half..=half)
        .flat_map(|dy| (-half..=half).map(move |dx| w + EVec2::new(dx as f32, dy as f32)))
        .collect()
}

/// Points every 10 world units along the polyline through `corners`.
pub(crate) fn dense(corners: &[Pos2]) -> Vec<Pos2> {
    let mut out = vec![corners[0]];
    for w in corners.windows(2) {
        let steps = ((w[1] - w[0]).length() / 10.0).ceil().max(1.0) as usize;
        out.extend((1..=steps).map(|k| w[0] + (w[1] - w[0]) * (k as f32 / steps as f32)));
    }
    out
}
