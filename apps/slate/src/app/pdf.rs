//! PDF page picker (hover) and explode-into-pages actions.

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
    /// Cached page count for a PDF path (populated on first hover).
    pub(crate) fn pdf_page_count(&mut self, path: &Path) -> u16 {
        let key = path.to_string_lossy().into_owned();
        if let Some(&n) = self.pdf_page_counts.get(&key) {
            return n;
        }
        let count = atlas_core::pdf::page_count(path).unwrap_or(1).max(1);
        self.pdf_page_counts.insert(key, count);
        count
    }

    /// Queue a thumbnail for a specific PDF page (hover strip previews).
    pub(crate) fn request_pdf_page_thumb(
        &mut self,
        path: PathBuf,
        size: u64,
        mtime: i64,
        page: u16,
    ) {
        let key = cache_key_page(&path.to_string_lossy(), size, mtime, Some(page));
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
        if !self.doc_mut().set_pdf_page(item_id, page) {
            return;
        }
        self.request_thumb(item_id);
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
        if slate_doc::media_kind(&item.path) != MediaKind::Pdf {
            return;
        }
        let page_count = self.pdf_page_count(&item.path);
        if page_count <= 1 {
            return;
        }

        let thumb_px = 56.0;
        let gap = 6.0;
        let pad = 8.0;
        let cols = page_count.min(8) as f32;
        let rows = ((page_count as f32) / cols).ceil();
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
        let painter = ui.painter_at(strip_rect);
        painter.rect_filled(strip_rect, CornerRadius::same(6), palette.card);
        painter.rect_stroke(
            strip_rect,
            CornerRadius::same(6),
            Stroke::new(1.0, palette.border_strong),
            StrokeKind::Inside,
        );
        painter.text(
            Pos2::new(strip_rect.min.x + pad, strip_rect.min.y + 4.0),
            Align2::LEFT_TOP,
            "PDF pages — click to set poster",
            FontId::proportional(10.5),
            palette.sub,
        );

        let mut selected_page: Option<u16> = None;
        let path = item.path.clone();
        let size = item.size;
        let mtime = item.mtime;
        let current_page = item.pdf_page;
        for page in 0..page_count {
            let col = (page as f32) % cols;
            let row = (page as f32 / cols).floor();
            let x = strip_rect.min.x + pad + col * (thumb_px + gap);
            let y = strip_rect.min.y + 18.0 + row * (thumb_px + gap);
            let cell = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(thumb_px));

            let page_key = cache_key_page(&path.to_string_lossy(), size, mtime, Some(page));
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
                    cell,
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
                    Stroke::new(2.0, palette.select),
                    StrokeKind::Inside,
                );
            } else {
                painter.rect_stroke(
                    cell,
                    CornerRadius::same(3),
                    Stroke::new(1.0, palette.border),
                    StrokeKind::Inside,
                );
            }

            let resp = ui.allocate_rect(cell, egui::Sense::click());
            if resp.clicked() {
                selected_page = Some(page);
            }
        }

        if let Some(page) = selected_page {
            self.set_pdf_poster_page(item_id, page);
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

    /// Topmost PDF image node under a board world point, if any.
    pub(crate) fn board_hovered_pdf(&self, world: Pos2) -> Option<(ItemId, Rect)> {
        let id = self.doc().scene.node_at(world.x, world.y)?;
        let n = self.doc().scene.node(id)?;
        let slate_doc::scene::NodeKind::Image(img) = &n.kind else {
            return None;
        };
        let item = self.doc().item(img.item)?;
        if slate_doc::media_kind(&item.path) != MediaKind::Pdf {
            return None;
        }
        let srect = self.board_xf().rect_w2s(n.rect);
        Some((img.item, srect))
    }
}
