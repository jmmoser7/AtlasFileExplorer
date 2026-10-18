//! Cached native clipping for every media body, including text and live textures.
use super::*;
use egui::epaint::{ClippedShape, Primitive, Vertex};
use std::sync::Mutex;

#[derive(Default)]
struct Cached {
    identity: Option<Arc<()>>,
    origin: Pos2,
    source: Vec<ClippedShape>,
    result: Vec<egui::Shape>,
    ppp: f32,
    rebuilds: u64,
}

/// Mark the existing painter; the application keeps its ordinary media painter.
pub fn media_host_shape_start(painter: &egui::Painter) -> usize {
    painter
        .ctx()
        .graphics(|g| g.get(painter.layer_id()).map_or(0, |p| p.next_idx().0))
}

/// Replace the captured body with truly clipped triangles. Unchanged frames
/// reuse the complete meshes; texture updates retain their texture identities.
/// Raw boundaries use the shared native 4x MSAA policy (PAINT.md).
pub fn clip_media_host_shapes(
    painter: &egui::Painter,
    id: Id,
    start: usize,
    geometry: Arc<MediaHostGeometry>,
) {
    if geometry.cutouts.is_empty() {
        return;
    }
    let ctx = painter.ctx();
    let ppp = ctx.pixels_per_point();
    let state = ctx.data_mut(|d| {
        if let Some(state) = d.get_temp::<Arc<Mutex<Cached>>>(id) {
            state
        } else {
            let state = Arc::new(Mutex::new(Cached::default()));
            d.insert_temp(id, state.clone());
            state
        }
    });
    // Move the buffers out briefly; no cache lock is held across egui access.
    let mut cached = std::mem::take(&mut *state.lock().unwrap_or_else(|e| e.into_inner()));
    let compatible = cached.ppp == ppp
        && cached
            .identity
            .as_ref()
            .is_some_and(|identity| Arc::ptr_eq(identity, &geometry.identity));
    let delta = geometry.origin - cached.origin;
    if compatible && delta != Vec2::ZERO {
        let transform = egui::emath::TSTransform::from_translation(delta);
        // Usually uniquely owned once the previous frame is consumed. Reuse
        // mesh/path buffers; a deliberately retained frame gets copy-on-write.
        for source in &mut cached.source {
            source.shape.transform(transform);
        }
    }
    let unchanged = compatible
        && ctx.graphics(|graphics| {
            graphics.get(painter.layer_id()).is_some_and(|list| {
                let entries = list.all_entries().skip(start);
                entries.len() == cached.source.len()
                    && entries
                        .zip(&cached.source)
                        .all(|(a, b)| same_shape(&a.shape, &b.shape))
            })
        });
    if unchanged {
        if delta != Vec2::ZERO {
            let transform = egui::emath::TSTransform::from_translation(delta);
            for shape in &mut cached.result {
                shape.transform(transform);
            }
        }
    } else {
        cached.source.clear();
        ctx.graphics(|g| {
            if let Some(list) = g.get(painter.layer_id()) {
                cached
                    .source
                    .extend(list.all_entries().skip(start).cloned());
            }
        });
        if cached.source.is_empty() {
            *state.lock().unwrap_or_else(|e| e.into_inner()) = cached;
            return;
        }
        let Cached { source, result, .. } = &mut cached;
        result.clear();
        result.extend(
            source
                .iter()
                .map(|entry| clipped_shape(ctx, entry, &geometry)),
        );
        cached.rebuilds += 1;
    }
    cached.origin = geometry.origin;
    cached.identity = Some(geometry.identity.clone());
    cached.ppp = ppp;
    ctx.graphics_mut(|graphics| {
        if let Some(list) = graphics.get_mut(painter.layer_id()) {
            for (offset, shape) in cached.result.iter().enumerate() {
                list.mutate_shape(egui::layers::ShapeIdx(start + offset), |entry| {
                    entry.shape = shape.clone()
                });
            }
        }
    });
    *state.lock().unwrap_or_else(|e| e.into_inner()) = cached;
}

