use super::*;

/// A small, plain hard stroke can follow its own centerline during camera
/// motion. Keep the authored ends and width, skipping subpixel samples before
/// egui expands the line. Detailed paint still owns all styled strokes.
pub(super) fn paint_motion_hairline(
    painter: &egui::Painter,
    xf: &BoardXf,
    node: &Node,
    shape: &ShapeNode,
    path: &PathData,
    fade: &impl Fn(Color32) -> Color32,
) -> bool {
    let stroke = &shape.stroke;
    let width = stroke.width * xf.z;
    let ppp = painter.ctx().pixels_per_point();
    if stroke.is_none()
        || width * ppp > 1.25
        || node.rect.w.abs().max(node.rect.h.abs()) * xf.z * ppp > 64.0
        || shape.fill.is_some()
        || shape.text.is_some()
        || node.clip.is_some()
        || path.closed
        || !path.extra.is_empty()
        || !path.tips.is_empty()
        || !path.corner_amounts.is_empty()
        || shape.corner != slate_doc::scene::Corner::Square
        || stroke.dash != Dash::Solid
        || stroke.profile != WidthProfile::Uniform
        || stroke.gaussian_blur > 0.0
        || stroke.tween_from.is_some()
        || stroke.arrows().contains(&true)
    {
        return false;
    }
    let Some((PathSeg::Line { to: end }, segments)) = path.segs.split_last() else {
        return false;
    };
    let project = |p| {
        xf.w2s(from_k(slate_doc::geom::world_point(
            p,
            node.rect,
            node.rotation_deg,
        )))
    };
    let mut previous = project(path.start);
    let mut points = Vec::with_capacity(path.segs.len().min(64) + 1);
    points.push(previous);
    let spacing_sq = (0.75 / ppp).powi(2);
    for segment in segments {
        let PathSeg::Line { to } = *segment else {
            return false;
        };
        let point = project(to);
        if point.distance_sq(previous) >= spacing_sq {
            points.push(point);
            previous = point;
        }
    }
    let end = project(*end);
    if end != previous {
        points.push(end);
    }
    if points.len() < 2 {
        return false;
    }
    painter.add(Shape::line(
        points,
        EStroke::new(width, fade(rgba32(stroke.color))),
    ));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_hairline_preserves_endpoints_and_leaves_styled_paths_to_full_paint() {
        let mut scene = slate_doc::Scene::default();
        let (rect, path) = points_to_path_data(
            &[
                Pos2::ZERO,
                Pos2::new(1.0, 0.0),
                Pos2::new(2.0, 0.0),
                Pos2::new(100.0, 20.0),
            ],
            false,
        );
        let node = drawn_path_node(
            &mut scene,
            rect,
            path,
            false,
            default_curve_stroke(Rgba::BLACK),
            PlacedTip {
                width: 2.0,
                color: Rgba::BLACK,
                opacity: 1.0,
            },
            None,
            DrawnTips::None,
        );
        let NodeKind::Shape(shape) = &node.kind else {
            panic!("expected a path shape");
        };
        let path = shape.path.as_ref().unwrap();
        let xf = BoardXf {
            center: Pos2::new(100.0, 100.0),
            offset: Vec2::ZERO,
            z: 0.25,
        };
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            assert!(paint_motion_hairline(
                &painter,
                &xf,
                &node,
                shape,
                path,
                &|c| c
            ));
            let mut dashed = shape.clone();
            dashed.stroke.dash = Dash::Dashed;
            assert!(!paint_motion_hairline(
                &painter,
                &xf,
                &node,
                &dashed,
                path,
                &|c| c
            ));
            let detail = BoardXf { z: 1.0, ..xf };
            assert!(!paint_motion_hairline(
                &painter,
                &detail,
                &node,
                shape,
                path,
                &|c| c
            ));
        });
        let outlines: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|item| match &item.shape {
                Shape::Path(line) => Some(line),
                _ => None,
            })
            .collect();
        assert_eq!(outlines.len(), 1);
        assert_eq!(
            outlines[0].points,
            [Pos2::new(100.0, 100.0), Pos2::new(125.0, 105.0)]
        );
        assert_eq!(outlines[0].stroke.width, shape.stroke.width * xf.z);
    }
}
