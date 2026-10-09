//! Snippet cards and spreadsheet cards.

use super::*;

// ---------- painting ----------

impl SlateApp {
    /// Cached excerpt for text-file snippet cards (same clamping as the
    /// artifact's `read_snippet`, so board and export show identical text).
    pub(crate) fn snippet_for(&mut self, item: ItemId, path: &std::path::Path) -> Option<String> {
        let _span = atlas_core::session_log::span("slate.snippet");
        if let Some(cached) = self.snippets.get(&item) {
            return cached.clone();
        }
        let path =
            slate_doc::scene::resolve_source(self.tab().path.as_deref(), &path.to_string_lossy());
        let snippet = slate_artifact::read_snippet(&path);
        self.snippets.insert(item, snippet.clone());
        snippet
    }

    pub(super) fn sheet_for(
        &mut self,
        item: ItemId,
        path: &std::path::Path,
    ) -> Option<Vec<Vec<atlas_core::office::SheetCell>>> {
        let _span = atlas_core::session_log::span("slate.sheet");
        self.sheets
            .entry(item)
            .or_insert_with(|| atlas_core::table::read_sheet_card(path))
            .clone()
    }

    /// Paper-like card with the file's opening lines — the board twin of the
    /// artifact's `.textcard`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_text_snippet_card(
        &mut self,
        painter: &egui::Painter,
        outline: &[Pos2],
        srect: Rect,
        item: ItemId,
        path: &std::path::Path,
        corner: slate_doc::scene::Corner,
        pointer: Option<Pos2>,
        z: f32,
    ) {
        let palette = self.palette();
        painter.add(egui::Shape::convex_polygon(
            outline.to_vec(),
            palette.card,
            EStroke::NONE,
        ));
        if self.text_doc_edit.as_ref().is_some_and(|e| e.item == item) {
            // The inline editor paints the words and the caret.
            return;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match self.snippet_for(item, path) {
            Some(snippet) => {
                let inner = text_card_inner(srect, corner, z);
                let clip = painter.with_clip_rect(inner);
                let body = canvas_scale::px(TEXT_CARD_BODY_PX, z);
                if canvas_text::legible(body) {
                    // World wrap is the card's inner width. The browser export
                    // wraps the same excerpt with CSS pre-wrap; see canvas_text.
                    let galley = zoom_text_galley(
                        clip.ctx(),
                        &snippet,
                        FontId::monospace(TEXT_CARD_BODY_PX),
                        inner.width() / z.max(1.0e-4),
                        TextAlign::Left,
                        palette.ink,
                        z,
                    );
                    clip.galley(inner.min, galley, palette.ink);
                }
                let caption = canvas_scale::px(8.5, z);
                if pointer.is_some_and(|p| srect.contains(p)) && canvas_text::legible(caption) {
                    canvas_text::text(
                        &clip,
                        Pos2::new(inner.min.x, inner.max.y),
                        Align2::LEFT_BOTTOM,
                        atlas_shell::widgets::trunc(&name, 24),
                        FontId::proportional(caption),
                        palette.sub,
                    );
                }
            }
            None => {
                let size = canvas_scale::px(11.0, z);
                if canvas_text::legible(size) {
                    canvas_text::text(
                        painter,
                        srect.center(),
                        Align2::CENTER_CENTER,
                        atlas_shell::widgets::trunc(&name, 18),
                        FontId::proportional(size),
                        palette.sub,
                    );
                }
            }
        }
    }

    /// CSV / Excel card. The grid is the card: fixed cell size, full bleed,
    /// hairline dividers. A larger card shows more cells; the rest scroll.
    /// The file name appears on hover. + sits beside the header, also on hover.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_sheet_card(
        &mut self,
        painter: &egui::Painter,
        outline: &[Pos2],
        srect: Rect,
        node: NodeId,
        item: ItemId,
        path: &std::path::Path,
        rows: &[Vec<atlas_core::office::SheetCell>],
        pointer: Option<Pos2>,
        z: f32,
    ) {
        let palette = self.palette();
        painter.add(egui::Shape::convex_polygon(
            outline.to_vec(),
            palette.card,
            EStroke::NONE,
        ));
        let grid = srect;
        if grid.width() < 4.0 || grid.height() < 4.0 || rows.is_empty() {
            return;
        }
        let cols = rows.iter().map(|row| row.len()).max().unwrap_or(1).max(1);
        let zoom = z.max(0.01);
        let view = Vec2::new(grid.width() / zoom, grid.height() / zoom);
        let prior = self.sheet_scroll.get(&node).copied().unwrap_or(Vec2::ZERO);
        let (custom_cols, custom_rows) = self.sheet_sizes(node);
        let tracks = sheet_tracks(view, cols, rows.len(), &custom_cols, &custom_rows, prior);
        if tracks.scroll == Vec2::ZERO {
            self.sheet_scroll.remove(&node);
        } else {
            self.sheet_scroll.insert(node, tracks.scroll);
        }
        let col_px: Vec<f32> = tracks
            .cols
            .iter()
            .map(|w| canvas_scale::px(*w, zoom))
            .collect();
        let row_px: Vec<f32> = tracks
            .rows
            .iter()
            .map(|h| canvas_scale::px(*h, zoom))
            .collect();
        let mut col_x = Vec::with_capacity(col_px.len());
        let mut row_y = Vec::with_capacity(row_px.len());
        let mut acc = 0.0;
        for w in &col_px {
            col_x.push(acc);
            acc += *w;
        }
        acc = 0.0;
        for h in &row_px {
            row_y.push(acc);
            acc += *h;
        }
        let scroll_x = canvas_scale::px(tracks.scroll.x, zoom);
        let scroll_y = canvas_scale::px(tracks.scroll.y, zoom);
        let open = self.sheet_open == Some(node);
        let hair = EStroke::new(canvas_scale::px(0.75, zoom), palette.line);
        let clip = painter.with_clip_rect(grid);
        let body = canvas_scale::px(10.0, zoom).min(row_px.first().copied().unwrap_or(12.0) * 0.62);
        let editing = self
            .sheet_edit
            .as_ref()
            .filter(|edit| edit.node == node)
            .map(|edit| (edit.row, edit.col));
        let first_row = row_y
            .iter()
            .enumerate()
            .position(|(i, y)| y + row_px[i] > scroll_y)
            .unwrap_or(rows.len())
            .min(rows.len());
        let last_row = row_y
            .iter()
            .position(|y| *y >= scroll_y + grid.height())
            .unwrap_or(rows.len())
            .min(rows.len());
        let first_col = col_x
            .iter()
            .enumerate()
            .position(|(i, x)| x + col_px[i] > scroll_x)
            .unwrap_or(cols)
            .min(cols);
        let last_col = col_x
            .iter()
            .position(|x| *x >= scroll_x + grid.width())
            .unwrap_or(cols)
            .min(cols);
        for ri in first_row..last_row {
            let row = &rows[ri];
            let y = grid.min.y + row_y[ri] - scroll_y;
            let row_h = row_px[ri];
            for ci in first_col..last_col {
                let x = grid.min.x + col_x[ci] - scroll_x;
                let col_w = col_px[ci];
                let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(col_w, row_h));
                let visible = rect.intersect(grid);
                if visible.width() < 0.5 || visible.height() < 0.5 {
                    continue;
                }
                let cell = row.get(ci);
                let authored = cell.and_then(|cell| cell.fill);
                let bg = if let Some(rgb) = authored {
                    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
                } else if ri == 0 {
                    palette.thumb_bg
                } else {
                    Color32::TRANSPARENT
                };
                if bg != Color32::TRANSPARENT {
                    clip.rect_filled(rect, 0.0, bg);
                }
                self.sheet_hits.push(SheetHit {
                    node,
                    item,
                    row: ri,
                    col: ci,
                    rect: visible,
                    add: false,
                });
                if editing == Some((ri, ci)) || !canvas_text::legible(body) {
                    continue;
                }
                let Some(text) = cell
                    .map(|cell| cell.text.as_str())
                    .filter(|t| !t.is_empty())
                else {
                    continue;
                };
                let ink = authored
                    .map(atlas_core::office::SheetCell::ink_on)
                    .map(|rgb| Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
                    .unwrap_or(palette.ink);
                let inset = canvas_scale::px(3.0, zoom);
                let budget = ((col_w - inset * 2.0) / body.max(1.0)).floor() as usize;
                canvas_text::text(
                    &clip,
                    Pos2::new(rect.left() + inset, rect.center().y),
                    Align2::LEFT_CENTER,
                    atlas_shell::widgets::trunc(text, budget.max(1)),
                    FontId::proportional(body),
                    ink,
                );
            }
        }
        for (ri, &ry) in row_y.iter().enumerate().take(last_row).skip(first_row) {
            if ri == 0 {
                continue;
            }
            let y = grid.min.y + ry - scroll_y;
            if y > grid.min.y && y < grid.max.y {
                clip.line_segment([Pos2::new(grid.min.x, y), Pos2::new(grid.max.x, y)], hair);
                if open {
                    let band = canvas_scale::px(5.0, zoom).max(3.0);
                    self.sheet_grips.push(SheetGrip {
                        node,
                        rect: Rect::from_center_size(
                            Pos2::new(grid.center().x, y),
                            Vec2::new(grid.width(), band),
                        ),
                        col: None,
                        row: Some(ri - 1),
                    });
                }
            }
        }
        for (ci, &cx) in col_x.iter().enumerate().take(last_col).skip(first_col) {
            if ci == 0 {
                continue;
            }
            let x = grid.min.x + cx - scroll_x;
            if x > grid.min.x && x < grid.max.x {
                clip.line_segment([Pos2::new(x, grid.min.y), Pos2::new(x, grid.max.y)], hair);
                if open {
                    let band = canvas_scale::px(5.0, zoom).max(3.0);
                    self.sheet_grips.push(SheetGrip {
                        node,
                        rect: Rect::from_center_size(
                            Pos2::new(x, grid.center().y),
                            Vec2::new(band, grid.height()),
                        ),
                        col: Some(ci - 1),
                        row: None,
                    });
                }
            }
        }
        if open && tracks.max_scroll.y > 0.5 && pointer.is_some_and(|p| grid.contains(p)) {
            let bar = canvas_scale::px(4.0, zoom);
            if !canvas_scale::too_small(bar) {
                let content_h: f32 = row_px.iter().sum();
                let thumb_h = (grid.height() * grid.height() / content_h.max(grid.height()))
                    .min(grid.height());
                let travel = (grid.height() - thumb_h).max(0.0);
                let t = if tracks.max_scroll.y <= 0.0 {
                    0.0
                } else {
                    tracks.scroll.y / tracks.max_scroll.y
                };
                let thumb = Rect::from_min_size(
                    Pos2::new(grid.max.x - bar, grid.min.y + travel * t),
                    Vec2::new(bar, thumb_h),
                );
                clip.rect_filled(thumb, bar * 0.5, palette.sub.gamma_multiply(0.65));
            }
        }
        if let Some((row, col)) = editing {
            let rect = Rect::from_min_size(
                Pos2::new(
                    grid.min.x + col_x.get(col).copied().unwrap_or(0.0) - scroll_x,
                    grid.min.y + row_y.get(row).copied().unwrap_or(0.0) - scroll_y,
                ),
                Vec2::new(
                    col_px.get(col).copied().unwrap_or(0.0),
                    row_px.get(row).copied().unwrap_or(0.0),
                ),
            );
            if let Some(edit) = self.sheet_edit.as_mut() {
                edit.screen = rect.intersect(grid);
                edit.font_px = body;
            }
        }
        let reach = canvas_scale::px(28.0, zoom);
        let hot = pointer.is_some_and(|p| {
            Rect::from_min_max(grid.min, Pos2::new(grid.max.x + reach, grid.max.y)).contains(p)
        });
        if open && hot && cols < atlas_core::table::SHEET_CARD_COLS {
            let d = canvas_scale::px(13.0, zoom).min(row_px.first().copied().unwrap_or(12.0));
            if !canvas_scale::too_small(d) {
                let center = Pos2::new(
                    grid.max.x + d * 0.95,
                    grid.min.y + row_px.first().copied().unwrap_or(d) * 0.5,
                );
                painter.circle_filled(center, d * 0.5, palette.panel);
                painter.circle_stroke(
                    center,
                    d * 0.5,
                    EStroke::new(canvas_scale::px(0.8, zoom), palette.border),
                );
                let plus = d * 0.62;
                if canvas_text::legible(plus) {
                    canvas_text::text(
                        painter,
                        center,
                        Align2::CENTER_CENTER,
                        "+",
                        FontId::proportional(plus),
                        palette.ink,
                    );
                }
                self.sheet_hits.push(SheetHit {
                    node,
                    item,
                    row: 0,
                    col: cols,
                    rect: Rect::from_center_size(center, Vec2::splat(d)),
                    add: true,
                });
            }
        }
        if pointer.is_some_and(|p| grid.contains(p)) {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            paint_ext_badge(painter, grid, &atlas_shell::widgets::trunc(&name, 24), zoom);
        }
        if open {
            let label = "Save";
            let size = canvas_scale::px(11.0, zoom);
            if canvas_text::legible(size) {
                let color = if self.sheet_dirty {
                    palette.accent
                } else {
                    palette.sub
                };
                let laid = canvas_text::layout_no_wrap(
                    painter,
                    label.into(),
                    FontId::proportional(size),
                    color,
                );
                let pad = canvas_scale::px(6.0, zoom);
                let rect = Rect::from_min_size(
                    Pos2::new(grid.max.x - laid.size().x - pad * 2.0, grid.min.y + pad),
                    laid.size() + Vec2::splat(pad * 2.0),
                );
                painter.rect_filled(rect, pad, palette.panel);
                laid.paint(painter, rect.min + Vec2::splat(pad), color);
                self.sheet_save_hit = Some(rect);
            }
        }
    }
}
