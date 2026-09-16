//! PDF page picker (hover) and explode-into-pages actions.

pub(crate) mod documents;

use super::{SlateApp, ThumbState};
use atlas_core::thumbs::{cache_key_page, ThumbRequest};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2,
};
use slate_doc::{ItemId, MediaKind};
use std::path::{Path, PathBuf};

use super::THUMB_GENERATION;

/// Thumbnail cache key for an item, accounting for PDF poster page.
pub fn item_thumb_key(item: &slate_doc::SlateItem) -> String {
    if item.pdf_page == 0 {
        item.cache_key.clone()
    } else {
        cache_key_page(
            &item.path.to_string_lossy(),
            item.size,
            item.mtime,
            Some(item.pdf_page),
        )
    }
}

impl SlateApp {
    /// Return resident state immediately; source I/O and conversion run on workers.
    pub(crate) fn resolved_item_preview(
        &mut self,
        item_id: ItemId,
    ) -> Option<(String, PathBuf, u64, Option<u16>)> {
        let item = self.doc().item(item_id)?.clone();
        if slate_doc::media::has_pages(&item.path) {
            self.documents
                .request(&item.path, slate_doc::media::is_powerpoint(&item.path));
        }
        if let Some(preview) = self.documents.ready(&item.path) {
            return Some((
                preview.key(item.pdf_page),
                preview.path.clone(),
                preview.bytes,
                Some(item.pdf_page),
            ));
        }
        Some((
            item_thumb_key(&item),
            item.path,
            item.size,
            (item.pdf_page > 0).then_some(item.pdf_page),
        ))
    }

    pub(crate) fn resolved_item_key(&self, item: &slate_doc::SlateItem) -> String {
        self.documents
            .ready(&item.path)
            .map(|p| p.key(item.pdf_page))
            .unwrap_or_else(|| item_thumb_key(item))
    }

    pub(crate) fn pdf_page_count(&mut self, path: &Path) -> u16 {
        self.documents
            .request(path, slate_doc::media::is_powerpoint(path));
        self.documents.ready(path).map_or(0, |p| p.pages)
    }

    /// Queue a thumbnail for a specific PDF page (hover strip previews).
    pub(crate) fn request_pdf_page_thumb(
        &mut self,
        path: PathBuf,
        size: u64,
        mtime: i64,
        page: u16,
    ) {
        let (key, path, size) = match self.documents.ready(&path) {
            Some(preview) => (preview.key(page), preview.path.clone(), preview.bytes),
            None => (
                cache_key_page(&path.to_string_lossy(), size, mtime, Some(page)),
                path,
                size,
            ),
        };
        if key.is_empty() || self.textures.contains_key(&key) {
            return;
        }
        let slot = self.next_thumb_slot;
        self.next_thumb_slot = self.next_thumb_slot.wrapping_add(1);
        self.thumb_slots.insert(slot, key.clone());
        self.thumbs.request(ThumbRequest {
            id: slot,
            generation: THUMB_GENERATION,
            path,
            key: key.clone(),
            color_only: false,
            shared_dir: None,
            src_bytes: size,
            pdf_page: Some(page),
        });
        self.textures.insert(key, ThumbState::Pending);
    }

