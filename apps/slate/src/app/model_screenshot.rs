//! Viewport screenshot export and drop-back camera restore.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_shell::file_picker::{first, PickRequest};
use eframe::egui::{self, Id, Pos2, Rect, Vec2};
use model_preview::view_meta::{self, ViewMetaInput};
use slate_doc::scene::{ImageAdjust, ImageNode, ModelCamera, NodeId, NodeKind, WorldRect};

use super::model3d::{self, screenshot_size, ModelNodeInfo};
use super::{PickerMsg, SlateApp};

const POPUP_ITEM_H: f32 = 28.0;
const POPUP_W: f32 = 200.0;

pub struct ModelScreenshotPopup {
    pub tab_id: u64,
    pub node: NodeId,
    /// Screen position when the menu opened (P2 pointer-attached chrome).
    pub anchor: Pos2,
    /// The click that opened the menu is still this frame's click, so it
    /// must not count as a click elsewhere.
    pub opening: bool,
}

/// An export waiting for the model's mesh to finish parsing.
pub struct PendingModelShot {
    pub tab_id: u64,
    pub node: NodeId,
    /// `None` exports to the canvas.
    pub dest: Option<PathBuf>,
}

/// Rendered viewport pixels plus the view they record.
struct ModelShot {
    rgba: Vec<u8>,
    w: u32,
    h: u32,
    meta: ViewMetaInput,
}

enum ViewDropMsg {
    Restored(view_meta::ViewMetaParsed),
    NoMeta,
    Err(String),
}

pub struct PendingViewDrop {
    pub tab_id: u64,
    pub model: NodeId,
    rx: Receiver<ViewDropMsg>,
}

impl SlateApp {
    pub(crate) fn open_model_screenshot_menu(&mut self, node: NodeId, anchor: Pos2) {
        self.model_shot_popup = Some(ModelScreenshotPopup {
            tab_id: self.tab().id,
            node,
            anchor,
            opening: true,
        });
    }

