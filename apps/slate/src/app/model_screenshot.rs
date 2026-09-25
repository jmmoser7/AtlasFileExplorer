//! Viewport screenshot export and drop-back camera restore.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Id, Pos2, Rect, Vec2};
use model_preview::view_meta::{self, ViewMetaInput};
use slate_doc::scene::{ImageAdjust, ImageNode, ModelCamera, NodeId, NodeKind, WorldRect};

use super::model3d::{self, capture_size, ModelNodeInfo};
use super::SlateApp;

const POPUP_ITEM_H: f32 = 28.0;
const POPUP_W: f32 = 200.0;

pub struct ModelScreenshotPopup {
    pub node: NodeId,
    /// Screen position when the menu opened (P2 pointer-attached chrome).
    pub anchor: Pos2,
}

enum ViewDropMsg {
    Restored(view_meta::ViewMetaParsed),
    NoMeta,
    Err(String),
}

pub struct PendingViewDrop {
    pub model: NodeId,
    rx: Receiver<ViewDropMsg>,
}

impl SlateApp {
    pub(crate) fn open_model_screenshot_menu(&mut self, node: NodeId, anchor: Pos2) {
        self.model_shot_popup = Some(ModelScreenshotPopup { node, anchor });
    }

    /// Pointer-attached menu (P2): screen-space placement under the cursor.
    pub(crate) fn paint_model_screenshot_popup(&mut self, ctx: &egui::Context) -> bool {
        let Some(popup) = self.model_shot_popup.as_ref() else {
            return false;
        };
        let menu_node = popup.node;
        let anchor = popup.anchor;
        let palette = self.palette();
        let mut choice_canvas = false;
        let mut choice_folder = false;
        let mut dismiss = false;

        let item_h = POPUP_ITEM_H;
        let w = POPUP_W;
        let h = item_h * 2.0 + 6.0;
        let top = anchor.y + 6.0;
        let rect = Rect::from_min_size(Pos2::new(anchor.x - w * 0.5, top), Vec2::new(w, h));

        let resp = egui::Area::new(Id::new(("model_shot_menu", menu_node.0)))
            .fixed_pos(rect.min)
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.card)
                    .show(ui, |ui| {
                        ui.set_min_width(w);
                        if ui.button("Export to canvas").clicked() {
                            choice_canvas = true;
                        }
                        if ui.button("Export to folder…").clicked() {
                            choice_folder = true;
                        }
                    })
            });
        if resp.response.clicked_elsewhere() {
            dismiss = true;
        }
        if choice_canvas {
            self.model_shot_popup = None;
            self.export_model_screenshot_canvas(menu_node);
        } else if choice_folder {
            self.model_shot_popup = None;
            self.export_model_screenshot_dialog(menu_node);
        } else if dismiss {
            self.model_shot_popup = None;
        }
        true
    }

    pub fn export_model_screenshot_dialog(&mut self, node: NodeId) {
        if self.picker_rx.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        self.picker_rx = Some(rx);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("PNG", &["png"])
                .add_filter("JPEG", &["jpg", "jpeg"])
                .add_filter("WebP", &["webp"])
                .save_file();
            let _ = tx.send(super::PickerMsg::ModelScreenshotSave { node, path: picked });
        });
    }

    pub fn export_model_screenshot_canvas(&mut self, node: NodeId) {
        match self.write_model_screenshot(node, None) {
            Ok(Some((_path, Some(item)))) => {
                if let Some(placed) = self.place_screenshot_beside_model(node, item) {
                    self.board_sel = std::iter::once(placed).collect();
                    self.toast("Viewport screenshot placed on the board");
                }
            }
            Ok(None) => self.toast("Still loading the 3D model — try again"),
            Ok(Some((_, None))) => {}
            Err(e) => self.toast(&e),
        }
    }

    pub fn finish_model_screenshot_save(&mut self, node: NodeId, path: PathBuf) {
        match self.write_model_screenshot(node, Some(&path)) {
            Ok(Some(_)) => self.toast(format!(
                "Saved {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
            Ok(None) => self.toast("Still loading the 3D model — try again"),
            Err(e) => self.toast(&e),
        }
    }

    fn write_model_screenshot(
        &mut self,
        node: NodeId,
        dest: Option<&Path>,
    ) -> Result<Option<(PathBuf, Option<slate_doc::ItemId>)>, String> {
        let Some((rgba, w, h, meta)) = self.render_model_screenshot_rgba(node)? else {
            return Ok(None);
        };
        let xmp = view_meta::build_xmp_packet(&meta).map_err(|e| e.to_string())?;
        let path = if let Some(p) = dest {
            p.to_path_buf()
        } else {
            self.model_screenshot_output_path(node, "png")?
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();
        match ext.as_str() {
            "png" => view_meta::write_png_with_xmp(&path, &rgba, w, h, &xmp)
                .map_err(|e| e.to_string())?,
            "jpg" | "jpeg" => {
                let rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
                view_meta::write_jpeg_with_xmp(&path, &rgb, w, h, 92, &xmp)
                    .map_err(|e| e.to_string())?;
            }
            "webp" => view_meta::write_webp_with_xmp(&path, &rgba, w, h, &xmp)
                .map_err(|e| e.to_string())?,
            _ => return Err("Use PNG, JPEG, or WebP".into()),
        }
        if dest.is_some() {
            return Ok(Some((path, None)));
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "viewport.png".into());
        let meta_fs = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        let mtime = file_mtime(&path);
        let key = super::cache_key(&path.to_string_lossy(), meta_fs.len(), mtime);
        let item = self.item_for_path(&path).unwrap_or_else(|| {
            self.doc_mut()
                .add_item(path.clone(), name, meta_fs.len(), mtime, key)
        });
        Ok(Some((path, Some(item))))
    }

    fn render_model_screenshot_rgba(
        &mut self,
        node: NodeId,
    ) -> Result<Option<(Vec<u8>, u32, u32, ViewMetaInput)>, String> {
        let info = self
            .model_node_info(node)
            .ok_or("That node is not a 3D model.")?;
        if self.model3d.external.contains(&info.cache_key) {
            return Err("Enscape standalones cannot export a mesh screenshot yet.".into());
        }
        let gl = self
            .gl
            .clone()
            .ok_or("3D viewports need GPU rendering (unavailable here).")?;
        let cam = self
            .model3d
            .live
            .get(&node)
            .map(|vp| vp.cam)
            .unwrap_or(info.cam);
        let (w, h) = capture_size(info.rect.w, info.rect.h);
        let adjust = self
            .doc()
            .scene
            .node(node)
            .and_then(slate_doc::scene::adjust_of)
            .unwrap_or_default();
        let Some(img) = self.model3d.render_capture_image(
            &gl,
            &info.cache_key,
            &cam,
            w,
            h,
            false,
            (!adjust.is_identity()).then_some(&adjust),
        ) else {
            self.model3d.request_model(&info.cache_key, &info.path);
            return Ok(None);
        };
        let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
        for p in &img.pixels {
            rgba.extend_from_slice(&p.to_srgba_unmultiplied());
        }
        let meta = self.view_meta_input(node, &info, cam, w, h, &adjust)?;
        Ok(Some((rgba, w, h, meta)))
    }

    fn view_meta_input(
        &self,
        node: NodeId,
        info: &ModelNodeInfo,
        cam: ModelCamera,
        w: u32,
        h: u32,
        adjust: &ImageAdjust,
    ) -> Result<ViewMetaInput, String> {
        let aspect = info.rect.w / info.rect.h.max(1.0);
        let model_path = slate_doc::scene::source_locator(self.tab().path.as_deref(), &info.path);
        let model_path = if view_meta::path_looks_absolute(&model_path) {
            String::new()
        } else {
            model_path
        };
        let model_hash = self
            .model3d
            .model_hashes
            .get(&info.cache_key)
            .cloned()
            .unwrap_or_default();
        let model_name = info
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let model_size = std::fs::metadata(&info.path).map(|m| m.len()).unwrap_or(0);
        Ok(ViewMetaInput {
            camera: cam,
            eye: model3d::eye_of(&cam),
            up: [0.0, 0.0, 1.0],
            aspect,
            width: w,
            height: h,
            model_name,
            model_path,
            model_hash,
            model_size,
            node_id: node.0,
            created_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            image_adjust_hash: (!adjust.is_identity()).then(|| adjust.cache_hash()),
        })
    }

    fn model_screenshot_output_path(&self, node: NodeId, ext: &str) -> Result<PathBuf, String> {
        if let Some(workbook) = self.tab().path.as_ref() {
            if let Some(parent) = workbook.parent() {
                let dir = parent.join("assets");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let mut path = dir.join(format!("viewport-{}-{}.{}", node.0, stamp, ext));
                let mut n = 1u32;
                while path.exists() && n < 100 {
                    path = dir.join(format!("viewport-{}-{}-{}.{}", node.0, stamp, n, ext));
                    n += 1;
                }
                return Ok(path);
            }
        }
        let ws = self
            .ai
            .config
            .workspace_dir
            .clone()
            .ok_or("Save the workbook or set an AI workspace first")?;
        let board = self
            .tab()
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled-board".into());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dir = atlas_ai::agent::output_dir(
            &ws.join(".atlas-ai").join("viewport-export"),
            &ws,
            Some(&board),
            "viewport",
            now,
        )
        .map_err(|e| e.to_string())?;
        Ok(dir.join(format!("view-{node}.{ext}", node = node.0)))
    }

    fn place_screenshot_beside_model(
        &mut self,
        model: NodeId,
        item: slate_doc::ItemId,
    ) -> Option<NodeId> {
        let model_node = self.doc().scene.node(model)?.clone();
        let gap = 24.0;
        let rect = WorldRect::new(
            model_node.rect.x + model_node.rect.w + gap,
            model_node.rect.y,
            model_node.rect.w,
            model_node.rect.h,
        );
        let node = self
            .doc_mut()
            .scene
            .build_node(rect, NodeKind::Image(ImageNode::new(item)));
        let ids = self.add_nodes(vec![node]);
        ids.into_iter().next()
    }

    pub(crate) fn model_node_at_world(&self, world: Pos2) -> Option<NodeId> {
        self.doc().scene.nodes.iter().rev().find_map(|n| {
            if !self.model_has_viewport(n.id) {
                return None;
            }
            n.rect.contains(world.x, world.y).then_some(n.id)
        })
    }

    pub(crate) fn queue_view_drop_from_path(&mut self, model: NodeId, image_path: PathBuf) {
        if self.pending_view_drop.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let msg = match view_meta::read_view_meta(&image_path) {
                Ok(Some(parsed)) => ViewDropMsg::Restored(parsed),
                Ok(None) => ViewDropMsg::NoMeta,
                Err(e) => ViewDropMsg::Err(e.to_string()),
            };
            let _ = tx.send(msg);
        });
        self.pending_view_drop = Some(PendingViewDrop { model, rx });
    }

    pub(crate) fn queue_view_drop_from_item(&mut self, model: NodeId, item: slate_doc::ItemId) {
        let Some(path) = self.doc().item(item).map(|i| i.path.clone()) else {
            return;
        };
        self.queue_view_drop_from_path(model, path);
    }

    pub(crate) fn maintain_view_drop(&mut self) {
        let Some(pending) = &self.pending_view_drop else {
            return;
        };
        let Ok(msg) = pending.rx.try_recv() else {
            return;
        };
        let model = pending.model;
        self.pending_view_drop = None;
        match msg {
            ViewDropMsg::NoMeta => self.toast("No saved Slate view in that image"),
            ViewDropMsg::Err(e) => self.toast(&e),
            ViewDropMsg::Restored(parsed) => self.apply_view_drop(model, parsed),
        }
    }

    pub(crate) fn apply_view_drop(&mut self, model: NodeId, parsed: view_meta::ViewMetaParsed) {
        let info = match self.model_node_info(model) {
            Some(i) => i,
            None => return,
        };
        let mismatch = !parsed.model_hash.is_empty()
            && self
                .model3d
                .model_hashes
                .get(&info.cache_key)
                .is_some_and(|h| h != &parsed.model_hash);
        self.last_board_edit = None;
        self.patch_nodes(&[model], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.model = parsed.camera;
            }
        });
        self.last_board_edit = None;
        if let Some(vp) = self.model3d.live.get_mut(&model) {
            vp.cam = parsed.camera;
        }
        if mismatch {
            self.toast("Model changed since capture — view applied anyway");
        } else {
            self.toast("Restored viewport camera from screenshot");
        }
    }

    pub(crate) fn maybe_intercept_image_drop_on_model(
        &mut self,
        paths: &[PathBuf],
        at: Pos2,
    ) -> bool {
        if paths.len() != 1 {
            return false;
        }
        let path = &paths[0];
        if !path.is_file() {
            return false;
        }
        if slate_doc::media_kind(path) != slate_doc::MediaKind::Image {
            return false;
        }
        let Some(model) = self.model_node_at_world(at) else {
            return false;
        };
        self.queue_view_drop_from_path(model, path.clone());
        true
    }

    pub(crate) fn maybe_intercept_node_drop_on_model(
        &mut self,
        moved: &[NodeId],
        world: Pos2,
    ) -> bool {
        let Some(model) = self.model_node_at_world(world) else {
            return false;
        };
        let Some(image) = moved.iter().find(|id| {
            self.doc().scene.node(**id).is_some_and(|n| {
                matches!(&n.kind, NodeKind::Image(img) if {
                    self.doc()
                        .item(img.item)
                        .is_some_and(|it| slate_doc::media_kind(&it.path) == slate_doc::MediaKind::Image)
                        && !self.model_has_viewport(**id)
                })
            })
        }) else {
            return false;
        };
        let Some(n) = self.doc().scene.node(*image) else {
            return false;
        };
        let NodeKind::Image(img) = &n.kind else {
            return false;
        };
        self.queue_view_drop_from_item(model, img.item);
        true
    }
}

fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
