//! Wired saved views: screenshot images → 3D viewport input port.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use eframe::egui::{self, Id, Rect, Ui};
use model_preview::view_meta;
use slate_doc::agent_inputs;
use slate_doc::scene::{ConnectorEnd, ModelCamera, NodeId, NodeKind, Side};

use atlas_shell::canvas_scale;
use atlas_shell::home::{album_index_strip, AlbumImage};

use super::SlateApp;

pub struct PendingViewWireCache {
    pub connector: NodeId,
    rx: Receiver<ViewWireCacheMsg>,
}

enum ViewWireCacheMsg {
    Camera(ModelCamera),
    Missing,
    Err(String),
}

struct WiredViewEntry {
    connector: NodeId,
    source: NodeId,
    camera: Option<ModelCamera>,
}

impl SlateApp {
    pub(crate) fn note_view_wires_added(&mut self, ids: &[NodeId]) {
        for id in ids {
            let Some(node) = self.doc().scene.node(*id) else {
                continue;
            };
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            if binding.slot.as_deref() != Some("view") {
                continue;
            }
            if conn.cached_slate_view.is_some() {
                continue;
            }
            if self
                .pending_view_wire_cache
                .iter()
                .any(|p| p.connector == *id)
            {
                continue;
            }
            let (source_end, _target) = if binding.input_b {
                (&conn.b, &conn.a)
            } else {
                (&conn.a, &conn.b)
            };
            let Some(source) = agent_inputs::endpoint_node(source_end) else {
                continue;
            };
            let Some(path) = self.image_item_path(source) else {
                continue;
            };
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let msg = match view_meta::read_view_meta(&path) {
                    Ok(Some(parsed)) => ViewWireCacheMsg::Camera(parsed.camera),
                    Ok(None) => ViewWireCacheMsg::Missing,
                    Err(e) => ViewWireCacheMsg::Err(e.to_string()),
                };
                let _ = tx.send(msg);
            });
            self.pending_view_wire_cache
                .push(PendingViewWireCache { connector: *id, rx });
        }
    }

    pub(crate) fn maintain_view_wire_cache(&mut self) {
        let mut done = Vec::new();
        let mut cameras = Vec::new();
        for (i, pending) in self.pending_view_wire_cache.iter().enumerate() {
            let Ok(msg) = pending.rx.try_recv() else {
                continue;
            };
            done.push(i);
            if let ViewWireCacheMsg::Camera(cam) = msg {
                cameras.push((pending.connector, cam));
            }
        }
        for i in done.into_iter().rev() {
            self.pending_view_wire_cache.remove(i);
        }
        for (connector, cam) in cameras {
            self.last_board_edit = None;
            self.patch_nodes(&[connector], |n| {
                if let NodeKind::Connector(c) = &mut n.kind {
                    c.cached_slate_view = Some(cam);
                }
            });
            self.last_board_edit = None;
        }
    }

    fn wired_views_for_model(&self, model: NodeId) -> Vec<WiredViewEntry> {
        let mut out = Vec::new();
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            if binding.slot.as_deref() != Some("view") {
                continue;
            }
            let (source_end, target_end) = if binding.input_b {
                (&conn.a, &conn.b)
            } else {
                (&conn.b, &conn.a)
            };
            let target = agent_inputs::endpoint_node(target_end);
            let source = agent_inputs::endpoint_node(source_end);
            if target != Some(model) {
                continue;
            }
            let Some(source) = source else {
                continue;
            };
            out.push(WiredViewEntry {
                connector: node.id,
                source,
                camera: conn.cached_slate_view,
            });
        }
        out.sort_by_key(|e| e.connector.0);
        out
    }

    pub(crate) fn paint_model_wired_view_strip(
        &mut self,
        ui: &Ui,
        model: NodeId,
        srect: Rect,
        zoom: f32,
    ) {
        let views = self.wired_views_for_model(model);
        if views.is_empty() {
            return;
        }
        let active = self.doc().scene.node(model).and_then(|n| match &n.kind {
            NodeKind::Image(i) => i.active_view_wire,
            _ => None,
        });
        let focus = views
            .iter()
            .position(|v| Some(v.connector) == active)
            .unwrap_or(0);
        let thumb_px = canvas_scale::px(44.0, zoom);
        let squares: Vec<AlbumImage> = views
            .iter()
            .map(|v| {
                let enabled = v.camera.is_some();
                let texture = self
                    .doc()
                    .scene
                    .node(v.source)
                    .and_then(|n| match &n.kind {
                        NodeKind::Image(img) => Some(img.item),
                        _ => None,
                    })
                    .and_then(|item| self.item_texture(item, thumb_px).map(|t| t.id()));
                AlbumImage {
                    texture,
                    size: egui::vec2(1.0, 1.0),
                    enabled,
                }
            })
            .collect();
        let runs: Vec<&str> = views.iter().map(|_| "view").collect();
        let shown = self.board_sel.contains(&model);
        if let Some(pick) = album_index_strip(
            ui,
            Id::new(("model-views", model.0)),
            srect,
            &squares,
            &runs,
            focus,
            shown,
            zoom,
            self.palette(),
            1,
        ) {
            self.apply_wired_view_pick(model, views[pick].connector);
        }
    }

    fn apply_wired_view_pick(&mut self, model: NodeId, connector: NodeId) {
        let Some(node) = self.doc().scene.node(connector) else {
            return;
        };
        let NodeKind::Connector(conn) = &node.kind else {
            return;
        };
        let Some(cam) = conn.cached_slate_view else {
            self.toast("No saved Slate view");
            return;
        };
        self.start_model_view_tween(model, cam);
        self.last_board_edit = None;
        self.patch_nodes(&[model], |n| {
            if let NodeKind::Image(i) = &mut n.kind {
                i.model = cam;
                i.active_view_wire = Some(connector);
            }
        });
        self.last_board_edit = None;
        if let Some(vp) = self.model3d.live.get_mut(&model) {
            vp.cam = cam;
        }
    }

    fn start_model_view_tween(&mut self, model: NodeId, to: ModelCamera) {
        if let Some(vp) = self.model3d.live.get_mut(&model) {
            vp.view_tween = Some(super::model3d::ViewTween {
                from: vp.cam,
                to,
                started: Instant::now(),
            });
        }
    }

    fn image_item_path(&self, image: NodeId) -> Option<PathBuf> {
        let node = self.doc().scene.node(image)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        self.doc().item(img.item).map(|i| i.path.clone())
    }
}
