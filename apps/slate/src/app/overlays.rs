//! Canvas overlays: minimap (M), canvas palette (double-click empty board),
//! command history window (F2), board search (Ctrl+F), and Tab cycling.
//!
//! The overlay *chrome* (painting, interaction) lives in `atlas-shell`
//! (`minimap.rs`, `palette.rs`, `history_ui.rs` — Constitution Art. X); this
//! module adapts Slate state into those plain-data models and applies the
//! returned actions. The search strip is Slate-specific P1 chrome (Atlas
//! focuses its Filters-dock field instead, per `docs/keymap/specs/overlays.md`).

use super::SlateApp;
use atlas_commands::{palette_query, CommandId, PaletteItem};
use atlas_shell::history_ui::{history_window, HistoryRow};
use atlas_shell::minimap::{minimap_ui, MinimapAction, MinimapModel};
use atlas_shell::palette::{palette_ui, PaletteAction, PaletteRow};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use slate_doc::scene::NodeKind;
use slate_doc::{ItemId, NodeId, ViewKind};
use std::collections::HashSet;

// ---------- search state ----------

/// One search result in the active tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchHit {
    /// Board view: a scene node (text content, frame title, or image whose
    /// linked item name / tag names match).
    Node(NodeId),
    /// Grid/Venn views: a pool item (name or tag names match).
    Item(ItemId),
}

/// Ctrl+F overlay state. Matching is a transient paint-time set — the scene
/// is never mutated and nothing is journaled.
#[derive(Default)]
pub struct SearchState {
    pub open: bool,
    pub query: String,
    /// Focus the query field on the next overlay frame.
    focus_pending: bool,
    /// Ordered hits for Enter / Shift+Enter cycling.
    hits: Vec<SearchHit>,
    cursor: usize,
    /// Fast membership sets for paint-time dimming.
    node_hits: HashSet<NodeId>,
    item_hits: HashSet<ItemId>,
    /// (query, scene generation, view) the caches were computed for.
    computed_for: Option<(String, u64, ViewKind)>,
}

impl SearchState {
    pub fn dimming_active(&self) -> bool {
        self.open && !self.query.trim().is_empty()
    }
}

impl SlateApp {
    // ---------- minimap (M) ----------

    /// Toggle from the `canvas.minimap` command; persisted in chrome prefs.
    pub(crate) fn toggle_minimap(&mut self) {
        self.minimap_on = !self.minimap_on;
        self.save_chrome_prefs();
    }

    pub(crate) fn save_chrome_prefs(&self) {
        atlas_shell::prefs::ChromePrefs {
            dock_side: self.dock_side,
            pinned_panels: self.dock_pins.clone(),
            minimap: self.minimap_on,
        }
        .save("slate");
    }

    /// Paint the minimap into the canvas and apply camera actions. Callers
    /// build the view-specific [`MinimapModel`]; this is a no-op while the
    /// minimap is toggled off (zero cost — Art. II).
    pub(crate) fn show_minimap(&mut self, ui: &mut egui::Ui, rect: Rect, model: MinimapModel) {
        if !self.minimap_on || self.presenting.is_some() {
            return;
        }
        match minimap_ui(ui, rect, &model, &mut self.minimap_state) {
            MinimapAction::None => {}
            MinimapAction::JumpTo(world) | MinimapAction::DragTo(world) => {
                self.tab_mut().cam.offset = world.to_vec2();
            }
            MinimapAction::Zoom {
                world_point,
                factor,
            } => {
                let screen = self.world_to_screen(world_point);
                self.zoom_at(screen, factor);
            }
        }
    }

