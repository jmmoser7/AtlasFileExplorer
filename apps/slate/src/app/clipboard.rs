//! Board clipboard: copy / cut / paste of scene nodes, and paste of an image
//! or files copied outside Slate.
//!
//! Node payload is plain `Vec<Node>` JSON (the same serde model the `.slate`
//! file uses), kept app-internally *and* put on the OS clipboard under
//! Slate's own registered format, so selections round-trip between tabs and
//! Slate instances. The same clipboard write offers other apps something they
//! can paste: copied pictures as a bitmap as shown (CF_DIBV5 and PNG) plus
//! their linked files, anything else as plain text, never the JSON. Slate's
//! own format is read before any of those on paste. A copied image
//! (screenshot, "Copy image") or a file list from elsewhere is not that
//! payload: those land as board items, the same intake as a drop. All
//! mutations go through the
//! journal (Constitution Art. VI): cut = one Remove group, paste = one Add
//! group. The pasted bitmap is a file the workbook links to, never bytes
//! stored in the `.slate` (Art. IX).
//!
//! Connector rules (`docs/keymap/specs/constraints.md` §3):
//! - connectors whose *both* anchored ends are inside the selection ride
//!   along automatically, even when not selected themselves;
//! - a copied connector's anchored end that points *outside* the copied set
//!   degrades to `Free` at its current world point (resolved at copy time,
//!   when the source scene is still available).

use super::SlateApp;
use eframe::egui::Pos2;
use image::RgbaImage;
use slate_doc::scene::{ConnectorEnd, GroupKey, Node, NodeKind, Scene, WireDisplay};
use slate_doc::{connector_anchor_on, WireHost};
use slate_doc::{NodeId, SlateDoc};
use std::collections::{HashMap, HashSet};
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Clipboard bitmaps larger than this are refused. A paste is one user
/// action, not a frame, but it still must not allocate a runaway buffer.
#[cfg(any(windows, test))]
const MAX_PASTE_PIXELS: u64 = 64 * 1024 * 1024;
#[cfg(windows)]
const MAX_PASTE_PNG_BYTES: usize = 80 * 1024 * 1024;

/// A copied bitmap is scaled down past this many pixels or this side length.
const MAX_COPY_PIXELS: f32 = 40.0 * 1024.0 * 1024.0;
const MAX_COPY_SIDE: f32 = 16384.0;

/// Slate's own clipboard format: the node payload JSON.
#[cfg_attr(any(test, not(windows)), allow(dead_code))]
const SLATE_CLIPBOARD_FORMAT: &str = "Slate.BoardNodes";

/// Step applied to each successive Ctrl+V paste of the same payload.
const PASTE_STEP: f32 = 24.0;

// ---------- pure payload / remap logic (unit-tested) ----------

/// Build the clipboard payload for `selected` out of `scene`: the selected
/// nodes in z-order, plus unselected connectors whose both anchored ends are
/// selected. Anchored ends leaving the payload become `Free` at their
/// current world point.
pub fn clipboard_payload(scene: &Scene, selected: &HashSet<NodeId>) -> Vec<Node> {
    let in_set = |end: &ConnectorEnd| match end {
        ConnectorEnd::Anchored { node, .. } => selected.contains(node),
        ConnectorEnd::Free { .. } => true,
    };
    let mut payload: Vec<Node> = Vec::new();
    for node in &scene.nodes {
        let take = selected.contains(&node.id)
            || matches!(&node.kind, NodeKind::Connector(c)
                if matches!(c.a, ConnectorEnd::Anchored { .. })
                    && matches!(c.b, ConnectorEnd::Anchored { .. })
                    && in_set(&c.a)
                    && in_set(&c.b));
        if !take {
            continue;
        }
        let mut node = node.clone();
        if let NodeKind::Connector(c) = &mut node.kind {
            for end in [&mut c.a, &mut c.b] {
                let ConnectorEnd::Anchored {
                    node: target,
                    side,
                    t,
                } = *end
                else {
                    continue;
                };
                if !selected.contains(&target) {
                    let point = scene
                        .node(target)
                        .map(|n| connector_anchor_on(n, side, t))
                        .unwrap_or([0.0, 0.0]);
                    *end = ConnectorEnd::Free { point };
                }
            }
        }
        payload.push(node);
    }
    payload
}

/// Rebuild a payload for insertion: fresh node ids (via `next_id`), fresh
/// group keys (one per distinct source key, via `next_group`), rects and
/// free connector points translated by `(dx, dy)`, and anchored connector
/// ends remapped onto the fresh ids. Ends anchored to nodes missing from the
/// payload (a foreign/hand-edited payload) degrade to `Free` at the offset
/// source anchor point when resolvable, else at the payload's first rect.
pub fn remap_for_paste(
    payload: &[Node],
    mut next_id: impl FnMut() -> NodeId,
    mut next_group: impl FnMut() -> GroupKey,
    dx: f32,
    dy: f32,
) -> Vec<Node> {
    let mut id_map: HashMap<NodeId, NodeId> = HashMap::new();
    for n in payload {
        id_map.insert(n.id, next_id());
    }
    let mut group_map: HashMap<GroupKey, GroupKey> = HashMap::new();
    let src_rect: HashMap<NodeId, slate_doc::scene::WorldRect> =
        payload.iter().map(|n| (n.id, n.rect)).collect();

    payload
        .iter()
        .map(|src| {
            let mut n = src.clone();
            n.id = id_map[&src.id];
            slate_doc::agent_chat::remap_view(&mut n, |id| id_map.get(&id).copied());
            n.rect = n.rect.translated(dx, dy);
            if let Some(g) = n.group {
                n.group = Some(*group_map.entry(g).or_insert_with(&mut next_group));
            }
            if let NodeKind::Image(ref mut img) = n.kind {
                slate_doc::image_paint::fresh_layer_node_ids(img, &mut next_id);
            }
            if let NodeKind::Connector(c) = &mut n.kind {
                if let Some(binding) = &mut c.binding {
                    if binding.order.is_empty() {
                        binding.order = vec![src.id.0];
                    }
                }
                for end in [&mut c.a, &mut c.b] {
                    match *end {
                        ConnectorEnd::Anchored { node, side, t } => {
                            if let Some(new_id) = id_map.get(&node) {
                                *end = ConnectorEnd::Anchored {
                                    node: *new_id,
                                    side,
                                    t,
                                };
                            } else {
                                let p = src_rect
                                    .get(&node)
                                    .map(|r| WireHost::from_rect(*r).anchor(side, t))
                                    .unwrap_or([src.rect.x, src.rect.y]);
                                *end = ConnectorEnd::Free {
                                    point: [p[0] + dx, p[1] + dy],
                                };
                            }
                        }
                        ConnectorEnd::Free { point } => {
                            *end = ConnectorEnd::Free {
                                point: [point[0] + dx, point[1] + dy],
                            };
                        }
                    }
                }
            }
            n
        })
        .collect()
}

