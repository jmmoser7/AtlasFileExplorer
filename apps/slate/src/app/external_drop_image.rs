//! Async image transfer/decode, then the ordinary paste asset intake.
use super::{external_drop, external_drop_classify::ImageSource, SlateApp};
use eframe::egui::{self, Pos2};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::Duration,
};

const MAX_PENDING_IMAGES: usize = 4;
struct Job {
    tab: u64,
    at: Pos2,
    rx: Receiver<Result<PathBuf, String>>,
}
#[derive(Default)]
pub(super) struct ImageDrops {
    jobs: Vec<Job>,
}

impl SlateApp {
    pub(super) fn process_external_drops(&mut self, ctx: &egui::Context) {
        self.poll_browser_images(ctx);
        // Dropped files land in the active workbook, uncategorized. On the
        // board they're also placed at the drop point; landing on a tagged
        // frame assigns its tags.
        let _drop_span = atlas_core::session_log::span("slate.drop");
        let mut native_drop = self.external_drop.pop();
        if self.feedback.owns_drops() {
            if let Some(external_drop::DropEvent {
                payload: external_drop::Payload::Files(paths),
                ..
            }) = native_drop.take()
            {
                self.feedback.attach_dropped(&paths);
                ctx.request_repaint();
            }
        }
        let mut drop_at = None;
        let mut drop_alt = None;
        let mut dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if self.feedback.owns_drops() {
            dropped.clear();
        }
        if native_drop.is_some() || !dropped.is_empty() {
            // Dragged out of the open dialog instead of picked: the drop is
            // the answer, so the dialog goes away as if cancelled.
            self.dialogs.close_all();
        }
        if let Some(event) = native_drop {
            ctx.request_repaint(); // drain any remaining bounded OS-drop backlog
            drop_at = Some(event.at);
            drop_alt = Some(event.alt);
            match event.payload {
                external_drop::Payload::Url(url) => {
                    self.drop_web_url(&url, event.at);
                }
                external_drop::Payload::Files(paths) => dropped.extend(paths),
                external_drop::Payload::Image(source) => {
                    self.drop_browser_image(source, event.at, ctx)
                }
            }
        }
        if !dropped.is_empty() {
            if self.at_home {
                self.leave_home();
                self.ensure_work_tab();
            }
            let at = drop_at
                .or_else(|| ctx.input(|i| i.pointer.hover_pos()))
                .map(|p| self.board_xf().s2w(p))
                .unwrap_or_else(|| self.tab().cam.offset.to_pos2());
            // An HTML page dropped on the board is a portal, not a snippet card
            // (D01). Alt keeps the old text card, which is the only way back.
            let alt = drop_alt.unwrap_or_else(|| ctx.input(|i| i.modifiers.alt));
            let screen = drop_at.or_else(|| ctx.input(|i| i.pointer.hover_pos()));
            self.ingest_dropped_paths(dropped, at, alt, screen);
        }
        // Dropped/added .slate files open as tabs, after placement above.
        self.drain_pending_workbooks();
        drop(_drop_span);
    }

