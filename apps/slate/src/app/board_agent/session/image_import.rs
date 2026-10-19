//! Register known image-engine outputs using the existing ready-file owner.
//! Only metadata crosses the worker boundary. This never picks an album item,
//! changes a node, paints a preview, or accepts an arbitrary import path.
use super::*;
use crate::app::items::ReadyFile;
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};

const PENDING: usize = 16;
const PER_FRAME: usize = 2;
const REMEMBER: usize = 256;
const MAX_DIMENSION_PROBE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Identity {
    tab: u64,
    workbook: Option<PathBuf>,
    node: NodeId,
    session: String,
    provider: String,
    output: String,
    request: String,
    path: PathBuf,
}

struct Work {
    identity: Identity,
    root: PathBuf,
}
struct ImportedImage {
    file: ReadyFile,
    dimensions: Option<[u32; 2]>,
}
type Completion = (Identity, Result<ImportedImage, String>);

struct ImportRecord {
    error: Option<String>,
    dimensions: Option<[u32; 2]>,
    failures: u8,
    retry_at: Instant,
}

#[derive(Default)]
pub(in crate::app::board_agent) struct KnownImageImports {
    worker: Option<(Sender<Work>, Receiver<Completion>)>,
    pending: HashSet<Identity>,
    completed: HashMap<Identity, ImportRecord>,
    candidates: BTreeSet<NodeId>,
    cursor: u64,
}

impl KnownImageImports {
    /// Fed by the existing portal walk; keep the next bounded range of IDs.
    /// No second scene/session-cache walk or per-frame whole-corpus vector.
    pub(in crate::app::board_agent) fn note_candidate(&mut self, node: NodeId) {
        if node.0 <= self.cursor {
            return;
        }
        self.candidates.insert(node);
        if self.candidates.len() > PENDING {
            self.candidates.pop_last();
        }
    }

    fn blocks_retry(&self, identity: &Identity, now: Instant) -> bool {
        self.completed
            .get(identity)
            .is_some_and(|record| record.error.is_none() || now < record.retry_at)
    }

    fn enqueue(&mut self, work: Work, now: Instant) -> bool {
        if self.pending.len() >= PENDING
            || self.pending.contains(&work.identity)
            || self.blocks_retry(&work.identity, now)
        {
            return false;
        }
        let (sender, _) = self.worker.get_or_insert_with(|| {
            let (send, receive) = bounded::<Work>(PENDING);
            let (done, results) = bounded::<Completion>(PENDING);
            // One bounded metadata worker, scoped to this document runtime.
            std::thread::spawn(move || {
                while let Ok(work) = receive.recv() {
                    let result = inspect(&work);
                    if done.send((work.identity, result)).is_err() {
                        break;
                    }
                }
            });
            (send, results)
        });
        let identity = work.identity.clone();
        if sender.try_send(work).is_err() {
            return false;
        }
        self.pending.insert(identity);
        true
    }

    fn receive(&mut self) -> Vec<Completion> {
        self.worker
            .as_ref()
            .map(|(_, receive)| receive.try_iter().take(PER_FRAME).collect())
            .unwrap_or_default()
    }

    fn remember(
        &mut self,
        identity: Identity,
        error: Option<String>,
        dimensions: Option<[u32; 2]>,
        now: Instant,
    ) {
        let failures = if error.is_some() {
            self.completed
                .get(&identity)
                .map_or(1, |record| record.failures.saturating_add(1))
                .min(8)
        } else {
            0
        };
        let backoff = Duration::from_millis(250)
            .saturating_mul(1u32 << failures.saturating_sub(1))
            .min(Duration::from_secs(30));
        if self.completed.len() >= REMEMBER {
            self.completed.clear();
        }
        self.completed.insert(
            identity,
            ImportRecord {
                error,
                dimensions,
                failures,
                retry_at: now + backoff,
            },
        );
    }
}

fn inspect(work: &Work) -> Result<ImportedImage, String> {
    // Canonicalization is deliberately on this worker. Reject traversal and
    // links escaping the provider's generated-output root before metadata is
    // admitted to the linked-item owner. No file bytes are opened.
    let root = std::fs::canonicalize(&work.root)
        .map_err(|error| format!("Image output folder is unavailable: {error}"))?;
    let path = std::fs::canonicalize(&work.identity.path)
        .map_err(|error| format!("Image output is unavailable: {error}"))?;
    if !path.starts_with(root) || slate_doc::media_kind(&path) != slate_doc::MediaKind::Image {
        return Err("This result is outside its image provider's output folder".into());
    }
    if atlas_core::cloud::is_dehydrated(&path) {
        return Err("Download this generated image before placing it".into());
    }
    // Keep the original stable locator after checking its canonical location.
    let file = ReadyFile::read(work.identity.path.clone())?;
    // Header probing is bounded and stays on the existing worker. Large or
    // unsupported outputs remain importable; their dimensions are unknown.
    let dimensions = (file.size <= MAX_DIMENSION_PROBE_BYTES)
        .then(|| image::image_dimensions(&path).ok())
        .flatten()
        .filter(|(width, height)| *width > 0 && *height > 0)
        .map(|(width, height)| [width, height]);
    Ok(ImportedImage { file, dimensions })
}

