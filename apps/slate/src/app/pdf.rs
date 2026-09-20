//! Paged-document poster page and unbundle. Album motion is `image_album`.

pub(crate) mod documents;

use super::{SlateApp, ThumbState};
use atlas_core::thumbs::{cache_key_page, ThumbRequest};
use eframe::egui::{self, Pos2, Rect, Stroke, Vec2};
use slate_doc::ItemId;
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
        self.request_thumb(new_item);
    }

    /// Spread every page of a selected PDF/PowerPoint onto the board as a grid.
    /// The focused page keeps the original node id (wires and style survive).
    pub fn unbundle_paged_media(&mut self, node_id: slate_doc::NodeId, focus: Option<u16>) -> bool {
        if self.tab().read_only {
            return false;
        }
        let Some(node) = self.doc().scene.node(node_id).cloned() else {
            return false;
        };
        if node.locked {
            return false;
        }
        let slate_doc::NodeKind::Image(img) = &node.kind else {
            return false;
        };
        let Some(item) = self.doc().item(img.item).cloned() else {
            return false;
        };
        if !slate_doc::media::has_pages(&item.path) {
            return false;
        }
        let count = self.pdf_page_count(&item.path);
        if count == 0 {
            self.toast(
                self.documents
                    .error(&item.path)
                    .unwrap_or("Document pages are still loading. Try again shortly.")
                    .to_string(),
            );
            return false;
        }
        if count <= 1 {
            self.toast("Document has only one page");
            return false;
        }
        let style = img.clone();
        let active = focus.unwrap_or(item.pdf_page).min(count - 1);
        let assignments = item.assignments.clone();
        let stem = Path::new(&item.file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| item.file_name.clone());
        let mut page_items = Vec::with_capacity(count as usize);
        for page in 0..count {
            let key = cache_key_page(
                &item.path.to_string_lossy(),
                item.size,
                item.mtime,
                Some(page),
            );
            let name = format!("{stem} — page {}", page + 1);
            let id = self.doc_mut().add_item_page(
                item.path.clone(),
                name,
                item.size,
                item.mtime,
                key,
                page,
            );
            for tag in assignments.values() {
                self.doc_mut().assign(id, *tag);
            }
            page_items.push(id);
        }
        let sizes = vec![(node.rect.w, node.rect.h); count as usize];
        let center = eframe::egui::Pos2::new(
            node.rect.x + node.rect.w * 0.5,
            node.rect.y + node.rect.h * 0.5,
        );
        let rects = super::board::grid_drop_rects(&sizes, center);
        let mut cmds = Vec::new();
        let mut after = node.clone();
        after.rect = rects[active as usize];
        if let slate_doc::NodeKind::Image(image) = &mut after.kind {
            image.item = page_items[active as usize];
        }
        cmds.push(slate_doc::scene::SceneCmd::Patch {
            before: Box::new(node.clone()),
            after: Box::new(after),
        });
        let mut ids = vec![node_id];
        let mut insertion = self.doc().scene.nodes.len();
        for page in 0..count {
            if page == active {
                continue;
            }
            let kind = slate_doc::NodeKind::Image(slate_doc::scene::ImageNode {
                item: page_items[page as usize],
                ..style.clone()
            });
            let mut child = self.doc_mut().scene.build_node(rects[page as usize], kind);
            child.rotation_deg = node.rotation_deg;
            child.opacity = node.opacity;
            child.clip = node.clip.clone();
            ids.push(child.id);
            cmds.push(slate_doc::scene::SceneCmd::Add {
                index: insertion,
                node: child,
            });
            insertion += 1;
        }
        if !self.commit_scene(cmds) {
            return false;
        }
        self.board_sel = ids.iter().copied().collect();
        for item_id in &page_items {
            self.request_thumb(*item_id);
        }
        self.inherit_frame_tags_after_move(&ids);
        true
    }

    pub(crate) fn selected_paged_node(&self) -> Option<slate_doc::NodeId> {
        self.board_sel
            .iter()
            .copied()
            .find(|&id| self.node_has_pages(id))
    }

    pub(crate) fn node_has_pages(&self, id: slate_doc::NodeId) -> bool {
        self.doc().scene.node(id).is_some_and(|n| match &n.kind {
            slate_doc::NodeKind::Image(img) => self
                .doc()
                .item(img.item)
                .is_some_and(|item| slate_doc::media::has_pages(&item.path)),
            _ => false,
        })
    }

    pub(crate) fn node_pdf_item(&self, id: slate_doc::NodeId) -> Option<slate_doc::SlateItem> {
        match &self.doc().scene.node(id)?.kind {
            slate_doc::NodeKind::Image(img) => self.doc().item(img.item).cloned(),
            _ => None,
        }
    }

    /// Thin album pallet over a selected paged document. Reuses Cover Flow motion.
    pub(crate) fn paint_pages_album(
        &mut self,
        ui: &mut egui::Ui,
        node_id: slate_doc::NodeId,
        host: Rect,
        zoom: f32,
        theme: atlas_shell::theme::Palette,
        focus: u16,
    ) -> PagesAlbumOutcome {
        let Some(item) = self.node_pdf_item(node_id) else {
            return PagesAlbumOutcome::default();
        };
        if !slate_doc::media::has_pages(&item.path) {
            return PagesAlbumOutcome::default();
        }
        let count = self.pdf_page_count(&item.path);
        if count == 0 {
            return PagesAlbumOutcome::default();
        }
        let focus = focus.min(count.saturating_sub(1));
        let layout = pages_album_layout(host, zoom);
        let mut images = Vec::with_capacity(count as usize);
        let path = item.path.clone();
        let size = item.size;
        let mtime = item.mtime;
        for page in 0..count {
            let distance = page.abs_diff(focus);
            let visible = distance <= 3 || count.saturating_sub(distance) <= 3;
            let (tex_id, tex_size) = if visible {
                let key = self
                    .documents
                    .ready(&path)
                    .map(|p| p.key(page))
                    .unwrap_or_else(|| {
                        cache_key_page(&path.to_string_lossy(), size, mtime, Some(page))
                    });
                if !self.textures.contains_key(&key) {
                    self.request_pdf_page_thumb(path.clone(), size, mtime, page);
                }
                match self.textures.get(&key) {
                    Some(ThumbState::Ready(tex)) => (Some(tex.id()), tex.size_vec2()),
                    _ => (None, Vec2::splat(1.0)),
                }
            } else {
                (None, Vec2::splat(1.0))
            };
            images.push(atlas_shell::home::AlbumImage {
                texture: tex_id,
                size: tex_size,
            });
        }
        let writable = !self.tab().read_only;
        let canvas = self.canvas_rect;
        let mut next = focus;
        let mut unbundle = false;
        let mut hover = false;
        egui::Area::new(egui::Id::new(("pages_album", node_id.0)))
            .order(egui::Order::Foreground)
            .fixed_pos(layout.pallet.min)
            .constrain(false)
            .movable(false)
            .fade_in(false)
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(canvas);
                ui.set_min_size(layout.pallet.size());
                atlas_shell::dock::paint_squircle(
                    ui.painter(),
                    layout.pallet,
                    theme.card,
                    Stroke::new(0.8 * zoom, theme.border_strong),
                    atlas_shell::tokens::current().dock.squircle_exponent,
                );
                next = atlas_shell::home::image_album(
                    ui,
                    egui::Id::new(("pdf-album", node_id.0)),
                    layout.album,
                    &images,
                    focus as usize,
                    count > 1,
                ) as u16;
                unbundle = atlas_shell::selection_tools::button(
                    ui,
                    layout.unbundle,
                    egui::Id::new(("pdf-unbundle", node_id.0)),
                    "Unbundle pages onto the board",
                    atlas_shell::icons::Icon::Pages,
                    false,
                    zoom,
                    theme,
                    1.0,
                    writable && count > 1,
                )
                .clicked();
                hover = ui
                    .ctx()
                    .pointer_latest_pos()
                    .is_some_and(|p| layout.pallet.contains(p));
            });
        let idle = ui.input(|i| !i.pointer.any_down() && i.smooth_scroll_delta.y.abs() < 0.1);
        PagesAlbumOutcome {
            focus: next,
            commit_page: idle && next != item.pdf_page,
            unbundle,
            hover,
        }
    }
}

