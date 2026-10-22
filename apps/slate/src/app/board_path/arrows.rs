//! Arrowheads and open vector bodies, tessellated together.
use super::*;

/// The arrowhead at end `end` (0 = first point, 1 = last) of an open curve
/// ([`slate_doc::geom::arrow_head`] aimed by [`slate_doc::geom::path_arrow`]),
/// added to its stroke ink with the same feathered edge so it caches with
/// the stroke. `color` is that end's tip on a stroke with per-vertex colors.
pub(crate) fn push_arrow_ink(
    ink: &mut InkMesh,
    bez: &BezPath,
    end: usize,
    width: f32,
    feather: f32,
    color: Option<[f32; 4]>,
) {
    let length = slate_doc::geom::arrow_len(width);
    push_arrow_sized_ink(
        ink,
        bez,
        end,
        [length, length * 0.8],
        feather,
        color.map(|c| [c; 2]),
    );
}

pub(super) fn push_arrow_sized_ink(
    ink: &mut InkMesh,
    bez: &BezPath,
    end: usize,
    size: [f32; 2],
    feather: f32,
    colors: Option<[[f32; 4]; 2]>,
) {
    let Some((tip, into)) = slate_doc::geom::path_arrow_sized(bez, end, size) else {
        return;
    };
    let tri = slate_doc::geom::arrow_head_sized(tip, into, size);
    let unit = |v: [f32; 2]| {
        let l = v[0].hypot(v[1]).max(1e-6);
        [v[0] / l, v[1] / l]
    };
    let first = ink.vertices.len() as u32;
    for p in tri {
        ink.vertices
            .push(vector_ink::InkVertex { pos: p, alpha: 1.0 });
    }
    for i in 0..3 {
        let p = tri[i];
        let a = unit([tri[(i + 1) % 3][0] - p[0], tri[(i + 1) % 3][1] - p[1]]);
        let b = unit([tri[(i + 2) % 3][0] - p[0], tri[(i + 2) % 3][1] - p[1]]);
        let out = unit([-(a[0] + b[0]), -(a[1] + b[1])]);
        let half = ((a[0] * b[0] + a[1] * b[1]).clamp(-1.0, 1.0).acos() * 0.5).sin();
        let d = feather / half.max(0.2);
        ink.vertices.push(vector_ink::InkVertex {
            pos: [p[0] + out[0] * d, p[1] + out[1] * d],
            alpha: 0.0,
        });
    }
    ink.indices.extend([first, first + 1, first + 2]);
    for i in 0..3u32 {
        let j = (i + 1) % 3;
        ink.indices.extend([first + i, first + j, first + 3 + j]);
        ink.indices
            .extend([first + i, first + 3 + j, first + 3 + i]);
    }
    if colors.is_some() || !ink.colors.is_empty() {
        let colors = colors.unwrap_or_else(|| [ink.colors.last().copied().unwrap_or([1.0; 4]); 2]);
        ink.colors.resize(first as usize, [1.0; 4]);
        ink.colors.extend([
            colors[0], colors[1], colors[1], colors[0], colors[1], colors[1],
        ]);
    }
}

/// World-space mesh of a vector path stroke. A path with per-vertex tips
/// (`PathData::vector_widths`) paints its varying width.
#[cfg(test)]
pub(crate) fn vector_stroke_ink(
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    zoom: f32,
) -> InkMesh {
    vector_stroke_ink_for(
        node,
        &shape_path_world_bez(node, shape, path),
        shape,
        path,
        zoom,
    )
}

pub(super) fn vector_stroke_ink_for(
    node: &Node,
    bez: &BezPath,
    shape: &ShapeNode,
    path: &PathData,
    zoom: f32,
) -> InkMesh {
    let arrows = if path.closed {
        [false; 2]
    } else {
        shape.stroke.arrows()
    };
    let ends = shape.stroke.end_caps().map(ink_cap);
    let full = bez;
    let trimmed_body;
    let bez = if arrows.contains(&true) {
        let w = shape.stroke.width;
        trimmed_body = slate_doc::geom::trim_arrow_sizes(
            bez,
            arrows,
            [shape.stroke.arrow_size(0, w), shape.stroke.arrow_size(1, w)],
        );
        &trimmed_body
    } else {
        bez
    };
    let mut style = stroke_style_world(&shape.stroke, zoom);
    let (ink_width, soft) = shape.stroke.paint_profile();
    style.width = ink_width;
    let feather = soft
        + if soft <= 0.0 {
            FEATHER_PX / zoom.max(atlas_core::display::SLATE_BOARD.min)
        } else {
            0.0
        };
    let tolerance = curve_tolerance(zoom);
    let tipped = (soft <= 0.0)
        .then(|| {
            slate_doc::geom::tipped_stroke(
                path,
                &shape.stroke,
                node.rect,
                node.rotation_deg,
                shape.corner,
            )
        })
        .flatten();
    match tipped {
        Some(t) => {
            let sizes = [
                shape
                    .stroke
                    .arrow_size(0, t.widths.first().copied().unwrap_or(style.width)),
                shape
                    .stroke
                    .arrow_size(1, t.widths.last().copied().unwrap_or(style.width)),
            ];
            let body = slate_doc::geom::trim_tipped_arrow_sizes(&t, arrows, sizes);
            let mut ink = vector_ink::stroke_mesh_ends(
                &body.bez,
                &style,
                ends,
                Some(&body.widths),
                body.colors.as_deref(),
                body.ease,
                feather,
                tolerance,
            );
            for end in (0..2).filter(|&i| arrows[i]) {
                let colors = slate_doc::geom::arrow_colors(&t, end, sizes[end][0]);
                push_arrow_sized_ink(&mut ink, &t.bez, end, sizes[end], feather, colors);
            }
            ink
        }
        None => {
            let mut ink = vector_ink::stroke_mesh_ends(
                bez,
                &style,
                ends,
                None,
                None,
                vector_ink::TipEase::Linear,
                feather,
                tolerance,
            );
            for end in (0..2).filter(|&i| arrows[i]) {
                push_arrow_sized_ink(
                    &mut ink,
                    full,
                    end,
                    shape.stroke.arrow_size(end, shape.stroke.width),
                    feather,
                    None,
                );
            }
            ink
        }
    }
}
