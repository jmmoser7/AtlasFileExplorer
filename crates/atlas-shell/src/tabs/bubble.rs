//! One tab/blister silhouette and coverage-AA fill.
use super::*;
use std::sync::Arc;

/// Chrome-tab bubble: convex far edge + concave shoulders that flare into the host.
/// The dock readout blister calls this so it cannot drift from the top-bar tab.
#[derive(Clone, Copy, PartialEq)]
pub enum TabBubble {
    Rounded {
        far_radius: f32,
        shoulder_radius: f32,
    },
    /// A full-height eased shoulder with horizontal tangents at both ends.
    Sloped { run: f32 },
}

impl TabBubble {
    pub fn from_topbar(metrics: &TopBarTokens) -> Self {
        Self::Rounded {
            far_radius: metrics.tab_top_radius,
            shoulder_radius: metrics.tab_shoulder_radius,
        }
    }
}

/// Which edge of the rect is the host the bubble blends into.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TabHost {
    /// Shoulders on the bottom (top-bar tab; blister rising into the canvas).
    Bottom,
    /// Shoulders on the top (blister hanging down into the readout).
    Top,
}

/// Scanline x-bounds of a tab bubble. `host` flips the silhouette vertically.
pub fn tab_scanline_x(rect: Rect, y: f32, bubble: TabBubble, host: TabHost) -> (f32, f32) {
    let y = match host {
        TabHost::Bottom => y,
        TabHost::Top => rect.top() + rect.bottom() - y,
    };
    let depth = rect.height().max(1.0);
    let (far_radius, shoulder_radius) = match bubble {
        TabBubble::Sloped { run } => {
            let t = ((y - rect.top()) / depth).clamp(0.0, 1.0);
            // Invert quintic smoothstep. Both slope and curvature meet the
            // flat edges continuously, so the shoulders read as a pull-up
            // from the host rather than a filleted trapezoid.
            let (mut low, mut high) = (0.0_f64, 1.0_f64);
            for _ in 0..20 {
                let u = (low + high) * 0.5;
                let y = u * u * u * (u * (u * 6.0 - 15.0) + 10.0);
                if y < f64::from(t) {
                    low = u;
                } else {
                    high = u;
                }
            }
            let u = if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                ((low + high) * 0.5) as f32
            };
            let inset = run.max(0.0).min(rect.width() * 0.49) * (1.0 - u);
            return (rect.left() + inset, rect.right() - inset);
        }
        TabBubble::Rounded {
            far_radius,
            shoulder_radius,
        } => (far_radius, shoulder_radius),
    };
    let shoulder = shoulder_radius.max(0.0).min(depth * 0.45);
    let radius = far_radius.max(0.0).min(depth * 0.45);
    let body_left = rect.left() + shoulder;
    let body_right = rect.right() - shoulder;

    if y < rect.top() + radius {
        let dy = y - (rect.top() + radius);
        let dx = (radius * radius - dy * dy).max(0.0).sqrt();
        (body_left + radius - dx, body_right - radius + dx)
    } else if y > rect.bottom() - shoulder && shoulder > 0.0 {
        let t = ((y - (rect.bottom() - shoulder)) / shoulder).clamp(0.0, 1.0);
        let flare = shoulder * (1.0 - (1.0 - t * t).sqrt());
        (body_left - flare, body_right + flare)
    } else {
        (body_left, body_right)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct BubbleKey {
    rect: Rect,
    fill_top: Color32,
    fill_bottom: Color32,
    bubble: TabBubble,
    host: TabHost,
    pixels_per_point: f32,
}

#[derive(Clone, Default)]
struct BubbleCache(Vec<(BubbleKey, Arc<egui::Mesh>)>);

/// One continuous, coverage-antialiased fill; never a stack of scanline strokes.
pub fn paint_tab_bubble(
    painter: &egui::Painter,
    rect: Rect,
    fill_top: Color32,
    fill_bottom: Color32,
    bubble: TabBubble,
    host: TabHost,
) {
    if !rect.is_positive() {
        return;
    }
    let ctx = painter.ctx();
    let id = painter.layer_id().id.with("bubble_fill_cache");
    let key = BubbleKey {
        rect,
        fill_top,
        fill_bottom,
        bubble,
        host,
        pixels_per_point: ctx.pixels_per_point(),
    };
    let cached = ctx
        .data(|data| data.get_temp::<BubbleCache>(id))
        .unwrap_or_default();
    let mesh = if let Some((_, mesh)) = cached.0.iter().find(|(old, _)| *old == key) {
        mesh.clone()
    } else {
        let points = tab_bubble_outline(rect, bubble, host);
        let polygon = points
            .iter()
            .map(|point| [point.x, point.y])
            .collect::<Vec<_>>();
        let (vertices, indices) = vector_ink::fill_triangles(&[polygon]);
        let boundary = (0..vertices.len() as u32).collect::<Vec<_>>();
        let mut mesh = egui::Mesh {
            indices,
            ..Default::default()
        };
        for [x, y] in vertices {
            let t = ((y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            mesh.colored_vertex(Pos2::new(x, y), lerp_color(fill_top, fill_bottom, t));
        }
        crate::mesh_aa::feather_boundary(&mut mesh, &boundary, 1.0 / key.pixels_per_point);
        let mesh = Arc::new(mesh);
        ctx.data_mut(|data| {
            let mut cache = data.get_temp::<BubbleCache>(id).unwrap_or_default();
            if cache.0.len() >= 8 {
                cache.0.remove(0);
            }
            cache.0.push((key, mesh.clone()));
            data.insert_temp(id, cache);
        });
        mesh
    };
    painter.add(egui::Shape::Mesh(mesh));
}

/// Closed silhouette of a tab bubble. The dock blister and the top-bar
/// active tab share this so the accent stroke cannot drift.
pub fn tab_bubble_outline(rect: Rect, bubble: TabBubble, host: TabHost) -> Vec<Pos2> {
    let mut points = Vec::new();
    let samples = (rect.height() * 2.0).ceil().clamp(8.0, 4096.0) as usize;
    for i in (0..=samples).rev() {
        let y = rect.top() + rect.height() * i as f32 / samples as f32;
        points.push(Pos2::new(tab_scanline_x(rect, y, bubble, host).0, y));
    }
    for i in 0..=samples {
        let y = rect.top() + rect.height() * i as f32 / samples as f32;
        points.push(Pos2::new(tab_scanline_x(rect, y, bubble, host).1, y));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sloped_shoulders_flare_continuously_without_a_vertical_stem() {
        let rect = Rect::from_min_size(Pos2::new(30.0, 40.0), Vec2::new(260.0, 30.0));
        let bubble = TabBubble::Sloped { run: 28.0 };
        let mut previous = rect.left() + 28.1;
        for step in 0..=100 {
            let y = rect.top() + rect.height() * step as f32 / 100.0;
            let (left, right) = tab_scanline_x(rect, y, bubble, TabHost::Bottom);
            assert!(
                left < previous,
                "shoulder must slope through its full height"
            );
            assert!((left + right - rect.left() - rect.right()).abs() < 0.001);
            let mirrored =
                tab_scanline_x(rect, rect.top() + rect.bottom() - y, bubble, TabHost::Top);
            assert!((left - mirrored.0).abs() < 0.001);
            previous = left;
        }
        assert!((previous - rect.left()).abs() < 0.001);
    }

    #[test]
    fn tab_fill_has_coverage_fringe_and_reuses_the_resting_mesh() {
        let ctx = egui::Context::default();
        let layer = egui::LayerId::background();
        let rect = Rect::from_min_size(Pos2::new(0.25, 0.25), Vec2::new(260.0, 30.0));
        let draw = |rect| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                paint_tab_bubble(
                    &ctx.layer_painter(layer),
                    rect,
                    Color32::GRAY,
                    Color32::DARK_GRAY,
                    TabBubble::Sloped { run: 28.0 },
                    TabHost::Bottom,
                );
            });
            ctx.data(|data| data.get_temp::<BubbleCache>(layer.id.with("bubble_fill_cache")))
                .unwrap()
                .0
                .last()
                .unwrap()
                .1
                .clone()
        };
        let first = draw(rect);
        assert!(first.is_valid() && !first.is_empty());
        assert!(
            first.vertices.iter().any(|v| v.color.a() == 0),
            "mesh needs transparent coverage vertices"
        );
        assert!(first.vertices.iter().any(|v| v.color.a() == 255));
        assert!(Arc::ptr_eq(&first, &draw(rect)));
        assert!(!Arc::ptr_eq(
            &first,
            &draw(rect.translate(Vec2::new(0.25, 0.0)))
        ));
    }
}
