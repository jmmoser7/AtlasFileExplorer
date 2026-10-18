//! Result save snapshots, batch naming, native picker and worker completion.
use super::*;
use atlas_shell::file_picker::{first, FilePicker, PickRequest};
use atlas_shell::result_actions::{BatchNameEdit, BatchSaveResponse};
use crossbeam_channel::Receiver;

#[cfg(test)]
mod tests;
mod worker;

#[derive(Clone)]
struct SaveSnapshot {
    address: ResultAddress,
    doc: slate_doc::SlateDoc,
    nodes: Vec<Node>,
    workbook: Option<PathBuf>,
    extensions: Vec<String>,
}

struct BatchSave {
    snapshot: SaveSnapshot,
    names: BatchNameEdit,
    opener: egui::LayerId,
}

struct SavePick {
    snapshot: SaveSnapshot,
    names: Option<BatchNameEdit>,
    destination: Option<PathBuf>,
}

#[derive(Default)]
pub(super) struct SaveController {
    pub(super) opener: Option<egui::LayerId>,
    naming: Option<BatchSave>,
    picker: Option<FilePicker<SavePick>>,
    worker: Option<Receiver<Result<usize, String>>>,
}

impl SlateApp {
    #[cfg(test)]
    pub(in crate::app::board_flow::result_actions) fn result_batch_name_for_test(
        &self,
    ) -> Option<&str> {
        self.agents
            .flow
            .results
            .save
            .naming
            .as_ref()
            .map(|batch| batch.names.base.as_str())
    }
    pub(super) fn begin_result_save(
        &mut self,
        _ctx: &egui::Context,
        address: ResultAddress,
    ) -> bool {
        let state = &self.agents.flow.results.save;
        if state.naming.is_some()
            || state.picker.as_ref().is_some_and(FilePicker::is_open)
            || state.worker.is_some()
        {
            self.toast("Finish the current image save first.");
            return false;
        }
        let (doc, nodes) = self.result_payload(&address);
        if nodes.len() != address.outputs.len() {
            return false;
        }
        let workbook = self.tab().path.clone();
        let extensions = nodes
            .iter()
            .map(|node| worker::preferred_extension(&doc, workbook.as_deref(), node))
            .collect();
        let snapshot = SaveSnapshot {
            address,
            doc,
            nodes,
            workbook,
            extensions,
        };
        if snapshot.nodes.len() > 1 {
            let opener = self
                .agents
                .flow
                .results
                .save
                .opener
                .take()
                .unwrap_or_else(egui::LayerId::background);
            self.agents.flow.results.save.naming = Some(BatchSave {
                snapshot,
                names: BatchNameEdit::default(),
                opener,
            });
            true
        } else {
            self.open_result_save_picker(snapshot, None)
        }
    }

    fn open_result_save_picker(
        &mut self,
        snapshot: SaveSnapshot,
        names: Option<BatchNameEdit>,
    ) -> bool {
        let request = if names.is_some() {
            PickRequest::folder().title("Choose folder for images")
        } else {
            PickRequest::save()
                .title("Save image as")
                .file_name(format!("render scene.{}", snapshot.extensions[0]))
                .filter("PNG", &["png"])
                .filter("JPEG", &["jpg", "jpeg"])
                .filter("WebP", &["webp"])
        };
        let mut picker = self
            .agents
            .flow
            .results
            .save
            .picker
            .take()
            .unwrap_or_else(|| self.dialogs.picker());
        let opened = picker.open(request, move |picked| SavePick {
            snapshot,
            names,
            destination: first(picked),
        });
        self.agents.flow.results.save.picker = Some(picker);
        opened
    }

    pub(super) fn result_save_input(&mut self, ctx: &egui::Context) -> bool {
        let picked = self
            .agents
            .flow
            .results
            .save
            .picker
            .as_mut()
            .and_then(|picker| picker.poll(ctx));
        if let Some(picked) = picked {
            if let Some(destination) = picked.destination {
                if self.result_address_valid(&picked.snapshot.address) {
                    let destinations = if let Some(names) = picked.names {
                        picked
                            .snapshot
                            .extensions
                            .iter()
                            .enumerate()
                            .map(|(index, extension)| {
                                destination.join(names.file_name(index, extension))
                            })
                            .collect()
                    } else {
                        let mut destination = destination;
                        if destination.extension().is_none() {
                            destination.set_extension(&picked.snapshot.extensions[0]);
                        }
                        vec![destination]
                    };
                    let (tx, rx) = crossbeam_channel::bounded(1);
                    let repaint = ctx.clone();
                    let multiple = picked.snapshot.nodes.len() > 1;
                    match std::thread::Builder::new()
                        .name("slate-save-images".into())
                        .spawn(move || {
                            let result = worker::save(&picked.snapshot, &destinations, multiple);
                            let _ = tx.send(result);
                            repaint.request_repaint();
                        }) {
                        Ok(_) => self.agents.flow.results.save.worker = Some(rx),
                        Err(error) => self.toast(format!("Could not start image save: {error}")),
                    }
                } else {
                    self.toast(
                        "The selected image results changed while the save dialog was open.",
                    );
                }
            }
        }
        let finished = self
            .agents
            .flow
            .results
            .save
            .worker
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    Some(Err("Image save stopped before completion.".into()))
                }
                Err(crossbeam_channel::TryRecvError::Empty) => None,
            });
        if let Some(result) = finished {
            self.agents.flow.results.save.worker = None;
            match result {
                Ok(count) => self.toast(format!(
                    "Saved {count} image{}.",
                    if count == 1 { "" } else { "s" }
                )),
                Err(error) => self.toast(error),
            }
        }
        let Some(mut batch) = self.agents.flow.results.save.naming.take() else {
            return false;
        };
        if !self.result_address_valid(&batch.snapshot.address) {
            return false;
        }
        let images = self.agent_images(batch.snapshot.address.host);
        let thumbnails: Vec<_> = batch
            .snapshot
            .address
            .outputs
            .iter()
            .map(|output| {
                let image = images.iter().find(|image| image.id == *output)?;
                self.linked_image_texture(
                    slate_doc::scene::resolve_source(self.tab().path.as_deref(), &image.source),
                    output,
                    112.0,
                )
                .map(|t| t.id())
            })
            .collect();
        match atlas_shell::result_actions::batch_save_dialog(
            ctx,
            &mut batch.names,
            &thumbnails,
            &batch.snapshot.extensions,
            batch.opener,
        ) {
            BatchSaveResponse::None => self.agents.flow.results.save.naming = Some(batch),
            BatchSaveResponse::Cancel => {}
            BatchSaveResponse::ChooseFolder => {
                self.open_result_save_picker(batch.snapshot, Some(batch.names));
            }
        }
        true
    }
}
