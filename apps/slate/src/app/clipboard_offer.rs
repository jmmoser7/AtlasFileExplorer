//! Async clipboard submission shared by board copy and preserved image results.
use super::*;

impl SlateApp {
    pub(crate) fn copy_image_result_payload(
        &mut self,
        ctx: &eframe::egui::Context,
        payload: &[Node],
        doc: &SlateDoc,
    ) -> usize {
        if payload.is_empty() {
            return 0;
        }
        let job = clipboard_job(ctx, payload, doc, self.tab().path.clone());
        if let Some(job) = job {
            self.os_clipboard.submit(job);
        }
        self.board_clipboard = payload.to_vec();
        self.board_paste_count = 0;
        payload.len()
    }

    /// One OS clipboard write for a copy: Slate's node format, plus a bitmap
    /// and file list for pictures or plain text for anything else.
    pub(super) fn offer_os_clipboard(&mut self, ctx: &eframe::egui::Context, payload: &[Node]) {
        let job = clipboard_job(ctx, payload, self.doc(), self.tab().path.clone());
        if let Some(job) = job {
            self.os_clipboard.submit(job);
        }
    }
}

fn clipboard_job(
    ctx: &eframe::egui::Context,
    payload: &[Node],
    doc: &SlateDoc,
    workbook: Option<PathBuf>,
) -> Option<ClipboardJob> {
    let picture_paths: Vec<Option<PathBuf>> = payload
        .iter()
        .map(|n| copy_picture_path(doc, workbook.as_deref(), n))
        .collect();
    let is_picture = |node: &Node| {
        payload
            .iter()
            .position(|n| n.id == node.id)
            .is_some_and(|i| picture_paths[i].is_some())
    };
    let text = clipboard_text_fallback(payload, is_picture);
    if !cfg!(windows) {
        if let Some(text) = &text {
            ctx.copy_text(text.clone());
        }
    }
    let nodes_json = serde_json::to_string(payload).ok()?;
    let (pictures, files) = if text.is_none() {
        (
            payload.to_vec(),
            picture_paths.into_iter().flatten().collect(),
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let mut wanted = HashSet::new();
    let mut stack: Vec<&Node> = pictures.iter().collect();
    while let Some(node) = stack.pop() {
        if let NodeKind::Image(img) = &node.kind {
            wanted.insert(img.item);
            stack.extend(img.paint_layers.iter().flat_map(|l| l.nodes.iter()));
            stack.extend(
                img.result_layers
                    .iter()
                    .flat_map(|result| result.layers.iter())
                    .flat_map(|layer| layer.nodes.iter()),
            );
        }
    }
    let mut job_doc = SlateDoc::new("clipboard");
    job_doc.items = doc
        .items
        .iter()
        .filter(|item| wanted.contains(&item.id))
        .cloned()
        .collect();
    Some(ClipboardJob {
        nodes_json,
        text,
        pictures,
        files,
        doc: job_doc,
        workbook,
    })
}
