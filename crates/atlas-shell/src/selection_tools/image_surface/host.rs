//! The one transient media silhouette: real side scallops and a raised tool notch.
use super::*;
use std::sync::Mutex;

/// A typed flow-port site, supplied by the document's wire-host owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediaPortNotch {
    pub center: Pos2,
    pub inward: Vec2,
    /// Matching camera-projected board-unit dot radius, before radial clearance.
    pub radius: f32,
    /// Half the straight capsule segment in screen pixels; zero is the ordinary circular notch.
    pub half_span: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaHostGeometry {
    pub outline: Vec<Vec<Pos2>>,
    pub cutouts: Vec<Vec<Pos2>>,
    pub(super) origin: Pos2,
    pub(super) identity: Arc<()>,
}

#[derive(Clone, PartialEq)]
struct Key {
    image: Vec<Pos2>,
    ports: Vec<MediaPortNotch>,
    tab: Option<Rect>,
    zoom: f32,
    ppp: f32,
}
#[derive(Clone)]
struct Cached {
    key: Key,
    geometry: Arc<MediaHostGeometry>,
}

/// Subtracts chrome from actual host pixels. No canvas-colored cover is drawn.
/// The resulting boundary is shared by texture clipping, stroke and shadow.
pub fn media_host_geometry(
    ctx: &egui::Context,
    id: Id,
    image: &[Pos2],
    ports: &[MediaPortNotch],
    tab: Option<Rect>,
    zoom: f32,
) -> Arc<MediaHostGeometry> {
    let origin = image.first().copied().unwrap_or(Pos2::ZERO);
    let state = ctx.data_mut(|d| {
        if let Some(state) = d.get_temp::<Arc<Mutex<Option<Cached>>>>(id) {
            state
        } else {
            let state = Arc::new(Mutex::new(None));
            d.insert_temp(id, state.clone());
            state
        }
    });
    let cached = state.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(mut cached) = cached {
        let same_geometry = cached.key.zoom == zoom
            && cached.key.ppp == ctx.pixels_per_point()
            && cached.key.image.len() == image.len()
            && cached.key.ports.len() == ports.len()
            && cached
                .key
                .image
                .iter()
                .zip(image)
                .all(|(a, b)| (*a - (*b - origin.to_vec2())).length_sq() < 0.00001)
            && cached.key.ports.iter().zip(ports).all(|(a, b)| {
                (a.center - (b.center - origin.to_vec2())).length_sq() < 0.00001
                    && a.inward == b.inward
                    && a.radius == b.radius
                    && a.half_span == b.half_span
            })
            && match (cached.key.tab, tab) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    (a.min - (b.min - origin.to_vec2())).length_sq() < 0.00001
                        && (a.max - (b.max - origin.to_vec2())).length_sq() < 0.00001
                }
                _ => false,
            };
        if same_geometry {
            if cached.geometry.origin != origin {
                let geometry = Arc::make_mut(&mut cached.geometry);
                let delta = origin - geometry.origin;
                for p in geometry
                    .outline
                    .iter_mut()
                    .chain(&mut geometry.cutouts)
                    .flatten()
                {
                    *p += delta;
                }
                geometry.origin = origin;
            }
            let geometry = cached.geometry.clone();
            *state.lock().unwrap_or_else(|e| e.into_inner()) = Some(cached);
            return geometry;
        }
    }
    let key = Key {
        image: image.iter().map(|p| *p - origin.to_vec2()).collect(),
        ports: ports
            .iter()
            .map(|p| MediaPortNotch {
                center: p.center - origin.to_vec2(),
                ..*p
            })
            .collect(),
        tab: tab.map(|r| r.translate(-origin.to_vec2())),
        zoom,
        ppp: ctx.pixels_per_point(),
    };
    let mut cutouts: Vec<Vec<Pos2>> = ports.iter().map(|port| scallop(*port)).collect();
    if let Some(mut tab) = tab {
        // Cross the edge by a physical pixel so the subtraction is unambiguous.
        tab.max.y += 1.0 / key.ppp;
        cutouts.push(crate::tabs::tab_bubble_outline(
            tab,
            crate::tabs::TabBubble::Sloped {
                run: super::super::image_presentation::BLISTER_SHOULDER_RUN * zoom,
            },
            crate::tabs::TabHost::Bottom,
        ));
    }
    let mut polygons = vec![vec![image.iter().map(|p| [p.x, p.y]).collect()]];
    for cutout in &cutouts {
        let cutter = vec![cutout.iter().map(|p| [p.x, p.y]).collect()];
        polygons = polygons
            .into_iter()
            .flat_map(|polygon| vector_ink::boolean_difference(&polygon, &cutter))
            .collect();
    }
    let outline = polygons
        .into_iter()
        .flat_map(|polygon| polygon.into_iter())
        .map(|ring| ring.into_iter().map(Pos2::from).collect())
        .collect();
    let geometry = Arc::new(MediaHostGeometry {
        outline,
        cutouts,
        origin,
        identity: Arc::new(()),
    });
    *state.lock().unwrap_or_else(|e| e.into_inner()) = Some(Cached {
        key,
        geometry: geometry.clone(),
    });
    geometry
}

