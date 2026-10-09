//! Frame size dialog and spreadsheet editing.

use super::*;

impl SlateApp {
    // ----- overlays ---------------------------------------------------------------

    /// Manual frame dimensions entry (opened from Frame → Custom…).
    pub(super) fn frame_custom_dialog(&mut self, ctx: &egui::Context, canvas: Rect) {
        if self.board_frame_custom.is_none() {
            return;
        }
        let palette = self.palette();
        let mut close = false;
        let mut apply = false;
        let mut w_buf = self.board_frame_custom.as_ref().unwrap().w.clone();
        let mut h_buf = self.board_frame_custom.as_ref().unwrap().h.clone();

        egui::Area::new(egui::Id::new("slate_frame_custom"))
            .fixed_pos(Pos2::new(canvas.center().x - 110.0, canvas.min.y + 52.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(palette.card)
                    .show(ui, |ui| {
                        ui.set_min_width(200.0);
                        ui.label(egui::RichText::new("Custom frame size").strong());
                        ui.label(
                            egui::RichText::new("World units (72 pt per inch)")
                                .small()
                                .color(palette.sub),
                        );
                        ui.horizontal(|ui| {
                            ui.label("W");
                            ui.add(
                                egui::TextEdit::singleline(&mut w_buf)
                                    .desired_width(72.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                            ui.label("H");
                            ui.add(
                                egui::TextEdit::singleline(&mut h_buf)
                                    .desired_width(72.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                apply = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
            });

        if let Some(draft) = self.board_frame_custom.as_mut() {
            draft.w.clone_from(&w_buf);
            draft.h.clone_from(&h_buf);
        }

        if apply {
            if let (Ok(w), Ok(h)) = (w_buf.trim().parse::<f32>(), h_buf.trim().parse::<f32>()) {
                if w >= MIN_DRAW && h >= MIN_DRAW {
                    self.board_frame_preset = FramePreset::Custom { w, h };
                    self.board_tool = BoardTool::Frame;
                    close = true;
                } else {
                    self.toast("Frame width and height must be at least 8 world units.");
                }
            } else {
                self.toast("Enter numeric width and height.");
            }
        }
        if close {
            self.board_frame_custom = None;
        }
    }

    pub(crate) fn swap_frame_order(&mut self, a: NodeId, b: NodeId) {
        let get = |app: &Self, id: NodeId| -> Option<u32> {
            match app.doc().scene.node(id).map(|n| &n.kind) {
                Some(NodeKind::Frame(f)) => Some(f.order),
                _ => None,
            }
        };
        let (Some(oa), Some(ob)) = (get(self, a), get(self, b)) else {
            return;
        };
        self.patch_nodes(&[a], |n| {
            if let NodeKind::Frame(f) = &mut n.kind {
                f.order = ob;
            }
        });
        self.last_board_edit = None; // keep the two patches from coalescing
        self.patch_nodes(&[b], |n| {
            if let NodeKind::Frame(f) = &mut n.kind {
                f.order = oa;
            }
        });
        self.last_board_edit = None;
    }

    pub(super) fn sheet_add_hit(&self, p: Pos2) -> Option<SheetHit> {
        self.sheet_hits
            .iter()
            .find(|hit| hit.add && hit.rect.contains(p))
            .cloned()
    }

    pub(crate) fn item_path(&self, item: ItemId) -> Option<PathBuf> {
        self.doc().item(item).map(|it| it.path.clone())
    }

    pub(super) fn push_sheet_mark(&mut self, mark: SheetMark) {
        let tab = self.tab_mut();
        tab.edits.push(BoardMark::Sheet(mark));
        tab.edit_redo.clear();
    }

    pub(super) fn open_sheet_cell(&mut self, hit: SheetHit) {
        self.commit_text_edit();
        self.commit_sheet_edit();
        let text = self
            .sheets
            .get(&hit.item)
            .and_then(|grid| grid.as_ref())
            .and_then(|rows| rows.get(hit.row))
            .and_then(|row| row.get(hit.col))
            .map(|cell| cell.text.clone())
            .unwrap_or_default();
        let font_px = canvas_scale::px(10.0, self.board_xf().z);
        self.board_sel.clear();
        self.board_sel.insert(hit.node);
        self.sheet_edit = Some(SheetEdit {
            node: hit.node,
            item: hit.item,
            row: hit.row,
            col: hit.col,
            buf: text.clone(),
            origin: text,
            fresh: false,
            screen: hit.rect,
            font_px,
        });
    }

    pub(super) fn scroll_sheet(&mut self, world: Pos2, scroll_px: f32, horizontal: bool) -> bool {
        let Some(id) = self.board_pick_node(world.x, world.y) else {
            return false;
        };
        let Some((item, view)) = self.doc().scene.node(id).and_then(|node| {
            let NodeKind::Image(img) = &node.kind else {
                return None;
            };
            Some((img.item, Vec2::new(node.rect.w, node.rect.h)))
        }) else {
            return false;
        };
        let Some((cols, nrows)) = self.sheets.get(&item).and_then(|grid| {
            grid.as_ref().map(|rows| {
                (
                    rows.iter().map(|row| row.len()).max().unwrap_or(1),
                    rows.len(),
                )
            })
        }) else {
            return false;
        };
        if nrows == 0 || self.sheet_open != Some(id) {
            return false;
        }
        let current = self.sheet_scroll.get(&id).copied().unwrap_or(Vec2::ZERO);
        let z = self.tab().cam.z.max(0.01);
        let delta = -scroll_px / z;
        let mut next = current;
        if horizontal {
            next.x += delta;
        } else {
            next.y += delta;
        }
        let (custom_cols, custom_rows) = self.sheet_sizes(id);
        let fitted = sheet_tracks(view, cols, nrows, &custom_cols, &custom_rows, next);
        let overflows = if horizontal {
            fitted.max_scroll.x > 0.5
        } else {
            fitted.max_scroll.y > 0.5
        };
        if !overflows {
            return false;
        }
        if fitted.scroll == Vec2::ZERO {
            self.sheet_scroll.remove(&id);
        } else {
            self.sheet_scroll.insert(id, fitted.scroll);
        }
        true
    }

    pub(super) fn reveal_sheet_cell(&mut self, node: NodeId, row: usize, col: usize) {
        let Some(rect) = self.doc().scene.node(node).map(|n| n.rect) else {
            return;
        };
        let mut scroll = self.sheet_scroll.get(&node).copied().unwrap_or(Vec2::ZERO);
        let x0 = col as f32 * SHEET_COL_WORLD;
        let y0 = row as f32 * SHEET_ROW_WORLD;
        if x0 < scroll.x {
            scroll.x = x0;
        } else if x0 + SHEET_COL_WORLD > scroll.x + rect.w {
            scroll.x = (x0 + SHEET_COL_WORLD - rect.w).max(0.0);
        }
        if y0 < scroll.y {
            scroll.y = y0;
        } else if y0 + SHEET_ROW_WORLD > scroll.y + rect.h {
            scroll.y = (y0 + SHEET_ROW_WORLD - rect.h).max(0.0);
        }
        if scroll == Vec2::ZERO {
            self.sheet_scroll.remove(&node);
        } else {
            self.sheet_scroll.insert(node, scroll);
        }
    }

    pub(super) fn add_sheet_column(&mut self, hit: SheetHit) {
        if self.refuse_read_only_edit() {
            return;
        }
        if self.sheet_open != Some(hit.node) {
            return;
        }
        self.commit_text_edit();
        self.commit_sheet_edit();
        let Some(path) = self.item_path(hit.item) else {
            return;
        };
        if atlas_core::cloud::is_dehydrated(&path) {
            self.toast("That spreadsheet is online-only");
            return;
        }
        let loaded = atlas_core::table::read_sheet_card(&path);
        let grid = self.sheets.entry(hit.item).or_insert(loaded);
        let Some(rows) = grid.as_mut() else {
            self.toast("Couldn't read that spreadsheet");
            return;
        };
        let col = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        if col >= atlas_core::table::SHEET_CARD_COLS {
            self.toast("This card is already full of columns");
            return;
        }
        if rows.is_empty() {
            rows.push(Vec::new());
        }
        rows[0].push(atlas_core::office::SheetCell {
            text: String::new(),
            fill: None,
        });
        self.sheet_dirty = true;
        self.reveal_sheet_cell(hit.node, 0, col);
        let font_px = canvas_scale::px(10.0, self.board_xf().z);
        self.board_sel.clear();
        self.board_sel.insert(hit.node);
        self.sheet_edit = Some(SheetEdit {
            node: hit.node,
            item: hit.item,
            row: 0,
            col,
            buf: String::new(),
            origin: String::new(),
            fresh: false,
            screen: hit.rect,
            font_px,
        });
    }

    pub(super) fn sheet_sizes(&self, node: NodeId) -> (Vec<f32>, Vec<f32>) {
        if let Some(resize) = &self.sheet_resize {
            if resize.node == node {
                return (resize.cols.clone(), resize.rows.clone());
            }
        }
        match self.doc().scene.node(node).map(|n| &n.kind) {
            Some(NodeKind::Image(img)) => (img.sheet.cols.clone(), img.sheet.rows.clone()),
            _ => (Vec::new(), Vec::new()),
        }
    }

    pub(super) fn sheet_node(&self, id: NodeId) -> bool {
        self.sheets.iter().any(|(item, grid)| {
            grid.as_ref().is_some_and(|rows| !rows.is_empty())
                && self.doc().scene.node(id).is_some_and(|n| match &n.kind {
                    NodeKind::Image(img) => img.item == *item,
                    _ => false,
                })
        })
    }

    /// Double-click opens the spreadsheet. Until then the card is a picture:
    /// the wheel zooms the board and cells do not take clicks.
    pub(super) fn enter_sheet(&mut self, node: NodeId) {
        if self.sheet_open == Some(node) {
            return;
        }
        if self.sheet_dirty {
            self.sheet_prompt = true;
            return;
        }
        self.contents_blur();
        self.sheet_edit = None;
        self.sheet_open = Some(node);
        self.sheet_dirty = false;
        let item = self.doc().scene.node(node).and_then(|n| match &n.kind {
            NodeKind::Image(img) => Some(img.item),
            _ => None,
        });
        self.sheet_baseline = item.and_then(|item| self.sheets.get(&item).cloned().flatten());
        self.board_sel = std::iter::once(node).collect();
    }

    pub(super) fn close_sheet(&mut self) {
        self.sheet_edit = None;
        self.sheet_open = None;
        self.sheet_dirty = false;
        self.sheet_baseline = None;
        self.sheet_prompt = false;
        self.sheet_resize = None;
    }

    pub(super) fn discard_sheet(&mut self) {
        if let Some(node) = self.sheet_open {
            if let Some(NodeKind::Image(img)) = self.doc().scene.node(node).map(|n| &n.kind) {
                let item = img.item;
                self.sheets.remove(&item);
            }
        }
        self.close_sheet();
    }

    /// Enter keeps the typed value on the card. The file changes on Save.
    pub(crate) fn commit_sheet_edit(&mut self) {
        let Some(edit) = self.sheet_edit.take() else {
            return;
        };
        if edit.buf == edit.origin {
            return;
        }
        let loaded = self
            .item_path(edit.item)
            .as_deref()
            .and_then(atlas_core::table::read_sheet_card);
        let grid = self.sheets.entry(edit.item).or_insert(loaded);
        let Some(rows) = grid.as_mut() else {
            return;
        };
        while rows.len() <= edit.row {
            rows.push(Vec::new());
        }
        while rows[edit.row].len() <= edit.col {
            rows[edit.row].push(atlas_core::office::SheetCell {
                text: String::new(),
                fill: None,
            });
        }
        rows[edit.row][edit.col].text = edit.buf;
        self.sheet_dirty = true;
    }

    pub(super) fn save_open_sheet(&mut self) -> bool {
        self.commit_sheet_edit();
        if !self.sheet_dirty {
            return true;
        }
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(node) = self.sheet_open else {
            return true;
        };
        let Some(item) = self.doc().scene.node(node).and_then(|n| match &n.kind {
            NodeKind::Image(img) => Some(img.item),
            _ => None,
        }) else {
            return false;
        };
        let Some(path) = self.item_path(item) else {
            return false;
        };
        if atlas_core::cloud::is_dehydrated(&path) {
            self.toast("That spreadsheet is online-only");
            return false;
        }
        let Some(grid) = self.sheets.get(&item).cloned().flatten() else {
            return false;
        };
        let base = self.sheet_baseline.clone().unwrap_or_default();
        let mut failed = false;
        let rows = grid.len().max(base.len());
        for row in 0..rows {
            let cols = grid
                .get(row)
                .map(|r| r.len())
                .unwrap_or(0)
                .max(base.get(row).map(|r| r.len()).unwrap_or(0));
            for col in 0..cols {
                let now = grid
                    .get(row)
                    .and_then(|r| r.get(col))
                    .map(|c| c.text.as_str())
                    .unwrap_or("");
                let was = base
                    .get(row)
                    .and_then(|r| r.get(col))
                    .map(|c| c.text.as_str())
                    .unwrap_or("");
                if now == was {
                    continue;
                }
                match atlas_core::table::write_sheet_cell(&path, row, col, now) {
                    Some(prior) => self.push_sheet_mark(SheetMark {
                        item,
                        path: path.clone(),
                        row,
                        col,
                        prior,
                    }),
                    None => failed = true,
                }
            }
        }
        if failed {
            self.toast("Couldn't write that spreadsheet");
            return false;
        }
        self.sheet_baseline = Some(grid);
        self.sheet_dirty = false;
        self.toast("Spreadsheet saved");
        true
    }
}
