//! Sheet peel, prompt, and resize grips.

use super::*;

impl SlateApp {
    pub(super) fn peel_sheet(&mut self, ui: &egui::Ui, xf: &BoardXf, pointer: Option<Pos2>) {
        let Some(id) = self.sheet_open else {
            return;
        };
        if self.sheet_prompt || self.sheet_resize.is_some() {
            return;
        }
        if !ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            return;
        }
        let Some(p) = pointer else {
            return;
        };
        if self.sheet_save_hit.is_some_and(|r| r.contains(p)) {
            let _ = self.save_open_sheet();
            return;
        }
        if self.sheet_grips.iter().any(|g| g.rect.contains(p)) {
            return;
        }
        if self
            .sheet_edit
            .as_ref()
            .is_some_and(|edit| edit.screen.contains(p))
        {
            return;
        }
        let inside = self
            .doc()
            .scene
            .node(id)
            .is_some_and(|n| xf.rect_w2s(n.rect).contains(p));
        if inside {
            return;
        }
        self.commit_sheet_edit();
        if self.sheet_dirty {
            self.sheet_prompt = true;
        } else {
            self.close_sheet();
        }
    }

    pub(super) fn sheet_prompt_frame(&mut self, ctx: &egui::Context) {
        if !self.sheet_prompt {
            return;
        }
        let Some(choice) = atlas_shell::widgets::confirm_window(
            ctx,
            "Save spreadsheet?",
            "This spreadsheet has unsaved cell changes.",
            "Save",
            "Don't save",
        ) else {
            return;
        };
        use atlas_shell::widgets::ConfirmChoice;
        match choice {
            ConfirmChoice::Primary => {
                if self.save_open_sheet() {
                    self.close_sheet();
                } else {
                    self.sheet_prompt = false;
                }
            }
            ConfirmChoice::Secondary => self.discard_sheet(),
            ConfirmChoice::Cancel => self.sheet_prompt = false,
        }
    }

    pub(super) fn begin_sheet_resize(&mut self, grip: SheetGrip, pointer: Pos2) {
        let (mut cols, mut rows) = self.sheet_sizes(grip.node);
        if cols.is_empty() || rows.is_empty() {
            let Some(rect) = self.doc().scene.node(grip.node).map(|n| n.rect) else {
                return;
            };
            let item = match self.doc().scene.node(grip.node).map(|n| &n.kind) {
                Some(NodeKind::Image(img)) => img.item,
                _ => return,
            };
            let (col_n, row_n) = self
                .sheets
                .get(&item)
                .and_then(|g| g.as_ref())
                .map(|rows| {
                    (
                        rows.iter().map(|r| r.len()).max().unwrap_or(1),
                        rows.len().max(1),
                    )
                })
                .unwrap_or((1, 1));
            let tracks = sheet_tracks(
                Vec2::new(rect.w, rect.h),
                col_n,
                row_n,
                &cols,
                &rows,
                Vec2::ZERO,
            );
            cols = tracks.cols;
            rows = tracks.rows;
        }
        let (start_px, start_size) = if let Some(col) = grip.col {
            (pointer.x, cols.get(col).copied().unwrap_or(SHEET_COL_WORLD))
        } else if let Some(row) = grip.row {
            (pointer.y, rows.get(row).copied().unwrap_or(SHEET_ROW_WORLD))
        } else {
            return;
        };
        self.sheet_resize = Some(SheetResize {
            node: grip.node,
            col: grip.col,
            row: grip.row,
            start_px,
            start_size,
            cols,
            rows,
        });
    }

    pub(super) fn update_sheet_resize(&mut self, pointer: Pos2) {
        let z = self.tab().cam.z.max(0.01);
        let Some(resize) = self.sheet_resize.as_mut() else {
            return;
        };
        if let Some(col) = resize.col {
            let next = (resize.start_size + (pointer.x - resize.start_px) / z).max(MIN_DRAW);
            if let Some(slot) = resize.cols.get_mut(col) {
                *slot = next;
            }
        } else if let Some(row) = resize.row {
            let next = (resize.start_size + (pointer.y - resize.start_px) / z).max(MIN_DRAW);
            if let Some(slot) = resize.rows.get_mut(row) {
                *slot = next;
            }
        }
    }

    pub(super) fn finish_sheet_resize(&mut self) {
        let Some(resize) = self.sheet_resize.take() else {
            return;
        };
        let cols = resize.cols;
        let rows = resize.rows;
        self.patch_nodes(&[resize.node], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.sheet.cols = cols.clone();
                img.sheet.rows = rows.clone();
            }
        });
    }

    pub(super) fn cancel_sheet_edit(&mut self) {
        let Some(edit) = self.sheet_edit.take() else {
            return;
        };
        if !edit.fresh {
            return;
        }
        let mark = match self.tab_mut().edits.pop() {
            Some(BoardMark::Sheet(mark))
                if mark.item == edit.item && mark.row == edit.row && mark.col == edit.col =>
            {
                mark
            }
            Some(other) => {
                self.tab_mut().edits.push(other);
                return;
            }
            None => return,
        };
        if atlas_core::table::revert_sheet_cell(&mark.path, mark.row, mark.col, &mark.prior)
            .is_none()
        {
            self.tab_mut().edits.push(BoardMark::Sheet(mark));
            self.toast("Couldn't change that spreadsheet");
            return;
        }
        self.sheets.remove(&edit.item);
    }

    pub(super) fn sheet_edit_overlay(&mut self, ctx: &egui::Context) {
        let Some((node, row, col, area, font_px, mut buf)) = self.sheet_edit.as_ref().map(|edit| {
            (
                edit.node,
                edit.row,
                edit.col,
                edit.screen,
                edit.font_px,
                edit.buf.clone(),
            )
        }) else {
            return;
        };
        if area.width() < 2.0 || area.height() < 2.0 {
            return;
        }
        let mut commit = false;
        let mut cancel = false;
        let font = FontId::proportional(font_px.max(4.0));
        egui::Area::new(egui::Id::new(("slate_sheet_edit", node.0, row, col)))
            .fixed_pos(area.min)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(area.width());
                ui.set_height(area.height());
                ui.set_clip_rect(area);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut buf)
                        .desired_width(area.width())
                        .frame(false)
                        .clip_text(true)
                        .margin(egui::Margin::symmetric(4, 0))
                        .font(font)
                        .vertical_align(egui::Align::Center),
                );
                let keep_focus = ui.memory(|m| m.focused().is_none_or(|fid| fid == resp.id));
                if keep_focus {
                    resp.request_focus();
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        if let Some(live) = self.sheet_edit.as_mut() {
            live.buf = buf;
        }
        if cancel {
            self.cancel_sheet_edit();
        } else if commit {
            self.commit_sheet_edit();
        }
    }
}
