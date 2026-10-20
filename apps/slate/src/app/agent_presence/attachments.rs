//! Explicit chat images use the same clipboard decoder and asset writer as the board.
use super::*;
use std::sync::mpsc;

const MAX_IMAGES: usize = 8;
#[derive(Default)]
pub(super) struct AttachmentJobs {
    pending: Vec<(String, mpsc::Receiver<Result<PathBuf, String>>)>,
}
impl AttachmentJobs {
    fn drain_ready(&mut self) -> Vec<(String, Result<PathBuf, String>)> {
        let mut ready = Vec::new();
        self.pending.retain(|(who, rx)| match rx.try_recv() {
            Ok(result) => {
                ready.push((who.clone(), result));
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(_) => false,
        });
        ready
    }
}

impl SlateApp {
    pub(super) fn presence_attach_image(
        &mut self,
        ctx: &egui::Context,
        who: &str,
    ) -> Result<(), String> {
        if who == "self" || who.starts_with("human:") {
            return Err("Image attachments currently require an agent conversation.".into());
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
            .pick_file()
        {
            if self.presence.attachments.pending.len() >= 2 {
                return Err("An image is still being attached. Try again in a moment.".into());
            }
            let (tx, rx) = mpsc::channel();
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let result = if atlas_core::cloud::is_dehydrated(&path) {
                    Err("Make this image available locally before attaching it.".into())
                } else if !path.is_file() {
                    Err("The attached image is unavailable.".into())
                } else {
                    Ok(path)
                };
                let _ = tx.send(result);
                repaint.request_repaint();
            });
            self.presence.attachments.pending.push((who.into(), rx));
        }
        Ok(())
    }
    fn presence_add_attachment(&mut self, who: &str, path: PathBuf) -> Result<(), String> {
        let c = self.presence.conversations.edit(who)?;
        if c.draft_images.len() >= MAX_IMAGES {
            return Err("Attach up to eight images per message.".into());
        }
        let mut paths = c.draft_images.clone();
        paths.push(path);
        c.set_draft_images(paths);
        self.presence.conversations.save(who);
        Ok(())
    }
    pub(super) fn presence_remove_attachment(
        &mut self,
        who: &str,
        index: usize,
    ) -> Result<(), String> {
        let c = self.presence.conversations.edit(who)?;
        let mut paths = c.draft_images.clone();
        if index < paths.len() {
            paths.remove(index);
            c.set_draft_images(paths);
        }
        self.presence.conversations.save(who);
        Ok(())
    }
    pub(crate) fn presence_poll_attachments(
        &mut self,
        ctx: &egui::Context,
        native_paste_edge: bool,
    ) -> bool {
        for (who, result) in self.presence.attachments.drain_ready() {
            let result = result.and_then(|path| self.presence_add_attachment(&who, path));
            if let Err(error) = result {
                self.toast(error);
            }
        }
        if self.at_home
            || self.doc().view.active_view != slate_doc::ViewKind::Board
            || self.presenting.is_some()
        {
            return false;
        }
        let Some(who) = self.presence.chrome.active_chat().map(str::to_owned) else {
            return false;
        };
        let focused =
            ctx.memory(|m| m.focused()) == Some(egui::Id::new(("presence-composer", who.as_str())));
        if focused {
            self.web_release_keyboard();
        }
        let native_owned =
            native_paste_edge && focused && who != "self" && !who.starts_with("human:");
        let frame_paste = ctx.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                || i.key_pressed(egui::Key::V) && i.modifiers.command
        });
        let paste = composer_owns_paste(focused, frame_paste, native_paste_edge);
        if !paste
            || who == "self"
            || who.starts_with("human:")
            || self.presence.attachments.pending.len() >= 2
        {
            return native_owned;
        }
        // OS clipboard decode and disk writes stay off the frame loop. Text paste
        // remains an ordinary TextEdit event, including when no bitmap is present.
        let (tx, rx) = mpsc::channel();
        let workbook = self.tab().path.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            if let Some(png) = atlas_core::clipboard_image::read_png() {
                let result =
                    super::super::clipboard_image::write_pasted_png(&png, workbook.as_deref())
                        .ok_or_else(|| "Could not save the pasted chat image.".into());
                let _ = tx.send(result);
                repaint.request_repaint();
            }
        });
        self.presence.attachments.pending.push((who, rx));
        native_owned
    }
}

fn composer_owns_paste(focused: bool, frame_paste: bool, native_paste_edge: bool) -> bool {
    focused && (frame_paste || native_paste_edge)
}

pub(super) fn image_inputs(paths: &[PathBuf]) -> atlas_agent::InputSnapshot {
    let mut input = atlas_agent::InputSnapshot::default();
    if !paths.is_empty() {
        input.wired.push(atlas_agent::ContextItem {
            node: 0,
            text: "Images explicitly attached by the human to this message.".into(),
            images: paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            outputs: Default::default(),
            active: None,
            depth: None,
            slot: Some(atlas_agent::InputSlot::Media),
        });
    }
    input
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bitmap_paste_edge_routes_only_to_the_focused_composer() {
        assert!(composer_owns_paste(true, false, true));
        assert!(!composer_owns_paste(true, false, false));
        assert!(!composer_owns_paste(false, false, true));
    }

    #[test]
    fn a_text_only_paste_worker_disconnects_instead_of_leaving_a_pending_job() {
        let mut jobs = AttachmentJobs::default();
        for _ in 0..2 {
            let (tx, rx) = mpsc::channel();
            jobs.pending.push(("agent".into(), rx));
            drop(tx); // The no-bitmap worker exits without creating an asset.
        }
        assert!(jobs.drain_ready().is_empty());
        assert!(
            jobs.pending.is_empty(),
            "text paste cannot exhaust the two-job image budget"
        );
        let (tx, rx) = mpsc::channel();
        jobs.pending.push(("agent".into(), rx));
        tx.send(Ok(PathBuf::from("pasted.png"))).unwrap();
        assert_eq!(
            jobs.drain_ready()[0].1.as_ref().unwrap(),
            &PathBuf::from("pasted.png")
        );
    }
    #[test]
    fn explicit_images_use_the_existing_media_input_contract() {
        let input = image_inputs(&[PathBuf::from("human.png")]);
        assert!(input.context.is_empty());
        assert_eq!(input.wired[0].images, ["human.png"]);
        assert_eq!(input.wired[0].slot, Some(atlas_agent::InputSlot::Media));
        assert!(image_inputs(&[]).wired.is_empty());
    }
}