    /// Pointer-attached menu (P2): screen-space placement under the cursor.
    pub(crate) fn paint_model_screenshot_popup(&mut self, ctx: &egui::Context) -> bool {
        if self
            .model_shot_popup
            .as_ref()
            .is_some_and(|popup| popup.tab_id != self.tab().id)
        {
            self.model_shot_popup = None;
            return false;
        }
        let Some(popup) = self.model_shot_popup.as_ref() else {
            return false;
        };
        let menu_node = popup.node;
        let menu_tab = popup.tab_id;
        let anchor = popup.anchor;
        let opening = popup.opening;
        let palette = self.palette();
        let mut choice_canvas = false;
        let mut choice_folder = false;
        let mut dismiss = false;

        let item_h = POPUP_ITEM_H;
        let w = POPUP_W;
        let h = item_h * 2.0 + 6.0;
        let top = anchor.y + 6.0;
        let rect = Rect::from_min_size(Pos2::new(anchor.x - w * 0.5, top), Vec2::new(w, h));

        let resp = egui::Area::new(Id::new(("model_shot_menu", menu_tab, menu_node.0)))
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
        if resp.response.clicked_elsewhere() && !opening {
            dismiss = true;
        }
        if let Some(popup) = self.model_shot_popup.as_mut() {
            popup.opening = false;
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
        let tab_id = self.tab().id;
        self.picker.open(
            PickRequest::save()
                .filter("PNG", &["png"])
                .filter("JPEG", &["jpg", "jpeg"])
                .filter("WebP", &["webp"]),
            move |picked| PickerMsg::ModelScreenshotSave {
                tab_id,
                node,
                path: first(picked),
            },
        );
    }

    pub fn export_model_screenshot_canvas(&mut self, node: NodeId) {
        self.run_model_screenshot(node, None, true);
    }

    pub fn finish_model_screenshot_save(&mut self, node: NodeId, path: PathBuf) {
        self.run_model_screenshot(node, Some(path), true);
    }

    /// Render, then write or place. A mesh that is not parsed yet (frozen
    /// viewports show a cached poster) queues the export instead of asking
    /// for a second click; `may_wait` is false on that retry.
    fn run_model_screenshot(&mut self, node: NodeId, dest: Option<PathBuf>, may_wait: bool) {
        match self.render_model_screenshot_rgba(node) {
            Ok(Some(shot)) => self.save_model_screenshot(node, shot, dest),
            Ok(None) if may_wait => {
                self.toast("Loading the 3D model — the screenshot follows");
                self.model_shot_pending = Some(PendingModelShot {
                    tab_id: self.tab().id,
                    node,
                    dest,
                });
            }
            Ok(None) => self.toast("The 3D model could not be rendered for a screenshot"),
            Err(e) => self.toast(format!("Screenshot failed: {e}")),
        }
    }

    /// Finish a queued export once its model has parsed (per frame; cheap
    /// until then).
    pub(crate) fn maintain_model_shot_pending(&mut self) {
        let Some(pending) = self.model_shot_pending.as_ref() else {
            return;
        };
        if self.at_home || pending.tab_id != self.tab().id {
            return;
        }
        let node = pending.node;
        let Some(info) = self.model_node_info(node) else {
            self.model_shot_pending = None;
            return;
        };
        let Some(outcome) = self.model3d.parse_outcome(&info.cache_key) else {
            return;
        };
        let pending = self.model_shot_pending.take().expect("checked above");
        match outcome {
            Ok(()) => self.run_model_screenshot(node, pending.dest, false),
            Err(e) => self.toast(format!("Screenshot failed: {e}")),
        }
    }

    /// Write the file, then (canvas export) link and place it. Every failure
    /// toasts.
    fn save_model_screenshot(&mut self, node: NodeId, shot: ModelShot, dest: Option<PathBuf>) {
        let to_canvas = dest.is_none();
        let path = match dest {
            Some(p) => p,
            None => match self.model_screenshot_output_path(node, "png") {
                Ok(p) => p,
                Err(e) => return self.toast(&e),
            },
        };
        if let Err(e) = write_model_screenshot(&path, &shot) {
            return self.toast(format!("Screenshot failed: {e}"));
        }
        if !to_canvas {
            return self.toast(format!(
                "Saved {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        let Some(item) = self.item_for_path(&path) else {
            return self.toast("Viewport screenshot could not be linked");
        };
        match self.place_screenshot_beside_model(node, item) {
            Some(placed) => {
                self.board_sel = std::iter::once(placed).collect();
                self.toast("Viewport screenshot placed on the board");
            }
            None => self.toast(format!(
                "Saved {} but could not place it on the board",
                path.display()
            )),
        }
    }

    fn render_model_screenshot_rgba(&mut self, node: NodeId) -> Result<Option<ModelShot>, String> {
        let info = self
            .model_node_info(node)
            .ok_or("That node is not a 3D model.")?;
        let (w, h) = screenshot_size(info.rect.w, info.rect.h);
        let aspect = info.rect.w / info.rect.h.max(1.0);
        let Some(img) = self.model_screenshot_pixels(node, w, h, aspect)? else {
            return Ok(None);
        };
        let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
        for p in &img.pixels {
            rgba.extend_from_slice(&p.to_srgba_unmultiplied());
        }
        let cam = self.model_screenshot_camera(node, &info);
        let adjust = self.model_screenshot_adjust(node);
        let meta = self.view_meta_input(node, &info, cam, w, h, &adjust)?;
        Ok(Some(ModelShot { rgba, w, h, meta }))
    }

    fn model_screenshot_camera(&self, node: NodeId, info: &ModelNodeInfo) -> ModelCamera {
        self.model3d
            .live
            .get(&node)
            .map(|vp| vp.cam)
            .unwrap_or(info.cam)
    }

    fn model_screenshot_adjust(&self, node: NodeId) -> ImageAdjust {
        self.doc()
            .scene
            .node(node)
            .and_then(slate_doc::scene::adjust_of)
            .unwrap_or_default()
    }

    /// The screenshot's pixels at `w` x `h`, projected at `aspect`.
    /// `Ok(None)` while the mesh is still parsing.
    pub(crate) fn model_screenshot_pixels(
        &mut self,
        node: NodeId,
        w: u32,
        h: u32,
        aspect: f32,
    ) -> Result<Option<egui::ColorImage>, String> {
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
        let cam = self.model_screenshot_camera(node, &info);
        let adjust = self.model_screenshot_adjust(node);
        let img = self.model3d.render_view_screenshot(
            &gl,
            &info.cache_key,
            &cam,
            (w, h, aspect),
            (!adjust.is_identity()).then_some(&adjust),
        );
        if img.is_none() {
            self.model3d.request_model(&info.cache_key, &info.path);
        }
        Ok(img)
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
        let tab_id = self.tab().id;
        std::thread::spawn(move || {
            let msg = if atlas_core::cloud::is_dehydrated(&image_path) {
                ViewDropMsg::NoMeta
            } else {
                match view_meta::read_view_meta(&image_path) {
                    Ok(Some(parsed)) => ViewDropMsg::Restored(parsed),
                    Ok(None) => ViewDropMsg::NoMeta,
                    Err(e) => ViewDropMsg::Err(e.to_string()),
                }
            };
            let _ = tx.send(msg);
        });
        self.pending_view_drop = Some(PendingViewDrop { tab_id, model, rx });
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
        let tab_id = pending.tab_id;
        let model = pending.model;
        self.pending_view_drop = None;
        if tab_id != self.tab().id {
            return;
        }
        match msg {
            ViewDropMsg::NoMeta => self.toast("No saved Slate view in that image"),
            ViewDropMsg::Err(e) => self.toast(&e),
            ViewDropMsg::Restored(parsed) => self.apply_view_drop(model, parsed),
        }
    }

    pub(crate) fn apply_view_drop(&mut self, model: NodeId, parsed: view_meta::ViewMetaParsed) {
        if self.doc().scene.node(model).is_none_or(|n| n.locked) || self.refuse_read_only_edit() {
            return;
        }
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

/// Encode by extension with the view packet embedded (PNG, JPEG, WebP).
fn write_model_screenshot(path: &Path, shot: &ModelShot) -> Result<(), String> {
    let xmp = view_meta::build_xmp_packet(&shot.meta).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let (rgba, w, h) = (&shot.rgba, shot.w, shot.h);
    match ext.as_str() {
        "png" => view_meta::write_png_with_xmp(path, rgba, w, h, &xmp),
        "jpg" | "jpeg" => {
            let rgb: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
            view_meta::write_jpeg_with_xmp(path, &rgb, w, h, 92, &xmp)
        }
        "webp" => view_meta::write_webp_with_xmp(path, rgba, w, h, &xmp),
        _ => return Err("Use PNG, JPEG, or WebP".into()),
    }
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::tests::Harness;
    use super::*;

    fn add_model(app: &mut SlateApp, path: PathBuf) -> NodeId {
        let item = app
            .doc_mut()
            .add_item(path, "model.obj", 1, 0, "model-cache");
        let node = app.doc_mut().scene.build_node(
            WorldRect::new(0.0, 0.0, 320.0, 200.0),
            NodeKind::Image(ImageNode::new(item)),
        );
        app.add_nodes(vec![node])[0]
    }

    /// A saved workbook with one selected triangle model on its board.
    fn selected_model(tag: &str) -> (Harness, NodeId) {
        let mut h = Harness::new(tag);
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.tab_mut().path = Some(h.base.join("book.slate"));
        let source = h.base.join("tri.obj");
        std::fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let items = h.app.add_paths(&[source]);
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        h.app.place_items_on_board(&items, Pos2::ZERO);
        let id = h.app.doc().scene.nodes.last().unwrap().id;
        let rect = h.app.doc().scene.node(id).unwrap().rect;
        h.app.zoom_to_rect(WorldRect::new(
            rect.x - rect.w,
            rect.y - rect.h,
            rect.w * 3.0,
            rect.h * 3.0,
        ));
        h.app.board_sel = std::iter::once(id).collect();
        for _ in 0..3 {
            h.frame();
        }
        (h, id)
    }

    fn click(h: &mut Harness, p: Pos2) {
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(p)));
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            });
        }
        h.frame();
    }

