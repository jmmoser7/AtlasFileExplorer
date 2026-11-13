//! Connector painting and transient host geometry, using the shared route owner.
use super::*;
use std::time::{Duration, Instant};

mod motion;
mod route_cache;
pub(crate) use route_cache::WireRouteCache;

const WIRE_BATCH_VERTICES: usize = 60_000;

#[derive(Default)]
struct WireStrokeBatch {
    mesh: egui::Mesh,
    trace: WirePaintTrace,
    motion_pixels_per_world: Option<f32>,
}

#[derive(Default)]
struct WirePaintTrace {
    route: Duration,
    key: Duration,
    cache: Duration,
    build: Duration,
    append: Duration,
    flush: Duration,
    route_calls: usize,
    route_hits: usize,
    meshes: usize,
    vertices: usize,
    indices: usize,
}

impl WirePaintTrace {
    fn start<const TRACE: bool>() -> Option<Instant> {
        TRACE.then(Instant::now)
    }

    fn record(started: Option<Instant>, elapsed: &mut Duration) {
        if let Some(started) = started {
            *elapsed += started.elapsed();
        }
    }

    fn emit(&self, routes: &WireRouteCache) {
        let (bezier_entries, bezier_capacity, bezier_payload_bytes) = routes.bezier_stats();
        let ms = |time: Duration| time.as_secs_f64() * 1_000.0;
        atlas_core::session_log::event(
            "slate.board.wires.detail",
            &format!(
                "route_ms={:.3} key_ms={:.3} lookup_ms={:.3} build_ms={:.3} append_ms={:.3} flush_ms={:.3} route_hits={} route_calls={} meshes={} vertices={} indices={} bezier_entries={} bezier_capacity={} bezier_payload_bytes={}",
                ms(self.route), ms(self.key), ms(self.cache.saturating_sub(self.build)),
                ms(self.build), ms(self.append), ms(self.flush), self.route_hits,
                self.route_calls, self.meshes, self.vertices, self.indices,
                bezier_entries, bezier_capacity, bezier_payload_bytes,
            ),
        );
    }
}

impl WireStrokeBatch {
    fn flush<const TRACE: bool>(&mut self, painter: &egui::Painter) {
        let started = WirePaintTrace::start::<TRACE>();
        let mesh = std::mem::take(&mut self.mesh);
        if !mesh.indices.is_empty() {
            if TRACE {
                self.trace.meshes += 1;
                self.trace.vertices += mesh.vertices.len();
                self.trace.indices += mesh.indices.len();
            }
            painter.add(egui::Shape::mesh(mesh));
        }
        WirePaintTrace::record(started, &mut self.trace.flush);
    }
}

impl SlateApp {
    pub(crate) fn paint_board_connectors(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        nodes: &[Node],
    ) {
        let cadence = if nodes.len() >= 10_000 { 4 } else { 16 };
        if self.frame_no.is_multiple_of(cadence) && self.session_log.persist_enabled() {
            self.paint_board_connector_batch::<true>(ui, painter, xf, nodes);
        } else {
            self.paint_board_connector_batch::<false>(ui, painter, xf, nodes);
        }
    }

