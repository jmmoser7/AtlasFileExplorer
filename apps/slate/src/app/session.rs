//! Linked File Atlas session: Atlas runs as a second native viewport of the
//! Slate process, sharing memory through `atlas_session::SharedSession`.
//!
//! Flow per frame:
//! 1. Slate publishes the active workbook's tag groups + its window rect.
//! 2. The embedded Atlas viewport renders (right-click tagging, drag-out).
//! 3. Slate drains the inbox (tag assignments) into the active document and
//!    resolves any released cross-window drag.

use super::SlateApp;
use atlas_session::{new_session, SessionTag, SessionTagGroup, SharedSession, TagAssignment};
use eframe::egui;
use slate_doc::TagId;

/// Screen-space origin of the Slate window (for drag targeting).
fn sess_window_origin(shared: &SharedSession) -> (f32, f32) {
    shared
        .lock()
        .ok()
        .and_then(|s| s.slate_window.map(|(x0, y0, _, _)| (x0, y0)))
        .unwrap_or((0.0, 0.0))
}

pub struct AtlasSession {
    pub shared: SharedSession,
    pub atlas: Box<native_file_atlas::AtlasApp>,
}

const ATLAS_VIEWPORT: &str = "slate-linked-atlas";

impl SlateApp {
    /// Open File Atlas as a linked viewport (or focus the existing one).
    pub fn open_atlas(&mut self, ctx: &egui::Context) {
        if self.atlas.is_some() {
            return;
        }
        let shared = new_session();
        let atlas = Box::new(native_file_atlas::AtlasApp::embedded(
            ctx,
            None,
            shared.clone(),
        ));
        self.atlas = Some(AtlasSession { shared, atlas });
        self.publish_session_tags();
        self.toast("File Atlas linked — right-click files there to tag them");
    }

    pub fn close_atlas(&mut self) {
        self.atlas = None;
    }

    /// Push the active workbook's tag structure into the bridge so Atlas can
    /// offer it in its context menus. Called whenever tags/doc change.
    pub fn publish_session_tags(&mut self) {
        let Some(sess) = &self.atlas else { return };
        let doc = &self.tabs[self.active_tab].doc;
        let groups: Vec<SessionTagGroup> = doc
            .groups
            .iter()
            .map(|g| SessionTagGroup {
                group_id: g.id.0,
                name: g.name.clone(),
                tags: g
                    .tags
                    .iter()
                    .map(|t| SessionTag {
                        tag_id: t.id.0,
                        name: t.name.clone(),
                        color: t.color,
                    })
                    .collect(),
            })
            .collect();
        if let Ok(mut s) = sess.shared.lock() {
            s.tag_groups = groups;
            s.workbook_name = doc.name.clone();
        }
    }

    /// Apply one bridge assignment to the active workbook.
    fn apply_assignment(&mut self, a: TagAssignment) {
        if slate_doc::media_kind(&a.file.path) == slate_doc::MediaKind::Workbook {
            // Workbooks never become items; open as a tab instead.
            self.pending_workbooks.push(a.file.path);
            return;
        }
        let id = self.doc_mut().add_item(
            a.file.path,
            a.file.file_name,
            a.file.size,
            a.file.mtime,
            a.file.cache_key,
        );
        for tag in a.tag_ids {
            self.doc_mut().assign(id, TagId(tag));
        }
    }