/// Union of payload rects (world), for centering pastes on a target point.
fn payload_bounds(payload: &[Node]) -> Option<(f32, f32, f32, f32)> {
    let mut it = payload.iter();
    let first = it.next()?;
    let mut min_x = first.rect.x;
    let mut min_y = first.rect.y;
    let mut max_x = first.rect.x + first.rect.w;
    let mut max_y = first.rect.y + first.rect.h;
    for n in it {
        min_x = min_x.min(n.rect.x);
        min_y = min_y.min(n.rect.y);
        max_x = max_x.max(n.rect.x + n.rect.w);
        max_y = max_y.max(n.rect.y + n.rect.h);
    }
    Some((min_x, min_y, max_x, max_y))
}

/// What a copy offers as plain text to other apps: nothing for pictures
/// alone (they go as a bitmap), the words of any text nodes, or else a
/// one-line summary. Never the node JSON.
pub(crate) fn clipboard_text_fallback(
    payload: &[Node],
    is_picture: impl Fn(&Node) -> bool,
) -> Option<String> {
    if payload.iter().all(&is_picture) {
        return None;
    }
    let words: Vec<&str> = payload
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Text(t) => Some(t.text.as_str()),
            NodeKind::Shape(s) => s.text.as_ref().map(|t| t.body.as_str()),
            _ => None,
        })
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    if !words.is_empty() {
        return Some(words.join("\n\n"));
    }
    let kinds: [(&str, &str); 8] = [
        ("picture", "pictures"),
        ("file card", "file cards"),
        ("frame", "frames"),
        ("shape", "shapes"),
        ("text box", "text boxes"),
        ("wire", "wires"),
        ("portal", "portals"),
        ("dock strip", "dock strips"),
    ];
    let mut counts = [0usize; 8];
    for n in payload {
        let slot = match &n.kind {
            NodeKind::Image(_) if is_picture(n) => 0,
            NodeKind::Image(_) => 1,
            NodeKind::Frame(_) => 2,
            NodeKind::Shape(_) => 3,
            NodeKind::Text(_) => 4,
            NodeKind::Connector(_) => 5,
            NodeKind::Portal(_) => 6,
            NodeKind::DockStrip(_) => 7,
        };
        counts[slot] += 1;
    }
    let parts: Vec<String> = kinds
        .iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .map(|((one, many), n)| format!("{n} {}", if n == 1 { one } else { many }))
        .collect();
    Some(format!("Slate selection: {}", parts.join(", ")))
}

/// Everything one copy puts on the OS clipboard, written in one open.
#[derive(Debug, Clone, Default)]
#[cfg_attr(not(windows), allow(dead_code))] // Only the Windows writer reads every format.
pub(crate) struct ClipboardWrite {
    /// Slate's own format: the node payload JSON.
    pub(crate) nodes_json: String,
    /// Plain text for other apps, when there is any. Never the JSON.
    pub(crate) text: Option<String>,
    /// The copied pictures as shown, PNG encoded.
    pub(crate) png: Option<Vec<u8>>,
    /// The same pixels as a CF_DIBV5 payload.
    pub(crate) dibv5: Option<Vec<u8>>,
    /// Linked source files of the copied pictures (CF_HDROP).
    pub(crate) files: Vec<PathBuf>,
}

/// A copy waiting for its bitmap. Rendering reads and decodes source files,
/// so it runs on a worker, never on the frame loop.
struct ClipboardJob {
    nodes_json: String,
    text: Option<String>,
    pictures: Vec<Node>,
    files: Vec<PathBuf>,
    /// Only the items the pictures and their paint layers link to.
    doc: SlateDoc,
}

/// The OS-clipboard side of copy: which write is newest, and whether it has
/// landed yet (until then, paste uses the in-app buffer).
#[derive(Default)]
pub(crate) struct OsClipboard {
    #[cfg_attr(test, allow(dead_code))]
    generation: Arc<AtomicU64>,
    writing: Arc<AtomicBool>,
    #[cfg(test)]
    last: Option<ClipboardWrite>,
}