/// Low-profile tool pallet along the bottom of a selected paged image.
pub(crate) fn pages_album_layout(host: Rect, zoom: f32) -> PagesAlbumLayout {
    let pad = 8.0 * zoom;
    let btn = atlas_shell::selection_tools::BUTTON_SIZE * zoom;
    let inner = 5.0 * zoom;
    let height = btn + inner * 2.0;
    let pallet = Rect::from_min_max(
        Pos2::new(host.left() + pad, host.bottom() - height - pad),
        Pos2::new(host.right() - pad, host.bottom() - pad),
    );
    let unbundle = Rect::from_center_size(
        Pos2::new(pallet.right() - inner - btn * 0.5, pallet.center().y),
        Vec2::splat(btn),
    );
    let album = Rect::from_min_max(
        Pos2::new(pallet.left() + inner, pallet.top() + inner),
        Pos2::new(
            (unbundle.left() - inner).max(pallet.left() + inner + 8.0 * zoom),
            pallet.bottom() - inner,
        ),
    );
    PagesAlbumLayout {
        pallet,
        album,
        unbundle,
    }
}

pub(crate) struct PagesAlbumLayout {
    pub pallet: Rect,
    pub album: Rect,
    pub unbundle: Rect,
}

#[derive(Default)]
pub(crate) struct PagesAlbumOutcome {
    pub focus: u16,
    pub commit_page: bool,
    pub unbundle: bool,
    pub hover: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_album_is_a_thin_pallet_over_the_host() {
        let host = Rect::from_min_size(Pos2::new(40.0, 20.0), Vec2::new(320.0, 240.0));
        let layout = pages_album_layout(host, 1.0);
        assert!(layout.pallet.height() <= 48.0);
        assert!(layout.pallet.bottom() <= host.bottom());
        assert!(layout.album.width() > layout.unbundle.width());
        assert!(layout.pallet.contains(layout.album.center()));
        assert!(layout.pallet.contains(layout.unbundle.center()));
    }
}