    /// Set which PDF page represents this workbook item.
    pub fn set_pdf_poster_page(&mut self, item_id: ItemId, page: u16) {
        if self.tab().read_only {
            return;
        }
        let Some(item) = self.doc().item(item_id).cloned() else {
            return;
        };
        if !slate_doc::media::has_pages(&item.path) || item.pdf_page == page {
            return;
        }
        if page >= self.pdf_page_count(&item.path) {
            return;
        }
        let ids: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                slate_doc::NodeKind::Image(i) if i.item == item_id => Some(n.id),
                _ => None,
            })
            .collect();
        if self.doc().view.active_view != slate_doc::ViewKind::Board || ids.is_empty() {
            // Preserve Grid/Venn item selection. Board page changes below are scene commands.
            self.doc_mut().set_pdf_page(item_id, page);
            self.request_thumb(item_id);
            return;
        }
        // Link each page as an item and journal the scene's pointer to it.
        // Undo returns to the previous page without rewriting source material.
        let key = cache_key_page(
            &item.path.to_string_lossy(),
            item.size,
            item.mtime,
            Some(page),
        );
        let new_item = self.doc_mut().add_item_page(
            item.path,
            item.file_name,
            item.size,
            item.mtime,
            key,
            page,
        );
        for tag in item.assignments.values() {
            self.doc_mut().assign(new_item, *tag);
        }
        self.patch_nodes(&ids, |n| {
            if let slate_doc::NodeKind::Image(i) = &mut n.kind {
                i.item = new_item;
            }
        });
        if let Some((_, shown, _, _)) = self.documents.picker.as_mut() {
            if *shown == item_id {
                *shown = new_item;
            }
        }
        self.request_thumb(new_item);
    }

    /// Replace one PDF item with one item per page, preserving tags on each.
    pub fn explode_pdf(&mut self, item_id: ItemId) {
        let Some(item) = self.doc().item(item_id).cloned() else {
            return;
        };
        if slate_doc::media_kind(&item.path) != MediaKind::Pdf {
            return;
        }
        let count = self.pdf_page_count(&item.path);
        if count == 0 {
            self.toast("Document pages are still loading. Try again shortly.");
            return;
        }
        if count <= 1 {
            self.toast("PDF has only one page");
            return;
        }
        let assignments = item.assignments.clone();
        let stem = Path::new(&item.file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| item.file_name.clone());
        let path = item.path.clone();
        let size = item.size;
        let mtime = item.mtime;

        self.doc_mut().remove_item(item_id);
        self.selection.remove(&item_id);

        let mut new_ids: Vec<ItemId> = Vec::new();
        for page in 0..count {
            let key = cache_key_page(&path.to_string_lossy(), size, mtime, Some(page));
            let name = format!("{stem} — page {}", page + 1);
            let id = self
                .doc_mut()
                .add_item_page(path.clone(), name, size, mtime, key, page);
            new_ids.push(id);
            for tag in assignments.values() {
                self.doc_mut().assign(id, *tag);
            }
        }

        if let Some(replacement) = new_ids.first().copied() {
            for node in &mut self.doc_mut().scene.nodes {
                if let slate_doc::scene::NodeKind::Image(img) = &mut node.kind {
                    if img.item == item_id {
                        img.item = replacement;
                    }
                }
            }
        }

        self.toast(format!("Exploded PDF into {} page item(s)", new_ids.len()));
    }

    /// Hover overlay: fan out page thumbnails for multi-page PDFs.
    pub(crate) fn paint_pdf_page_picker(
        &mut self,
        ui: &mut egui::Ui,
        item_id: ItemId,
        card_rect: Rect,
        palette: &atlas_shell::theme::Palette,
    ) {
        let Some(item) = self.doc().item(item_id).cloned() else {
            return;
        };
        if !slate_doc::media::has_pages(&item.path) {
            return;
        }
        let page_count = self.pdf_page_count(&item.path);
        if page_count == 0 {
            let message = self
                .documents
                .error(&item.path)
                .unwrap_or("Loading document pages…");
            let r = Rect::from_center_size(
                card_rect.center_bottom() + Vec2::new(0.0, 14.0),
                Vec2::new(480.0, 24.0),
            );
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                atlas_shell::widgets::trunc(message, 85),
                FontId::proportional(11.0),
                palette.sub,
            );
            ui.interact(
                r,
                ui.id().with(("document-status", item_id)),
                egui::Sense::hover(),
            )
            .on_hover_text(message);
            return;
        }
        if page_count <= 1 && item.pdf_page == 0 {
            return;
        }

        const WINDOW: u16 = 32;
        let window_id = ui
            .id()
            .with(("document-pages", self.tab().id, item.path.clone()));
        let start = ui
            .data(|d| d.get_temp::<u16>(window_id))
            .unwrap_or((item.pdf_page / WINDOW) * WINDOW)
            .min(((page_count - 1) / WINDOW) * WINDOW);
        let end = start.saturating_add(WINDOW).min(page_count);
        let visible = end - start;
        let thumb_px = 56.0;
        let gap = 6.0;
        let pad = 8.0;
        let cols = visible.min(8) as f32;
        let rows = ((visible as f32) / cols).ceil();
        let strip_w = cols * thumb_px + (cols - 1.0).max(0.0) * gap + pad * 2.0;
        let strip_h = rows * thumb_px + (rows - 1.0).max(0.0) * gap + pad * 2.0 + 18.0;

        let mut origin = card_rect.center_bottom() + Vec2::new(0.0, 8.0);
        origin.x -= strip_w * 0.5;
        // Keep on screen within the canvas.
        let canvas = self.canvas_rect;
        if origin.x + strip_w > canvas.right() - 4.0 {
            origin.x = canvas.right() - strip_w - 4.0;
        }
        if origin.x < canvas.left() + 4.0 {
            origin.x = canvas.left() + 4.0;
        }
        if origin.y + strip_h > canvas.bottom() - 4.0 {
            origin = card_rect.center_top() - Vec2::new(strip_w * 0.5, strip_h + 8.0);
        }

        let strip_rect = Rect::from_min_size(origin, Vec2::new(strip_w, strip_h));
        self.documents.picker = Some((self.tab().id, item_id, card_rect, strip_rect));
        let painter = ui.painter_at(strip_rect);
        painter.rect_filled(strip_rect, CornerRadius::same(6), palette.card);
        painter.rect_stroke(
            strip_rect,
            CornerRadius::same(6),
            Stroke::new(1.0_f32, palette.border_strong),
            StrokeKind::Inside,
        );
        painter.text(
            Pos2::new(strip_rect.min.x + pad, strip_rect.min.y + 4.0),
            Align2::LEFT_TOP,
            format!(
                "{} · {}–{} / {}",
                if slate_doc::media::is_powerpoint(&item.path) {
                    "Slides"
                } else {
                    "Pages"
                },
                start + 1,
                end,
                page_count
            ),
            FontId::proportional(10.5),
            palette.sub,
        );

        if page_count > WINDOW {
            let y = strip_rect.top() + 2.0;
            let prev = Rect::from_min_size(
                Pos2::new(strip_rect.right() - 48.0, y),
                Vec2::new(20.0, 16.0),
            );
            let next = prev.translate(Vec2::new(22.0, 0.0));
            if ui
                .put(prev, egui::Button::new("‹"))
                .on_hover_text("Previous pages")
                .clicked()
            {
                ui.data_mut(|d| d.insert_temp(window_id, start.saturating_sub(WINDOW)));
            }
            if ui
                .put(next, egui::Button::new("›"))
                .on_hover_text("Next pages")
                .clicked()
                && end < page_count
            {
                ui.data_mut(|d| d.insert_temp(window_id, end));
            }
        }
        let mut selected_page: Option<u16> = None;
        let path = item.path.clone();
        let size = item.size;
        let mtime = item.mtime;
        let current_page = item.pdf_page;
        for page in start..end {
            let col = ((page - start) as f32) % cols;
            let row = ((page - start) as f32 / cols).floor();
            let x = strip_rect.min.x + pad + col * (thumb_px + gap);
            let y = strip_rect.min.y + 18.0 + row * (thumb_px + gap);
            let cell = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(thumb_px));

            let page_key = self
                .documents
                .ready(&path)
                .map(|p| p.key(page))
                .unwrap_or_else(|| {
                    cache_key_page(&path.to_string_lossy(), size, mtime, Some(page))
                });
            if !self.textures.contains_key(&page_key) {
                self.request_pdf_page_thumb(path.clone(), size, mtime, page);
            }

            let is_current = current_page == page;
            let fill = if is_current {
                palette.select.gamma_multiply(0.25)
            } else {
                palette.thumb_bg
            };
            painter.rect_filled(cell, CornerRadius::same(3), fill);
            if let Some(ThumbState::Ready(tex)) = self.textures.get(&page_key) {
                painter.image(
                    tex.id(),
                    Rect::from_center_size(
                        cell.center(),
                        tex.size_vec2()
                            * (cell.width() / tex.size_vec2().x)
                                .min(cell.height() / tex.size_vec2().y),
                    ),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            } else {
                painter.text(
                    cell.center(),
                    Align2::CENTER_CENTER,
                    format!("{}", page + 1),
                    FontId::proportional(11.0),
                    palette.sub,
                );
            }
            if is_current {
                painter.rect_stroke(
                    cell,
                    CornerRadius::same(3),
                    Stroke::new(2.0_f32, palette.select),
                    StrokeKind::Inside,
                );
            } else {
                painter.rect_stroke(
                    cell,
                    CornerRadius::same(3),
                    Stroke::new(1.0_f32, palette.border),
                    StrokeKind::Inside,
                );
            }

            let resp = ui.interact(cell, window_id.with(page), egui::Sense::click());
            if resp.clicked() {
                selected_page = Some(page);
            }
        }

        if let Some(page) = selected_page {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("board.media.page"),
                Some(format!("{}:{page}", item_id.0)),
            );
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

    pub(crate) fn document_picker_contains(&self, pointer: Option<Pos2>) -> bool {
        self.documents.picker.is_some_and(|(tab, item, _, popup)| {
            tab == self.tab().id
                && pointer.is_some_and(|p| popup.contains(p))
                && self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .any(|n| matches!(&n.kind,slate_doc::NodeKind::Image(i) if i.item==item))
        })
    }

    /// Topmost PDF image node under a board world point, if any.
    pub(crate) fn board_hovered_pdf(&self, world: Pos2) -> Option<(ItemId, Rect)> {
        let screen = self.board_xf().w2s(world);
        if let Some((tab, item, card, popup)) = self.documents.picker {
            if tab == self.tab().id
                && card.union(popup).contains(screen)
                && self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .any(|n| matches!(&n.kind,slate_doc::NodeKind::Image(i) if i.item==item))
            {
                return Some((item, card));
            }
        }
        let id = self.doc().scene.node_at(world.x, world.y)?;
        let n = self.doc().scene.node(id)?;
        let slate_doc::scene::NodeKind::Image(img) = &n.kind else {
            return None;
        };
        let item = self.doc().item(img.item)?;
        if !slate_doc::media::has_pages(&item.path) {
            return None;
        }
        let srect = self.board_xf().rect_w2s(n.rect);
        Some((img.item, srect))
    }
}