impl SlateApp {
    fn known_image_import_for_output(
        &self,
        node: NodeId,
        output: &str,
        request: &str,
    ) -> Option<Work> {
        let agent = slate_doc::agent_chat::agent(self.doc().scene.node(node)?)?;
        if !atlas_ai::agent::local_image_engine(&agent.provider) {
            return None;
        }
        let images = self.agent_images(node);
        let image = images
            .iter()
            .find(|image| image.id == output && image.request == request)?;
        if image.id.is_empty() || image.request.is_empty() || image.source.is_empty() {
            return None;
        }
        let path = resolve_source(self.tab().path.as_deref(), &image.source);
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return None;
        }
        let root = if let Some(workbook) = &self.tab().path {
            atlas_core::workbook_assets::generated_provider_dir(workbook, &agent.provider)
        } else {
            let workspace = self.ai.config.workspace_dir.as_deref()?;
            let link = self.agent_link_dir(node, workspace)?;
            let tag = link.file_name()?.to_str()?;
            atlas_core::workbook_assets::generated_output_dir(
                None,
                &atlas_core::index::data_dir(),
                &agent.provider,
                tag,
                "",
            )
        };
        Some(Work {
            identity: Identity {
                tab: self.tab().id,
                workbook: self.tab().path.clone(),
                node,
                session: agent.session.clone(),
                provider: agent.provider.clone(),
                output: image.id.clone(),
                request: image.request.clone(),
                path,
            },
            root,
        })
    }

    fn known_image_import(&self, node: NodeId) -> Option<Work> {
        let images = self.agent_images(node);
        let image = images.get(self.agent_shown_index(node)?)?;
        self.known_image_import_for_output(node, &image.id, &image.request)
    }

    pub(crate) fn known_agent_image_dimensions(
        &self,
        node: NodeId,
        output: &str,
        request: &str,
    ) -> Option<[u32; 2]> {
        let identity = self
            .known_image_import_for_output(node, output, request)?
            .identity;
        self.agents
            .image_imports
            .completed
            .get(&identity)
            .and_then(|record| record.dimensions)
    }

    fn known_image_import_current(&self, identity: &Identity) -> bool {
        !self.tab().read_only
            && !self.at_home
            && self
                .known_image_import(identity.node)
                .is_some_and(|work| work.identity == *identity)
            && self.agents.cancelled_generations.get(&identity.node) != Some(&identity.request)
    }

    pub(crate) fn agent_image_import_problem(&self, node: NodeId) -> Option<&str> {
        let work = self.known_image_import(node)?;
        self.agents
            .image_imports
            .completed
            .get(&work.identity)?
            .error
            .as_deref()
    }

    pub(in crate::app::board_agent) fn pump_known_agent_image_imports(
        &mut self,
        ctx: &egui::Context,
    ) {
        self.pump_known_agent_image_imports_at(ctx, Instant::now());
    }

    fn pump_known_agent_image_imports_at(&mut self, ctx: &egui::Context, now: Instant) {
        for (identity, result) in self.agents.image_imports.receive() {
            self.agents.image_imports.pending.remove(&identity);
            if !self.known_image_import_current(&identity) {
                continue;
            }
            let (error, dimensions) = match result {
                Ok(image) => {
                    self.item_for_ready_path(&image.file.path, image.file.size, image.file.mtime);
                    (None, image.dimensions)
                }
                Err(error) => (Some(error), None),
            };
            self.agents
                .image_imports
                .remember(identity, error, dimensions, now);
        }
        if self.tab().read_only || self.at_home {
            return;
        }
        // Candidates came from the existing session-owner walk. Admit two
        // files and inspect at most sixteen known result identities per frame.
        let nodes = std::mem::take(&mut self.agents.image_imports.candidates);
        if nodes.is_empty() {
            self.agents.image_imports.cursor = 0;
        }
        let mut admitted = 0;
        for node in nodes {
            self.agents.image_imports.cursor = node.0;
            let Some(work) = self.known_image_import(node) else {
                continue;
            };
            if self.agents.image_imports.blocks_retry(&work.identity, now)
                || self.agents.image_imports.pending.contains(&work.identity)
            {
                continue;
            }
            if self.registered_agent_image_item(node).is_some() {
                continue;
            }
            if self.agents.cancelled_generations.get(&node) == Some(&work.identity.request) {
                continue;
            }
            if self.agents.image_imports.enqueue(work, now) {
                admitted += 1;
            }
            if admitted >= PER_FRAME {
                break;
            }
        }
        if !self.agents.image_imports.pending.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        if let Some(retry) = self
            .agents
            .image_imports
            .completed
            .values()
            .filter(|record| record.error.is_some() && record.retry_at > now)
            .map(|record| record.retry_at)
            .min()
        {
            ctx.request_repaint_after(
                retry
                    .saturating_duration_since(now)
                    .max(Duration::from_millis(40)),
            );
        }
    }
}

#[cfg(test)]
mod tests;