    /// Board minimap model: node rects colored by kind, frames as outlines,
    /// hidden nodes skipped. `generation` keys the shell's cached texture.
    pub(crate) fn board_minimap_model(&self) -> Option<MinimapModel> {
        let palette = self.palette();
        let nodes = &self.doc().scene.nodes;
        let mut bounds = Rect::NOTHING;
        let mut blocks: Vec<(Rect, Color32)> = Vec::new();
        for n in nodes {
            if n.hidden || matches!(n.kind, NodeKind::Connector(_)) {
                continue;
            }
            let r =
                Rect::from_min_size(Pos2::new(n.rect.x, n.rect.y), Vec2::new(n.rect.w, n.rect.h));
            bounds = bounds.union(r);
            match &n.kind {
                NodeKind::Frame(_) => {
                    // Frames read as slide outlines (research: the frame
                    // outline is the minimap's slide differentiator).
                    let t = (r.width().max(r.height()) * 0.02).max(2.0);
                    let c = palette.border_strong;
                    blocks.push((Rect::from_min_size(r.min, Vec2::new(r.width(), t)), c));
                    blocks.push((
                        Rect::from_min_size(
                            Pos2::new(r.min.x, r.max.y - t),
                            Vec2::new(r.width(), t),
                        ),
                        c,
                    ));
                    blocks.push((Rect::from_min_size(r.min, Vec2::new(t, r.height())), c));
                    blocks.push((
                        Rect::from_min_size(
                            Pos2::new(r.max.x - t, r.min.y),
                            Vec2::new(t, r.height()),
                        ),
                        c,
                    ));
                }
                NodeKind::Image(_) => blocks.push((r, palette.portal.gamma_multiply(0.85))),
                NodeKind::Text(_) => blocks.push((r, palette.sub.gamma_multiply(0.8))),
                NodeKind::Shape(_) => blocks.push((r, palette.accent.gamma_multiply(0.8))),
                NodeKind::Connector(_) => {}
            }
        }
        if !bounds.is_positive() {
            return None;
        }
        Some(MinimapModel {
            bounds: bounds.expand(40.0),
            blocks,
            viewport: self.camera_world_rect(),
            generation: self.scene_gen.wrapping_mul(31).wrapping_add(self.tab().id),
        })
    }

    /// Current camera rect in world space (for the minimap viewport box).
    pub(crate) fn camera_world_rect(&self) -> Rect {
        Rect::from_min_max(
            self.screen_to_world(self.canvas_rect.min),
            self.screen_to_world(self.canvas_rect.max),
        )
    }

    // ---------- canvas palette (double-click empty board) ----------

    /// Open the palette at a screen point, remembering the world point under
    /// it (placeables place there — the Grasshopper gesture).
    pub(crate) fn open_board_palette(&mut self, screen: Pos2, world: Pos2) {
        self.palette_state.open_at(screen, world);
        self.refresh_palette_items();
        self.push_history(CommandId("board.palette"), None);
    }

    fn refresh_palette_items(&mut self) {
        let ctx = self.command_ctx();
        self.palette_items = palette_query(&self.registry, ctx, &self.palette_state.query);
        // Overlays spec: on an empty query, placeables (frame, text,
        // shapes…) rank above general commands. Stable partition keeps the
        // alphabetical order inside each section.
        if self.palette_state.query.trim().is_empty() {
            self.palette_items
                .sort_by_key(|it| !it.id.0.starts_with("board.tool."));
        }
    }

    /// Frame step for the palette overlay; call once per frame after the
    /// canvas so the popup floats above it.
    pub(crate) fn palette_frame(&mut self, ctx: &egui::Context) {
        if !self.palette_state.open {
            // A wire-pending connect only survives while its palette is up.
            self.wire_pending = None;
            return;
        }
        let rows: Vec<PaletteRow> = self
            .palette_items
            .iter()
            .map(|it| PaletteRow {
                label: it.name.to_string(),
                hint: self
                    .registry
                    .by_id(it.id)
                    .map(|s| s.binding.split(&[',', '—', '('][..]).next().unwrap_or(""))
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            })
            .collect();
        match palette_ui(ctx, &mut self.palette_state, &rows) {
            PaletteAction::None => {}
            PaletteAction::Dismiss => {
                // Wire released on empty + palette dismissed = no connector.
                self.wire_pending = None;
            }
            PaletteAction::QueryChanged => self.refresh_palette_items(),
            PaletteAction::Execute(i) => {
                if let Some(item) = self.palette_items.get(i).cloned() {
                    self.palette_execute(ctx, item);
                }
            }
        }
    }