    fn drop_browser_image(&mut self, source: ImageSource, screen: Pos2, ctx: &egui::Context) {
        if !self.web_drop_enabled()
            || !self.canvas_rect.contains(screen)
            || self.portal_chrome.maximized.is_some()
        {
            return;
        }
        if self.image_drops.jobs.len() >= MAX_PENDING_IMAGES {
            self.toast("Please wait for the pending image drops.");
            return;
        }
        let tab = self.tab().id;
        let at = self.board_xf().s2w(screen);
        let workbook = self.tab().path.clone();
        let (tx, rx) = mpsc::channel();
        self.image_drops.jobs.push(Job { tab, at, rx });
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = image_png(source).and_then(|png| {
                super::clipboard_image::write_pasted_png(&png, workbook.as_deref())
                    .ok_or_else(|| "Couldn't save the dropped image.".to_string())
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn poll_browser_images(&mut self, ctx: &egui::Context) {
        // Tab IDs survive tab switches; a closed workbook cannot receive a
        // late completion, and an inactive workbook waits until it is shown.
        self.image_drops
            .jobs
            .retain(|job| self.tabs.iter().any(|tab| tab.id == job.tab));
        let active = self.tab().id;
        let ready = self
            .image_drops
            .jobs
            .iter()
            .enumerate()
            .find_map(|(index, job)| {
                if self.at_home
                    || job.tab != active
                    || self.doc().view.active_view != slate_doc::ViewKind::Board
                {
                    return None;
                }
                match job.rx.try_recv() {
                    Ok(result) => Some((index, result)),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some((index, Err("Couldn't prepare the dropped image.".into())))
                    }
                    Err(mpsc::TryRecvError::Empty) => None,
                }
            });
        if let Some((index, result)) = ready {
            let job = self.image_drops.jobs.remove(index);
            match result {
                Ok(path) if !self.tab().read_only && self.presenting.is_none() => {
                    // No screen target: browser pictures always create a new
                    // image, rather than rebinding a portal or image layer.
                    self.place_pasted_image_path(path, job.at);
                }
                Ok(_) => self.toast(
                    "The image drop was cancelled because the workbook is read-only or presenting.",
                ),
                Err(message) => self.toast(message),
            }
        }
        if !self.image_drops.jobs.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}

fn image_png(source: ImageSource) -> Result<Vec<u8>, String> {
    use atlas_core::clipboard_image::{MAX_PASTE_PIXELS, MAX_PASTE_PNG_BYTES};
    let bytes = match source {
        ImageSource::Bitmap(bytes) => {
            return atlas_core::clipboard_image::decode_clipboard_bitmap(&bytes)
                .filter(|png| atlas_core::clipboard_image::png_within_limits(png))
                .ok_or_else(|| "The dropped bitmap is unsupported or too large.".into())
        }
        ImageSource::Encoded(bytes) => bytes,
        ImageSource::Url(url) => {
            if super::board_web::web_url_text(&url).is_none() {
                return Err("The image address must use HTTP or HTTPS.".into());
            }
            let mut request = atlas_curl::Request::get(url).timeout(Duration::from_secs(30));
            request.follow_redirects = true;
            request
                .send_bounded(MAX_PASTE_PNG_BYTES)
                .map_err(|error| error.to_string())?
        }
    };
    if bytes.len() > MAX_PASTE_PNG_BYTES {
        return Err("The dropped image is too large.".into());
    }
    let reader = || {
        image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|_| "The drop did not contain a supported image.".to_string())
    };
    let (w, h) = reader()?
        .into_dimensions()
        .map_err(|_| "Couldn't read the image dimensions.".to_string())?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > MAX_PASTE_PIXELS {
        return Err("The dropped image is too large.".into());
    }
    let mut reader = reader()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_PASTE_PIXELS * 4);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "Couldn't decode the dropped image.".to_string())?;
    let mut png = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|_| "Couldn't prepare the dropped image.".to_string())?;
    let png = png.into_inner();
    atlas_core::clipboard_image::png_within_limits(&png)
        .then_some(png)
        .ok_or_else(|| "The dropped image is too large.".into())
}

#[cfg(test)]
mod tests {
    use super::super::tests::Harness;
    use super::*;

    fn picture() -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(160, 90, image::Rgba([20, 40, 60, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn browser_picture_places_pixels_with_aspect_and_one_undo() {
        let mut h = Harness::new("browser_picture");
        h.app.new_tab();
        h.app.tab_mut().path = Some(h.base.join("board.slate"));
        h.frame();
        let at = h.app.canvas_rect.center();
        h.app.external_drop.push_test(external_drop::DropEvent {
            payload: external_drop::Payload::Image(ImageSource::Encoded(picture())),
            at,
            alt: false,
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while h.app.doc().scene.nodes.is_empty() {
            h.frame();
            assert!(
                std::time::Instant::now() < deadline,
                "image drop did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let node = &h.app.doc().scene.nodes[0];
        assert!(matches!(node.kind, slate_doc::scene::NodeKind::Image(_)));
        assert!((node.rect.w / node.rect.h - 16.0 / 9.0).abs() < 0.001);
        h.app.board_undo();
        assert!(h.app.doc().scene.nodes.is_empty());
    }

    #[test]
    fn invalid_browser_image_never_becomes_a_page_portal() {
        assert!(image_png(ImageSource::Encoded(b"<html>page</html>".to_vec())).is_err());
    }

    #[test]
    fn browser_image_completion_waits_for_the_original_tab() {
        let mut h = Harness::new("browser_picture_tab");
        h.app.new_tab();
        let origin = h.app.tab().id;
        let origin_index = h.app.active_tab;
        h.app.tab_mut().path = Some(h.base.join("origin.slate"));
        let path = super::super::clipboard_image::write_pasted_png(
            &picture(),
            h.app.tab().path.as_deref(),
        )
        .unwrap();
        let (tx, rx) = mpsc::channel();
        h.app.image_drops.jobs.push(Job {
            tab: origin,
            at: Pos2::ZERO,
            rx,
        });
        tx.send(Ok(path)).unwrap();
        h.app.new_tab();
        h.app.poll_browser_images(&h.ctx);
        assert!(
            h.app.doc().scene.nodes.is_empty(),
            "the other tab stays unchanged"
        );
        assert_eq!(h.app.image_drops.jobs.len(), 1);
        h.app.active_tab = origin_index;
        h.app.poll_browser_images(&h.ctx);
        assert_eq!(h.app.doc().scene.nodes.len(), 1);
        assert!(h.app.image_drops.jobs.is_empty());
    }
}