    /// Per-frame session pump; also renders the embedded Atlas viewport.
    pub fn session_frame(&mut self, ctx: &egui::Context) {
        let Some(sess) = &mut self.atlas else { return };
        let shared = sess.shared.clone();

        // Publish Slate's window rect (screen coords) for drag targeting.
        let slate_rect = ctx.input(|i| {
            let vp = i.viewport();
            vp.outer_rect.map(|r| (r.min.x, r.min.y, r.max.x, r.max.y))
        });
        if let Ok(mut s) = shared.lock() {
            s.slate_window = slate_rect;
        }

        // Render the Atlas viewport (immediate: same thread, shared Context).
        let viewport_id = egui::ViewportId::from_hash_of(ATLAS_VIEWPORT);
        let workbook = self.tabs[self.active_tab].doc.name.clone();
        let atlas = &mut sess.atlas;
        let mut close_atlas = false;
        ctx.show_viewport_immediate(
            viewport_id,
            egui::ViewportBuilder::default()
                .with_title(format!("File Atlas — linked to {workbook}"))
                .with_inner_size([1280.0, 800.0])
                .with_min_inner_size([800.0, 500.0]),
            |ctx, _class| {
                atlas.run_frame(ctx);
                if ctx.input(|i| i.viewport().close_requested()) {
                    close_atlas = true;
                }
            },
        );

        // Drain assignments and the cross-window drag.
        let (inbox, drag_done) = {
            let mut s = shared.lock().unwrap();
            let inbox: Vec<TagAssignment> = std::mem::take(&mut s.inbox);
            let mut drag_done = None;
            if let Some(drag) = &s.drag {
                if drag.released {
                    let inside = drag
                        .screen_pos
                        .map(|(x, y)| s.point_in_slate(x, y))
                        .unwrap_or(false);
                    drag_done = Some((drag.files.clone(), inside, drag.screen_pos));
                    s.drag = None;
                }
            }
            if s.close_requested {
                s.close_requested = false;
                close_atlas = true;
            }
            (inbox, drag_done)
        };

        let had_inbox = !inbox.is_empty();
        for a in inbox {
            self.apply_assignment(a);
        }
        if had_inbox {
            ctx.request_repaint();
        }

        if let Some((files, inside, screen_pos)) = drag_done {
            if inside {
                let n = files.len();
                let mut ids = Vec::new();
                for f in files {
                    // Workbooks dragged over from Atlas open as tabs — they
                    // never become items (no workbook-in-workbook).
                    if slate_doc::media_kind(&f.path) == slate_doc::MediaKind::Workbook {
                        self.pending_workbooks.push(f.path);
                        continue;
                    }
                    // Dropped without tags: lands in the Uncategorized tray
                    // (unless it hits a tagged frame on the board, below).
                    ids.push(self.doc_mut().add_item(
                        f.path,
                        f.file_name,
                        f.size,
                        f.mtime,
                        f.cache_key,
                    ));
                }
                // On the board, also place the drop where it landed.
                if self.doc().view.active_view == slate_doc::ViewKind::Board {
                    let origin = sess_window_origin(&shared);
                    let world = screen_pos
                        .map(|(x, y)| {
                            let local = egui::Pos2::new(x - origin.0, y - origin.1);
                            self.board_xf().s2w(local)
                        })
                        .unwrap_or_else(|| self.tabs[self.active_tab].cam.offset.to_pos2());
                    self.place_items_on_board(&ids, world);
                }
                self.toast(format!("{n} file(s) dropped from File Atlas"));
            }
        }

        if close_atlas {
            self.close_atlas();
        } else {
            // Keep pumping while the linked window is open.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    /// Ghost overlay while an Atlas drag hovers over the Slate window.
    pub fn draw_session_drag_hint(&self, ctx: &egui::Context) {
        let Some(sess) = &self.atlas else { return };
        let Ok(s) = sess.shared.lock() else { return };
        let Some(drag) = &s.drag else { return };
        if drag.released {
            return;
        }
        let Some((x, y)) = drag.screen_pos else {
            return;
        };
        if !s.point_in_slate(x, y) {
            return;
        }
        // Convert screen position to local window coordinates.
        let origin = s
            .slate_window
            .map(|(x0, y0, _, _)| (x0, y0))
            .unwrap_or((0.0, 0.0));
        let local = egui::Pos2::new(x - origin.0, y - origin.1);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("slate_drag_hint"),
        ));
        let palette = self.palette();
        painter.circle_filled(local, 14.0, palette.accent.gamma_multiply(0.85));
        painter.text(
            local,
            egui::Align2::CENTER_CENTER,
            format!("{}", drag.files.len()),
            egui::FontId::proportional(12.0),
            palette.bg,
        );
    }
}
