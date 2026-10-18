//! Existing pure node-property transforms; UI and prepared commands share this owner.
use crate::scene::{
    self, Corner, Dash, ImageAdjust, Node, NodeKind, Rgba, ShapeKind, StrokeCap, StrokeJoin,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Property {
    FillRgb([u8; 3]),
    FillAlpha(u8),
    Filled(bool),
    StrokeRgb([u8; 3]),
    StrokeAlpha(u8),
    StrokeWidth(f32),
    Dash(Dash),
    Cap(StrokeCap),
    Join(StrokeJoin),
    CornerTreatment(bool),
    CornerMode(bool),
    CornerAmount(f32),
    WireRouting(crate::WireRouting),
    WireArrows(bool),
    ImageAdjust(ImageAdjust),
    TextFamily(scene::Typeface),
    TextSize(f32),
    TextAlign(scene::TextAlign),
    TextRgb([u8; 3]),
    TextAlpha(u8),
    TextBold(bool),
    TextItalic(bool),
    TextUnderline(bool),
    BumperOn(bool),
    BumperBuffer(f32),
    BumperFriction(f32),
    /// Regular polygon side count (3–12).
    RegularSides(u8),
}

impl Property {
    pub fn apply(&self, node: &mut Node, item_path: Option<&std::path::Path>) {
        match *self {
            Self::BumperOn(on) => {
                node.bumper = (on && crate::bumper::supports_bumper(node))
                    .then(|| node.bumper.unwrap_or_default());
            }
            Self::BumperBuffer(v) | Self::BumperFriction(v) => {
                if !crate::bumper::supports_bumper(node) {
                    return;
                }
                let mut b = node.bumper.unwrap_or_default();
                if matches!(self, Self::BumperBuffer(_)) {
                    b.buffer = v;
                } else {
                    b.friction = v;
                }
                node.bumper = Some(b.clamped());
            }
            Self::WireRouting(routing) => {
                if let NodeKind::Connector(c) = &mut node.kind {
                    c.routing = Some(routing);
                }
            }
            Self::WireArrows(enabled) => {
                if let NodeKind::Connector(c) = &mut node.kind {
                    if !enabled {
                        c.arrow_a = false;
                        c.arrow_b = false;
                    } else if !c.arrow_a && !c.arrow_b {
                        c.arrow_b = true;
                    }
                }
            }
            Self::FillRgb(_) | Self::FillAlpha(_) | Self::Filled(_) => {
                let theme_relative = match &node.kind {
                    NodeKind::Portal(p) => p.fill_follows_theme() || p.slate_fill_follows_theme(),
                    NodeKind::Frame(f) => f.fill_follows_theme(),
                    _ => false,
                };
                let mut c = scene::fill_of(node).unwrap_or(Rgba([128, 128, 128, 255]));
                match *self {
                    Self::FillRgb(rgb) => {
                        c.0[..3].copy_from_slice(&rgb);
                        if theme_relative {
                            c.0[3] = 255;
                        }
                    }
                    Self::FillAlpha(a) => c.0[3] = a,
                    Self::Filled(false) => {
                        scene::set_fill(node, None);
                        return;
                    }
                    _ => {}
                }
                scene::set_fill(node, Some(c));
            }
            Self::RegularSides(sides) => {
                if let NodeKind::Shape(s) = &mut node.kind {
                    if s.shape == ShapeKind::RegularPolygon {
                        let old = s.sides;
                        s.sides = scene::clamp_regular_sides(sides);
                        crate::vertex_style::reside_closed_form_style(s, old, s.phase_deg);
                    }
                }
            }
            Self::CornerTreatment(_) | Self::CornerMode(_) | Self::CornerAmount(_) => {
                if !scene::supports_corners(node) {
                    return;
                }
                let (w, h) = (node.rect.w, node.rect.h);
                scene::edit_corner(node, item_path, |corner| {
                    let (mut chamfer, percent, mut amount) = corner.parameters();
                    match *self {
                        Self::CornerTreatment(v) => chamfer = v,
                        Self::CornerMode(v) => return corner.with_mode(v, w, h),
                        Self::CornerAmount(v) => amount = v,
                        _ => {}
                    }
                    Corner::from_parameters(chamfer, percent, amount)
                });
                if matches!(self, Self::CornerAmount(_)) {
                    scene::clear_vertex_corner_amounts(node);
                }
            }
            Self::ImageAdjust(adjust) => scene::set_adjust(node, adjust),
            Self::TextFamily(family) => {
                map_text_style(node, |face, _, _, _, _| *face = family);
            }
            Self::TextBold(value) | Self::TextItalic(value) | Self::TextUnderline(value) => {
                map_text_style(node, |_, _, _, _, style| match self {
                    Self::TextBold(_) => style.bold = value,
                    Self::TextItalic(_) => style.italic = value,
                    _ => style.underline = value,
                });
            }
            Self::TextSize(size) => {
                if size.is_finite() {
                    let size = size.clamp(1.0, 512.0);
                    map_text_style(node, |_, stored, _, _, _| *stored = size);
                }
            }
            Self::TextAlign(align) => {
                map_text_style(node, |_, _, _, stored, _| *stored = align);
            }
            Self::TextRgb(rgb) => {
                map_text_style(node, |_, _, color, _, _| color.0[..3].copy_from_slice(&rgb));
            }
            Self::TextAlpha(alpha) => {
                map_text_style(node, |_, _, color, _, _| color.0[3] = alpha);
            }
            _ => {
                let theme_relative =
                    matches!(&node.kind, NodeKind::Portal(p) if p.stroke_follows_theme());
                let Some(mut s) = scene::stroke_of(node) else {
                    return;
                };
                match *self {
                    Self::StrokeRgb(rgb) => {
                        s.color.0[..3].copy_from_slice(&rgb);
                        if theme_relative {
                            s.color.0[3] = 255;
                            if s.width <= 0.0 {
                                s.width = 1.0;
                            }
                        }
                    }
                    Self::StrokeAlpha(a) => s.color.0[3] = a,
                    Self::StrokeWidth(v) if v.is_finite() => s.width = v.max(0.0),
                    Self::Dash(v) => s.dash = v,
                    Self::Cap(v) => s.cap = v,
                    Self::Join(v) => s.join = v,
                    _ => {}
                }
                scene::set_stroke(node, s);
                if matches!(*self, Self::StrokeRgb(_) | Self::StrokeAlpha(_)) {
                    if let NodeKind::Connector(c) = &mut node.kind {
                        c.set_color(Some(s.color));
                    }
                }
            }
        }
    }

    /// [`Self::apply`] scoped to the picked grips of a curve
    /// (P1.curve.vertex-style): a stroke width edits those vertices only.
    /// With no grip picked, or on a node without grips, the whole node.
    pub fn apply_at(&self, node: &mut Node, item_path: Option<&std::path::Path>, points: &[usize]) {
        if !points.is_empty() {
            let edited = match *self {
                Self::StrokeWidth(v) if v.is_finite() => {
                    edit_vertex_tips(node, points, |t| t.width = v.max(0.0))
                }
                Self::StrokeRgb(rgb) => {
                    edit_vertex_tips(node, points, |t| t.color.0[..3].copy_from_slice(&rgb))
                }
                Self::StrokeAlpha(a) => edit_vertex_tips(node, points, |t| t.color.0[3] = a),
                Self::CornerAmount(v) => {
                    set_picked_corner_amounts(node, item_path, points, v);
                    return;
                }
                _ => false,
            };
            if edited {
                return;
            }
        }
        if let Self::StrokeWidth(v) = *self {
            if v.is_finite() && scale_stamped_curve(node, None, v.max(0.0), None) {
                return;
            }
        }
        self.apply(node, item_path);
        match *self {
            Self::StrokeRgb(rgb) => {
                edit_every_tip(node, |t| t.color.0[..3].copy_from_slice(&rgb));
            }
            Self::StrokeAlpha(a) => edit_every_tip(node, |t| t.color.0[3] = a),
            _ => {}
        }
    }
}

/// Picked grips of `node` that turn a corner, in pick order: the turning
/// vertices of a line polyline (`polyline_vertex_grip_edges`), or any
/// vertex of a rectangle or regular polygon (P1.shape.vertex-style).
pub fn picked_corners(node: &Node, points: &[usize]) -> Vec<usize> {
    if points.is_empty() || !scene::supports_corners(node) {
        return Vec::new();
    }
    if let NodeKind::Shape(s) = &node.kind {
        if let Some(n) = crate::vertex_style::closed_form_vertex_count(s) {
            return points.iter().copied().filter(|&p| p < n).collect();
        }
    }
    let chamfer = scene::resolved_corner(node, None).parameters().0;
    let corners: Vec<usize> = crate::geom::polyline_vertex_grip_edges(node, chamfer)
        .into_iter()
        .map(|(v, _)| v)
        .collect();
    points
        .iter()
        .copied()
        .filter(|p| corners.contains(p))
        .collect()
}

/// The Corners amount with corners picked (user, 28 September 2026, ep2):
/// each picked corner takes `amount` as its own override, in the shape
/// corner's units (a percentage resolves against the box like the shared
/// corner); every other corner keeps its amount.
fn set_picked_corner_amounts(
    node: &mut Node,
    item_path: Option<&std::path::Path>,
    points: &[usize],
    amount: f32,
) {
    let (chamfer, percent, _) = scene::resolved_corner(node, item_path).parameters();
    let world = if percent {
        Corner::from_parameters(chamfer, true, amount)
            .effective(node.rect.w, node.rect.h)
            .1
    } else {
        amount
    };
    for v in picked_corners(node, points) {
        scene::set_vertex_corner_amount(node, v, world);
    }
}

/// A whole-curve color or texture edit sets every vertex's tip too
/// (P1.curve.vertex-style): stored tips paint over the stroke, on a hard
/// curve and a stamped brush stroke alike.
pub fn edit_every_tip(node: &mut Node, edit: impl Fn(&mut scene::StrokeSpan)) {
    let NodeKind::Shape(s) = &mut node.kind else {
        return;
    };
    if s.shape != ShapeKind::Path {
        if let Some(mut path) = crate::vertex_style::closed_form_path(s) {
            if !path.tips.is_empty() {
                crate::vertex_style::edit_every_tip(&mut path, &mut s.stroke, edit);
                crate::vertex_style::store_closed_form_style(s, &path);
            }
        }
        return;
    }
    let Some(path) = s.path.as_mut() else {
        return;
    };
    if path.tips.is_empty() && s.stroke.tween_from.is_none() {
        return;
    }
    crate::vertex_style::edit_every_tip(std::sync::Arc::make_mut(path), &mut s.stroke, edit);
}

/// Whole-curve width (and softness, when given) of a stamped stroke with
/// stored tips: every tip scales from `base`, else from the curve as it is
/// (`vertex_style::scale_stamped_tips`). `false` when the node paints one
/// tip or is not stamped, so the stroke itself takes the edit.
pub fn scale_stamped_curve(
    node: &mut Node,
    base: Option<&Node>,
    width: f32,
    softness: Option<f32>,
) -> bool {
    let base = base.and_then(|b| match &b.kind {
        NodeKind::Shape(s) => Some((s.path.clone()?, s.stroke)),
        _ => None,
    });
    let NodeKind::Shape(s) = &mut node.kind else {
        return false;
    };
    if s.shape != ShapeKind::Path || !s.stroke.paints_as_stamp() {
        return false;
    }
    let Some(path) = s.path.as_mut() else {
        return false;
    };
    let softness = softness
        .unwrap_or_else(|| crate::vertex_style::curve_tip(Some(&**path), &s.stroke).softness);
    crate::vertex_style::scale_stamped_tips(
        std::sync::Arc::make_mut(path),
        &mut s.stroke,
        base.as_ref().map(|(p, st)| (&**p, st)),
        width,
        softness,
    )
}

/// Set the painted opacity of the `points` (grip indices) of a path node or
/// closed form through the share rule (`vertex_style::set_grip_opacity`),
/// node opacity included.
pub fn set_vertex_opacity(node: &mut Node, points: &[usize], opacity: f32) -> bool {
    let (rect, rotation, node_opacity) = (node.rect, node.rotation_deg, node.opacity);
    let NodeKind::Shape(s) = &mut node.kind else {
        return false;
    };
    let Some(mut path) = crate::vertex_style::vertex_style_path(s) else {
        return false;
    };
    let mut stroke = s.stroke;
    let Some(top) = crate::vertex_style::set_grip_opacity(
        &mut path,
        &mut stroke,
        rect,
        rotation,
        node_opacity,
        points,
        opacity,
    ) else {
        return false;
    };
    crate::vertex_style::store_vertex_style_path(s, path);
    s.stroke = stroke;
    node.opacity = top;
    true
}

/// The painted tip at the first of `points` (grip indices) of a path node
/// or closed form.
pub fn picked_tip(node: &Node, points: &[usize]) -> Option<scene::StrokeSpan> {
    let first = *points.first()?;
    let NodeKind::Shape(s) = &node.kind else {
        return None;
    };
    let tips = crate::vertex_style::grip_tips(
        &crate::vertex_style::vertex_style_path(s)?,
        &s.stroke,
        node.rect,
        node.rotation_deg,
    )?;
    tips.get(first).copied()
}

/// Edit the stroke tips at `points` (grip indices) of a path node or closed
/// form.
pub fn edit_vertex_tips(
    node: &mut Node,
    points: &[usize],
    edit: impl Fn(&mut scene::StrokeSpan),
) -> bool {
    let (rect, rotation) = (node.rect, node.rotation_deg);
    let NodeKind::Shape(s) = &mut node.kind else {
        return false;
    };
    let Some(mut path) = crate::vertex_style::vertex_style_path(s) else {
        return false;
    };
    let mut stroke = s.stroke;
    if !crate::vertex_style::edit_grip_tips(&mut path, &mut stroke, rect, rotation, points, edit) {
        return false;
    }
    crate::vertex_style::store_vertex_style_path(s, path);
    s.stroke = stroke;
    true
}

fn map_text_style(
    node: &mut Node,
    edit: impl FnOnce(
        &mut scene::Typeface,
        &mut f32,
        &mut scene::Rgba,
        &mut scene::TextAlign,
        &mut scene::TextStyle,
    ),
) {
    if let NodeKind::Text(text) = &mut node.kind {
        edit(
            &mut text.family,
            &mut text.size,
            &mut text.color,
            &mut text.align,
            &mut text.style,
        );
        return;
    }
    if let Some(text) = scene::ensure_shape_text(node) {
        edit(
            &mut text.family,
            &mut text.size,
            &mut text.color,
            &mut text.align,
            &mut text.style,
        );
    }
}
