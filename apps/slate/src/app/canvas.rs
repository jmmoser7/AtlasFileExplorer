//! The slate canvas: tag-grouped grid and Venn presentations.
//!
//! Both presentations lay items out in world space and share one camera
//! (pan/zoom/turbo-pan identical to File Atlas). The Venn view renders
//! literal overlapping tag circles with circle-cropped thumbnails packed
//! inside them (`circle-pack` crate does the geometry).

use super::{SlateApp, ThumbState};
use circle_pack::{venn_layout, Circle, VennItem, VennSet};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};
use slate_doc::{link_status, ItemId, LinkStatus, TagId, ViewKind};
use std::collections::BTreeMap;

const SECTION_GAP: f32 = 56.0;
const HEADER_H: f32 = 30.0;
const CARD_PAD: f32 = 7.0;
/// World radius of item circles fed to the Venn engine (the engine may
/// shrink crowded regions; we render whatever radius comes back).
const VENN_ITEM_R: f32 = 9.0;
/// Venn engine units → world units.
const VENN_SCALE: f32 = 9.0;
const ZOOM_MIN: f32 = 0.05;
const ZOOM_MAX: f32 = 3.5;

/// One laid-out thumbnail (world space).
struct Placed {
    id: ItemId,
    rect: Rect,
    /// Venn mode: render as a circle of this radius instead of a card.
    circle_r: Option<f32>,
}

struct GridSection {
    label: String,
    chips: Vec<(String, [u8; 3])>,
    header_pos: Pos2,
}

struct Layout {
    placed: Vec<Placed>,
    sections: Vec<GridSection>,
    /// Venn set circles: (tag, name, color, circle, member count).
    venn_sets: Vec<(TagId, String, [u8; 3], Circle)>,
    bounds: Rect,
}

impl SlateApp {
    pub(crate) fn world_to_screen(&self, w: Pos2) -> Pos2 {
        let cam = self.tab().cam;
        self.canvas_rect.center() + (w.to_vec2() - cam.offset) * cam.z
    }

    pub(crate) fn screen_to_world(&self, s: Pos2) -> Pos2 {
        let cam = self.tab().cam;
        (((s - self.canvas_rect.center()) / cam.z) + cam.offset).to_pos2()
    }