    fn click_screenshot_button(h: &mut Harness) {
        let button = h
            .app
            .model_screenshot_button()
            .expect("a selected model offers the screenshot button");
        click(h, button.center());
    }

    #[test]
    fn a_model_draws_and_hits_no_view_port_at_rest() {
        use slate_doc::agent_inputs::{input_ports_of, MODEL_VIEW_PORT_T};
        use slate_doc::scene::Side;
        let (mut h, model) = selected_model("view_port_rest");
        let xf = h.app.board_xf();
        let node = h.app.doc().scene.node(model).unwrap().clone();
        let old_port = slate_doc::WireHost::from_node_flow(&node, input_ports_of(&node, true))
            .anchor(Side::Left, MODEL_VIEW_PORT_T);
        let at = xf.w2s(Pos2::new(old_port[0], old_port[1]));
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(at)));
        let out = h.ctx.run(Default::default(), |ctx| {
            let layer = egui::LayerId::new(egui::Order::Foreground, Id::new("ports"));
            h.app.paint_flow_ports(&ctx.layer_painter(layer), &xf);
        });
        assert!(
            out.shapes.is_empty(),
            "selected, pointer on the old site: nothing painted"
        );
        assert_eq!(h.app.wire_grip_at(at, &xf), None, "and nothing to grab");
    }

    #[test]
    fn a_view_wire_snaps_to_the_model_edge_during_a_drag_and_binds() {
        use slate_doc::scene::Side;
        let (mut h, model) = selected_model("view_port_drag");
        let png = h.base.join("shot.png");
        image::RgbaImage::new(4, 4).save(&png).unwrap();
        let item = h
            .app
            .doc_mut()
            .add_item(png, "shot.png", 1, 0, "shot-cache");
        let rect = h.app.doc().scene.node(model).unwrap().rect;
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(rect.x - rect.w * 0.8, rect.y, rect.w * 0.4, rect.h * 0.4),
            NodeKind::Image(ImageNode::new(item)),
        );
        let shot = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.clear();
        h.frame();

        let xf = h.app.board_xf();
        let shot_rect = h.app.doc().scene.node(shot).unwrap().rect;
        let grip = xf.w2s(super::super::board_wire::grip_point(shot_rect, Side::Right));
        let Some(super::super::board::BoardDrag::Wire(mut wd)) =
            h.app
                .begin_gesture_for_test(grip, xf.s2w(grip), Default::default())
        else {
            panic!("a wire starts at the picture's grip");
        };
        let near_edge = Pos2::new(rect.x - 2.0 / xf.z, rect.y + rect.h * 0.7);
        h.app.wire_drag_update(&mut wd, near_edge, false);
        let (node, side, t) = wd.snap.expect("near the edge, the model takes the wire");
        assert_eq!((node, side), (model, Side::Left));
        assert!(
            (t - 0.7).abs() < 0.05,
            "at the pointer, not a fixed port: {t}"
        );
        h.app.finish_wire_drag(wd);
        let binding = h
            .app
            .doc()
            .scene
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                NodeKind::Connector(c) => c.binding.clone(),
                _ => None,
            })
            .expect("the wire is bound");
        assert_eq!(
            binding.slot.as_deref(),
            Some(atlas_agent::InputSlot::View.id())
        );
    }

    #[test]
    fn the_screenshot_button_opens_a_menu_that_stays_open() {
        let (mut h, id) = selected_model("shot_button_opens");
        click_screenshot_button(&mut h);
        let popup = h.app.model_shot_popup.as_ref().expect("the menu is open");
        assert_eq!(popup.node, id);
        h.frame();
        assert!(h.app.model_shot_popup.is_some(), "and stays open");
    }

    #[test]
    fn export_to_canvas_requests_a_capture_and_says_why_it_cannot() {
        let (mut h, id) = selected_model("shot_export_no_gl");
        click_screenshot_button(&mut h);
        assert!(h.app.model_shot_popup.is_some(), "the menu is open");
        let tab = h.app.tab().id;
        let menu = h
            .ctx
            .memory(|m| m.area_rect(Id::new(("model_shot_menu", tab, id.0))))
            .expect("the menu was laid out");
        let nodes = h.app.doc().scene.nodes.len();
        // "Export to canvas" is the upper of the two left-aligned buttons.
        click(
            &mut h,
            Pos2::new(menu.left() + 24.0, menu.top() + menu.height() * 0.25),
        );
        assert!(h.app.model_shot_popup.is_none(), "choosing closes the menu");
        assert!(
            h.app
                .toasts
                .iter()
                .any(|(m, _)| m.starts_with("Screenshot failed") && m.contains("GPU rendering")),
            "the capture ran and its failure is visible: {:?}",
            h.app.toasts.iter().map(|(m, _)| m).collect::<Vec<_>>()
        );
        assert!(
            !h.app
                .toasts
                .iter()
                .any(|(m, _)| m.starts_with("3D viewports")),
            "the menu click did not reach the board beneath it"
        );
        assert_eq!(h.app.doc().scene.nodes.len(), nodes);
    }

    #[test]
    fn a_rendered_screenshot_lands_as_one_image_node_and_one_file() {
        let (mut h, id) = selected_model("shot_writer");
        let info = h.app.model_node_info(id).unwrap();
        let (w, hh) = (8u32, 4u32);
        let cam = ModelCamera {
            yaw: 0.7,
            distance: 5.0,
            ..info.cam
        };
        let meta = h
            .app
            .view_meta_input(id, &info, cam, w, hh, &ImageAdjust::default())
            .unwrap();
        let shot = ModelShot {
            rgba: vec![200; (w * hh * 4) as usize],
            w,
            h: hh,
            meta,
        };
        let before = h.app.doc().scene.nodes.len();
        h.app.save_model_screenshot(id, shot, None);
        let scene = &h.app.doc().scene;
        assert_eq!(scene.nodes.len(), before + 1, "exactly one node added");
        let placed = scene.nodes.last().unwrap();
        let NodeKind::Image(img) = &placed.kind else {
            panic!("an image node");
        };
        let path = h.app.doc().item(img.item).unwrap().path.clone();
        assert!(
            path.starts_with(h.base.join("assets")),
            "{}",
            path.display()
        );
        let files: Vec<_> = std::fs::read_dir(h.base.join("assets")).unwrap().collect();
        assert_eq!(files.len(), 1, "exactly one file written");
        let restored = view_meta::read_view_meta(&path)
            .unwrap()
            .expect("view packet");
        assert!((restored.camera.yaw - 0.7).abs() < 1e-4);
        assert_eq!(
            h.app.board_sel.iter().copied().collect::<Vec<_>>(),
            vec![placed.id]
        );
    }

    fn parsed(camera: ModelCamera) -> view_meta::ViewMetaParsed {
        view_meta::ViewMetaParsed {
            camera,
            model_name: "model.obj".into(),
            model_path: "model.obj".into(),
            model_hash: String::new(),
            model_size: 1,
            node_id: 1,
        }
    }

    #[test]
    fn view_drop_result_never_crosses_tabs_with_colliding_node_ids() {
        let mut h = Harness::new("view_drop_tabs");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let first_id = add_model(&mut h.app, h.base.join("first.obj"));
        let first_tab = h.app.tab().id;
        let restored = ModelCamera {
            yaw: 1.25,
            distance: 8.0,
            ..ModelCamera::default()
        };
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(ViewDropMsg::Restored(parsed(restored))).unwrap();
        h.app.pending_view_drop = Some(PendingViewDrop {
            tab_id: first_tab,
            model: first_id,
            rx,
        });

        h.app.new_tab();
        let second_id = add_model(&mut h.app, h.base.join("second.obj"));
        assert_eq!(first_id, second_id, "fixture must exercise an id collision");
        h.app.maintain_view_drop();
        let second_camera = match &h.app.doc().scene.node(second_id).unwrap().kind {
            NodeKind::Image(image) => image.model,
            _ => unreachable!(),
        };
        assert_ne!(second_camera, restored);

        h.app.switch_tab(0);
        let first_camera = match &h.app.doc().scene.node(first_id).unwrap().kind {
            NodeKind::Image(image) => image.model,
            _ => unreachable!(),
        };
        assert_ne!(first_camera, restored, "inactive result must be discarded");
    }

    #[test]
    fn view_drop_refuses_read_only_workbook() {
        let mut h = Harness::new("view_drop_read_only");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let id = add_model(&mut h.app, h.base.join("readonly.obj"));
        let before = match &h.app.doc().scene.node(id).unwrap().kind {
            NodeKind::Image(image) => image.model,
            _ => unreachable!(),
        };
        let restored = ModelCamera {
            yaw: 0.9,
            distance: 6.0,
            ..before
        };
        h.app.tab_mut().read_only = true;
        h.app.apply_view_drop(id, parsed(restored));
        let after = match &h.app.doc().scene.node(id).unwrap().kind {
            NodeKind::Image(image) => image.model,
            _ => unreachable!(),
        };
        assert_eq!(after, before);
        assert!(h
            .app
            .toasts
            .iter()
            .any(|(message, _)| message.contains("read-only")));
    }
}
