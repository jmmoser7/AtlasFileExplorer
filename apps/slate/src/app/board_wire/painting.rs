//! Connector painting and transient host geometry, using the shared route owner.
use super::*;
use slate_doc::agent_inputs::endpoint_node;
use slate_doc::wire::{connector_route_in_scene_with_hosts, SceneWireRouting};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_CACHED_ROUTES: usize = 4096;
const MAX_CACHED_ROUTE_POINTS: usize = 256;
const MAX_CACHED_BEZIER_ROUTES: usize = 65_536;
/// Sum of the live key and value sizes. Hash table allocation is reported
/// separately as the number of entries its current capacity can hold.
const BEZIER_ROUTE_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;
const WIRE_BATCH_VERTICES: usize = 60_000;

#[derive(Default)]
struct WireStrokeBatch {
    mesh: egui::Mesh,
    trace: WirePaintTrace,
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
        let ms = |time: Duration| time.as_secs_f64() * 1_000.0;
        atlas_core::session_log::event(
            "slate.board.wires.detail",
            &format!(
                "route_ms={:.3} key_ms={:.3} lookup_ms={:.3} build_ms={:.3} append_ms={:.3} flush_ms={:.3} route_hits={} route_calls={} meshes={} vertices={} indices={} bezier_entries={} bezier_capacity={} bezier_payload_bytes={}",
                ms(self.route), ms(self.key), ms(self.cache.saturating_sub(self.build)),
                ms(self.build), ms(self.append), ms(self.flush), self.route_hits,
                self.route_calls, self.meshes, self.vertices, self.indices,
                routes.beziers.len(), routes.beziers.capacity(),
                routes.beziers.len() * std::mem::size_of::<(NodeId, CachedBezier)>(),
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

#[derive(Default)]
pub(crate) struct WireRouteCache {
    revision: Option<(u64, u64, u64, u64, WireRouting)>,
    scene: SceneWireRouting,
    paths: HashMap<NodeId, CachedRoute>,
    beziers: HashMap<NodeId, CachedBezier>,
}

struct CachedRoute {
    ends: [ConnectorEnd; 2],
    hosts: [Option<WireHost>; 2],
    path: Option<ConnectorPath>,
}

struct CachedBezier {
    ends: [ConnectorEnd; 2],
    width: u32,
    paths: Option<[ConnectorBezier; 2]>,
}

impl WireRouteCache {
    fn retain_bezier(&mut self, id: NodeId, route: CachedBezier) {
        let limit = MAX_CACHED_BEZIER_ROUTES
            .min(BEZIER_ROUTE_PAYLOAD_BYTES / std::mem::size_of::<(NodeId, CachedBezier)>());
        // Keep admitted routes stable when a board exceeds the limit. A full
        // traversal must not evict every entry before the next frame.
        if self.beziers.len() < limit || self.beziers.contains_key(&id) {
            self.beziers.insert(id, route);
        }
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
        let mut batch = WireStrokeBatch::default();
        let theme_wire = self.theme_wire_rgba();
        let field = self.field_selection();
        let media = self.prepared_media_chrome_host();
        for node in nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
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
        if TRACE {
            batch.trace.emit(&self.wire_route_cache.borrow());
        }
    }

    pub(crate) fn connector_path_visible(
        &self,
        id: NodeId,
        conn: &ConnectorNode,
    ) -> Option<ConnectorPath> {
        let scene = &self.doc().scene;
        let routing = conn.effective_routing(self.board_wire_routing);
        if routing == WireRouting::Bezier {
            return self.visible_bezier_paths(id, conn).0;
        }

        let mut cache = self.current_wire_route_cache();
        let ends = [conn.a, conn.b];
        let host_ids = ends.each_ref().map(endpoint_node);
        let hosts = host_ids.map(|id| id.and_then(|id| self.visible_wire_host(id)));
        // Memory routes can resolve the authored ends even while a caller
        // previews different ends. Open stroke hosts carry sampled geometry.
        let cacheable = scene.node(id).is_some_and(|node| {
            matches!(&node.kind, NodeKind::Connector(authored) if [authored.a, authored.b] == ends)
        }) && hosts.iter().flatten().all(WireHost::is_area);
        if cacheable {
            if let Some(cached) = cache.paths.get(&id) {
                if cached.ends == ends && cached.hosts == hosts {
                    return cached.path.clone();
                }
            }
        }
        let path = cache.scene.route(
            scene,
            Some(id),
            &conn.a,
            &conn.b,
            routing,
            |id| match host_ids.iter().position(|host| *host == Some(id)) {
                Some(index) => hosts[index].clone(),
                None => self.visible_wire_host(id),
            },
        );
        if cacheable
            && cache.paths.len() < MAX_CACHED_ROUTES
            && !matches!(&path, Some(ConnectorPath::Orthogonal(points)) if points.len() > MAX_CACHED_ROUTE_POINTS)
        {
            cache.paths.insert(
                id,
                CachedRoute {
                    ends,
                    hosts,
                    path: path.clone(),
                },
            );
        }
        path
    }

    fn current_wire_route_cache(&self) -> std::cell::RefMut<'_, WireRouteCache> {
        let doc = self.doc();
        let revision = (
            self.tab().id,
            self.scene_gen,
            doc.scene.scene_gen(),
            doc.item_paths_revision(),
            self.board_wire_routing,
        );
        let mut cache = self.wire_route_cache.borrow_mut();
        if cache.revision != Some(revision) {
            cache.revision = Some(revision);
            cache.scene = SceneWireRouting::default();
            cache.paths.clear();
            cache.beziers.clear();
        }
        cache
    }

    fn visible_bezier_paths(
        &self,
        id: NodeId,
        conn: &ConnectorNode,
    ) -> (Option<ConnectorPath>, Option<ConnectorPath>, bool) {
        let scene = &self.doc().scene;
        let ends = [conn.a, conn.b];
        let host_ids = ends.each_ref().map(endpoint_node);
        let width = connector_drawn_stroke(conn.stroke).width;
        // Query actual replacing previews. Idle avatars do not change a
        // route, and a replacing host can move without a scene revision.
        let previews = host_ids.map(|id| id.and_then(|id| self.presence_preview(id)));
        let mut cache = (self.board_drag.is_none()
            && !self.slate_nesting()
            && previews.iter().all(Option::is_none))
        .then(|| self.current_wire_route_cache());
        if let Some(hit) = cache
            .as_ref()
            .and_then(|cache| cache.beziers.get(&id))
            .filter(|hit| hit.ends == ends && hit.width == width.to_bits())
        {
            let (raw, drawn) = hit.paths.map_or((None, None), |[raw, drawn]| {
                (
                    Some(ConnectorPath::Bezier(raw)),
                    Some(ConnectorPath::Bezier(drawn)),
                )
            });
            return (raw, drawn, true);
        }
        let hosts = std::array::from_fn::<_, 2, _>(|index| {
            previews[index]
                .or_else(|| scene.node(host_ids[index]?))
                .filter(|node| !node.hidden)
                .map(|node| self.wire_host(node))
        });
        let host_of = |id| match host_ids.iter().position(|host| *host == Some(id)) {
            Some(index) => hosts[index].clone(),
            None => self.visible_wire_host(id),
        };
        let raw = connector_route_in_scene_with_hosts(
            scene,
            Some(id),
            &conn.a,
            &conn.b,
            WireRouting::Bezier,
            host_of,
        );
        let drawn = raw
            .as_ref()
            .map(|path| retreat_off_hosts(path.clone(), &conn.a, &conn.b, host_of, width * 0.5));
        let compact = match (&raw, &drawn) {
            (Some(ConnectorPath::Bezier(raw)), Some(ConnectorPath::Bezier(drawn))) => {
                Some(Some([*raw, *drawn]))
            }
            (None, None) => Some(None),
            _ => None,
        };
        if let (Some(cache), Some(paths)) = (cache.as_mut(), compact) {
            // Memory paths can use the authored endpoints even when a caller
            // supplies a transient connector. Only retain authored ends so
            // the preview checks above cover every possible route host.
            if scene.node(id).is_some_and(|node| {
                matches!(&node.kind, NodeKind::Connector(authored) if [authored.a, authored.b] == ends)
            }) {
                cache.retain_bezier(id, CachedBezier { ends, width: width.to_bits(), paths });
            }
        }
        (raw, drawn, false)
    }

    pub(crate) fn visible_wire_host(&self, id: NodeId) -> Option<WireHost> {
        self.presence_preview(id)
            .or_else(|| self.doc().scene.node(id))
            .filter(|node| !node.hidden)
            .map(|node| self.wire_host(node))
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
        stroke.color = conn.paint_color(theme_wire);
        let opacity = (node.opacity
            * match conn.display {
                WireDisplay::Faint => FAINT_OPACITY,
                WireDisplay::Default => 1.0,
            })
        .clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(opacity);
        let base = fade(rgba32(stroke.color));

        if !stroke.is_none() {
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

#[cfg(test)]
mod cache_tests {
    use super::*;
    use crate::app::tests::Harness;

    #[test]
    fn bezier_route_retention_covers_the_dense_board_within_payload_budget() {
        let mut cache = WireRouteCache::default();
        let ends = [ConnectorEnd::Free { point: [0.0, 0.0] }; 2];
        for id in 1..=MAX_CACHED_BEZIER_ROUTES as u64 + 1 {
            cache.retain_bezier(
                NodeId(id),
                CachedBezier {
                    ends,
                    width: 0,
                    paths: None,
                },
            );
        }
        assert!((1..=37_200).all(|id| cache.beziers.contains_key(&NodeId(id))));
        assert!(cache.beziers.len() <= MAX_CACHED_BEZIER_ROUTES);
        assert!(
            cache.beziers.len() * std::mem::size_of::<(NodeId, CachedBezier)>()
                <= BEZIER_ROUTE_PAYLOAD_BYTES
        );
    }

    #[test]
    fn settled_beziers_keep_colliding_ids_and_follow_live_host_motion() {
        let mut h = Harness::new("settled-beziers-retain-and-invalidate");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.board_wire_routing = WireRouting::Bezier;
        let host = h.app.doc_mut().scene.build_node(
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            NodeKind::Image(slate_doc::scene::ImageNode::new(slate_doc::ItemId::NONE)),
        );
        let host = h.app.add_nodes(vec![host])[0];
        let start = ConnectorEnd::Anchored {
            node: host,
            side: Side::Right,
            t: 0.5,
        };
        let first = h.app.build_connector(
            start,
            ConnectorEnd::Free {
                point: [250.0, 80.0],
            },
        );
        let mut second = h.app.build_connector(
            start,
            ConnectorEnd::Free {
                point: [450.0, 160.0],
            },
        );
        second.id = NodeId(first.id.0 + 4096);
        let ids = h.app.add_nodes(vec![first, second]);
        let read = |app: &SlateApp, id: NodeId| {
            let NodeKind::Connector(conn) = &app.doc().scene.node(id).unwrap().kind else {
                panic!("expected connector")
            };
            app.visible_bezier_paths(id, conn)
        };
        let first = read(&h.app, ids[0]);
        let second = read(&h.app, ids[1]);
        assert!(!first.2 && !second.2);
        assert_eq!(
            read(&h.app, ids[0]),
            (first.0.clone(), first.1.clone(), true)
        );
        assert_eq!(read(&h.app, ids[1]), (second.0, second.1, true));
        h.app.doc_mut().scene.node_mut(host).unwrap().rect.x -= 20.0;
        let moved = read(&h.app, ids[0]);
        assert!(!moved.2);
        assert_ne!(moved.0, first.0);
        assert_ne!(moved.1, first.1);
        assert!(read(&h.app, ids[0]).2);
    }
}
