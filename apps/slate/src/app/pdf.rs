//! Paged-document poster page and unbundle. Album motion is `image_album`.

pub(crate) mod documents;

use super::{SlateApp, ThumbState};
use atlas_core::thumbs::{cache_key_page, ThumbRequest};
use eframe::egui::{self, Pos2, Rect, Stroke, Vec2};
use slate_doc::ItemId;
use std::path::{Path, PathBuf};

use super::THUMB_GENERATION;

pub(crate) struct QueuedUnbundle {
    tab: u64,
    node: slate_doc::NodeId,
    focus: Option<u16>,
    source: PathBuf,
    writing: Option<crossbeam_channel::Receiver<Result<Vec<PageFile>, String>>>,
}

struct PageFile {
    locator: PathBuf,
    bytes: u64,
    mtime: i64,
    name: String,
}

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
        let fs_path = slate_doc::scene::resolve_source(
            self.tab().path.as_deref(),
            &item.path.to_string_lossy(),
        );
        if slate_doc::media::has_pages(&item.path) {
            self.documents.request(&item.path);
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
            fs_path,
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
        self.documents.request(path);
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

    /// Spread every page of a selected PDF, PowerPoint, or Word document onto
    /// the board as a grid. The focused page keeps the original node id.
    /// A preview that is still rendering is queued and finished when it arrives.
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
        let tab = self.tab().id;
        if self
            .queued_unbundles
            .iter()
            .any(|job| job.tab == tab && job.node == node_id)
        {
            return true;
        }
        let failed = self
            .documents
            .error(&item.path)
            .map(str::to_string)
            .filter(|_| self.documents.ready(&item.path).is_none());
        if let Some(error) = failed {
            self.toast(error);
            return false;
        }
        let count = self.pdf_page_count(&item.path);
        if count == 0 {
            self.queued_unbundles.push(QueuedUnbundle {
                tab,
                node: node_id,
                focus,
                source: item.path,
                writing: None,
            });
            return true;
        }
        if count <= 1 {
            self.toast("Document has only one page");
            return false;
        }
        if slate_doc::media::is_word_document(&item.path) {
            self.begin_word_pages(QueuedUnbundle {
                tab,
                node: node_id,
                focus,
                source: item.path,
                writing: None,
            });
            true
        } else {
            self.place_pdf_pages(node_id, focus)
        }
    }

    pub(crate) fn pump_document_jobs(&mut self, ctx: &eframe::egui::Context) {
        self.documents.poll(ctx);
        self.complete_queued_unbundles();
    }

    pub(crate) fn complete_queued_unbundles(&mut self) {
        let jobs = std::mem::take(&mut self.queued_unbundles);
        for job in jobs {
            self.advance_unbundle(job);
        }
    }

    fn advance_unbundle(&mut self, mut job: QueuedUnbundle) {
        if !self.tabs.iter().any(|tab| tab.id == job.tab) {
            return;
        }
        if self.tab().id != job.tab {
            self.queued_unbundles.push(job);
            return;
        }
        if let Some(rx) = job.writing.take() {
            match rx.try_recv() {
                Ok(Ok(pages)) => {
                    self.place_word_pages(job.node, job.focus, &pages);
                }
                Ok(Err(error)) => self.toast(error),
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    job.writing = Some(rx);
                    self.queued_unbundles.push(job);
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.toast("Document page render stopped.");
                }
            }
            return;
        }
        let failed = self
            .documents
            .error(&job.source)
            .map(str::to_string)
            .filter(|_| self.documents.ready(&job.source).is_none());
        if let Some(error) = failed {
            self.toast(error);
            return;
        }
        let count = self.pdf_page_count(&job.source);
        if count == 0 {
            self.queued_unbundles.push(job);
            return;
        }
        if count <= 1 {
            self.toast("Document has only one page");
            return;
        }
        if slate_doc::media::is_word_document(&job.source) {
            self.begin_word_pages(job);
        } else {
            self.place_pdf_pages(job.node, job.focus);
        }
    }

    fn begin_word_pages(&mut self, job: QueuedUnbundle) {
        let Some(preview) = self.documents.ready(&job.source).cloned() else {
            self.queued_unbundles.push(job);
            return;
        };
        let Some(node) = self.doc().scene.node(job.node).cloned() else {
            return;
        };
        let slate_doc::NodeKind::Image(img) = &node.kind else {
            return;
        };
        let Some(item) = self.doc().item(img.item).cloned() else {
            return;
        };
        if item.path != job.source {
            return;
        }
        let stem = document_stem(&item.file_name);
        let workbook = self.tab().path.clone();
        let dir = page_image_dir(workbook.as_deref(), &stem, &job.source);
        let wake = self.documents.ui_ctx();
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let result = write_word_pages(
                &preview.path,
                preview.pages,
                &dir,
                workbook.as_deref(),
                &stem,
            );
            let _ = tx.send(result);
            wake.request_repaint();
        });
        self.queued_unbundles.push(QueuedUnbundle {
            writing: Some(rx),
            ..job
        });
    }

    fn place_pdf_pages(&mut self, node_id: slate_doc::NodeId, focus: Option<u16>) -> bool {
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
        let count = self
            .documents
            .ready(&item.path)
            .map(|preview| preview.pages)
            .unwrap_or(0);
        if count <= 1 {
            return false;
        }
        let active = focus.unwrap_or(item.pdf_page).min(count - 1);
        let assignments = item.assignments.clone();
        let stem = document_stem(&item.file_name);
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
        self.layout_page_nodes(node_id, active, &page_items)
    }

    fn place_word_pages(
        &mut self,
        node_id: slate_doc::NodeId,
        focus: Option<u16>,
        pages: &[PageFile],
    ) {
        if pages.len() <= 1 {
            self.toast("Document has only one page");
            return;
        }
        let Some(node) = self.doc().scene.node(node_id).cloned() else {
            return;
        };
        let slate_doc::NodeKind::Image(img) = &node.kind else {
            return;
        };
        let Some(source_item) = self.doc().item(img.item).cloned() else {
            return;
        };
        let active = focus.unwrap_or(0).min(pages.len() as u16 - 1);
        let assignments = source_item.assignments.clone();
        let mut page_items = Vec::with_capacity(pages.len());
        for page in pages {
            let key = atlas_core::thumbs::cache_key(
                &page.locator.to_string_lossy(),
                page.bytes,
                page.mtime,
            );
            let id = self.doc_mut().add_item(
                page.locator.clone(),
                page.name.clone(),
                page.bytes,
                page.mtime,
                key,
            );
            for tag in assignments.values() {
                self.doc_mut().assign(id, *tag);
            }
            page_items.push(id);
        }
        self.layout_page_nodes(node_id, active, &page_items);
    }

    fn layout_page_nodes(
        &mut self,
        node_id: slate_doc::NodeId,
        active: u16,
        page_items: &[slate_doc::ItemId],
    ) -> bool {
        let Some(node) = self.doc().scene.node(node_id).cloned() else {
            return false;
        };
        if node.locked {
            return false;
        }
        let slate_doc::NodeKind::Image(img) = &node.kind else {
            return false;
        };
        if page_items.is_empty() || active as usize >= page_items.len() {
            return false;
        }
        let style = img.clone();
        let sizes = vec![(node.rect.w, node.rect.h); page_items.len()];
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
        for (page, item) in page_items.iter().copied().enumerate() {
            if page == active as usize {
                continue;
            }
            let kind = slate_doc::NodeKind::Image(slate_doc::scene::ImageNode {
                item,
                ..style.clone()
            });
            let mut child = self.doc_mut().scene.build_node(rects[page], kind);
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
        for item_id in page_items {
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
                enabled: true,
            });
        }
        let writable = !self.tab().read_only;
        let canvas = self.canvas_rect;
        let mut next = focus;
        let mut unbundle = false;
        let mut hover = false;
        let album = atlas_shell::selection_tools::popup_area(
            ui.ctx(),
            egui::Id::new(("pages_album", node_id.0)),
            ui.layer_id(),
        );
        album
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
                    atlas_shell::home::AlbumInput {
                        drag: count > 1,
                        wheel: count > 1,
                        host_paints_rest: false,
                    },
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

