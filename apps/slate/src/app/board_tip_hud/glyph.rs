//! Cached palette swatch glyphs.
use super::palette::*;
use super::*;
use slate_doc::scene::BrushTexture;
fn texture_swatch(
    ctx: &egui::Context,
    texture: BrushTexture,
) -> Option<(egui::TextureId, egui::Rect)> {
    type Swatch = (egui::TextureHandle, egui::Rect);
    let id = egui::Id::new(("slate.tip_swatch", texture.label()));
    if let Some((tex, rect)) = ctx.data(|d| d.get_temp::<Swatch>(id)) {
        return Some((tex.id(), rect));
    }
    let tip = vector_ink::StampStyle {
        diameter: SWATCH_STROKE,
        softness: 0.15,
        rgba: [255; 4],
        grain: texture.grain(),
    };
    let reach = PALETTE_ICON_R - SWATCH_STROKE * 0.5 - 3.0;
    let points: Vec<vector_ink::TipPoint> = (0..=32)
        .map(|k| {
            let t = k as f32 / 32.0;
            let x = (t - 0.5) * 2.0 * reach;
            let y = -(t * std::f32::consts::TAU).sin() * reach * 0.3;
            vector_ink::TipPoint { pos: [x, y], tip }
        })
        .collect();
    let img = vector_ink::stamp_tipped(&[points], 1.0 / 3.0)?;
    let pixels = img
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| Color32::from_white_alpha(p[3]))
        .collect();
    let image = egui::ColorImage {
        size: [img.width as usize, img.height as usize],
        pixels,
    };
    let tex = ctx.load_texture(
        format!("tip-swatch-{}", texture.label()),
        image,
        egui::TextureOptions::LINEAR,
    );
    let rect = egui::Rect::from_min_size(
        Pos2::new(img.origin[0], img.origin[1]),
        egui::vec2(img.width as f32 * img.pixel, img.height as f32 * img.pixel),
    );
    ctx.data_mut(|d| d.insert_temp(id, (tex.clone(), rect)));
    Some((tex.id(), rect))
}

/// The curve `style` really produces, on a small bent path around the
/// swatch center: the same stroke style, taper, and arrowhead the board
/// paints. Tessellated once and kept in egui memory.
fn curve_glyph(ctx: &egui::Context, choice: TipChoice, both: bool) -> vector_ink::InkMesh {
    let key = match choice {
        TipChoice::End(style) => 16 + style as u8 * 2 + u8::from(both),
        TipChoice::Curve(style) => style as u8,
        TipChoice::Texture(_) => 255,
    };
    let id = egui::Id::new(("slate.tip_curve_glyph", key));
    if let Some(ink) = ctx.data(|d| d.get_temp::<vector_ink::InkMesh>(id)) {
        return ink;
    }
    let mut stroke = Stroke {
        width: 4.0,
        ..Stroke::default()
    };
    match choice {
        TipChoice::Curve(style) => apply_curve_style(&mut stroke, style),
        // The end being set is the glyph's right end; the left stays flat.
        TipChoice::End(style) => {
            let ends: &[usize] = if both { &[0, 1] } else { &[1] };
            for &i in ends {
                stroke.set_end(i, style.end());
            }
        }
        TipChoice::Texture(_) => {}
    }
    let mut bez = vector_ink::kurbo::BezPath::new();
    bez.move_to((-13.0, 7.0));
    bez.line_to((-2.0, -6.0));
    bez.line_to((14.0, 4.0));
    let ink_style = super::super::board_path::stroke_style_world(&stroke, 1.0);
    let feather = 0.8;
    let arrows = stroke.arrows();
    let body = slate_doc::geom::trim_arrow_ends(&bez, arrows, [stroke.width; 2]);
    let ends = stroke.end_caps().map(super::super::board_path::ink_cap);
    let mut ink = vector_ink::stroke_mesh_ends(
        &body,
        &ink_style,
        ends,
        None,
        None,
        vector_ink::TipEase::Linear,
        feather,
        0.05,
    );
    for end in (0..2).filter(|&i| arrows[i]) {
        super::super::board_path::push_arrow_ink(&mut ink, &bez, end, stroke.width, feather, None);
    }
    ctx.data_mut(|d| d.insert_temp(id, ink.clone()));
    ink
}

/// Each choice drawn as what it produces, in the tool's color.
pub(super) fn paint_choice_glyph(
    painter: &egui::Painter,
    at: Pos2,
    choice: TipChoice,
    both: bool,
    ink: Color32,
) {
    match choice {
        TipChoice::Texture(texture) => {
            if let Some((tex, rect)) = texture_swatch(painter.ctx(), texture) {
                painter.image(
                    tex,
                    rect.translate(at.to_vec2()),
                    egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    ink,
                );
            }
        }
        TipChoice::Curve(_) | TipChoice::End(_) => {
            let ink_mesh = curve_glyph(painter.ctx(), choice, both);
            let mut mesh = egui::Mesh::default();
            mesh.vertices.reserve(ink_mesh.vertices.len());
            for v in &ink_mesh.vertices {
                mesh.colored_vertex(
                    at + egui::vec2(v.pos[0], v.pos[1]),
                    ink.gamma_multiply(v.alpha),
                );
            }
            mesh.indices = ink_mesh.indices;
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}