impl OsClipboard {
    fn writing(&self) -> bool {
        self.writing.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn last_write(&self) -> Option<&ClipboardWrite> {
        self.last.as_ref()
    }

    /// Slate's own payload on the clipboard, if the newest write is there.
    fn slate_nodes(&self) -> Option<String> {
        #[cfg(test)]
        {
            self.last.as_ref().map(|w| w.nodes_json.clone())
        }
        #[cfg(all(windows, not(test)))]
        {
            read_slate_clipboard()
        }
        #[cfg(all(not(windows), not(test)))]
        {
            None
        }
    }

    fn submit(&mut self, job: ClipboardJob) {
        #[cfg(test)]
        {
            self.last = Some(render_clipboard(job));
        }
        #[cfg(not(test))]
        {
            let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
            self.writing.store(true, Ordering::Release);
            let newest = Arc::clone(&self.generation);
            let writing = Arc::clone(&self.writing);
            let spawned = std::thread::Builder::new()
                .name("slate-clipboard".into())
                .spawn(move || {
                    let write = render_clipboard(job);
                    if newest.load(Ordering::Acquire) == generation {
                        write_os_clipboard(&write);
                    }
                    if newest.load(Ordering::Acquire) == generation {
                        writing.store(false, Ordering::Release);
                    }
                });
            if spawned.is_err() {
                self.writing.store(false, Ordering::Release);
            }
        }
    }
}

fn render_clipboard(job: ClipboardJob) -> ClipboardWrite {
    let bitmap = render_pictures(&job.doc, &job.pictures);
    ClipboardWrite {
        png: bitmap
            .as_ref()
            .and_then(|b| encode_png(b.width(), b.height(), b.as_raw())),
        dibv5: bitmap.as_ref().map(encode_dibv5),
        files: job.files.into_iter().filter(|p| p.is_file()).collect(),
        nodes_json: job.nodes_json,
        text: job.text,
    }
}

/// The copied pictures composed as they sit on the board: each one as shown
/// (`image_composite::composite_rgba`), scaled so the sharpest keeps its own
/// resolution, turned by its rotation, faded by its opacity. Transparent
/// between pictures. Cloud placeholders are skipped, never downloaded.
fn render_pictures(doc: &SlateDoc, pictures: &[Node]) -> Option<RgbaImage> {
    let shown: Vec<(&Node, RgbaImage)> = pictures
        .iter()
        .filter_map(|node| {
            let NodeKind::Image(img) = &node.kind else {
                return None;
            };
            let source = &doc.item(img.item)?.path;
            if atlas_core::cloud::is_dehydrated(source) {
                return None;
            }
            let pixels = super::image_composite::composite_rgba(doc, node, img, source, true)?;
            Some((node, pixels))
        })
        .collect();
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut scale: f32 = 0.0;
    for (node, pixels) in &shown {
        for (x, y) in node.rect.corners_rotated(node.rotation_deg) {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        scale = scale
            .max(pixels.width() as f32 / node.rect.w.max(1e-3))
            .max(pixels.height() as f32 / node.rect.h.max(1e-3));
    }
    if shown.is_empty() || scale <= 0.0 {
        return None;
    }
    let (bw, bh) = ((max_x - min_x).max(1e-3), (max_y - min_y).max(1e-3));
    scale = scale
        .min((MAX_COPY_PIXELS / (bw * bh)).sqrt())
        .min(MAX_COPY_SIDE / bw.max(bh));
    let w = (bw * scale).round().max(1.0) as u32;
    let h = (bh * scale).round().max(1.0) as u32;
    let mut canvas = RgbaImage::new(w, h);
    for (node, pixels) in &shown {
        let tw = (node.rect.w * scale).round().max(1.0) as u32;
        let th = (node.rect.h * scale).round().max(1.0) as u32;
        let resized;
        let fitted = if pixels.dimensions() == (tw, th) {
            pixels
        } else {
            resized = image::imageops::resize(pixels, tw, th, image::imageops::Triangle);
            &resized
        };
        let (cx, cy) = node.rect.center();
        let center = ((cx - min_x) * scale, (cy - min_y) * scale);
        draw_turned(&mut canvas, fitted, center, node.rotation_deg, node.opacity);
    }
    Some(canvas)
}

/// Source-over `src` onto `canvas`, centered at `center` and turned
/// clockwise by `rotation_deg` (y down), bilinear-sampled.
fn draw_turned(
    canvas: &mut RgbaImage,
    src: &RgbaImage,
    center: (f32, f32),
    rotation_deg: f32,
    opacity: f32,
) {
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    let (hw, hh) = (sw * 0.5, sh * 0.5);
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let ex = hw * cos.abs() + hh * sin.abs();
    let ey = hw * sin.abs() + hh * cos.abs();
    let x0 = (center.0 - ex).floor().max(0.0) as u32;
    let y0 = (center.1 - ey).floor().max(0.0) as u32;
    let x1 = ((center.0 + ex).ceil().max(0.0) as u32).min(canvas.width());
    let y1 = ((center.1 + ey).ceil().max(0.0) as u32).min(canvas.height());
    let opacity = opacity.clamp(0.0, 1.0);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - center.0;
            let dy = y as f32 + 0.5 - center.1;
            let sx = dx * cos + dy * sin + hw - 0.5;
            let sy = -dx * sin + dy * cos + hh - 0.5;
            if sx < -0.5 || sy < -0.5 || sx > sw - 0.5 || sy > sh - 0.5 {
                continue;
            }
            let [r, g, b, a] = sample_bilinear(src, sx, sy);
            let a = a * opacity;
            if a <= 0.0 {
                continue;
            }
            let dst = canvas.get_pixel_mut(x, y);
            let da = dst.0[3] as f32 / 255.0;
            let out_a = a + da * (1.0 - a);
            for (c, s) in dst.0[..3].iter_mut().zip([r, g, b]) {
                let d = *c as f32;
                *c = ((s * a + d * da * (1.0 - a)) / out_a)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            dst.0[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Straight-alpha color (0–255) and alpha (0–1) at a fractional pixel,
/// weighting color by alpha so transparent texels do not darken edges.
fn sample_bilinear(src: &RgbaImage, x: f32, y: f32) -> [f32; 4] {
    let max_x = src.width() as f32 - 1.0;
    let max_y = src.height() as f32 - 1.0;
    let (x, y) = (x.clamp(0.0, max_x), y.clamp(0.0, max_y));
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as u32, y0 as u32);
    let x1 = (x0 + 1).min(src.width() - 1);
    let y1 = (y0 + 1).min(src.height() - 1);
    let mut acc = [0.0f32; 4];
    for (px, py, w) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        if w <= 0.0 {
            continue;
        }
        let p = src.get_pixel(px, py).0;
        let a = p[3] as f32 / 255.0 * w;
        acc[0] += p[0] as f32 * a;
        acc[1] += p[1] as f32 * a;
        acc[2] += p[2] as f32 * a;
        acc[3] += a;
    }
    if acc[3] <= 0.0 {
        return [0.0; 4];
    }
    [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3]]
}

/// A CF_DIBV5 payload: BITMAPV5HEADER, 32-bit BGRA with an alpha mask, sRGB,
/// rows bottom-up (Word rejects a negative height).
pub(crate) fn encode_dibv5(img: &RgbaImage) -> Vec<u8> {
    const HEADER: u32 = 124;
    const BI_BITFIELDS: u32 = 3;
    const LCS_SRGB: u32 = 0x7352_4742;
    const LCS_GM_IMAGES: u32 = 4;
    let (w, h) = img.dimensions();
    let pixel_bytes = w * h * 4;
    let mut out = Vec::with_capacity((HEADER + pixel_bytes) as usize);
    out.extend_from_slice(&HEADER.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&BI_BITFIELDS.to_le_bytes());
    out.extend_from_slice(&pixel_bytes.to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // resolution, palette counts
    for mask in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff, 0xff00_0000] {
        out.extend_from_slice(&mask.to_le_bytes());
    }
    out.extend_from_slice(&LCS_SRGB.to_le_bytes());
    out.extend_from_slice(&[0u8; 48]); // endpoints, gamma
    out.extend_from_slice(&LCS_GM_IMAGES.to_le_bytes());
    out.extend_from_slice(&[0u8; 12]); // profile data, profile size, reserved
    for row in img.rows().rev() {
        for p in row {
            out.extend_from_slice(&[p.0[2], p.0[1], p.0[0], p.0[3]]);
        }
    }
    out
}

/// The linked file behind a copied node, when that node is a picture that
/// goes on the clipboard as a bitmap.
fn copy_picture_path(doc: &SlateDoc, node: &Node) -> Option<PathBuf> {
    let NodeKind::Image(img) = &node.kind else {
        return None;
    };
    let path = &doc.item(img.item)?.path;
    (slate_doc::media_kind(path) == slate_doc::MediaKind::Image).then(|| path.clone())
}

// ---------- app-side commands ----------

impl SlateApp {
    /// Ctrl+C: payload from the board selection → app clipboard + OS text.
    /// Returns the number of copied nodes (0 = nothing to copy).
    pub(crate) fn board_copy(&mut self, ctx: &eframe::egui::Context) -> usize {
        let selected: HashSet<NodeId> = self.board_sel.iter().copied().collect();
        if selected.is_empty() {
            return 0;
        }
        let mut payload = clipboard_payload(&self.doc().scene, &selected);
        if payload.is_empty() {
            return 0;
        }
        // A picture an agent makes copies as the picture it shows, alone.
        for node in &mut payload {
            let NodeKind::Image(img) = &node.kind else {
                continue;
            };
            if img.agent.is_none() {
                continue;
            }
            let item = if img.item.is_none() {
                self.agent_shown_path(node.id)
                    .and_then(|path| self.item_for_path(&path))
            } else {
                Some(img.item)
            };
            if let (Some(item), NodeKind::Image(img)) = (item, &mut node.kind) {
                img.item = item;
                img.agent = None;
            }
        }
        self.offer_os_clipboard(ctx, &payload);
        let n = payload.len();
        self.board_clipboard = payload;
        self.board_paste_count = 0;
        n
    }

    /// One OS clipboard write for a copy: Slate's node format, plus a bitmap
    /// and file list for pictures or plain text for anything else.
    fn offer_os_clipboard(&mut self, ctx: &eframe::egui::Context, payload: &[Node]) {
        let doc = self.doc();
        let picture_paths: Vec<Option<PathBuf>> =
            payload.iter().map(|n| copy_picture_path(doc, n)).collect();
        let is_picture = |node: &Node| {
            payload
                .iter()
                .position(|n| n.id == node.id)
                .is_some_and(|i| picture_paths[i].is_some())
        };
        let text = clipboard_text_fallback(payload, is_picture);
        if !cfg!(windows) {
            if let Some(text) = &text {
                ctx.copy_text(text.clone());
            }
        }
        let Ok(nodes_json) = serde_json::to_string(payload) else {
            return;
        };
        let (pictures, files) = if text.is_none() {
            (
                payload.to_vec(),
                picture_paths.into_iter().flatten().collect(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let mut wanted = HashSet::new();
        let mut stack: Vec<&Node> = pictures.iter().collect();
        while let Some(node) = stack.pop() {
            if let NodeKind::Image(img) = &node.kind {
                wanted.insert(img.item);
                stack.extend(img.paint_layers.iter().flat_map(|l| l.nodes.iter()));
            }
        }
        let mut job_doc = SlateDoc::new("clipboard");
        job_doc.items = doc
            .items
            .iter()
            .filter(|item| wanted.contains(&item.id))
            .cloned()
            .collect();
        self.os_clipboard.submit(ClipboardJob {
            nodes_json,
            text,
            pictures,
            files,
            doc: job_doc,
        });
    }

    /// Ctrl+V from Slate's own clipboard format. `None` when the clipboard
    /// holds something else (an outside image, files, text), which the
    /// caller then pastes the usual way. While a copy is still being
    /// written, the in-app buffer stands in for it.
    pub(crate) fn paste_slate_clipboard(&mut self, at: Option<Pos2>) -> Option<usize> {
        if self.os_clipboard.writing() {
            return Some(self.board_paste(None, at));
        }
        let json = self.os_clipboard.slate_nodes()?;
        match self.board_paste(Some(&json), at) {
            0 => None,
            n => Some(n),
        }
    }

    /// Ctrl+X: copy + one journaled Remove group.
    pub(crate) fn board_cut(&mut self, ctx: &eframe::egui::Context) -> usize {
        let n = self.board_copy(ctx);
        if n > 0 {
            let ids: Vec<NodeId> = self.board_sel.iter().copied().collect();
            self.delete_board_nodes(&ids);
        }
        n
    }

    /// The freshest payload available: OS clipboard JSON when it parses as
    /// `Vec<Node>` (cross-instance paste), else the app-internal buffer.
    fn paste_payload(&self, os_text: Option<&str>) -> Vec<Node> {
        if let Some(text) = os_text {
            if let Ok(nodes) = serde_json::from_str::<Vec<Node>>(text) {
                if !nodes.is_empty() {
                    return nodes;
                }
            }
        }
        self.board_clipboard.clone()
    }

    /// Ctrl+V / Ctrl+Shift+V. `at` = world target for the payload center
    /// (`None` = paste in place at source coordinates). One journaled Add
    /// group; the copies become the selection. Returns pasted count.
    pub(crate) fn board_paste(&mut self, os_text: Option<&str>, at: Option<Pos2>) -> usize {
        // A pasted URL is a page, not text (D01). Checked before the scene
        // payload so copying nodes and copying a link never compete.
        if let Some(text) = os_text {
            let target = at.unwrap_or_else(|| self.tab().cam.offset.to_pos2());
            if self.paste_web_url(text, target) {
                return 1;
            }
        }
        let mut payload = self.paste_payload(os_text);
        if payload.is_empty() {
            return 0;
        }
        self.fork_agent_train_payload(&mut payload);
        let (dx, dy) = match at {
            Some(p) => {
                let Some((min_x, min_y, max_x, max_y)) = payload_bounds(&payload) else {
                    return 0;
                };
                let step = self.board_paste_count as f32 * PASTE_STEP;
                (
                    p.x - (min_x + max_x) * 0.5 + step,
                    p.y - (min_y + max_y) * 0.5 + step,
                )
            }
            None => (0.0, 0.0),
        };
        let nodes = {
            let scene = &mut self.doc_mut().scene;
            // Scene id/key allocation stays inside the scene: build a probe
            // node per fresh id so ids can never collide with existing ones.
            let mut fresh_ids: Vec<NodeId> = Vec::with_capacity(payload.len());
            for _ in 0..payload.len() {
                let probe = scene.build_node(
                    slate_doc::scene::WorldRect::new(0.0, 0.0, 1.0, 1.0),
                    NodeKind::Text(slate_doc::scene::TextNode {
                        text: String::new(),
                        family: Default::default(),
                        size: 1.0,
                        color: slate_doc::scene::Rgba::opaque(0, 0, 0),
                        align: Default::default(),
                        fill: None,
                        stroke: Default::default(),
                        agent: None,
                    }),
                );
                fresh_ids.push(probe.id);
            }
            let mut ids = fresh_ids.into_iter();
            remap_for_paste(
                &payload,
                move || ids.next().expect("one fresh id per payload node"),
                || scene.alloc_group_key(),
                dx,
                dy,
            )
        };
        // A copied context card leaves its provenance wire behind, so the
        // paste is an ordinary board node.
        let nodes: Vec<_> = nodes
            .into_iter()
            .filter(|n| {
                !matches!(&n.kind, NodeKind::Connector(c)
                    if c.binding.is_none() && c.display == WireDisplay::Faint)
            })
            .collect();
        let count = nodes.len();
        let ids = self.add_nodes(nodes);
        if !ids.is_empty() {
            self.board_sel = ids.into_iter().collect();
            if at.is_some() {
                self.board_paste_count += 1;
            }
        }
        count
    }

    /// Paste an image or files from the OS clipboard. Returns true when that
    /// clipboard was the thing to paste, so a URL or node payload does not
    /// also land. Text-only clipboards return false.
    pub(crate) fn paste_os_clipboard(&mut self, at: Pos2, os_text: Option<&str>) -> bool {
        if self.at_home || self.doc().view.active_view != slate_doc::ViewKind::Board {
            return false;
        }
        #[cfg(windows)]
        if let Some(media) = read_clipboard_media() {
            let at = self.stepped_paste_at(at);
            let placed = self.place_os_media(media, at);
            if placed {
                self.board_paste_count += 1;
            }
            return true;
        }
        if let Some(text) = os_text {
            if text_looks_like_paths(text) {
                if self.refuse_read_only_edit() {
                    return true;
                }
                let at = self.stepped_paste_at(at);
                match existing_paths_from_text(text) {
                    Some(paths) => {
                        if self.ingest_dropped_paths(paths, at, false, None) {
                            self.board_paste_count += 1;
                        } else {
                            self.toast("Couldn't place what was on the clipboard.");
                        }
                    }
                    None => self.toast("Clipboard file is missing."),
                }
                return true;
            }
        }
        false
    }

    /// Same intake as an OS file drop: web pages become portals, folders queue
    /// the lens chooser, workbooks queue as tabs, everything else is an item.
    pub(crate) fn ingest_dropped_paths(
        &mut self,
        paths: Vec<PathBuf>,
        at: Pos2,
        alt: bool,
        screen_at: Option<Pos2>,
    ) -> bool {
        if paths.is_empty() {
            return false;
        }
        let on_board = self.doc().view.active_view == slate_doc::ViewKind::Board;
        let paths = if on_board && !alt {
            let after_web = self.divert_web_drops(&paths, at);
            let after_folders = self.queue_folder_drop_choosers(&after_web, at);
            self.queue_workbook_drops(&after_folders, at, true)
        } else {
            paths
        };
        if paths.is_empty() {
            return true;
        }
        if on_board && self.maybe_intercept_image_drop_on_model(&paths, at) {
            return true;
        }
        let items = self.add_paths(&paths);
        if on_board && !items.is_empty() {
            let defer_to_capsule = if alt || items.len() != 1 {
                false
            } else {
                let item = items[0];
                match self.doc().item(item) {
                    None => false,
                    Some(it) if slate_doc::media_kind(&it.path) != slate_doc::MediaKind::Image => {
                        false
                    }
                    Some(_) => {
                        if let (Some(target), Some(screen)) =
                            (self.image_drop_target_at(at), screen_at)
                        {
                            self.offer_image_file_drop(target, item, screen);
                            true
                        } else {
                            false
                        }
                    }
                }
            };
            if !defer_to_capsule {
                self.place_items_on_board(&items, at);
            }
        }
        !items.is_empty()
    }

    fn stepped_paste_at(&self, at: Pos2) -> Pos2 {
        let step = self.board_paste_count as f32 * PASTE_STEP;
        Pos2::new(at.x + step, at.y + step)
    }

    #[cfg(windows)]
    fn place_os_media(&mut self, media: OsPaste, at: Pos2) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let placed = match media {
            OsPaste::Files(paths) => self.ingest_dropped_paths(paths, at, false, None),
            OsPaste::Png(bytes) => match self.write_pasted_png(&bytes) {
                Some(path) => self.ingest_dropped_paths(vec![path], at, false, None),
                None => {
                    self.toast("Couldn't save the pasted image.");
                    return false;
                }
            },
        };
        if !placed {
            self.toast("Couldn't place what was on the clipboard.");
        }
        placed
    }

    /// A pasted bitmap has no source file. Write one beside a saved workbook
    /// (or under app data until the workbook has a folder) and link to it.
    #[cfg(windows)]
    fn write_pasted_png(&self, png: &[u8]) -> Option<PathBuf> {
        if png.is_empty() || !png_within_limits(png) {
            return None;
        }
        let dir = atlas_core::workbook_assets::paste_dir(
            self.tab().path.as_deref(),
            &atlas_core::index::data_dir(),
        );
        std::fs::create_dir_all(&dir).ok()?;
        let path = unique_paste_path(&dir);
        std::fs::write(&path, png).ok()?;
        Some(path)
    }

    /// Rising edge of Ctrl+V (Shift selects paste-in-place). egui does not
    /// deliver this as a key when the clipboard has no text.
    pub(crate) fn take_paste_chord_edge(&mut self, focused: bool) -> Option<bool> {
        #[cfg(windows)]
        {
            let held = paste_chord_held();
            let down = held.is_some();
            if !focused {
                self.paste_chord_down = down;
                return None;
            }
            let edge = down && !self.paste_chord_down;
            self.paste_chord_down = down;
            edge.then(|| held.unwrap_or(false))
        }
        #[cfg(not(windows))]
        {
            let _ = focused;
            None
        }
    }
}

#[cfg(windows)]
enum OsPaste {
    Files(Vec<PathBuf>),
    Png(Vec<u8>),
}

#[cfg(windows)]
fn unique_paste_path(dir: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut path = dir.join(format!("paste-{stamp}.png"));
    let mut n = 1u32;
    while path.exists() && n < 100 {
        path = dir.join(format!("paste-{stamp}-{n}.png"));
        n += 1;
    }
    path
}

/// One clipboard line that names a file: a Windows path, or a `file://` URL.
pub(crate) fn clipboard_text_path(line: &str) -> Option<PathBuf> {
    let line = line.trim().trim_matches('"').trim();
    if line.is_empty() || line.starts_with('[') || line.starts_with('{') {
        return None;
    }
    if line.contains("://") && !line.to_ascii_lowercase().starts_with("file://") {
        return None;
    }
    // `file:///tmp/x` keeps its root; `file:///C:/x` drops the slash before the drive.
    let rest = match line.strip_prefix("file://") {
        Some(r) => match r.as_bytes() {
            [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => &r[1..],
            _ => r,
        },
        None => line,
    };
    let rest = percent_decode(rest);
    if rest.is_empty() {
        return None;
    }
    let path = PathBuf::from(rest);
    path.is_absolute().then_some(path)
}

fn text_looks_like_paths(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    !lines.is_empty()
        && lines.len() <= 64
        && lines.iter().all(|line| clipboard_text_path(line).is_some())
}

fn existing_paths_from_text(text: &str) -> Option<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let path = clipboard_text_path(line)?;
        if !path.is_file() && !path.is_dir() {
            return None;
        }
        paths.push(path);
    }
    (!paths.is_empty()).then_some(paths)
}

fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(windows)]
fn png_within_limits(bytes: &[u8]) -> bool {
    if bytes.len() < 24 || bytes.len() > MAX_PASTE_PNG_BYTES || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
    {
        return false;
    }
    let Ok(reader) = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()
    else {
        return false;
    };
    let Ok((w, h)) = reader.into_dimensions() else {
        return false;
    };
    w > 0 && h > 0 && u64::from(w) * u64::from(h) <= MAX_PASTE_PIXELS
}

/// Turn a clipboard DIB, DIBV5, or BMP file into a PNG. 24- and 32-bit
/// uncompressed bitmaps only — that is what screenshots and "Copy image"
/// actually put on the clipboard.
#[cfg(any(windows, test))]
pub(crate) fn decode_clipboard_bitmap(data: &[u8]) -> Option<Vec<u8>> {
    const BI_RGB: u32 = 0;
    const BI_BITFIELDS: u32 = 3;
    let (dib, file_pixel_off) = if data.starts_with(b"BM") && data.len() >= 14 {
        let off = u32::from_le_bytes(data[10..14].try_into().ok()?) as usize;
        if off < 14 || off > data.len() {
            return None;
        }
        (&data[14..], Some(off - 14))
    } else {
        (data, None)
    };
    if dib.len() < 40 {
        return None;
    }
    let bi_size = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    if bi_size < 40 || bi_size > dib.len() {
        return None;
    }
    let width = i32::from_le_bytes(dib[4..8].try_into().ok()?);
    let height_raw = i32::from_le_bytes(dib[8..12].try_into().ok()?);
    if width <= 0 || height_raw == 0 || height_raw == i32::MIN {
        return None;
    }
    let width = width as u32;
    let top_down = height_raw < 0;
    let height = height_raw.unsigned_abs();
    let count = u64::from(width).checked_mul(u64::from(height))?;
    if count == 0 || count > MAX_PASTE_PIXELS {
        return None;
    }
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    if bit_count != 24 && bit_count != 32 {
        return None;
    }
    if compression != BI_RGB && compression != BI_BITFIELDS {
        return None;
    }
    let masks = if compression == BI_BITFIELDS {
        if dib.len() < 52 {
            return None;
        }
        let r = u32::from_le_bytes(dib[40..44].try_into().ok()?);
        let g = u32::from_le_bytes(dib[44..48].try_into().ok()?);
        let b = u32::from_le_bytes(dib[48..52].try_into().ok()?);
        let a = if bi_size >= 56 && dib.len() >= 56 {
            u32::from_le_bytes(dib[52..56].try_into().ok()?)
        } else {
            0
        };
        if r == 0 || g == 0 || b == 0 {
            return None;
        }
        Some([r, g, b, a])
    } else {
        None
    };
    let bpp = (bit_count / 8) as usize;
    let stride = (width as usize * bpp + 3) & !3;
    let pixels_off = if let Some(off) = file_pixel_off {
        off
    } else if compression == BI_BITFIELDS && bi_size == 40 {
        let rows = height as usize;
        if dib_fits(dib, 56, rows, stride) && !dib_fits(dib, 52, rows, stride) {
            56
        } else {
            52
        }
    } else {
        bi_size
    };
    if !dib_fits(dib, pixels_off, height as usize, stride) {
        return None;
    }
    let pixels = &dib[pixels_off..];
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    let mut any_alpha = false;
    for y in 0..height as usize {
        let src_y = if top_down { y } else { height as usize - 1 - y };
        let row = &pixels[src_y * stride..src_y * stride + width as usize * bpp];
        for x in 0..width as usize {
            let px = &row[x * bpp..x * bpp + bpp];
            let (r, g, b, a) = if let Some(masks) = masks {
                let mut dword = [0u8; 4];
                dword[..bpp].copy_from_slice(px);
                let v = u32::from_le_bytes(dword);
                let a = if masks[3] == 0 {
                    255
                } else {
                    mask_channel(v, masks[3])
                };
                (
                    mask_channel(v, masks[0]),
                    mask_channel(v, masks[1]),
                    mask_channel(v, masks[2]),
                    a,
                )
            } else if bpp == 4 {
                (px[2], px[1], px[0], px[3])
            } else {
                (px[2], px[1], px[0], 255)
            };
            any_alpha |= a != 0;
            let i = (y * width as usize + x) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = a;
        }
    }
    // 32-bit screenshots store unused alpha as zero. A PNG with real
    // transparency arrives as PNG bytes and never takes this path.
    if bpp == 4 && masks.is_none() && !any_alpha {
        for px in rgba.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
    }
    encode_png(width, height, &rgba)
}

#[cfg(any(windows, test))]
fn dib_fits(data: &[u8], off: usize, rows: usize, stride: usize) -> bool {
    rows.checked_mul(stride)
        .and_then(|n| off.checked_add(n))
        .is_some_and(|end| end <= data.len())
}

#[cfg(any(windows, test))]
fn mask_channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let bits = mask.count_ones();
    let value = (pixel & mask) >> shift;
    if bits >= 8 {
        (value >> (bits - 8)) as u8
    } else {
        let max = (1u32 << bits) - 1;
        ((value * 255) / max) as u8
    }
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    use image::ImageEncoder;
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(bytes)
}

#[cfg(windows)]
fn read_clipboard_media() -> Option<OsPaste> {
    let raw = {
        let _clip = clipboard_win::Clipboard::new_attempts(5).ok()?;
        read_clipboard_raw()
    };
    match raw? {
        ClipRaw::Files(paths) => Some(OsPaste::Files(paths)),
        ClipRaw::Png(bytes) => Some(OsPaste::Png(bytes)),
        ClipRaw::Dib(bytes) => decode_clipboard_bitmap(&bytes).map(OsPaste::Png),
    }
}

#[cfg(windows)]
enum ClipRaw {
    Files(Vec<PathBuf>),
    Png(Vec<u8>),
    Dib(Vec<u8>),
}

#[cfg(windows)]
fn read_clipboard_raw() -> Option<ClipRaw> {
    use clipboard_win::formats::{Bitmap, FileList, RawData, CF_DIB, CF_DIBV5};
    use clipboard_win::{Format, Getter};
    let mut paths: Vec<PathBuf> = Vec::new();
    if FileList.read_clipboard(&mut paths).is_ok() {
        if paths.len() > atlas_core::shell_drag::MAX_DRAG_PATHS {
            paths.truncate(atlas_core::shell_drag::MAX_DRAG_PATHS);
        }
        let paths: Vec<_> = paths
            .into_iter()
            .filter(|p| p.is_file() || p.is_dir())
            .collect();
        if !paths.is_empty() {
            return Some(ClipRaw::Files(paths));
        }
    }
    for name in ["PNG", "image/png"] {
        let Some(fmt) = clipboard_win::register_format(name) else {
            continue;
        };
        let format = RawData(fmt.get());
        if !format.is_format_avail() {
            continue;
        }
        let mut bytes = Vec::new();
        if format.read_clipboard(&mut bytes).is_ok() && png_within_limits(&bytes) {
            return Some(ClipRaw::Png(bytes));
        }
    }
    for format in [CF_DIBV5, CF_DIB] {
        let format = RawData(format);
        if !format.is_format_avail() {
            continue;
        }
        let mut bytes = Vec::new();
        if format.read_clipboard(&mut bytes).is_ok() && !bytes.is_empty() {
            return Some(ClipRaw::Dib(bytes));
        }
    }
    if Bitmap.is_format_avail() {
        let mut bytes = Vec::new();
        if Bitmap.read_clipboard(&mut bytes).is_ok() && !bytes.is_empty() {
            return Some(ClipRaw::Dib(bytes));
        }
    }
    None
}

/// Puts one copy on the clipboard in a single open, so every format belongs
/// to the same copy. Windows synthesizes CF_DIB and CF_BITMAP from CF_DIBV5.
#[cfg(all(windows, not(test)))]
fn write_os_clipboard(write: &ClipboardWrite) {
    use clipboard_win::{formats::CF_DIBV5, options::NoClear, raw};
    let Ok(_clip) = clipboard_win::Clipboard::new_attempts(10) else {
        return;
    };
    if raw::empty().is_err() {
        return;
    }
    if let Some(fmt) = clipboard_win::register_format(SLATE_CLIPBOARD_FORMAT) {
        let _ = raw::set_without_clear(fmt.get(), write.nodes_json.as_bytes());
    }
    if let Some(dib) = &write.dibv5 {
        let _ = raw::set_without_clear(CF_DIBV5, dib);
    }
    if let (Some(png), Some(fmt)) = (&write.png, clipboard_win::register_format("PNG")) {
        let _ = raw::set_without_clear(fmt.get(), png);
    }
    let files: Vec<&str> = write.files.iter().filter_map(|p| p.to_str()).collect();
    if !files.is_empty() {
        let _ = raw::set_file_list_with(&files, NoClear);
    }
    if let Some(text) = &write.text {
        let _ = raw::set_string_with(text, NoClear);
    }
}

/// Elsewhere the copy's text went out through egui; there is no bitmap slot.
#[cfg(all(not(windows), not(test)))]
fn write_os_clipboard(_write: &ClipboardWrite) {}

#[cfg(all(windows, not(test)))]
fn read_slate_clipboard() -> Option<String> {
    use clipboard_win::formats::RawData;
    use clipboard_win::{Format, Getter};
    let format = RawData(clipboard_win::register_format(SLATE_CLIPBOARD_FORMAT)?.get());
    let _clip = clipboard_win::Clipboard::new_attempts(5).ok()?;
    if !format.is_format_avail() {
        return None;
    }
    let mut bytes = Vec::new();
    format.read_clipboard(&mut bytes).ok()?;
    String::from_utf8(bytes).ok()
}

/// `Some(shift)` while Ctrl+V is held and Alt is not. VK_V is the key Windows
/// uses for the paste chord on the layouts this app ships for.
#[cfg(windows)]
fn paste_chord_held() -> Option<bool> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT, VK_V,
    };
    fn down(vk: u16) -> bool {
        (unsafe { GetAsyncKeyState(i32::from(vk)) }) < 0
    }
    if down(VK_CONTROL.0) && down(VK_V.0) && !down(VK_MENU.0) {
        Some(down(VK_SHIFT.0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{ConnectorNode, Side, Stroke, TextNode, WireDisplay, WorldRect};

    fn text_node(scene: &mut Scene, x: f32, y: f32) -> Node {
        scene.build_node(
            WorldRect::new(x, y, 100.0, 50.0),
            NodeKind::Text(TextNode {
                text: "t".into(),
                family: Default::default(),
                size: 12.0,
                color: slate_doc::scene::Rgba::opaque(0, 0, 0),
                align: Default::default(),
                fill: None,
                stroke: Default::default(),
                agent: None,
            }),
        )
    }

    fn connector(scene: &mut Scene, a: NodeId, b: NodeId) -> Node {
        scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Connector(ConnectorNode {
                crosstalk: None,
                routing: None,
                binding: None,
                a: ConnectorEnd::Anchored {
                    node: a,
                    side: Side::Right,
                    t: 0.5,
                },
                b: ConnectorEnd::Anchored {
                    node: b,
                    side: Side::Left,
                    t: 0.5,
                },
                stroke: Stroke::none(),
                arrow_a: false,
                arrow_b: false,
                label: None,
                display: WireDisplay::Default,
            }),
        )
    }

    /// Copy 2 nodes + their connector → paste → fresh ids, same relative
    /// geometry; a connector end anchored outside the set became Free.
    #[test]
    fn payload_includes_bridging_connector_and_degrades_outside_anchor() {
        let mut scene = Scene::default();
        let a = text_node(&mut scene, 0.0, 0.0);
        let b = text_node(&mut scene, 300.0, 0.0);
        let c = text_node(&mut scene, 600.0, 0.0);
        let (ida, idb, idc) = (a.id, b.id, c.id);
        scene.nodes.extend([a, b, c]);
        let wire_ab = connector(&mut scene, ida, idb);
        let wire_bc = connector(&mut scene, idb, idc);
        let (wab, wbc) = (wire_ab.id, wire_bc.id);
        scene.nodes.extend([wire_ab, wire_bc]);

        // Select a, b, and the b→c wire (c itself stays out).
        let selected: HashSet<NodeId> = [ida, idb, wbc].into_iter().collect();
        let payload = clipboard_payload(&scene, &selected);

        // a, b, the auto-included a→b wire, and the selected b→c wire.
        let ids: HashSet<NodeId> = payload.iter().map(|n| n.id).collect();
        assert_eq!(ids, [ida, idb, wab, wbc].into_iter().collect());

        // The b→c wire's far end degraded to Free at c's left-mid anchor.
        let bc = payload.iter().find(|n| n.id == wbc).unwrap();
        let NodeKind::Connector(cn) = &bc.kind else {
            panic!("connector expected")
        };
        assert!(matches!(cn.a, ConnectorEnd::Anchored { node, .. } if node == idb));
        match cn.b {
            ConnectorEnd::Free { point } => {
                assert_eq!(point, [600.0, 25.0]); // left side midpoint of c
            }
            _ => panic!("outside anchor must degrade to Free"),
        }

        // The a→b wire stays fully anchored.
        let ab = payload.iter().find(|n| n.id == wab).unwrap();
        let NodeKind::Connector(cn) = &ab.kind else {
            panic!("connector expected")
        };
        assert!(matches!(cn.a, ConnectorEnd::Anchored { .. }));
        assert!(matches!(cn.b, ConnectorEnd::Anchored { .. }));
    }

    #[test]
    fn remap_gives_fresh_ids_and_preserves_relative_geometry() {
        let mut scene = Scene::default();
        let mut a = text_node(&mut scene, 10.0, 20.0);
        let b = text_node(&mut scene, 310.0, 20.0);
        let g = scene.alloc_group_key();
        a.group = Some(g);
        let (ida, idb) = (a.id, b.id);
        scene.nodes.extend([a, b]);
        let wire = connector(&mut scene, ida, idb);
        let wid = wire.id;
        scene.nodes.push(wire);

        let selected: HashSet<NodeId> = [ida, idb, wid].into_iter().collect();
        let payload = clipboard_payload(&scene, &selected);

        let mut next = 1000u64;
        let mut next_group = 5000u64;
        let out = remap_for_paste(
            &payload,
            || {
                next += 1;
                NodeId(next)
            },
            || {
                next_group += 1;
                GroupKey(next_group)
            },
            24.0,
            24.0,
        );

        // Every id is fresh and distinct.
        let old: HashSet<NodeId> = payload.iter().map(|n| n.id).collect();
        let new: HashSet<NodeId> = out.iter().map(|n| n.id).collect();
        assert_eq!(new.len(), out.len());
        assert!(old.is_disjoint(&new));

        // Relative geometry survives the offset.
        let ra = out[0].rect;
        let rb = out[1].rect;
        assert_eq!(rb.x - ra.x, 300.0);
        assert_eq!(ra.x, 34.0);
        assert_eq!(ra.y, 44.0);

        // Group keys are freshly allocated.
        assert_eq!(out[0].group, Some(GroupKey(5001)));

        // The connector follows the remapped ids.
        let NodeKind::Connector(cn) = &out[2].kind else {
            panic!("connector expected")
        };
        assert!(matches!(cn.a, ConnectorEnd::Anchored { node, .. } if node == out[0].id));
        assert!(matches!(cn.b, ConnectorEnd::Anchored { node, .. } if node == out[1].id));
    }

    #[test]
    fn remap_offsets_free_points_and_shares_group_keys() {
        let mut scene = Scene::default();
        let mut a = text_node(&mut scene, 0.0, 0.0);
        let mut b = text_node(&mut scene, 200.0, 0.0);
        let g = scene.alloc_group_key();
        a.group = Some(g);
        b.group = Some(g);
        let free_wire = scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Connector(ConnectorNode {
                crosstalk: None,
                routing: None,
                binding: None,
                a: ConnectorEnd::Free { point: [5.0, 6.0] },
                b: ConnectorEnd::Free { point: [7.0, 8.0] },
                stroke: Stroke::none(),
                arrow_a: false,
                arrow_b: false,
                label: None,
                display: WireDisplay::Default,
            }),
        );
        let payload = vec![a, b, free_wire];

        let mut next = 0u64;
        let mut groups = 0u64;
        let out = remap_for_paste(
            &payload,
            || {
                next += 1;
                NodeId(next)
            },
            || {
                groups += 1;
                GroupKey(100 + groups)
            },
            10.0,
            -10.0,
        );

        // One shared source key → one shared fresh key.
        assert_eq!(out[0].group, out[1].group);
        assert_eq!(out[0].group, Some(GroupKey(101)));
        assert_eq!(groups, 1);

        let NodeKind::Connector(cn) = &out[2].kind else {
            panic!("connector expected")
        };
        assert_eq!(
            (cn.a, cn.b),
            (
                ConnectorEnd::Free {
                    point: [15.0, -4.0]
                },
                ConnectorEnd::Free {
                    point: [17.0, -2.0]
                }
            )
        );
    }

    fn dib_header(width: i32, height: i32, bit_count: u16) -> Vec<u8> {
        let mut h = vec![0u8; 40];
        h[0..4].copy_from_slice(&40u32.to_le_bytes());
        h[4..8].copy_from_slice(&width.to_le_bytes());
        h[8..12].copy_from_slice(&height.to_le_bytes());
        h[12..14].copy_from_slice(&1u16.to_le_bytes());
        h[14..16].copy_from_slice(&bit_count.to_le_bytes());
        h
    }

    fn png_pixel(png: &[u8]) -> [u8; 4] {
        image::load_from_memory(png)
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)
            .0
    }

    #[test]
    fn dib_32_zero_alpha_pastes_as_opaque_color() {
        let mut dib = dib_header(1, 1, 32);
        dib.extend_from_slice(&[0, 0, 255, 0]); // B, G, R, unused alpha → red
        let png = decode_clipboard_bitmap(&dib).unwrap();
        assert_eq!(png_pixel(&png), [255, 0, 0, 255]);
    }

    #[test]
    fn dib_24_pads_rows_and_keeps_color() {
        let mut dib = dib_header(1, 1, 24);
        dib.extend_from_slice(&[10, 20, 30, 0]); // BGR + pad
        let png = decode_clipboard_bitmap(&dib).unwrap();
        assert_eq!(png_pixel(&png), [30, 20, 10, 255]);
    }

    #[test]
    fn top_down_dib_keeps_the_first_row_on_top() {
        let mut dib = dib_header(1, -2, 32);
        dib.extend_from_slice(&[0, 0, 255, 0]); // top: red
        dib.extend_from_slice(&[0, 255, 0, 0]); // bottom: green
        let png = decode_clipboard_bitmap(&dib).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(0, 1).0, [0, 255, 0, 255]);
    }

    #[test]
    fn bmp_file_header_points_at_the_pixels() {
        let mut dib = dib_header(1, 1, 32);
        dib.extend_from_slice(&[255, 0, 0, 0]); // blue channel in BGRA, zero alpha
        let mut bmp = vec![0u8; 14];
        bmp[0..2].copy_from_slice(b"BM");
        let off = (14 + dib.len() - 4) as u32;
        bmp[10..14].copy_from_slice(&off.to_le_bytes());
        bmp.extend_from_slice(&dib);
        let png = decode_clipboard_bitmap(&bmp).unwrap();
        assert_eq!(png_pixel(&png), [0, 0, 255, 255]);
    }

    #[test]
    fn dibv5_round_trips_through_the_paste_decoder() {
        let img = image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([x as u8 * 80, y as u8 * 120, 200, 255 - x as u8 * 100])
        });
        let dib = encode_dibv5(&img);
        assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 124);
        assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 3);
        // Bottom-up rows: Word refuses a negative height.
        assert_eq!(i32::from_le_bytes(dib[8..12].try_into().unwrap()), 2);
        assert_eq!(dib.len(), 124 + 3 * 2 * 4);
        let png = decode_clipboard_bitmap(&dib).unwrap();
        let back = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(back, img);
    }

    #[test]
    fn the_text_fallback_is_never_the_node_json() {
        let mut scene = Scene::default();
        let words = text_node(&mut scene, 0.0, 0.0);
        let rect = scene.build_node(
            WorldRect::new(0.0, 0.0, 10.0, 10.0),
            NodeKind::Shape(slate_doc::scene::ShapeNode {
                shape: slate_doc::scene::ShapeKind::Rect,
                fill: None,
                stroke: Stroke::none(),
                corner: Default::default(),
                sides: 6,
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: None,
            }),
        );
        let no_pictures = |_: &Node| false;
        assert_eq!(
            clipboard_text_fallback(&[words.clone(), rect.clone()], no_pictures).as_deref(),
            Some("t")
        );
        let shapes = clipboard_text_fallback(&[rect.clone(), rect.clone()], no_pictures).unwrap();
        assert_eq!(shapes, "Slate selection: 2 shapes");
        let json = serde_json::to_string(std::slice::from_ref(&rect)).unwrap();
        assert_ne!(shapes, json);
        assert!(!shapes.trim_start().starts_with('['));
        let all_pictures = |_: &Node| true;
        assert_eq!(clipboard_text_fallback(&[rect], all_pictures), None);
    }

    #[test]
    fn clipboard_text_path_accepts_a_file_url_and_rejects_a_page() {
        let (input, expected) = if cfg!(windows) {
            ("file:///C:/shots/a%20b.png", "C:/shots/a b.png")
        } else {
            ("file:///tmp/a%20b.png", "/tmp/a b.png")
        };
        assert_eq!(
            clipboard_text_path(input).as_deref(),
            Some(std::path::Path::new(expected))
        );
        assert!(clipboard_text_path("https://example.com/a.png").is_none());
        assert!(clipboard_text_path("[{\"id\":1}]").is_none());
    }
}
