//! Viewport screenshot export and drop-back camera restore.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use atlas_shell::file_picker::{first, PickRequest};
use atlas_shell::selection_tools::popup_area;
use eframe::egui::{self, Color32, Id, Pos2, Rect, Vec2};
use model_preview::view_meta::{self, ViewMetaInput};
use slate_doc::scene::{ImageAdjust, ImageNode, ModelCamera, Node, NodeId, NodeKind, WorldRect};

use super::board::{BoardDrag, BoardXf};
use super::model3d::{self, screenshot_size, ModelNodeInfo, ViewTween};
use super::{PickerMsg, SlateApp};

mod popup;
mod preview;
mod restore;

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

#[derive(Debug)]
enum ViewDropMsg {
    Restored(view_meta::ViewMetaParsed, Option<egui::ColorImage>),
    NoMeta,
    Err(String),
}

pub struct PendingViewDrop {
    pub tab_id: u64,
    pub model: NodeId,
    image_path: PathBuf,
    picture: Option<Node>,
    pointer: Pos2,
    rx: Receiver<ViewDropMsg>,
}

/// Longest side of a dragged file's picture at the pointer (screen px,
/// pointer-attached like P2.GhostFollow).
const SINK_FILE_PX: f32 = 120.0;

type PreviewMsg = Option<(view_meta::ViewMetaParsed, Option<egui::ColorImage>)>;

/// What carries the saved view being dragged over a model.
#[derive(Clone, PartialEq)]
pub(crate) enum ViewDropSource {
    Node(NodeId),
    File(PathBuf),
}

/// A board picture released over a model, resolved before the move rewinds.
pub(crate) struct NodeViewDrop {
    model: NodeId,
    image: NodeId,
    item: slate_doc::ItemId,
    picture: Node,
}

/// A saved view held over a model: the viewport orients to it and the
/// picture sinks in. Derived state, never journaled (media D38); release
/// commits one camera patch, leaving puts everything back.
pub struct ViewDropPreview {
    tab_id: u64,
    model: NodeId,
    source: ViewDropSource,
    rx: Option<Receiver<PreviewMsg>>,
    parsed: Option<view_meta::ViewMetaParsed>,
    thumb: Option<egui::TextureHandle>,
    /// The live pose the preview took over from.
    original: Option<ModelCamera>,
    /// The preview opened a frozen viewport and closes it again.
    entered: bool,
    returning: bool,
    sink: f32,
    tick: Instant,
    pointer: Pos2,
    /// A committed fast drop keeps only this derived visual lifecycle alive.
    released: bool,
    /// Release-time pixels/geometry, independent of the rewound source node.
    picture: Option<Node>,
}

impl ViewDropPreview {
    fn new(tab_id: u64, model: NodeId, source: ViewDropSource, pointer: Pos2) -> Self {
        Self {
            tab_id,
            model,
            source,
            pointer,
            rx: None,
            parsed: None,
            thumb: None,
            original: None,
            entered: false,
            returning: false,
            sink: 0.0,
            tick: Instant::now(),
            released: false,
            picture: None,
        }
    }
}

impl SlateApp {
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
            if workbook.parent().is_some() {
                let dir = atlas_core::workbook_assets::assets_root(workbook);
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
}

/// A small picture of a dragged image file (worker thread only).
fn file_thumbnail(path: &Path) -> Option<egui::ColorImage> {
    let img = image::open(path).ok()?.thumbnail(256, 256).to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    ))
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
mod regression;
#[cfg(test)]
mod tests;
