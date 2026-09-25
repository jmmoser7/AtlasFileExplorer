//! Wired saved views: screenshot images → 3D viewport input port.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use atlas_agent::InputSlot;
use eframe::egui::{self, Id, Rect, Ui};
use model_preview::view_meta;
use slate_doc::agent_inputs;
use slate_doc::scene::{ModelCamera, NodeId, NodeKind};
use slate_doc::wire_host::WireHost;

use atlas_shell::canvas_scale;
use atlas_shell::home::{album_index_strip, AlbumImage};

use super::SlateApp;

pub struct PendingViewWireCache {
    pub tab_id: u64,
    pub connector: NodeId,
    pub item_key: String,
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
    /// A model keeps the plain host: View wires land through the ordinary
    /// edge snap, so no port is drawn or hit at rest.
    pub(crate) fn wire_host(&self, node: &slate_doc::scene::Node) -> WireHost {
        WireHost::from_node(node)
    }

    pub(crate) fn note_view_wires_added(&mut self, ids: &[NodeId]) {
        let tab_id = self.tab().id;
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
            if binding.slot.as_deref() != Some(InputSlot::View.id()) {
                continue;
            }
            let source_end = binding.source_end(conn);
            let Some(source) = agent_inputs::endpoint_node(source_end) else {
                continue;
            };
            let Some(item_key) = self.image_item_cache_key(source) else {
                continue;
            };
            if self.model3d.view_wire_meta.contains_key(&item_key) {
                continue;
            }
            if self
                .pending_view_wire_cache
                .iter()
                .any(|p| p.tab_id == tab_id && p.connector == *id)
            {
                continue;
            }
            let Some(path) = self.image_item_path(source) else {
                continue;
            };
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let msg = if atlas_core::cloud::is_dehydrated(&path) {
                    ViewWireCacheMsg::Missing
                } else {
                    match view_meta::read_view_meta(&path) {
                        Ok(Some(parsed)) => ViewWireCacheMsg::Camera(parsed.camera),
                        Ok(None) => ViewWireCacheMsg::Missing,
                        Err(e) => ViewWireCacheMsg::Err(e.to_string()),
                    }
                };
                let _ = tx.send(msg);
            });
            self.pending_view_wire_cache.push(PendingViewWireCache {
                tab_id,
                connector: *id,
                item_key,
                rx,
            });
        }
    }

    pub(crate) fn maintain_view_wire_cache(&mut self) {
        let tab_id = self.tab().id;
        let mut done = Vec::new();
        let mut updates = Vec::new();
        let mut toasts = Vec::new();
        for (i, pending) in self.pending_view_wire_cache.iter().enumerate() {
            let Ok(msg) = pending.rx.try_recv() else {
                continue;
            };
            done.push(i);
            let cam = match msg {
                ViewWireCacheMsg::Camera(cam) => Some(cam),
                ViewWireCacheMsg::Missing => None,
                ViewWireCacheMsg::Err(e) => {
                    if pending.tab_id == tab_id {
                        toasts.push(e);
                    }
                    None
                }
            };
            updates.push((pending.item_key.clone(), cam));
        }
        for msg in toasts {
            self.toast(&msg);
        }
        for i in done.into_iter().rev() {
            self.pending_view_wire_cache.remove(i);
        }
        for (key, cam) in updates {
            self.model3d.view_wire_meta.insert(key, cam);
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
            if binding.slot.as_deref() != Some(InputSlot::View.id()) {
                continue;
            }
            let source_end = binding.source_end(conn);
            let target_end = binding.target_end(conn);
            let target = agent_inputs::endpoint_node(target_end);
            let source = agent_inputs::endpoint_node(source_end);
            if target != Some(model) {
                continue;
            }
            let Some(source) = source else {
                continue;
            };
            let camera = self
                .image_item_cache_key(source)
                .and_then(|k| self.model3d.view_wire_meta.get(&k))
                .and_then(|c| *c);
            out.push(WiredViewEntry {
                connector: node.id,
                source,
                camera,
            });
        }
        out.sort_by_key(|e| e.connector.0);
        out
    }

    fn active_view_connector(&self, model: NodeId, views: &[WiredViewEntry]) -> Option<NodeId> {
        let cam = self.doc().scene.node(model).and_then(|n| match &n.kind {
            NodeKind::Image(i) => Some(i.model),
            _ => None,
        })?;
        views
            .iter()
            .find(|v| v.camera == Some(cam))
            .map(|v| v.connector)
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
        let active = self.active_view_connector(model, &views);
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
            Some("No saved Slate view"),
        ) {
            self.apply_wired_view_pick(model, views[pick].connector);
        }
    }

    fn apply_wired_view_pick(&mut self, model: NodeId, connector: NodeId) {
        if self.doc().scene.node(model).is_none_or(|n| n.locked) || self.refuse_read_only_edit() {
            return;
        }
        let views = self.wired_views_for_model(model);
        let Some(cam) = views
            .iter()
            .find(|v| v.connector == connector)
            .and_then(|v| v.camera)
        else {
            self.toast("No saved Slate view");
            return;
        };
        self.start_model_view_tween(model, cam);
        self.last_board_edit = None;
        self.patch_nodes(&[model], |n| {
            if let NodeKind::Image(i) = &mut n.kind {
                i.model = cam;
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
        self.image_item(image).and_then(|item| self.item_path(item))
    }

    fn image_item_cache_key(&self, image: NodeId) -> Option<String> {
        self.image_item(image)
            .and_then(|item| self.doc().item(item))
            .map(|item| item.cache_key.clone())
    }
}
