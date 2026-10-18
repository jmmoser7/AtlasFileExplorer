//! Existing HTML export worker; pinned artifact sources are verified before serialization.
use super::*;

impl SlateApp {
    pub(super) fn do_export_active(&mut self, dir: PathBuf) {
        if self.export_rx.is_some() {
            self.toast("An export is already running.");
            return;
        }
        // Web portals: resolve local sources and capture whatever posters the
        // pool can give us, before anything is written.
        let (web_sources, web_posters) = self.export_web_maps();
        let safe: String = self
            .doc()
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let out = dir.join(format!("{}-slides", safe.trim_matches('-')));
        let mut opts = slate_artifact::ExportOptions {
            agent_images: self.export_agent_images(),
            agent_replies: self.export_agent_replies(),
            inline_assets: self.export_inline,
            thumbs: self.export_thumb_map(),
            model_posters: self.export_model_poster_map(),
            web_sources,
            web_posters,
            wire_routing: self.board_wire_routing,
            wire_theme: Some(board::to_rgba(self.palette().wire)),
            ..Default::default()
        };
        let workbook = self.tab().path.clone();
        let mut doc = self.doc().clone();
        self.resolve_export_image_layers(&mut doc);
        for item in &mut doc.items {
            item.path =
                slate_doc::scene::resolve_source(workbook.as_deref(), &item.path.to_string_lossy());
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.export_rx = Some(rx);
        self.toast("Exporting artifact…");
        std::thread::spawn(move || {
            if let Err(error) = board_agent::prepare_artifact_export_sources(
                &doc,
                workbook.as_deref(),
                &mut opts.web_sources,
            ) {
                let _ = tx.send((out, Err(error)));
                return;
            }
            let loaded = board_slate::collect_slate_boards(&doc, workbook.as_deref());
            opts.workbook = workbook;
            opts.slate_boards = loaded
                .into_iter()
                .map(|(key, (path, child))| {
                    (key, slate_artifact::ExportedBoard { path, doc: child })
                })
                .collect();
            let mut cloud_only = 0;
            for images in opts.agent_images.values_mut() {
                images.retain(|path| {
                    if atlas_core::cloud::is_dehydrated(path) {
                        cloud_only += 1;
                        false
                    } else {
                        true
                    }
                });
            }
            let result = slate_artifact::export_html(&doc, &out, &opts)
                .map(|mut report| {
                    report.missing_assets += cloud_only;
                    report
                })
                .map_err(|e| e.to_string());
            let _ = tx.send((out, result));
        });
    }
}
