//! Result save snapshots, batch naming, native picker and worker completion.
use super::*;
use atlas_shell::file_picker::{first, FilePicker, PickRequest};
use atlas_shell::result_actions::{BatchNameEdit, BatchSaveResponse};
use crossbeam_channel::Receiver;

#[cfg(test)]
mod tests;
mod worker;

#[derive(Clone)]
enum SaveTarget {
    Results(ResultAddress),
    BoardImage {
        tab: u64,
        node: Box<Node>,
        source: PathBuf,
    },
}

#[derive(Clone)]
struct SaveSnapshot {
    target: SaveTarget,
    doc: slate_doc::SlateDoc,
    nodes: Vec<Node>,
    workbook: Option<PathBuf>,
    extensions: Vec<String>,
}

impl SaveTarget {
    fn valid(&self, workbook: &Option<PathBuf>, app: &SlateApp) -> bool {
        if &app.tab().path != workbook {
            return false;
        }
        match self {
            SaveTarget::Results(address) => app.result_address_valid(address),
            SaveTarget::BoardImage { tab, node, source } => {
                app.tab().id == *tab
                    && app.doc().scene.node(node.id) == Some(node.as_ref())
                    && app.image_item_path(node.id).as_ref() == Some(source)
            }
        }
    }
}

impl SaveSnapshot {
    fn valid(&self, app: &SlateApp) -> bool {
        self.target.valid(&self.workbook, app)
    }
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

#[derive(Debug)]
struct SavedImages {
    paths: Vec<PathBuf>,
    next_single: Vec<PathBuf>,
}

struct SaveWork {
    target: SaveTarget,
    workbook: Option<PathBuf>,
    receiver: Receiver<Result<SavedImages, String>>,
}

#[derive(Default)]
pub(super) struct SaveController {
    pub(super) opener: Option<egui::LayerId>,
    naming: Option<BatchSave>,
    picker: Option<FilePicker<SavePick>>,
    worker: Option<SaveWork>,
    next_single: Vec<PathBuf>,
}

impl SaveController {
    fn single_save_path(&self, extension: &str) -> PathBuf {
        self.next_single
            .iter()
            .find(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
            })
            .cloned()
            .unwrap_or_else(|| format!("render scene.{extension}").into())
    }
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
        if !self.image_save_ready() {
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
            target: SaveTarget::Results(address),
            doc,
            nodes,
            workbook,
            extensions,
        };
        self.begin_image_save(snapshot)
    }

    pub(crate) fn board_image_save_available(&self, id: NodeId) -> bool {
        matches!(
            self.doc().scene.node(id).map(|n| &n.kind),
            Some(NodeKind::Image(_))
        ) && self
            .image_item_path(id)
            .is_some_and(|path| slate_doc::media_kind(&path) == slate_doc::MediaKind::Image)
    }

    pub(super) fn dispatch_board_image_save(
        &mut self,
        ctx: &egui::Context,
        detail: Option<&str>,
    ) -> bool {
        let node = match detail {
            Some(id) => id.parse::<u64>().ok().map(NodeId),
            None => self
                .board_sel
                .iter()
                .copied()
                .find(|id| self.board_image_save_available(*id)),
        };
        node.is_some_and(|node| self.begin_board_image_save(ctx, node))
    }

    pub(crate) fn begin_board_image_save(&mut self, ctx: &egui::Context, id: NodeId) -> bool {
        if !self.board_image_save_available(id) {
            return false;
        }
        if let Some(ImageResultId::Output(output)) = self.image_result_id(id) {
            let Some(binding) = self.result_binding(id) else {
                return false;
            };
            return self.begin_result_save(
                ctx,
                ResultAddress {
                    tab: self.tab().id,
                    host: id,
                    binding,
                    outputs: vec![output],
                },
            );
        }
        if !self.image_save_ready() {
            return false;
        }
        let Some(snapshot) = self.board_image_save_snapshot(id) else {
            return false;
        };
        self.begin_image_save(snapshot)
    }

    fn board_image_save_snapshot(&self, id: NodeId) -> Option<SaveSnapshot> {
        let original = self.doc().scene.node(id)?.clone();
        let source = self.image_item_path(id)?;
        let mut image = self.image_for_selected_result(id)?;
        image.agent = None;
        image.result_layers.clear();
        let mut node = original.clone();
        node.kind = NodeKind::Image(image);
        let mut doc = slate_doc::SlateDoc::new("image save");
        doc.items = self.doc().items.clone();
        let workbook = self.tab().path.clone();
        let extension = worker::preferred_extension(&doc, workbook.as_deref(), &node);
        Some(SaveSnapshot {
            target: SaveTarget::BoardImage {
                tab: self.tab().id,
                node: Box::new(original),
                source,
            },
            doc,
            nodes: vec![node],
            workbook,
            extensions: vec![extension],
        })
    }

    fn image_save_ready(&mut self) -> bool {
        let state = &self.agents.flow.results.save;
        if state.naming.is_some()
            || state.picker.as_ref().is_some_and(FilePicker::is_open)
            || state.worker.is_some()
        {
            self.toast("Finish the current image save first.");
            return false;
        }
        true
    }

    fn begin_image_save(&mut self, snapshot: SaveSnapshot) -> bool {
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
            let suggested = self
                .agents
                .flow
                .results
                .save
                .single_save_path(&snapshot.extensions[0]);
            let request = PickRequest::save()
                .title("Save image as")
                .file_name(suggested.file_name().unwrap_or_default().to_string_lossy())
                .filter("PNG", &["png"])
                .filter("JPEG", &["jpg", "jpeg"])
                .filter("WebP", &["webp"]);
            match suggested
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
            {
                Some(directory) => request.directory(directory),
                None => request,
            }
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
                if picked.snapshot.valid(self) {
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
                    let snapshot = picked.snapshot;
                    let target = snapshot.target.clone();
                    let workbook = snapshot.workbook.clone();
                    match std::thread::Builder::new()
                        .name("slate-save-images".into())
                        .spawn(move || {
                            let result = worker::save(&snapshot, &destinations);
                            let _ = tx.send(result);
                            repaint.request_repaint();
                        }) {
                        Ok(_) => {
                            self.agents.flow.results.save.worker = Some(SaveWork {
                                target,
                                workbook,
                                receiver: rx,
                            })
                        }
                        Err(error) => self.toast(format!("Could not start image save: {error}")),
                    }
                } else {
                    self.toast("The selected image changed while the save dialog was open.");
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
            .and_then(|work| match work.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    Some(Err("Image save stopped before completion.".into()))
                }
                Err(crossbeam_channel::TryRecvError::Empty) => None,
            });
        if let Some(result) = finished {
            let work = self.agents.flow.results.save.worker.take().unwrap();
            if work.target.valid(&work.workbook, self) {
                match result {
                    Ok(SavedImages { paths, next_single }) => {
                        if paths.len() == 1 {
                            self.agents.flow.results.save.next_single = next_single;
                        }
                        match paths.as_slice() {
                            [path] => self.toast(format!(
                                "Saved image as {}.",
                                path.file_name()
                                    .unwrap_or(path.as_os_str())
                                    .to_string_lossy()
                            )),
                            _ => self.toast(format!(
                                "Saved {} images to {}.",
                                paths.len(),
                                paths
                                    .first()
                                    .and_then(|path| path.parent())
                                    .unwrap_or_else(|| std::path::Path::new("."))
                                    .display()
                            )),
                        }
                    }
                    Err(error) => self.toast(error),
                }
            }
        }
        let Some(mut batch) = self.agents.flow.results.save.naming.take() else {
            return false;
        };
        if !batch.snapshot.valid(self) {
            return false;
        }
        let SaveTarget::Results(address) = &batch.snapshot.target else {
            return false;
        };
        let images = self.agent_images(address.host);
        let thumbnails: Vec<_> = address
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