    /// Execute a palette row. Placeable commands with a click-to-place
    /// default (frame, text) place at the stored world point immediately;
    /// the drag-defined shapes (rect / ellipse / line / pen) arm their tool
    /// — accepted P1 behavior, documented in `COMMANDS.md`.
    fn palette_execute(&mut self, ctx: &egui::Context, item: PaletteItem) {
        let world = self.palette_state.world;
        match item.id.0 {
            "board.tool.frame" => {
                self.place_frame_at(world);
                self.push_history(item.id, Some("placed".into()));
                self.connect_pending_wire_to_selection();
            }
            "board.tool.text" => {
                self.place_text_at(world);
                self.push_history(item.id, Some("placed".into()));
                self.connect_pending_wire_to_selection();
            }
            "board.tool.sticky" => {
                self.place_sticky_at(world);
                self.connect_pending_wire_to_selection();
            }
            _ => {
                self.dispatch(ctx, item.id, None);
                // Non-placing commands drop a pending wire connect (only
                // an immediately placed node can auto-connect).
                self.wire_pending = None;
            }
        }
    }

    /// If a wire drag ended on empty canvas and the palette just placed a
    /// node (it becomes the selection), auto-connect to its nearest side.
    fn connect_pending_wire_to_selection(&mut self) {
        if self.wire_pending.is_none() {
            return;
        }
        if self.board_sel.len() == 1 {
            let placed = *self.board_sel.iter().next().unwrap();
            self.resolve_pending_wire(placed);
        } else {
            self.wire_pending = None;
        }
    }

    // ---------- history window (F2) ----------

