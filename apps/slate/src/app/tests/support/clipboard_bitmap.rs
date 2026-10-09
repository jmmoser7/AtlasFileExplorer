//! Clipboard bitmaps, painted colors, and the rotate cursor.

use super::*;

pub(crate) fn copied_bitmap(h: &Harness) -> image::RgbaImage {
    let write = h.app.os_clipboard.last_write().expect("a clipboard write");
    let png = write.png.as_ref().expect("a PNG on the clipboard");
    let from_png = image::load_from_memory(png).unwrap().to_rgba8();
    let dib = write.dibv5.as_ref().expect("a DIBV5 on the clipboard");
    let from_dib = image::load_from_memory(
        &atlas_core::clipboard_image::decode_clipboard_bitmap(dib).unwrap(),
    )
    .unwrap()
    .to_rgba8();
    assert_eq!(from_png, from_dib, "PNG and DIBV5 carry the same pixels");
    from_png
}

pub(crate) fn copy_selection(h: &mut Harness, ids: &[NodeId]) {
    h.app.board_sel = ids.iter().copied().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.copy"), None));
}

/// Every color a frame painted: mesh vertices and shape fills and strokes.
pub(crate) fn painted_colors(out: &egui::FullOutput) -> Vec<egui::Color32> {
    fn walk(shape: &egui::Shape, acc: &mut Vec<egui::Color32>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
            egui::Shape::Mesh(m) => acc.extend(m.vertices.iter().map(|v| v.color)),
            egui::Shape::Circle(c) => acc.extend([c.fill, c.stroke.color]),
            egui::Shape::Path(p) => {
                acc.push(p.fill);
                if let egui::epaint::PathStroke {
                    color: egui::epaint::ColorMode::Solid(c),
                    ..
                } = p.stroke
                {
                    acc.push(c);
                }
            }
            egui::Shape::Rect(r) => acc.extend([r.fill, r.stroke.color]),
            _ => {}
        }
    }
    let mut acc = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut acc);
    }
    acc
}

/// Painted strongly in the test's brush red (230, 20, 20).
pub(crate) fn brush_red(c: egui::Color32) -> bool {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    a > 8 && r > 150 && g < 90 && b < 90
}

/// The pointer during rotation: the OS arrow hides and a circular arrow in
/// the Windows cursor scheme (white glyph, black outline) takes its place.
pub(crate) fn assert_rotate_pointer(out: &egui::FullOutput, at: Pos2, phase: &str) {
    assert_eq!(
        out.platform_output.cursor_icon,
        egui::CursorIcon::None,
        "{phase}: the OS arrow gives way to the rotate pointer"
    );
    fn walk<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::epaint::PathShape>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::Path(p) => out.push(p),
            _ => {}
        }
    }
    let mut paths = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut paths);
    }
    let near: Vec<&egui::epaint::PathShape> = paths
        .into_iter()
        .filter(|p| {
            !p.closed && !p.points.is_empty() && p.points.iter().all(|q| q.distance(at) < 24.0)
        })
        .collect();
    let solid = |p: &egui::epaint::PathShape| match p.stroke.color {
        egui::epaint::ColorMode::Solid(c) => Some(c),
        _ => None,
    };
    let glyph = near
        .iter()
        .copied()
        .find(|p| solid(p) == Some(egui::Color32::WHITE))
        .unwrap_or_else(|| panic!("{phase}: a white rotate glyph at the pointer"));
    let outline = near
        .iter()
        .copied()
        .find(|p| solid(p) == Some(egui::Color32::BLACK))
        .unwrap_or_else(|| panic!("{phase}: a black outline under the glyph"));
    assert!(
        outline.stroke.width > glyph.stroke.width,
        "{phase}: outline rims the glyph"
    );
    let n = glyph.points.len() as f32;
    let c = glyph
        .points
        .iter()
        .fold(EVec2::ZERO, |a, p| a + p.to_vec2())
        / n;
    let mut angles: Vec<f32> = glyph
        .points
        .iter()
        .map(|p| (p.to_vec2() - c).angle().to_degrees())
        .collect();
    angles.sort_by(f32::total_cmp);
    let mut gap: f32 = 360.0 - (angles[angles.len() - 1] - angles[0]);
    for w in angles.windows(2) {
        gap = gap.max(w[1] - w[0]);
    }
    assert!(
        360.0 - gap >= 250.0,
        "{phase}: a circular arrow, not a quarter arc (sweep {})",
        360.0 - gap
    );
}