// The board camera can round equivalent screen projections differently. This
// tolerance is under 1/300 point and cannot change UVs, style, or content.
fn same_shape(a: &egui::Shape, b: &egui::Shape) -> bool {
    use egui::Shape::*;
    let near = |a: Pos2, b: Pos2| (a - b).length_sq() < 0.00001;
    let points =
        |a: &[Pos2], b: &[Pos2]| a.len() == b.len() && a.iter().zip(b).all(|(a, b)| near(*a, *b));
    match (a, b) {
        (Mesh(a), Mesh(b)) => {
            a.texture_id == b.texture_id
                && a.indices == b.indices
                && a.vertices.len() == b.vertices.len()
                && a.vertices
                    .iter()
                    .zip(&b.vertices)
                    .all(|(a, b)| near(a.pos, b.pos) && a.uv == b.uv && a.color == b.color)
        }
        (Vec(a), Vec(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_shape(a, b)),
        (Path(a), Path(b)) => {
            a.closed == b.closed
                && a.fill == b.fill
                && a.stroke == b.stroke
                && points(&a.points, &b.points)
        }
        (
            LineSegment {
                points: a,
                stroke: sa,
            },
            LineSegment {
                points: b,
                stroke: sb,
            },
        ) => sa == sb && points(a, b),
        (Rect(a), Rect(b)) if near(a.rect.min, b.rect.min) && near(a.rect.max, b.rect.max) => {
            let mut a = a.clone();
            a.rect = b.rect;
            a == *b
        }
        (Text(a), Text(b)) if near(a.pos, b.pos) => {
            let mut a = a.clone();
            a.pos = b.pos;
            a == *b
        }
        (Circle(a), Circle(b)) if near(a.center, b.center) => {
            let mut a = *a;
            a.center = b.center;
            a == *b
        }
        (Ellipse(a), Ellipse(b)) if near(a.center, b.center) => {
            let mut a = *a;
            a.center = b.center;
            a == *b
        }
        (QuadraticBezier(a), QuadraticBezier(b)) if points(&a.points, &b.points) => {
            let mut a = a.clone();
            a.points = b.points;
            a == *b
        }
        (CubicBezier(a), CubicBezier(b)) if points(&a.points, &b.points) => {
            let mut a = a.clone();
            a.points = b.points;
            a == *b
        }
        _ => a == b,
    }
}

fn clipped_shape(
    ctx: &egui::Context,
    source: &ClippedShape,
    geometry: &MediaHostGeometry,
) -> egui::Shape {
    let bounds = source.shape.visual_bounding_rect();
    let cuts: Vec<_> = geometry
        .cutouts
        .iter()
        .filter(|cut| bounds.intersects(Rect::from_points(cut)))
        .collect();
    if cuts.is_empty() {
        return source.shape.clone();
    }
    // Build complete geometry: a later pan/viewport expansion changes the
    // external scissor but must not resurrect content culled during caching.
    let frame_bounds = geometry.outline.first().map(|ring| Rect::from_points(ring));
    let stroke = match &source.shape {
        egui::Shape::Rect(rect)
            if rect.fill == Color32::TRANSPARENT
                && rect.stroke_kind == egui::StrokeKind::Middle
                && frame_bounds == Some(rect.rect) =>
        {
            Some(rect.stroke.into())
        }
        egui::Shape::Path(path)
            if path.fill == Color32::TRANSPARENT
                && path.closed
                && frame_bounds == Some(Rect::from_points(&path.points)) =>
        {
            Some(path.stroke.clone())
        }
        _ => None,
    };
    let notched_stroke = stroke.is_some();
    let shape = stroke.map_or_else(
        || source.shape.clone(),
        |stroke| {
            egui::Shape::Vec(
                geometry
                    .outline
                    .iter()
                    .map(|outline| {
                        egui::Shape::Path(egui::epaint::PathShape {
                            points: outline.clone(),
                            closed: true,
                            fill: Color32::TRANSPARENT,
                            stroke: stroke.clone(),
                        })
                    })
                    .collect(),
            )
        },
    );
    let complete = ClippedShape {
        clip_rect: Rect::EVERYTHING,
        shape,
    };
    let mut shapes: Vec<_> = ctx
        .tessellate(vec![complete], ctx.pixels_per_point())
        .into_iter()
        .map(|primitive| match primitive.primitive {
            Primitive::Mesh(mesh) => egui::Shape::mesh(if notched_stroke {
                mesh
            } else {
                clip_mesh(&mesh, &cuts)
            }),
            // Native media uses texture meshes; leave a foreign callback intact.
            Primitive::Callback(callback) => egui::Shape::Callback(callback),
        })
        .collect();
    match shapes.len() {
        0 => egui::Shape::Noop,
        1 => shapes.pop().unwrap(),
        _ => egui::Shape::Vec(shapes),
    }
}

fn clip_mesh(source: &egui::Mesh, cuts: &[&Vec<Pos2>]) -> egui::Mesh {
    let cuts: Vec<_> = cuts
        .iter()
        .map(|cut| {
            (
                Rect::from_points(cut),
                vec![cut.iter().map(|p| [p.x, p.y]).collect()],
            )
        })
        .collect();
    let mut mesh = egui::Mesh::with_texture(source.texture_id);
    mesh.vertices.extend_from_slice(&source.vertices);
    for tri in source.indices.as_chunks::<3>().0 {
        let original = [
            source.vertices[tri[0] as usize],
            source.vertices[tri[1] as usize],
            source.vertices[tri[2] as usize],
        ];
        let positions = original.map(|v| v.pos);
        let bounds = Rect::from_points(&positions);
        let touching: Vec<_> = cuts
            .iter()
            .filter(|(rect, _)| bounds.intersects(*rect))
            .collect();
        if touching.is_empty() {
            mesh.indices.extend_from_slice(tri);
            continue;
        }
        let mut polygons = vec![vec![positions.iter().map(|p| [p.x, p.y]).collect()]];
        for (_, cut) in touching {
            polygons = polygons
                .into_iter()
                // Render triangles must retain their shared edges. The CAD
                // cleaner intentionally moves/collapses nearby boundary points.
                .flat_map(|p| vector_ink::boolean_difference_raw(&p, cut))
                .collect();
        }
        for polygon in polygons {
            let (positions, indices) = vector_ink::fill_triangles(&polygon);
            let offset = mesh.vertices.len() as u32;
            mesh.vertices.extend(
                positions
                    .into_iter()
                    .map(|p| interpolate(original, Pos2::from(p))),
            );
            mesh.indices.extend(indices.into_iter().map(|i| i + offset));
        }
    }
    mesh
}

fn interpolate(v: [Vertex; 3], p: Pos2) -> Vertex {
    let a = v[1].pos - v[0].pos;
    let b = v[2].pos - v[0].pos;
    let q = p - v[0].pos;
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let denominator = cross(a, b);
    if denominator.abs() < 1e-8 {
        return Vertex { pos: p, ..v[0] };
    }
    let wb = cross(q, b) / denominator;
    let wc = cross(a, q) / denominator;
    let weights = [1.0 - wb - wc, wb, wc];
    let uv = v
        .iter()
        .zip(weights)
        .fold(Vec2::ZERO, |sum, (v, w)| sum + v.uv.to_vec2() * w)
        .to_pos2();
    let color: [u8; 4] = std::array::from_fn(|channel| {
        v.iter()
            .zip(weights)
            .map(|(v, w)| v.color.to_array()[channel] as f32 * w)
            .sum::<f32>()
            .round()
            .clamp(0.0, 255.0) as u8
    });
    Vertex {
        pos: p,
        uv,
        color: Color32::from_rgba_premultiplied(color[0], color[1], color[2], color[3]),
    }
}

#[cfg(test)]
mod tests;
