//! Board clipboard: copy / cut / paste of scene nodes, and paste of an image
//! or files copied outside Slate.
//!
//! Node payload is plain `Vec<Node>` JSON (the same serde model the `.slate`
//! file uses), kept app-internally *and* mirrored to the OS clipboard as text
//! so selections round-trip between tabs and Slate instances. A copied image
//! (screenshot, "Copy image") or a file list is not that JSON: those land as
//! board items, the same intake as a drop. All mutations go through the
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
use slate_doc::scene::{ConnectorEnd, GroupKey, Node, NodeKind, Scene, WireDisplay};
use slate_doc::NodeId;
use slate_doc::{connector_anchor_on, WireHost};
use std::collections::{HashMap, HashSet};
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;

/// Clipboard bitmaps larger than this are refused. A paste is one user
/// action, not a frame, but it still must not allocate a runaway buffer.
const MAX_PASTE_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_PASTE_PNG_BYTES: usize = 80 * 1024 * 1024;

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
        if let Ok(json) = serde_json::to_string(&payload) {
            ctx.copy_text(json);
        }
        let n = payload.len();
        self.board_clipboard = payload;
        self.board_paste_count = 0;
        n
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
        let payload = self.paste_payload(os_text);
        if payload.is_empty() {
            return 0;
        }
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
                        if self.ingest_dropped_paths(paths, at, false) {
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
        let items = self.add_paths(&paths);
        if on_board && !items.is_empty() {
            self.place_items_on_board(&items, at);
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
            OsPaste::Files(paths) => self.ingest_dropped_paths(paths, at, false),
            OsPaste::Png(bytes) => match self.write_pasted_png(&bytes) {
                Some(path) => self.ingest_dropped_paths(vec![path], at, false),
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
        let dir = self
            .tab()
            .path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.join("assets")))
            .unwrap_or_else(|| atlas_core::index::data_dir().join("pasted"));
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
            return edge.then(|| held.unwrap_or(false));
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
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }
    encode_png(width, height, &rgba)
}

fn dib_fits(data: &[u8], off: usize, rows: usize, stride: usize) -> bool {
    rows.checked_mul(stride)
        .and_then(|n| off.checked_add(n))
        .is_some_and(|end| end <= data.len())
}

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
                agent: None,
            }),
        )
    }

    fn connector(scene: &mut Scene, a: NodeId, b: NodeId) -> Node {
        scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Connector(ConnectorNode {
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