    pub(crate) fn world_rect_to_screen(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.world_to_screen(r.min), self.world_to_screen(r.max))
    }

    // ----- layout -------------------------------------------------------------

    fn all_tag_ids(&self) -> Vec<TagId> {
        self.doc()
            .groups
            .iter()
            .flat_map(|g| g.tags.iter().map(|t| t.id))
            .collect()
    }

    fn tag_label(&self, id: TagId) -> (String, [u8; 3]) {
        self.doc()
            .tag(id)
            .map(|(_, t)| (t.name.clone(), t.color))
            .unwrap_or_else(|| ("?".into(), [128, 128, 128]))
    }

    fn grid_layout(&self) -> Layout {
        let cell = self.cell;
        let doc = self.doc();
        let active = self.all_tag_ids();
        let buckets: BTreeMap<Vec<TagId>, Vec<ItemId>> = doc.combination_buckets(&active);

        // Deterministic section order: fewer tags first, then by label.
        let mut sections_src: Vec<(Vec<TagId>, Vec<ItemId>)> = buckets.into_iter().collect();
        sections_src.sort_by_key(|(k, _)| (k.len(), k.clone()));
        let uncategorized = doc.uncategorized_items();
        if !uncategorized.is_empty() {
            sections_src.push((Vec::new(), uncategorized));
        }

        let mut placed = Vec::new();
        let mut sections = Vec::new();
        let mut y = 0.0f32;
        let mut max_w = 0.0f32;

        for (combo, items) in &sections_src {
            let n = items.len().max(1);
            let cols = ((n as f32 * 2.0).sqrt().ceil() as usize).clamp(2, 10);
            let label = if combo.is_empty() {
                "Uncategorized".to_string()
            } else {
                combo
                    .iter()
                    .map(|t| self.tag_label(*t).0)
                    .collect::<Vec<_>>()
                    .join("  +  ")
            };
            let chips = combo.iter().map(|t| self.tag_label(*t)).collect();
            sections.push(GridSection {
                label,
                chips,
                header_pos: Pos2::new(0.0, y),
            });
            y += HEADER_H;

            for (i, id) in items.iter().enumerate() {
                let col = i % cols;
                let row = i / cols;
                let min = Pos2::new(col as f32 * cell, y + row as f32 * cell);
                placed.push(Placed {
                    id: *id,
                    rect: Rect::from_min_size(min, Vec2::splat(cell - 8.0)),
                    circle_r: None,
                });
                max_w = max_w.max((col + 1) as f32 * cell);
            }
            let rows = n.div_ceil(cols);
            y += rows as f32 * cell + SECTION_GAP;
        }

        let bounds = Rect::from_min_max(
            Pos2::new(-20.0, -20.0),
            Pos2::new(max_w.max(cell) + 20.0, y.max(100.0)),
        );
        Layout {
            placed,
            sections,
            venn_sets: Vec::new(),
            bounds,
        }
    }

    fn venn_layout_now(&self) -> Layout {
        let doc = self.doc();
        let focus = &self.tab().venn_focus;
        let focused: Vec<TagId> = if focus.is_empty() {
            self.all_tag_ids()
        } else {
            self.all_tag_ids()
                .into_iter()
                .filter(|t| focus.contains(t))
                .collect()
        };

        let sets: Vec<VennSet> = focused
            .iter()
            .map(|t| VennSet {
                id: t.0,
                weight: doc.items_with_tag(*t).len().max(1) as f32,
            })
            .collect();
        let items: Vec<VennItem> = doc
            .items
            .iter()
            .map(|it| {
                let member: Vec<u64> = it
                    .assignments
                    .values()
                    .filter(|t| focused.contains(t))
                    .map(|t| t.0)
                    .collect();
                VennItem {
                    id: it.id.0,
                    sets: member,
                    r: VENN_ITEM_R,
                }
            })
            .collect();

        let layout = venn_layout(&sets, &items);

        let scale = |c: &Circle| Circle {
            x: c.x * VENN_SCALE,
            y: c.y * VENN_SCALE,
            r: c.r * VENN_SCALE,
        };

        let mut bounds = Rect::NOTHING;
        let venn_sets: Vec<(TagId, String, [u8; 3], Circle)> = layout
            .set_circles
            .iter()
            .map(|(id, c)| {
                let c = scale(c);
                let (name, color) = self.tag_label(TagId(*id));
                bounds = bounds.union(Rect::from_center_size(
                    Pos2::new(c.x, c.y),
                    Vec2::splat(c.r * 2.0),
                ));
                (TagId(*id), name, color, c)
            })
            .collect();

        let mut placed: Vec<Placed> = layout
            .item_circles
            .iter()
            .map(|(id, c)| {
                let c = scale(c);
                Placed {
                    id: ItemId(*id),
                    rect: Rect::from_center_size(Pos2::new(c.x, c.y), Vec2::splat(c.r * 2.0)),
                    circle_r: Some(c.r),
                }
            })
            .collect();

        // Uncategorized tray: a row of circles beneath the diagram.
        let uncategorized = doc.uncategorized_items();
        if bounds.is_negative() {
            bounds = Rect::from_min_size(Pos2::ZERO, Vec2::splat(10.0));
        }
        if !uncategorized.is_empty() {
            let r = VENN_ITEM_R * VENN_SCALE * 0.85;
            let per_row = 12usize;
            let y0 = bounds.max.y + 90.0;
            for (i, id) in uncategorized.iter().enumerate() {
                let col = (i % per_row) as f32;
                let row = (i / per_row) as f32;
                let center =
                    Pos2::new(bounds.min.x + r + col * (r * 2.2), y0 + r + row * (r * 2.2));
                placed.push(Placed {
                    id: *id,
                    rect: Rect::from_center_size(center, Vec2::splat(r * 2.0)),
                    circle_r: Some(r),
                });
            }
            let rows = uncategorized.len().div_ceil(per_row);
            bounds.max.y = y0 + rows as f32 * (r * 2.2) + 20.0;
        }

        let mut sections = Vec::new();
        if !uncategorized.is_empty() {
            sections.push(GridSection {
                label: "Uncategorized".into(),
                chips: Vec::new(),
                header_pos: Pos2::new(bounds.min.x, bounds.max.y - 60.0),
            });
        }

        Layout {
            placed,
            sections,
            venn_sets,
            bounds: bounds.expand(40.0),
        }
    }

    // ----- interaction ----------------------------------------------------------

    pub(crate) fn fit_view(&mut self, bounds: Rect) {
        let canvas = self.canvas_rect;
        let z = ((canvas.width() / bounds.width().max(1.0))
            .min(canvas.height() / bounds.height().max(1.0))
            * 0.92)
            .clamp(ZOOM_MIN, ZOOM_MAX);
        let cam = &mut self.tab_mut().cam;
        cam.z = z;
        cam.offset = bounds.center().to_vec2();
    }

    pub(crate) fn zoom_at(&mut self, pointer: Pos2, factor: f32) {
        let world_before = self.screen_to_world(pointer);
        let cam = &mut self.tab_mut().cam;
        cam.z = (cam.z * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let cam_z = cam.z;
        let center = self.canvas_rect.center();
        self.tab_mut().cam.offset = world_before.to_vec2() - (pointer - center) / cam_z;
        self.tab_mut().grid_fade_armed = true;
    }

    pub(crate) fn bump_grid_fade(&mut self, time: f64) {
        self.tab_mut().grid_fade.bump(time);
    }

    pub(crate) fn flush_grid_fade_armed(&mut self, time: f64) {
        if self.tab().grid_fade_armed {
            self.tab_mut().grid_fade.bump(time);
            self.tab_mut().grid_fade_armed = false;
        }
    }

    pub(crate) fn open_path(path: &std::path::Path) {
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(path)
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
        }
    }

    // ----- main entry -------------------------------------------------------------

    pub fn canvas(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        self.canvas_rect = rect;
        let palette = self.palette();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, palette.bg);
        self.flush_grid_fade_armed(ui.ctx().input(|i| i.time));

        // The Board is the authored open-world canvas; it owns its own
        // input/paint loop (see `board.rs`).
        if self.doc().view.active_view == ViewKind::Board {
            self.board_canvas(ui, rect);
            // The board owns its own camera; the mini menu only offers the
            // full-screen toggle here (zoom lives in the board toolbar keys).
            self.mini_menu(ui.ctx(), rect, None);
            return;
        }

        if self.doc().view.active_view == ViewKind::Lens {
            self.lens_canvas(ui, rect);
            return;
        }

        if self.doc().items.is_empty() && self.doc().groups.is_empty() {
            self.welcome(ui, rect);
            return;
        }

        let layout = match self.doc().view.active_view {
            ViewKind::Venn => self.venn_layout_now(),
            _ => self.grid_layout(),
        };

        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let pointer = ui.ctx().pointer_latest_pos();
        let now = ui.ctx().input(|i| i.time);
        let mut canvas_nav = false;

        // --- camera: zoom / pan / turbo pan ---
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                if ui.input(|i| i.modifiers.shift) {
                    let z = self.tab().cam.z;
                    self.tab_mut().cam.offset.x -= scroll / z;
                    canvas_nav = true;
                } else if let Some(p) = pointer {
                    self.zoom_at(p, 1.0 + scroll * 0.0015);
                    canvas_nav = true;
                }
            }
        }
        ui.input(|i| {
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.zoom_at(rect.center(), 1.2);
            }
            if i.key_pressed(egui::Key::Minus) {
                self.zoom_at(rect.center(), 1.0 / 1.2);
            }
            if i.key_pressed(egui::Key::F) && !i.modifiers.ctrl {
                self.fit_view(layout.bounds);
            }
        });
        // Turbo pan mutates a copy (borrow rules), then we write it back.
        let mut cam_offset_tmp = self.tab().cam.offset;
        let ctx = ui.ctx().clone();
        let turbo_active = self
            .turbo_pan
            .step(&ctx, rect, pointer, &mut cam_offset_tmp);
        if turbo_active {
            let z = self.tab().cam.z;
            let old = self.tab().cam.offset;
            // TurboPanState emits screen-px deltas with "content follows the
            // pointer" polarity; our camera offset is the world point at the
            // canvas center, so convert and invert.
            self.tab_mut().cam.offset = old - (cam_offset_tmp - old) / z;
            canvas_nav = true;
        } else if resp.dragged_by(egui::PointerButton::Primary)
            || resp.dragged_by(egui::PointerButton::Secondary)
        {
            let delta = resp.drag_delta();
            let z = self.tab().cam.z;
            self.tab_mut().cam.offset -= delta / z;
            canvas_nav = true;
        }
        if canvas_nav {
            self.bump_grid_fade(now);
        }

        // --- hit test (topmost last-drawn wins; reverse iterate) ---
        let hovered_item: Option<ItemId> = pointer.filter(|p| rect.contains(*p)).and_then(|p| {
            let w = self.screen_to_world(p);
            layout
                .placed
                .iter()
                .rev()
                .find(|pl| match pl.circle_r {
                    Some(r) => {
                        (Pos2::new(pl.rect.center().x, pl.rect.center().y) - w).length() <= r
                    }
                    None => pl.rect.contains(w),
                })
                .map(|pl| pl.id)
        });

        // --- clicks ---
        if resp.clicked() {
            match hovered_item {
                Some(id) => {
                    let ctrl = ui.input(|i| i.modifiers.ctrl);
                    if ctrl {
                        if !self.selection.remove(&id) {
                            self.selection.insert(id);
                        }
                    } else {
                        self.selection.clear();
                        self.selection.insert(id);
                    }
                }
                None => self.selection.clear(),
            }
        }
        if resp.double_clicked() {
            if let Some(id) = hovered_item {
                if let Some(path) = self.doc().item(id).map(|it| it.path.clone()) {
                    self.open_item_path(&path);
                }
            }
        }
        let secondary = resp.secondary_clicked() && !self.turbo_pan.should_suppress_context_menu();
        self.turbo_pan.acknowledge_context_menu();
        if secondary {
            if let (Some(id), Some(p)) = (hovered_item, pointer) {
                self.menu = Some((id, p));
            }
        }

        // --- paint ---
        let grid_alpha = self.tab().grid_fade.alpha(now);
        self.paint_dot_grid(&painter, rect, &palette, grid_alpha);
        self.paint_venn_sets(&painter, &layout, &palette);
        self.paint_sections(&painter, &layout, &palette);
        self.paint_items(ui, &painter, &layout, hovered_item, &palette);

        if let Some(id) = hovered_item {
            if let Some(pl) = layout.placed.iter().find(|p| p.id == id) {
                let srect = self.world_rect_to_screen(pl.rect);
                self.paint_pdf_page_picker(ui, id, srect, &palette);
            }
        }

        self.mini_menu(ui.ctx(), rect, Some(layout.bounds));
        self.action_menu(ui.ctx(), &palette);
    }

    /// Lower-left canvas mini menu (shared chrome): ⛶ full-screen toggle +
    /// zoom controls when the shared camera is in charge (`fit_bounds` set).
    pub(crate) fn mini_menu(&mut self, ctx: &egui::Context, rect: Rect, fit_bounds: Option<Rect>) {
        use atlas_shell::widgets::{canvas_mini_menu, MiniMenuAction, MiniMenuModel};
        let action = canvas_mini_menu(
            ctx,
            "slate",
            rect,
            MiniMenuModel {
                zoom_pct: fit_bounds.map(|_| self.tab().cam.z * 100.0),
                fullscreen: self.tab().chrome.canvas_fullscreen,
            },
        );
        match action {
            Some(MiniMenuAction::ZoomOut) => self.zoom_at(rect.center(), 1.0 / 1.2),
            Some(MiniMenuAction::ZoomReset) => {
                let f = 1.0 / self.tab().cam.z;
                self.zoom_at(rect.center(), f);
            }
            Some(MiniMenuAction::ZoomIn) => self.zoom_at(rect.center(), 1.2),
            Some(MiniMenuAction::Fit) => {
                if let Some(bounds) = fit_bounds {
                    self.fit_view(bounds);
                }
            }
            Some(MiniMenuAction::ToggleFullscreen) => self.toggle_canvas_fullscreen(),
            None => {}
        }
    }

    /// Cover Flow home — recent workbooks (same shared `HomeScreen` as Atlas).
    pub(crate) fn home_screen(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        match self.home.show(ui, &palette, &self.recents) {
            Some(atlas_shell::home::HomeScreenAction::New) => self.home_new_workspace(),
            Some(atlas_shell::home::HomeScreenAction::Open(path)) => {
                if path.is_file() {
                    self.open_doc_at(path);
                } else if atlas_shell::home::is_synthetic_cover_path(&path) {
                    self.home_new_workspace();
                } else {
                    self.toast("That workbook is no longer available");
                    self.recents.remove_missing();
                    self.recents.save("slate");
                }
            }
            None => {}
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let palette = self.palette();
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(rect.height() * 0.38);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Open project…")
                                .size(16.0)
                                .color(palette.ink),
                        )
                        .min_size(egui::vec2(200.0, 40.0)),
                    )
                    .clicked()
                {
                    self.open_doc_dialog();
                }
            });
        });
    }

    // ----- painting ------------------------------------------------------------

    pub(crate) fn paint_dot_grid(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        palette: &atlas_shell::theme::Palette,
        alpha: f32,
    ) {
        if alpha <= 0.001 {
            return;
        }
        let dot = palette.grid_dot.gamma_multiply(alpha);
        let z = self.tab().cam.z;
        let step_w = 64.0;
        let step = step_w * z;
        if step < 9.0 {
            return;
        }
        let origin = self.world_to_screen(Pos2::ZERO);
        let x0 = origin.x + ((rect.left() - origin.x) / step).floor() * step;
        let y0 = origin.y + ((rect.top() - origin.y) / step).floor() * step;
        let mut y = y0;
        while y < rect.bottom() {
            let mut x = x0;
            while x < rect.right() {
                painter.circle_filled(Pos2::new(x, y), 1.0, dot);
                x += step;
            }
            y += step;
        }
    }

    fn paint_venn_sets(
        &self,
        painter: &egui::Painter,
        layout: &Layout,
        _palette: &atlas_shell::theme::Palette,
    ) {
        let z = self.tab().cam.z;
        for (_, name, color, c) in &layout.venn_sets {
            let center = self.world_to_screen(Pos2::new(c.x, c.y));
            let r = c.r * z;
            let accent = Color32::from_rgb(color[0], color[1], color[2]);
            painter.circle_filled(center, r, accent.gamma_multiply(0.055));
            painter.circle_stroke(center, r, Stroke::new(2.0, accent.gamma_multiply(0.8)));
            painter.text(
                center - Vec2::new(0.0, r + 12.0),
                Align2::CENTER_BOTTOM,
                name,
                FontId::proportional((13.0 * z.max(0.8)).clamp(11.0, 20.0)),
                accent,
            );
        }
    }

    fn paint_sections(
        &self,
        painter: &egui::Painter,
        layout: &Layout,
        palette: &atlas_shell::theme::Palette,
    ) {
        let z = self.tab().cam.z;
        for s in &layout.sections {
            let pos = self.world_to_screen(s.header_pos);
            let mut x = pos.x;
            for (name, color) in &s.chips {
                let accent = Color32::from_rgb(color[0], color[1], color[2]);
                painter.circle_filled(Pos2::new(x + 4.0, pos.y + 8.0), 4.0 * z.max(0.6), accent);
                x += 12.0 * z.max(0.6);
                let _ = name;
            }
            painter.text(
                Pos2::new(x + 4.0, pos.y + 8.0),
                Align2::LEFT_CENTER,
                &s.label,
                FontId::proportional((13.0 * z).clamp(10.0, 22.0)),
                palette.ink,
            );
        }
    }

    fn paint_items(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        layout: &Layout,
        hovered: Option<ItemId>,
        palette: &atlas_shell::theme::Palette,
    ) {
        let z = self.tab().cam.z;
        let visible = self.canvas_rect.expand(80.0);
        let ppp = ui.ctx().pixels_per_point();

        for pl in &layout.placed {
            let srect = self.world_rect_to_screen(pl.rect);
            if !visible.intersects(srect) {
                continue;
            }
            let Some(item) = self.doc().item(pl.id) else {
                continue;
            };
            let name = item.file_name.clone();
            let missing = link_status(item) == LinkStatus::Missing;
            let selected = self.selection.contains(&pl.id);
            let is_hovered = hovered == Some(pl.id);

            // Best resident texture for the on-screen size: full-res preview
            // when loaded, thumbnail meanwhile (lazy upgrade, never blocks).
            let desired_px = srect.width().max(srect.height()) * ppp;
            let tex = self.item_texture(pl.id, desired_px);

            match pl.circle_r {
                Some(r_world) => {
                    let center = srect.center();
                    let r = r_world * z;
                    match &tex {
                        Some(t) => circle_image(painter, t, center, r),
                        None => {
                            painter.circle_filled(center, r, palette.thumb_bg);
                        }
                    }
                    let ring = if selected {
                        Stroke::new(2.5, palette.select)
                    } else if is_hovered {
                        Stroke::new(1.5, palette.ink.gamma_multiply(0.7))
                    } else {
                        Stroke::new(1.0, palette.border_strong)
                    };
                    painter.circle_stroke(center, r, ring);
                    if missing {
                        painter.circle_filled(
                            center + Vec2::new(r * 0.6, -r * 0.6),
                            (4.0 * z).clamp(3.0, 7.0),
                            Color32::from_rgb(0xe0, 0x6c, 0x5c),
                        );
                    }
                }
                None => {
                    let fill = if is_hovered {
                        palette.card_hover
                    } else {
                        palette.card
                    };
                    painter.rect_filled(srect, CornerRadius::same(4), fill);
                    let stroke = if selected {
                        Stroke::new(2.0, palette.select)
                    } else {
                        Stroke::new(1.0, palette.border)
                    };
                    painter.rect_stroke(srect, CornerRadius::same(4), stroke, StrokeKind::Inside);

                    let pad = CARD_PAD * z;
                    let label_h = if z > 0.45 { 15.0 * z } else { 0.0 };
                    let thumb_rect = Rect::from_min_max(
                        srect.min + Vec2::splat(pad),
                        Pos2::new(srect.max.x - pad, srect.max.y - pad - label_h),
                    );
                    match &tex {
                        Some(t) => {
                            let size = t.size_vec2();
                            let scale =
                                (thumb_rect.width() / size.x).min(thumb_rect.height() / size.y);
                            let draw = Rect::from_center_size(thumb_rect.center(), size * scale);
                            painter.image(
                                t.id(),
                                draw,
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        None => {
                            painter.rect_filled(
                                thumb_rect,
                                CornerRadius::same(3),
                                palette.thumb_bg,
                            );
                            let ext = std::path::Path::new(&name)
                                .extension()
                                .map(|e| e.to_string_lossy().to_uppercase())
                                .unwrap_or_default();
                            painter.text(
                                thumb_rect.center(),
                                Align2::CENTER_CENTER,
                                ext,
                                FontId::proportional((12.0 * z).clamp(9.0, 18.0)),
                                palette.sub,
                            );
                        }
                    }
                    if label_h > 0.0 {
                        painter.text(
                            Pos2::new(srect.center().x, srect.max.y - pad),
                            Align2::CENTER_BOTTOM,
                            atlas_shell::widgets::trunc(&name, 20),
                            FontId::proportional((10.5 * z).clamp(8.0, 14.0)),
                            palette.sub,
                        );
                    }
                    if missing {
                        painter.circle_filled(
                            srect.right_top() + Vec2::new(-8.0, 8.0),
                            4.0,
                            Color32::from_rgb(0xe0, 0x6c, 0x5c),
                        );
                    }
                }
            }
        }

        if self
            .textures
            .values()
            .any(|t| matches!(t, ThumbState::Pending))
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    // ----- context menu -----------------------------------------------------------

    fn action_menu(&mut self, ctx: &egui::Context, palette: &atlas_shell::theme::Palette) {
        let Some((item_id, pos)) = self.menu else {
            return;
        };
        let targets = self.action_targets(item_id);
        type MenuGroup = (slate_doc::GroupId, String, Vec<(TagId, String, [u8; 3])>);
        let groups: Vec<MenuGroup> = self
            .doc()
            .groups
            .iter()
            .map(|g| {
                (
                    g.id,
                    g.name.clone(),
                    g.tags
                        .iter()
                        .map(|t| (t.id, t.name.clone(), t.color))
                        .collect(),
                )
            })
            .collect();

        let mut close = false;
        let mut dismiss = false;
        egui::Area::new(egui::Id::new("slate_action_menu"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(190.0);
                    ui.label(
                        egui::RichText::new(format!("{} file(s)", targets.len()))
                            .small()
                            .color(palette.sub),
                    );
                    ui.separator();
                    if groups.is_empty() {
                        ui.label(
                            egui::RichText::new("No tags yet — create groups in the Tags panel")
                                .small()
                                .color(palette.sub),
                        );
                    }
                    for (group_id, group_name, tags) in &groups {
                        ui.label(
                            egui::RichText::new(group_name)
                                .small()
                                .strong()
                                .color(palette.ink),
                        );
                        for (tag_id, name, color) in tags {
                            let all_have = targets.iter().all(|t| {
                                self.doc()
                                    .item(*t)
                                    .map(|it| it.assignments.get(group_id) == Some(tag_id))
                                    .unwrap_or(false)
                            });
                            let accent = Color32::from_rgb(color[0], color[1], color[2]);
                            let label = egui::RichText::new(format!(
                                "{} {}",
                                if all_have { "◉" } else { "○" },
                                name
                            ))
                            .color(accent);
                            if ui.selectable_label(false, label).clicked() {
                                if all_have {
                                    self.unassign_group(&targets, *group_id);
                                } else {
                                    self.assign_tag(&targets, *tag_id);
                                }
                                // Keep the menu open: multi-tag assignment in
                                // one right-click instance.
                            }
                        }
                        ui.add_space(2.0);
                    }
                    ui.separator();
                    let pdf_targets: Vec<ItemId> = targets
                        .iter()
                        .copied()
                        .filter(|id| {
                            self.doc()
                                .item(*id)
                                .map(|it| {
                                    slate_doc::media_kind(&it.path) == slate_doc::MediaKind::Pdf
                                })
                                .unwrap_or(false)
                        })
                        .collect();
                    if pdf_targets.len() == 1 && ui.button("Explode PDF into pages…").clicked() {
                        self.explode_pdf(pdf_targets[0]);
                        close = true;
                    }
                    if ui.button("Place on board").clicked() {
                        let center = self.tab().cam.offset.to_pos2();
                        self.place_items_on_board(&targets, center);
                        self.doc_mut().view.active_view = ViewKind::Board;
                        close = true;
                    }
                    if ui.button("Remove from workbook").clicked() {
                        for t in &targets {
                            self.doc_mut().remove_item(*t);
                            self.selection.remove(t);
                        }
                        close = true;
                    }
                    if ui.button("Done").clicked() {
                        close = true;
                    }
                });
            });
        // Dismiss when clicking elsewhere.
        ctx.input(|i| {
            if i.pointer.any_pressed() {
                if let Some(p) = i.pointer.interact_pos() {
                    let near = Rect::from_min_size(pos, Vec2::new(230.0, 420.0)).expand(8.0);
                    if !near.contains(p) {
                        dismiss = true;
                    }
                }
            }
        });
        if close || dismiss {
            self.menu = None;
        }
    }
}

/// Paint a texture cropped to a circle (triangle-fan mesh with circular UVs).
/// Non-square thumbnails are center-cropped.
fn circle_image(painter: &egui::Painter, tex: &egui::TextureHandle, center: Pos2, r: f32) {
    let size = tex.size_vec2();
    let (uv_rx, uv_ry) = if size.x >= size.y {
        (0.5 * size.y / size.x, 0.5)
    } else {
        (0.5, 0.5 * size.x / size.y)
    };
    let n = 40usize;
    let mut mesh = egui::Mesh::with_texture(tex.id());
    mesh.vertices.push(egui::epaint::Vertex {
        pos: center,
        uv: Pos2::new(0.5, 0.5),
        color: Color32::WHITE,
    });
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        let (sin, cos) = a.sin_cos();
        mesh.vertices.push(egui::epaint::Vertex {
            pos: center + Vec2::new(cos * r, sin * r),
            uv: Pos2::new(0.5 + cos * uv_rx, 0.5 + sin * uv_ry),
            color: Color32::WHITE,
        });
    }
    for i in 1..=n {
        mesh.indices
            .extend_from_slice(&[0, i as u32, (i + 1) as u32]);
    }
    painter.add(mesh);
}