fn document_stem(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.to_string());
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "document".into()
    } else {
        cleaned
    }
}

/// Two documents with the same name in different folders must not share a
/// folder: the second unbundle would overwrite the first one's linked pages.
fn page_image_dir(workbook: Option<&Path>, stem: &str, source: &Path) -> PathBuf {
    let hash = source
        .to_string_lossy()
        .bytes()
        .fold(0xcbf29ce4u32, |hash, byte| {
            hash.wrapping_mul(0x01000193) ^ u32::from(byte)
        });
    let name = format!("{stem}-{hash:08x}");
    if let Some(dir) = workbook.and_then(|path| path.parent()) {
        return dir.join("assets").join("documents").join(name);
    }
    atlas_core::index::data_dir()
        .join("document-pages")
        .join(name)
}

fn write_word_pages(
    pdf: &Path,
    pages: u16,
    dir: &Path,
    workbook: Option<&Path>,
    stem: &str,
) -> Result<Vec<PageFile>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut written: Vec<PageFile> = Vec::with_capacity(pages as usize);
    for page in 0..pages {
        let absolute = dir.join(format!("page-{}.png", page + 1));
        if let Err(error) = atlas_core::pdf::write_page_png(pdf, page, &absolute) {
            for file in &written {
                let path =
                    slate_doc::scene::resolve_source(workbook, &file.locator.to_string_lossy());
                let _ = std::fs::remove_file(path);
            }
            return Err(error);
        }
        let meta = std::fs::metadata(&absolute).map_err(|e| e.to_string())?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_secs() as i64)
            .unwrap_or(0);
        written.push(PageFile {
            locator: PathBuf::from(slate_doc::scene::source_locator(workbook, &absolute)),
            bytes: meta.len(),
            mtime,
            name: format!("{stem} — page {}", page + 1),
        });
    }
    Ok(written)
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
