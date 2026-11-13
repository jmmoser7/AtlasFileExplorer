//! Slate clipboard: scene-node JSON for in-app copy/paste, plus an OS bitmap,
//! linked files, or plain text for other apps. Outside images and file lists
//! use the same intake as drops. Cut and paste are journaled; pasted bitmaps
//! are linked files, never bytes stored in the workbook (Constitution VI/IX).
//!
//! Connector rules (`docs/keymap/specs/constraints.md` §3):
//! - connectors whose *both* anchored ends are inside the selection ride
//!   along automatically, even when not selected themselves;
//! - a copied connector's anchored end that points *outside* the copied set
//!   degrades to `Free` at its current world point (resolved at copy time,
//!   when the source scene is still available).
use super::SlateApp;

#[path = "clipboard_render.rs"]
mod clipboard_render;
#[path = "clipboard_offer.rs"]
mod offer;
use clipboard_render::{copy_picture_path, encode_dibv5};
use eframe::egui::Pos2;
use slate_doc::scene::{ConnectorEnd, GroupKey, Node, NodeKind, Scene, WireDisplay};
use slate_doc::{connector_anchor_on, WireHost};
use slate_doc::{NodeId, SlateDoc};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Slate's own clipboard format: the node payload JSON.
#[cfg_attr(any(test, not(windows)), allow(dead_code))]
const SLATE_CLIPBOARD_FORMAT: &str = "Slate.BoardNodes";

/// Step applied to each successive Ctrl+V paste of the same payload.
const PASTE_STEP: f32 = 24.0;

// ---------- pure payload / remap logic (unit-tested) ----------

/// Selected nodes in z-order, plus connectors whose anchored ends are both selected.
/// Anchored ends leaving the payload become `Free` at their current world point.
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
            NodeKind::Image(image) => image.text_override.as_deref(),
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
    workbook: Option<PathBuf>,
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
    let bitmap =
        clipboard_render::render_pictures(&job.doc, job.workbook.as_deref(), &job.pictures);
    ClipboardWrite {
        png: bitmap.as_ref().and_then(|b| {
            atlas_core::clipboard_image::encode_png(b.width(), b.height(), b.as_raw())
        }),
        dibv5: bitmap.as_ref().map(encode_dibv5),
        files: job.files.into_iter().filter(|p| p.is_file()).collect(),
        nodes_json: job.nodes_json,
        text: job.text,
    }
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
            let NodeKind::Image(_) = &node.kind else {
                continue;
            };
            if let Some(display) = self.image_for_selected_result(node.id) {
                if let NodeKind::Image(img) = &mut node.kind {
                    img.paint_layers = display.paint_layers;
                    img.result_layers.clear();
                }
            }
            let item = match &node.kind {
                NodeKind::Image(img) if img.agent.is_some() && img.item.is_none() => self
                    .agent_shown_path(node.id)
                    .and_then(|path| self.item_for_path(&path)),
                NodeKind::Image(img) if img.agent.is_some() => Some(img.item),
                _ => None,
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
        let mut nodes = {
            let scene = &mut self.doc_mut().scene;
            // Scene id/key allocation stays inside the scene: build a probe
            // node per fresh id so ids can never collide with existing ones.
            let mut stack: Vec<&Node> = payload.iter().collect();
            let mut count = 0;
            while let Some(node) = stack.pop() {
                count += 1;
                if let NodeKind::Image(image) = &node.kind {
                    stack.extend(
                        image
                            .paint_layers
                            .iter()
                            .chain(image.result_layers.iter().flat_map(|result| &result.layers))
                            .flat_map(|layer| layer.nodes.iter()),
                    );
                }
            }
            let mut fresh_ids: Vec<NodeId> = Vec::with_capacity(count);
            for _ in 0..count {
                let probe = scene.build_node(
                    slate_doc::scene::WorldRect::new(0.0, 0.0, 1.0, 1.0),
                    NodeKind::Text(slate_doc::scene::TextNode {
                        answers: Default::default(),
                        text: String::new(),
                        style: Default::default(),
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
                move || {
                    ids.next()
                        .expect("one fresh id per node, including paint children")
                },
                || scene.alloc_group_key(),
                dx,
                dy,
            )
        };
        self.keep_external_chat_hosts(&mut nodes, &payload);
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
            OsPaste::Png(bytes) => self.place_pasted_png(&bytes, at),
        };
        if !placed {
            self.toast("Couldn't place what was on the clipboard.");
        }
        placed
    }

    /// Rising edge of Ctrl+V (Shift selects paste-in-place). egui does not
    /// deliver this as a key when the clipboard has no text.
    pub(crate) fn take_paste_chord_edge(&mut self, focused: bool) -> Option<bool> {
        #[cfg(windows)]
        {
            let held = paste_chord_held();
            // egui owns text pastes, including Slate selections with a text
            // fallback. Its Paste event may arrive a frame after this poll;
            // emitting both is two journal commits for one physical chord.
            paste_fallback_edge(
                &mut self.paste_chord_down,
                held,
                focused,
                clipboard_has_text(),
            )
        }
        #[cfg(not(windows))]
        {
            let _ = focused;
            None
        }
    }
}

#[cfg(any(windows, test))]
fn paste_fallback_edge(
    was_down: &mut bool,
    held: Option<bool>,
    focused: bool,
    has_text: bool,
) -> Option<bool> {
    let edge = held.is_some() && !*was_down;
    *was_down = held.is_some();
    (focused && edge && !has_text).then(|| held.unwrap_or(false))
}

#[cfg(windows)]
fn clipboard_has_text() -> bool {
    use windows::Win32::System::DataExchange::IsClipboardFormatAvailable;
    // CF_UNICODETEXT; querying offered formats does not open the clipboard.
    unsafe { IsClipboardFormatAvailable(13).is_ok() }
}

#[cfg(windows)]
enum OsPaste {
    Files(Vec<PathBuf>),
    Png(Vec<u8>),
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
fn read_clipboard_media() -> Option<OsPaste> {
    let raw = {
        let _clip = clipboard_win::Clipboard::new_attempts(5).ok()?;
        read_clipboard_raw()
    };
    match raw? {
        ClipRaw::Files(paths) => Some(OsPaste::Files(paths)),
        ClipRaw::Image(image) => image.into_png().map(OsPaste::Png),
    }
}

#[cfg(windows)]
enum ClipRaw {
    Files(Vec<PathBuf>),
    Image(atlas_core::clipboard_image::ClipImage),
}

#[cfg(windows)]
fn read_clipboard_raw() -> Option<ClipRaw> {
    use clipboard_win::formats::FileList;
    use clipboard_win::Getter;
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
    atlas_core::clipboard_image::read_open().map(ClipRaw::Image)
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
mod tests;
