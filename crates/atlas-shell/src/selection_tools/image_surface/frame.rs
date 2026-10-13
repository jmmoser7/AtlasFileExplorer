//! The image, apron, authored frame stroke and selection use one silhouette.
use super::*;
use vector_ink::kurbo::{BezPath, RoundedRect, RoundedRectRadii, Shape};

const SELECTION_GAP: f32 = 2.0;

#[derive(Clone, Debug)]
pub struct ImageFrameGeometry {
    pub body: Vec<Vec<Pos2>>,
    pub selection: Vec<Vec<Pos2>>,
}

#[derive(Clone, PartialEq)]
struct FrameKey {
    image: Vec<Pos2>,
    apron: Option<Rect>,
    join_corners: bool,
    zoom: f32,
    stroke_width: f32,
    selection_width: f32,
    pixels_per_point: f32,
}

#[derive(Clone)]
struct CachedFrame {
    key: FrameKey,
    geometry: Arc<ImageFrameGeometry>,
}

/// The app supplies the model's real image outline; shell supplies only the
/// attached editor's geometry. The selection centerline clears the full stroke.
#[allow(clippy::too_many_arguments)]
pub fn image_frame_geometry(
    ctx: &egui::Context,
    id: Id,
    image: &[Pos2],
    apron: Option<Rect>,
    join_corners: bool,
    zoom: f32,
    stroke_width: f32,
    selection_width: f32,
) -> Arc<ImageFrameGeometry> {
    let key = FrameKey {
        image: image.to_vec(),
        apron,
        join_corners,
        zoom,
        stroke_width,
        selection_width,
        pixels_per_point: ctx.pixels_per_point(),
    };
    if let Some(cached) = ctx
        .data(|data| data.get_temp::<CachedFrame>(id))
        .filter(|cached| cached.key == key)
    {
        return cached.geometry;
    }
    let mut body = vec![image.to_vec()];
    if let Some(mut apron) = apron.filter(|rect| rect.is_positive()) {
        let host = Rect::from_points(image);
        apron.min.y = if join_corners {
            host.center().y
        } else {
            host.bottom() - 1.0 / key.pixels_per_point
        };
        let radius = canvas_scale::px(APRON_RADIUS, zoom).round().min(255.0) as f64;
        let rect = RoundedRect::new(
            apron.left() as f64,
            apron.top() as f64,
            apron.right() as f64,
            apron.bottom() as f64,
            RoundedRectRadii::new(0.0, 0.0, radius, radius),
        );
        let panel = vector_ink::flatten_contours(&rect.to_path(0.1), 0.1);
        let host = vec![image.iter().map(|p| [p.x, p.y]).collect()];
        body = vector_ink::boolean_union(&host, &panel)
            .into_iter()
            .filter_map(|polygon| polygon.into_iter().next())
            .map(|ring| ring.into_iter().map(Pos2::from).collect())
            .collect();
    }
    let padding =
        stroke_width.max(0.0) * 0.5 + selection_width * 0.5 + canvas_scale::px(SELECTION_GAP, zoom);
    let selection = body
        .iter()
        .filter_map(|ring| outset(ring, padding))
        .collect();
    let geometry = Arc::new(ImageFrameGeometry { body, selection });
    ctx.data_mut(|data| {
        data.insert_temp(
            id,
            CachedFrame {
                key,
                geometry: geometry.clone(),
            },
        )
    });
    geometry
}

fn outset(points: &[Pos2], amount: f32) -> Option<Vec<Pos2>> {
    let first = points.first()?;
    let mut path = BezPath::new();
    path.move_to((first.x as f64, first.y as f64));
    for point in &points[1..] {
        path.line_to((point.x as f64, point.y as f64));
    }
    path.close_path();
    let style = vector_ink::StrokeStyle {
        width: amount * 2.0,
        cap: vector_ink::Cap::Butt,
        join: vector_ink::Join::Round,
        taper: None,
        dash: None,
    };
    let contours =
        vector_ink::flatten_contours(&vector_ink::stroke_outline(&path, &style, 0.1), 0.1);
    contours
        .into_iter()
        .max_by(|a, b| area(a).abs().total_cmp(&area(b).abs()))
        .map(|ring| ring.into_iter().map(Pos2::from).collect())
}

fn area(points: &[[f32; 2]]) -> f32 {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum::<f32>()
        * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apron_and_image_have_one_exterior_with_no_stroke_across_the_join() {
        let ctx = egui::Context::default();
        for zoom in [0.5, 1.0, 2.0] {
            let host = Rect::from_min_size(Pos2::new(40.0, 50.0), Vec2::new(480.0, 240.0) * zoom);
            let corners = vec![
                host.left_top(),
                host.right_top(),
                host.right_bottom(),
                host.left_bottom(),
            ];
            let apron =
                Rect::from_min_size(host.left_bottom(), Vec2::new(host.width(), 100.0 * zoom));
            let frame = image_frame_geometry(
                &ctx,
                Id::new(("frame-test", zoom.to_bits())),
                &corners,
                Some(apron),
                true,
                zoom,
                6.0 * zoom,
                1.5 * zoom,
            );
            assert_eq!(
                frame.body.len(),
                1,
                "joined assembly must have one exterior"
            );
            assert_eq!(frame.selection.len(), 1);
            for ring in &frame.body {
                for (a, b) in ring
                    .iter()
                    .zip(ring.iter().cycle().skip(1))
                    .take(ring.len())
                {
                    assert!(
                        !((a.y - host.bottom()).abs() < 0.01
                            && (b.y - host.bottom()).abs() < 0.01
                            && (a.x - b.x).abs() > host.width() * 0.5),
                        "no horizontal seam across the image"
                    );
                }
            }
            let body = Rect::from_points(&frame.body[0]);
            let ring = Rect::from_points(&frame.selection[0]);
            let clearance = (3.0 + 0.75 + 2.0) * zoom;
            for actual in [
                body.left() - ring.left(),
                body.top() - ring.top(),
                ring.right() - body.right(),
                ring.bottom() - body.bottom(),
            ] {
                assert!((actual - clearance).abs() < 0.15 * zoom);
            }
        }
    }

    #[test]
    fn resting_geometry_is_cached_and_stroke_width_expands_selection_only() {
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        let outline = vec![
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        let id = Id::new("frame-cache-test");
        let frame = |width| image_frame_geometry(&ctx, id, &outline, None, true, 1.0, width, 1.5);
        let first = frame(0.0);
        assert!(Arc::ptr_eq(&first, &frame(0.0)));
        let wide = frame(12.0);
        assert!(!Arc::ptr_eq(&first, &wide));
        assert_eq!(first.body, wide.body);
        let old = Rect::from_points(&first.selection[0]);
        let new = Rect::from_points(&wide.selection[0]);
        assert!((old.left() - new.left() - 6.0).abs() < 0.01);
        assert!((new.right() - old.right() - 6.0).abs() < 0.01);
    }
}
