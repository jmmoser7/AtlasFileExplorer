//! Visible connector route resolution and bounded caching.
use super::*;
use slate_doc::agent_inputs::endpoint_node;
use slate_doc::wire::{connector_route_in_scene_with_hosts, SceneWireRouting};
use std::collections::HashMap;

const MAX_CACHED_ROUTES: usize = 4096;
const MAX_CACHED_ROUTE_POINTS: usize = 256;
const MAX_CACHED_BEZIER_ROUTES: usize = 65_536;
/// Sum of the live key and value sizes. Hash table allocation is reported
/// separately as the number of entries its current capacity can hold.
const BEZIER_ROUTE_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

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
    pub(super) fn bezier_stats(&self) -> (usize, usize, usize) {
        let entries = self.beziers.len();
        (
            entries,
            self.beziers.capacity(),
            entries * std::mem::size_of::<(NodeId, CachedBezier)>(),
        )
    }

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

    pub(super) fn visible_bezier_paths(
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