    pub(crate) fn history_frame(&mut self, ctx: &egui::Context) {
        if !self.history_open {
            return;
        }
        let now = std::time::SystemTime::now();
        // Newest first, straight out of the atlas-commands ring buffer.
        let rows: Vec<HistoryRow> = self
            .cmd_history
            .iter()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|e| HistoryRow {
                name: e.name.to_string(),
                detail: e.detail.clone().unwrap_or_default(),
                author: match &e.author {
                    atlas_commands::CmdAuthor::Human => String::new(),
                    atlas_commands::CmdAuthor::Agent(name) => name.clone(),
                },
                ago: ago_text(now, e.at),
            })
            .collect();
        let mut open = self.history_open;
        history_window(ctx, &mut open, &rows);
        self.history_open = open;
    }

    // ---------- board search (Ctrl+F) ----------

    pub(crate) fn open_search(&mut self) {
        self.search.open = true;
        self.search.focus_pending = true;
    }

    /// Recompute the match sets when the query, the scene, or the view
    /// changed. Cheap string scan over the active tab only.
    fn refresh_search_matches(&mut self) {
        let view = self.doc().view.active_view;
        let key = (self.search.query.clone(), self.scene_gen, view);
        if self.search.computed_for.as_ref() == Some(&key) {
            return;
        }
        let q = self.search.query.trim().to_lowercase();
        let mut hits = Vec::new();
        let mut node_hits = HashSet::new();
        let mut item_hits = HashSet::new();
        if !q.is_empty() {
            let doc = self.doc();
            let item_matches = |id: ItemId| -> bool {
                let Some(it) = doc.item(id) else {
                    return false;
                };
                if it.file_name.to_lowercase().contains(&q) {
                    return true;
                }
                it.assignments.values().any(|t| {
                    doc.tag(*t)
                        .is_some_and(|(_, tag)| tag.name.to_lowercase().contains(&q))
                })
            };
            if view == ViewKind::Board {
                for n in &doc.scene.nodes {
                    if n.hidden {
                        continue;
                    }
                    let is_match = match &n.kind {
                        NodeKind::Text(t) => t.text.to_lowercase().contains(&q),
                        NodeKind::Frame(f) => f.title.to_lowercase().contains(&q),
                        NodeKind::Image(img) => item_matches(img.item),
                        NodeKind::Connector(c) => c
                            .label
                            .as_ref()
                            .is_some_and(|l| l.to_lowercase().contains(&q)),
                        NodeKind::Shape(_) => false,
                    };
                    if is_match {
                        hits.push(SearchHit::Node(n.id));
                        node_hits.insert(n.id);
                    }
                }
            } else {
                for it in &doc.items {
                    if item_matches(it.id) {
                        hits.push(SearchHit::Item(it.id));
                        item_hits.insert(it.id);
                    }
                }
            }
        }
        self.search.hits = hits;
        self.search.node_hits = node_hits;
        self.search.item_hits = item_hits;
        self.search.cursor = 0;
        self.search.computed_for = Some(key);
    }

    /// Paint-time dim set for the board: `Some(matching nodes)` while a
    /// query is live in Board view. Never mutates the scene.
    pub(crate) fn search_node_matches(&self) -> Option<&HashSet<NodeId>> {
        (self.search.dimming_active() && self.doc().view.active_view == ViewKind::Board)
            .then_some(&self.search.node_hits)
    }

    /// Paint-time dim set for Grid/Venn thumbnails.
    pub(crate) fn search_item_matches(&self) -> Option<&HashSet<ItemId>> {
        let view = self.doc().view.active_view;
        (self.search.dimming_active() && matches!(view, ViewKind::Grid | ViewKind::Venn))
            .then_some(&self.search.item_hits)
    }

    /// The hit the camera last flew to (outlined on the board).
    pub(crate) fn search_current_hit(&self) -> Option<SearchHit> {
        self.search
            .dimming_active()
            .then(|| self.search.hits.get(self.search.cursor).copied())
            .flatten()
    }

    /// Advance the cursor and fly the camera to the hit (reuses the fit /
    /// zoom-to-rect plumbing).
    fn search_cycle(&mut self, dir: i64) {
        let len = self.search.hits.len();
        if len == 0 {
            return;
        }
        self.search.cursor = (self.search.cursor as i64 + dir).rem_euclid(len as i64) as usize;
        let hit = self.search.hits[self.search.cursor];
        let target = match hit {
            SearchHit::Node(id) => self.doc().scene.node(id).map(|n| {
                Rect::from_min_size(Pos2::new(n.rect.x, n.rect.y), Vec2::new(n.rect.w, n.rect.h))
            }),
            SearchHit::Item(id) => self.layout_rect_of_item(id),
        };
        if let Some(r) = target {
            self.fly_to_world_rect(r);
        }
    }

    /// Camera flight used by search cycling: fit the rect with generous
    /// context so the surroundings stay legible.
    pub(crate) fn fly_to_world_rect(&mut self, r: Rect) {
        let margin = (r.width().max(r.height()) * 0.75).max(120.0);
        self.fit_view(r.expand(margin));
        let now = self.frame_time;
        self.bump_grid_fade(now);
    }

    /// The Ctrl+F strip: top-center overlay with the query field, the match
    /// count, and prev/next. Esc closes (full opacity returns, camera kept).
    pub(crate) fn search_frame(&mut self, ctx: &egui::Context) {
        if !self.search.open || self.presenting.is_some() {
            return;
        }
        self.refresh_search_matches();
        let palette = self.palette();
        let canvas = self.canvas_rect;
        let mut close = false;
        let mut cycle: i64 = 0;
        egui::Area::new(egui::Id::new("slate_search_strip"))
            .fixed_pos(Pos2::new(canvas.center().x - 190.0, canvas.min.y + 10.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.card)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.search.query)
                                    .hint_text("Search names, text, frames, tags…")
                                    .desired_width(200.0),
                            );
                            if self.search.focus_pending {
                                resp.request_focus();
                                self.search.focus_pending = false;
                            }
                            let (enter, shift, esc) = ui.input(|i| {
                                (
                                    i.key_pressed(egui::Key::Enter),
                                    i.modifiers.shift,
                                    i.key_pressed(egui::Key::Escape),
                                )
                            });
                            if resp.has_focus() || resp.lost_focus() {
                                if enter {
                                    cycle = if shift { -1 } else { 1 };
                                    resp.request_focus();
                                }
                                if esc {
                                    close = true;
                                }
                            }
                            let (count, at) = (
                                self.search.hits.len(),
                                (self.search.cursor + 1).min(self.search.hits.len()),
                            );
                            let label = if self.search.query.trim().is_empty() {
                                String::new()
                            } else {
                                format!("{at}/{count}")
                            };
                            ui.label(egui::RichText::new(label).small().color(palette.sub));
                            if ui
                                .small_button("◀")
                                .on_hover_text("Previous (Shift+Enter)")
                                .clicked()
                            {
                                cycle = -1;
                            }
                            if ui.small_button("▶").on_hover_text("Next (Enter)").clicked() {
                                cycle = 1;
                            }
                            if ui.small_button("✕").on_hover_text("Close (Esc)").clicked() {
                                close = true;
                            }
                        });
                    });
            });
        // The query may have changed this frame; keep the sets fresh so the
        // dim pass and count agree with what the user sees.
        self.refresh_search_matches();
        if cycle != 0 {
            self.search_cycle(cycle);
        }
        if close {
            self.search.open = false;
        }
    }

    // ---------- Tab cycling ----------

    /// Tab / Shift+Tab: cycle objects in reading order, select, and nudge
    /// the camera minimally so the object is in view.
    pub(crate) fn cycle_objects(&mut self, dir: i64) {
        match self.doc().view.active_view {
            ViewKind::Board => self.cycle_board_nodes(dir),
            // Lens focus cycling is not in P1 (graph traversal ≠ reading order).
            ViewKind::Lens => {}
            _ => self.cycle_items(dir),
        }
    }

    fn cycle_board_nodes(&mut self, dir: i64) {
        let entries: Vec<(NodeId, f32, f32, f32)> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && !n.locked && !matches!(n.kind, NodeKind::Connector(_)))
            .map(|n| (n.id, n.rect.x, n.rect.y, n.rect.y + n.rect.h))
            .collect();
        let order = reading_order(&entries);
        // A group counts as one stop: only its first member (reading order)
        // stays in the cycle; landing on it selects the whole group.
        let mut seen_groups: HashSet<slate_doc::scene::GroupKey> = HashSet::new();
        let order: Vec<NodeId> = order
            .into_iter()
            .filter(
                |id| match self.doc().scene.node(*id).and_then(|n| n.group) {
                    Some(g) => seen_groups.insert(g),
                    None => true,
                },
            )
            .collect();
        if order.is_empty() {
            return;
        }
        let current = order.iter().position(|id| self.board_sel.contains(id));
        let next = match current {
            Some(i) => (i as i64 + dir).rem_euclid(order.len() as i64) as usize,
            None => {
                if dir >= 0 {
                    0
                } else {
                    order.len() - 1
                }
            }
        };
        let id = order[next];
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.expand_board_selection();
        if let Some(n) = self.doc().scene.node(id) {
            let r =
                Rect::from_min_size(Pos2::new(n.rect.x, n.rect.y), Vec2::new(n.rect.w, n.rect.h));
            self.nudge_camera_to_world_rect(r);
        }
    }

    fn cycle_items(&mut self, dir: i64) {
        let order = self.layout_item_order();
        if order.is_empty() {
            return;
        }
        let current = (self.selection.len() == 1)
            .then(|| self.selection.iter().next().copied())
            .flatten()
            .and_then(|id| order.iter().position(|o| *o == id));
        let next = match current {
            Some(i) => (i as i64 + dir).rem_euclid(order.len() as i64) as usize,
            None => {
                if dir >= 0 {
                    0
                } else {
                    order.len() - 1
                }
            }
        };
        let id = order[next];
        self.selection.clear();
        self.selection.insert(id);
        if let Some(r) = self.layout_rect_of_item(id) {
            self.nudge_camera_to_world_rect(r);
        }
    }

    /// Pan the camera by the minimal delta that brings the world rect into
    /// view (with padding). Zoom is untouched — this is the auto-pan Miro
    /// lacks, not a zoom jump.
    pub(crate) fn nudge_camera_to_world_rect(&mut self, r: Rect) {
        let screen = Rect::from_min_max(self.world_to_screen(r.min), self.world_to_screen(r.max));
        let view = self.canvas_rect.shrink(48.0);
        if view.contains_rect(screen) {
            return;
        }
        let mut dx = 0.0f32;
        let mut dy = 0.0f32;
        if screen.min.x < view.min.x {
            dx = screen.min.x - view.min.x;
        } else if screen.max.x > view.max.x {
            dx = screen.max.x - view.max.x;
        }
        if screen.min.y < view.min.y {
            dy = screen.min.y - view.min.y;
        } else if screen.max.y > view.max.y {
            dy = screen.max.y - view.max.y;
        }
        // Objects larger than the viewport anchor on their top-left corner.
        if screen.width() > view.width() {
            dx = screen.min.x - view.min.x;
        }
        if screen.height() > view.height() {
            dy = screen.min.y - view.min.y;
        }
        let z = self.tab().cam.z.max(f32::EPSILON);
        self.tab_mut().cam.offset += Vec2::new(dx, dy) / z;
    }
}