    fn paint_board_connector_batch<const TRACE: bool>(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        nodes: &[Node],
    ) {
        let mut batch = WireStrokeBatch {
            motion_pixels_per_world: self
                .board_camera_in_motion()
                .then(|| xf.z * painter.ctx().pixels_per_point()),
            ..Default::default()
        };
        let theme_wire = self.theme_wire_rgba();
        let field = self.field_selection();
        let media = self.prepared_media_chrome_host();
        let bundles = self.grasshopper_bundles(nodes);
        let bundled: std::collections::HashSet<_> = bundles
            .iter()
            .flat_map(|bundle| bundle.wires.iter().copied())
            .collect();
        for node in nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            if bundled.contains(&node.id) {
                continue;
            }
            // Existing chrome can belong to the authored host behind a
            // presence preview. Keep that node's ordinary paint ordering.
            if field == Some(node.id)
                || media == Some(node.id)
                || self.presence_preview(node.id).is_some()
            {
                batch.flush::<TRACE>(painter);
                self.paint_board_node(ui, painter, xf, node, true);
            } else {
                self.paint_connector_in_batch::<TRACE>(
                    painter, xf, node, conn, theme_wire, &mut batch,
                );
            }
        }
        batch.flush::<TRACE>(painter);
        let palette = self.palette();
        for bundle in &bundles {
            let selected = bundle.wires.iter().any(|id| self.board_sel.contains(id));
            bundle.paint(
                painter,
                xf,
                if selected {
                    palette.select
                } else {
                    palette.wire
                },
                selected,
            );
        }
        if TRACE {
            batch.trace.emit(&self.wire_route_cache.borrow());
        }
    }

    // ----- painting -----

    /// Wire-drag preview: rubber-band bezier (solid when snapped), the snap
    /// highlight ring, and the modifier glyph (+ add / − detach).
    pub(crate) fn paint_wire_drag(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        wd: &WireDrag,
        mods: egui::Modifiers,
    ) {
        let palette = self.palette();
        if let WireMode::Add { from } = &wd.mode {
            let a = ConnectorEnd::Anchored {
                node: from.0,
                side: from.1,
                t: from.2,
            };
            let b = match wd.snap {
                Some((node, side, t)) => ConnectorEnd::Anchored { node, side, t },
                None => ConnectorEnd::Free {
                    point: [wd.cursor.x, wd.cursor.y],
                },
            };
            let scene = &self.doc().scene;
            if let Some(path) = slate_doc::wire::connector_route_in_scene_with_hosts(
                scene,
                None,
                &a,
                &b,
                self.board_wire_routing,
                |id| self.visible_wire_host(id),
            ) {
                let crosstalk = self.is_crosstalk_port(*from);
                let mut stroke = connector_drawn_stroke(if crosstalk {
                    slate_doc::crosstalk::wire_stroke()
                } else {
                    self.default_wire_stroke()
                });
                if !crosstalk {
                    stroke.color = super::super::board::to_rgba(palette.wire);
                }
                let path = retreat_off_hosts(
                    path,
                    &a,
                    &b,
                    |id| self.visible_wire_host(id),
                    stroke.width * 0.5,
                );
                let color =
                    rgba32(stroke.color).gamma_multiply(if wd.snap.is_some() { 1.0 } else { 0.55 });
                paint_route_preview(
                    painter,
                    xf,
                    &path,
                    EStroke::new(atlas_shell::canvas_scale::px(stroke.width, xf.z), color),
                );
            }
        }
        // Snap highlight.
        if let Some((node, side, t)) = wd.snap {
            if let Some(n) = self
                .presence_preview(node)
                .or_else(|| self.doc().scene.node(node))
            {
                let s = xf.w2s(self.wire_port_point(n, side, t));
                painter.circle_stroke(
                    s,
                    atlas_shell::canvas_scale::px(8.0, xf.z),
                    EStroke::new(atlas_shell::canvas_scale::px(2.0, xf.z), palette.accent),
                );
            }
        }
        // Modifier glyph near the pointer.
        let glyph = if mods.ctrl && !mods.shift {
            Some("−")
        } else if mods.shift && !mods.ctrl {
            Some("+")
        } else {
            None
        };
        if let Some(g) = glyph {
            let at = xf.w2s(wd.cursor) + Vec2::new(14.0, -14.0);
            painter.circle_filled(at, 8.0, palette.card);
            painter.text(
                at,
                Align2::CENTER_CENTER,
                g,
                FontId::proportional(13.0),
                palette.ink,
            );
        }
    }

    /// Paint one connector node: tessellated through the path-mesh cache
    /// (geometry hash in the key), Faint = 40% opacity, filled-triangle
    /// arrowheads sized like the artifact, label centered at the midpoint.
    pub(crate) fn paint_connector(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        conn: &ConnectorNode,
    ) {
        let mut batch = WireStrokeBatch::default();
        self.paint_connector_in_batch::<false>(
            painter,
            xf,
            node,
            conn,
            self.theme_wire_rgba(),
            &mut batch,
        );
        batch.flush::<false>(painter);
    }

    fn paint_connector_in_batch<const TRACE: bool>(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        conn: &ConnectorNode,
        theme_wire: slate_doc::scene::Rgba,
        batch: &mut WireStrokeBatch,
    ) {
        // Hidden-anchor rule: unresolvable connectors are skipped entirely
        // (matches the artifact writer).
        if conn.crosstalk.is_some() && self.crosstalk_duplicate(node.id) {
            return;
        }
        let mut stroke = connector_drawn_stroke(conn.stroke);
        let route_started = WirePaintTrace::start::<TRACE>();
        let (path, route_hit) =
            if conn.effective_routing(self.board_wire_routing) == WireRouting::Bezier {
                let (_, drawn, hit) = self.visible_bezier_paths(node.id, conn);
                (drawn, hit)
            } else {
                (
                    self.connector_path_visible(node.id, conn).map(|path| {
                        retreat_off_hosts(
                            path,
                            &conn.a,
                            &conn.b,
                            |id| self.visible_wire_host(id),
                            stroke.width * 0.5,
                        )
                    }),
                    false,
                )
            };
        if let Some(started) = route_started {
            batch.trace.route += started.elapsed();
            batch.trace.route_calls += 1;
            batch.trace.route_hits += usize::from(route_hit);
        }
        let Some(path) = path else {
            return;
        };
        stroke.color = if conn.grasshopper.is_some() {
            theme_wire
        } else {
            conn.paint_color(theme_wire)
        };
        let opacity = (node.opacity
            * match conn.display {
                WireDisplay::Faint => FAINT_OPACITY,
                WireDisplay::Default => 1.0,
            })
        .clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(opacity);
        let base = fade(rgba32(stroke.color));
        if let Some(access) = self
            .grasshopper_access(conn)
            .filter(|access| *access != slate_doc::scene::GrasshopperPortAccess::Item)
        {
            batch.flush::<TRACE>(painter);
            super::grasshopper::paint_grasshopper_fancy_wire(
                painter,
                xf,
                &path,
                access,
                base,
                stroke.width,
            );
            return;
        }

        // Small plain connectors keep their endpoints and screen coverage
        // without building another antialiased mesh for each wheel notch.
        let drew_motion_chord = batch
            .motion_pixels_per_world
            .is_some_and(|pixels_per_world| {
                let coverage = stroke.width * pixels_per_world;
                !stroke.is_none()
                    && coverage <= 0.75
                    && node.rect.w.abs().max(node.rect.h.abs()) * pixels_per_world <= 16.0
                    && matches!(path, ConnectorPath::Bezier(_))
                    && stroke.dash == Dash::Solid
                    && stroke.profile == WidthProfile::Uniform
                    && !conn.arrow_a
                    && !conn.arrow_b
                    && !self.board_sel.contains(&node.id)
                    && batch.append_subpixel_chord::<TRACE>(painter, xf, &path, coverage, base)
            });
        if !stroke.is_none() && !drew_motion_chord {
            let key_started = WirePaintTrace::start::<TRACE>();
            let bucket = board_path::zoom_bucket(xf.z);
            let key = connector_cache_key(
                &path,
                self.board_wire_routing,
                &stroke,
                conn.display,
                bucket,
            );
            WirePaintTrace::record(key_started, &mut batch.trace.key);
            let cache_started = WirePaintTrace::start::<TRACE>();
            let cached = self.path_mesh_cache.get_or_build_stroke(node.id, key, || {
                let build_started = WirePaintTrace::start::<TRACE>();
                let style = board_path::stroke_style_world(&stroke, xf.z);
                let feather =
                    board_path::FEATHER_PX / xf.z.max(atlas_core::display::SLATE_BOARD.min);
                let kurbo = connector_path_kurbo(&path);
                let mesh = vector_ink::stroke_mesh(
                    &kurbo,
                    &style,
                    feather,
                    board_path::curve_tolerance(xf.z),
                );
                let mesh = board_path::CachedInkMesh::from_wire(mesh);
                WirePaintTrace::record(build_started, &mut batch.trace.build);
                mesh
            });
            WirePaintTrace::record(cache_started, &mut batch.trace.cache);
            if batch.mesh.vertices.len() + cached.vertex_count() > WIRE_BATCH_VERTICES {
                batch.flush::<TRACE>(painter);
            }
            let append_started = WirePaintTrace::start::<TRACE>();
            cached.append_to_wire_batch(&mut batch.mesh, xf, base, WIRE_BATCH_VERTICES);
            WirePaintTrace::record(append_started, &mut batch.trace.append);
        }

        // Arrowheads: filled triangles, tip at the endpoint, base back along
        // the tangent into the curve; size matches the artifact.
        let arrow = |tip: [f32; 2], into: [f32; 2]| {
            let [a, b, c] = slate_doc::geom::arrow_head(tip, into, stroke.width);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    xf.w2s(Pos2::new(a[0], a[1])),
                    xf.w2s(Pos2::new(b[0], b[1])),
                    xf.w2s(Pos2::new(c[0], c[1])),
                ],
                base,
                EStroke::NONE,
            ));
        };
        if conn.arrow_a || conn.arrow_b {
            batch.flush::<TRACE>(painter);
        }
        if conn.arrow_a {
            arrow(path.start(), path.start_dir());
        }
        if conn.arrow_b {
            arrow(path.end(), path.end_dir());
        }

        // Label at the curve midpoint (skipped while its inline edit is up).
        if self
            .wire_label_edit
            .as_ref()
            .is_some_and(|(id, _)| *id == node.id)
        {
            return;
        }
        if let Some(label) = conn.label.as_deref().filter(|l| !l.is_empty()) {
            let size = atlas_shell::canvas_scale::px(CONNECTOR_LABEL_SIZE, xf.z);
            if atlas_shell::canvas_text::legible(size) {
                batch.flush::<TRACE>(painter);
                let m = path.midpoint();
                atlas_shell::canvas_text::text(
                    painter,
                    xf.w2s(Pos2::new(m[0], m[1])),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(size),
                    base,
                );
            }
        }
    }

    /// Selection adornment for a connector: curve highlight + endpoint dots
    /// (draggable — detach), instead of the rect outline/handles.
    pub(crate) fn paint_connector_selection(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
    ) {
        let node = self
            .shape_properties
            .preview
            .iter()
            .find(|n| n.id == node.id)
            .unwrap_or(node);
        let NodeKind::Connector(conn) = &node.kind else {
            return;
        };
        if conn.grasshopper.as_deref().is_some_and(|wire| wire.bundled) {
            return;
        }
        let Some(path) = self.connector_path_visible(node.id, conn) else {
            return;
        };
        let anchors = [path.start(), path.end()];
        let (path, _) = drawn_connector_with_hosts(path, conn, |id| self.visible_wire_host(id));
        let palette = self.palette();
        paint_route_preview(
            painter,
            xf,
            &path,
            EStroke::new(atlas_shell::canvas_scale::px(1.5, xf.z), palette.select),
        );
        let r = atlas_shell::canvas_scale::px(4.5, xf.z);
        let ends = anchors.map(|p| xf.w2s(Pos2::new(p[0], p[1])));
        for p in ends {
            painter.circle_filled(p, r, palette.bg);
            painter.circle_stroke(
                p,
                r,
                EStroke::new(atlas_shell::canvas_scale::px(2.0, xf.z), palette.select),
            );
        }
    }
}

pub(crate) fn paint_route_preview(
    painter: &egui::Painter,
    xf: &BoardXf,
    path: &ConnectorPath,
    stroke: EStroke,
) {
    match path {
        ConnectorPath::Bezier(bez) => {
            let pts = [
                xf.w2s(Pos2::new(bez.p0[0], bez.p0[1])),
                xf.w2s(Pos2::new(bez.c1[0], bez.c1[1])),
                xf.w2s(Pos2::new(bez.c2[0], bez.c2[1])),
                xf.w2s(Pos2::new(bez.p3[0], bez.p3[1])),
            ];
            painter.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    pts,
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ),
            ));
        }
        ConnectorPath::Orthogonal(pts) => {
            let screen: Vec<Pos2> = pts.iter().map(|p| xf.w2s(Pos2::new(p[0], p[1]))).collect();
            atlas_shell::dock::rounded_route(
                painter,
                &screen,
                atlas_shell::canvas_scale::px(ORTHO_CORNER_RADIUS, xf.z),
                stroke,
            );
        }
    }
}