fn scallop(port: MediaPortNotch) -> Vec<Pos2> {
    let half_span = port.half_span.max(0.0);
    let (radius, shoulder) = crate::selection_tools::hit_regions::wire_notch_geometry(port.radius);
    let inward = port.inward.normalized();
    let along = Vec2::new(-inward.y, inward.x);
    let point = |x: f32, y: f32| port.center + inward * x + along * y;
    let mut path = vector_ink::kurbo::BezPath::new();
    let p = |x, y| {
        let p = point(x, y);
        (p.x as f64, p.y as f64)
    };
    path.move_to(p(-radius * 2.0, -radius - shoulder - half_span));
    path.line_to(p(0.0, -radius - shoulder - half_span));
    let join_x = radius * 0.4;
    let join_y = (radius * radius - join_x * join_x).sqrt();
    path.curve_to(
        p(0.0, -radius - shoulder * 0.45 - half_span),
        p(join_x * 0.32, -join_y - radius * 0.12 - half_span),
        p(join_x, -join_y - half_span),
    );
    let angle = (join_y / radius).asin();
    for index in 1..=16 {
        let theta = -angle + angle * index as f32 / 16.0;
        path.line_to(p(radius * theta.cos(), radius * theta.sin() - half_span));
    }
    path.line_to(p(radius, half_span));
    for index in 1..=16 {
        let theta = angle * index as f32 / 16.0;
        path.line_to(p(radius * theta.cos(), radius * theta.sin() + half_span));
    }
    path.curve_to(
        p(join_x * 0.32, join_y + radius * 0.12 + half_span),
        p(0.0, radius + shoulder * 0.45 + half_span),
        p(0.0, radius + shoulder + half_span),
    );
    path.line_to(p(-radius * 2.0, radius + shoulder + half_span));
    path.close_path();
    vector_ink::flatten_contours(&path, 0.15)
        .into_iter()
        .next()
        .unwrap_or_default()
        .into_iter()
        .map(Pos2::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_port_scallop_keeps_reviewed_radial_clearance() {
        let points = scallop(MediaPortNotch {
            center: Pos2::ZERO,
            inward: Vec2::X,
            radius: 6.0,
            half_span: 0.0,
        });
        let inward_extent = points
            .iter()
            .map(|point| point.x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((inward_extent - 7.4).abs() < 0.01);
        assert!((inward_extent - 6.0 - 1.4).abs() < 0.01);
    }

    #[test]
    fn capped_capsule_scallops_preserve_clearance_and_invalidate_cached_blank_or_populated_hosts() {
        let ctx = egui::Context::default();
        let image = [
            Pos2::ZERO,
            Pos2::new(128.0, 0.0),
            Pos2::new(128.0, 160.0),
            Pos2::new(0.0, 160.0),
        ];
        for populated in [false, true] {
            let id = Id::new(("capsule-host", populated));
            let mut previous = None;
            for (incoming, height) in [(1, 12.0f32), (2, 16.0), (4, 24.0), (7, 24.0)] {
                let half_span = (height - 12.0) * 0.5;
                let notch = MediaPortNotch {
                    center: Pos2::new(-1.427, 64.0),
                    inward: Vec2::X,
                    radius: 6.0,
                    half_span,
                };
                let geometry = media_host_geometry(&ctx, id, &image, &[notch], None, 1.0);
                let cutout = &geometry.cutouts[0];
                let extent = cutout
                    .iter()
                    .map(|point| point.x - notch.center.x)
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!((extent - 7.4).abs() < 0.02);
                assert!(cutout
                    .iter()
                    .any(|point| point.y < notch.center.y - half_span - 7.4));
                if incoming == 7 {
                    assert!(
                        Arc::ptr_eq(previous.as_ref().unwrap(), &geometry),
                        "more than four inputs reuse capped capsule geometry"
                    );
                }
                previous = Some(geometry);
            }
        }
    }

    #[test]
    fn port_notch_consumes_the_already_projected_radius_exactly_once() {
        let ctx = egui::Context::default();
        let image = [
            Pos2::ZERO,
            Pos2::new(128.0, 0.0),
            Pos2::new(128.0, 128.0),
            Pos2::new(0.0, 128.0),
        ];
        let notch = MediaPortNotch {
            center: Pos2::new(0.0, 64.0),
            inward: Vec2::X,
            radius: 6.0,
            half_span: 0.0,
        };
        let normal = media_host_geometry(
            &ctx,
            Id::new("port-screen-size"),
            &image,
            &[notch],
            None,
            1.0,
        );
        let zoomed = media_host_geometry(
            &ctx,
            Id::new("port-screen-size"),
            &image,
            &[notch],
            None,
            128.0,
        );
        assert_eq!(
            normal.cutouts, zoomed.cutouts,
            "caller supplies the projected board radius; the cutout must not scale it twice"
        );
        let tapered = media_host_geometry(
            &ctx,
            Id::new("port-screen-size"),
            &image,
            &[MediaPortNotch {
                radius: 3.0,
                ..notch
            }],
            None,
            128.0,
        );
        assert!(!Arc::ptr_eq(&zoomed.identity, &tapered.identity));
        assert_ne!(zoomed.cutouts, tapered.cutouts);
        let extent = |geometry: &MediaHostGeometry| {
            geometry.cutouts[0]
                .iter()
                .map(|p| p.x)
                .fold(f32::NEG_INFINITY, f32::max)
        };
        assert!((extent(&tapered) - 3.7).abs() < 0.01);
        assert!((extent(&zoomed) - 7.4).abs() < 0.01);
    }

    #[test]
    fn cutouts_remove_host_pixels_and_reuse_one_cached_silhouette() {
        let ctx = egui::Context::default();
        let host = Rect::from_min_size(Pos2::ZERO, Vec2::new(680.0, 400.0));
        let image = vec![
            host.left_top(),
            host.right_top(),
            host.right_bottom(),
            host.left_bottom(),
        ];
        let ports = [MediaPortNotch {
            center: Pos2::new(0.0, 30.0),
            inward: Vec2::X,
            radius: 6.0,
            half_span: 0.0,
        }];
        let tab = Some(super::super::super::blister_tab_rect(host, 1.0, 7));
        let build = || media_host_geometry(&ctx, Id::new("host-test"), &image, &ports, tab, 1.0);
        let first = build();
        assert!(Arc::ptr_eq(&first, &build()));
        let rings: Vec<_> = first
            .outline
            .iter()
            .map(|r| r.iter().map(|p| [p.x, p.y]).collect())
            .collect();
        for p in [[4.0, 30.0], [340.0, 390.0]] {
            assert!(!vector_ink::point_in_polygon(&rings, p), "{p:?}");
        }
        for p in [[12.0, 30.0], [340.0, 350.0], [20.0, 390.0]] {
            assert!(vector_ink::point_in_polygon(&rings, p), "{p:?}");
        }
        let delta = Vec2::new(37.25, -29.5);
        let shifted = image.iter().map(|p| *p + delta).collect::<Vec<_>>();
        let shifted_ports = ports.map(|p| MediaPortNotch {
            center: p.center + delta,
            ..p
        });
        let moved = media_host_geometry(
            &ctx,
            Id::new("host-test"),
            &shifted,
            &shifted_ports,
            tab.map(|t| t.translate(delta)),
            1.0,
        );
        assert!(
            Arc::ptr_eq(&first.identity, &moved.identity),
            "panning must not rerun boolean geometry"
        );
        for (old, new) in first
            .outline
            .iter()
            .flatten()
            .zip(moved.outline.iter().flatten())
        {
            assert!((*new - (*old + delta)).length_sq() < 0.00001);
        }
        let id = Id::new("host-pan-buffers");
        let pointer = {
            let g = media_host_geometry(&ctx, id, &image, &ports, tab, 1.0);
            g.outline[0].as_ptr()
        };
        let moved = media_host_geometry(
            &ctx,
            id,
            &shifted,
            &shifted_ports,
            tab.map(|t| t.translate(delta)),
            1.0,
        );
        assert_eq!(
            pointer,
            moved.outline[0].as_ptr(),
            "panning reuses the cached contour allocation"
        );
    }
}