/// Reading order over `(id, x, y_top, y_bottom)` entries: rows grouped by
/// vertical overlap with the row's topmost member, rows top→bottom, members
/// left→right (Miro's engineering-tested traversal).
pub fn reading_order<T: Copy + PartialEq>(items: &[(T, f32, f32, f32)]) -> Vec<T> {
    let mut remaining: Vec<(T, f32, f32, f32)> = items.to_vec();
    let mut out = Vec::with_capacity(items.len());
    while !remaining.is_empty() {
        // Seed: the topmost remaining entry (ties broken by x).
        let seed = remaining
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.2.total_cmp(&b.2).then(a.1.total_cmp(&b.1)))
            .map(|(i, _)| i)
            .expect("non-empty");
        let (_, _, seed_top, seed_bottom) = remaining[seed];
        // The row: everything vertically overlapping the seed's interval.
        let mut row: Vec<(T, f32, f32, f32)> = Vec::new();
        remaining.retain(|e| {
            let overlaps = e.2 < seed_bottom && e.3 > seed_top;
            if overlaps {
                row.push(*e);
            }
            !overlaps
        });
        row.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)));
        out.extend(row.into_iter().map(|e| e.0));
    }
    out
}

/// "2 s" / "3 m" / "1 h" relative-time text for the history window.
fn ago_text(now: std::time::SystemTime, at: std::time::SystemTime) -> String {
    let secs = now
        .duration_since(at)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    match secs {
        0..=4 => "now".into(),
        5..=59 => format!("{secs} s ago"),
        60..=3599 => format!("{} m ago", secs / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (id, x, y_top, y_bottom)
    fn e(id: u32, x: f32, y: f32, h: f32) -> (u32, f32, f32, f32) {
        (id, x, y, y + h)
    }

    #[test]
    fn reading_order_rows_by_overlap_then_x() {
        // Row 1: b(x=10) a(x=100) overlap vertically despite offset tops;
        // Row 2: c below them; d far right in row 2.
        let items = [
            e(1, 100.0, 0.0, 50.0),   // a
            e(2, 10.0, 20.0, 50.0),   // b — overlaps a's [0,50)
            e(3, 0.0, 120.0, 40.0),   // c
            e(4, 300.0, 130.0, 40.0), // d — overlaps c
        ];
        assert_eq!(reading_order(&items), vec![2, 1, 3, 4]);
    }

    #[test]
    fn reading_order_non_overlapping_stack_is_top_to_bottom() {
        let items = [
            e(1, 0.0, 200.0, 10.0),
            e(2, 0.0, 0.0, 10.0),
            e(3, 0.0, 100.0, 10.0),
        ];
        assert_eq!(reading_order(&items), vec![2, 3, 1]);
    }

    #[test]
    fn reading_order_empty_is_empty() {
        assert!(reading_order::<u32>(&[]).is_empty());
    }
}
