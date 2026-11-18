//! Grasshopper definitions in the existing model viewport and board journal.

use super::{application_connector, board_portal::capture::output, SlateApp, SlateTab};
use atlas_connector_runtime::headless_rhino::grasshopper::{Output, Request, Worker};
use atlas_connector_runtime::transport::Cancellation;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use slate_doc::scene::{
    GrasshopperControl, GrasshopperControlId, GrasshopperOverrides, GrasshopperValue, ImageNode,
    NodeId, NodeKind, WorldRect,
};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

mod animation;
mod api;
mod links;
mod packages;
mod schedule;
mod source_watch;
mod worker;
use api::grasshopper_item;
use worker::{response_status, source_revision, start_worker, viewer_id};
#[cfg(test)]
mod tests;
pub(crate) use packages::is_local_plugin as is_grasshopper_plugin;

const LIVE_INTERVAL: Duration = Duration::from_millis(33);
type ViewerKey = (u64, NodeId);
type PortPackets = BTreeMap<GrasshopperControlId, serde_json::Value>;
struct SourceRevisionUpdate {
    key: ViewerKey,
    instance: u64,
    source_key: String,
    before: Option<String>,
    current: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) enum EditPhase {
    Preview,
    Commit,
    Cancel,
}

pub(crate) struct PanelState {
    pub status: Option<String>,
    pub controls: Vec<GrasshopperControl>,
    pub can_bake: bool,
    pub can_retry: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct Source {
    path: PathBuf,
    key: String,
    item_path: PathBuf,
    workbook_path: Option<PathBuf>,
}

impl Source {
    fn matches(&self, tab: &SlateTab, item: &slate_doc::SlateItem) -> bool {
        self.key == item.cache_key && self.item_path == item.path && self.workbook_path == tab.path
    }
}

struct Preview {
    key: String,
    path: PathBuf,
    _owner: Output,
}

struct View {
    source: Source,
    instance: u64,
    desired: GrasshopperOverrides,
    authored: GrasshopperOverrides,
    draft: Option<(GrasshopperControlId, GrasshopperValue)>,
    draft_changed: bool,
    generation: u64,
    source_revision: Option<String>,
    ports: Vec<slate_doc::scene::GrasshopperPort>,
    port_revision: Option<String>,
    linked: Option<PortPackets>,
    wait: Option<String>,
    outputs: Option<PortPackets>,
    due: Option<Instant>,
    retry_startup: bool,
    needs_plugins: bool,
    local_plugins: Vec<PathBuf>,
    controls: Vec<GrasshopperControl>,
    status: Option<String>,
    preview: Option<Preview>,
    displayed_poster: Option<egui::TextureHandle>,
    ready_revision: Option<String>,
    bake: Option<BakeRequest>,
    completed_bake: Option<BakedFile>,
}

impl View {
    fn new(source: Source, instance: u64) -> Self {
        Self {
            source,
            instance,
            desired: GrasshopperOverrides::default(),
            authored: GrasshopperOverrides::default(),
            draft: None,
            draft_changed: false,
            generation: 0,
            source_revision: None,
            ports: Vec::new(),
            port_revision: None,
            linked: Some(PortPackets::new()),
            wait: None,
            outputs: None,
            due: Some(Instant::now()),
            retry_startup: false,
            needs_plugins: false,
            local_plugins: Vec::new(),
            controls: Vec::new(),
            status: Some("Starting Grasshopper…".into()),
            preview: None,
            displayed_poster: None,
            ready_revision: None,
            bake: None,
            completed_bake: None,
        }
    }

    fn update_inputs(&mut self, saved: Option<&GrasshopperOverrides>) {
        let changed = saved.map_or_else(
            || !self.authored.is_empty(),
            |saved| *saved != self.authored,
        );
        if changed {
            self.authored = saved.cloned().unwrap_or_default();
        }
        self.update_desired(changed);
    }

    fn update_desired(&mut self, changed: bool) {
        if !std::mem::take(&mut self.draft_changed) && !changed {
            return;
        }
        let mut desired = self.authored.clone();
        if let Some((id, value)) = &self.draft {
            let _ = desired.set(id.clone(), value.clone());
        }
        if desired != self.desired {
            self.desired = desired;
            let delay = if self.draft.is_some() {
                LIVE_INTERVAL
            } else {
                Duration::ZERO
            };
            self.invalidate(delay);
        }
    }

    fn invalidate(&mut self, delay: Duration) {
        self.generation += 1;
        let due = Instant::now() + delay;
        self.due = Some(self.due.map_or(due, |pending| pending.min(due)));
        self.ready_revision = None;
        self.outputs = None;
        self.bake = None;
        self.completed_bake = None;
        self.status = Some("Updating preview…".into());
    }
}

struct BakeRequest {
    destination: PathBuf,
    revision: String,
    overrides: GrasshopperOverrides,
    inputs: PortPackets,
    generation: u64,
}

struct Job {
    viewer: ViewerKey,
    instance: u64,
    source: Source,
    overrides: GrasshopperOverrides,
    inputs: PortPackets,
    generation: u64,
    bake: Option<BakeRequest>,
    retry_startup: bool,
    local_plugins: Vec<PathBuf>,
}

enum Work {
    Solve(Box<Job>),
    Close(String),
}

struct Completion {
    viewer: ViewerKey,
    instance: u64,
    source: Source,
    generation: u64,
    was_bake: bool,
    result: Result<Solved, SolveError>,
}

enum WorkerEvent {
    Stage {
        viewer: ViewerKey,
        instance: u64,
        generation: u64,
        message: &'static str,
    },
    Finished(Box<Completion>),
}

enum SolveError {
    Failed(String),
    SourceChanged,
}

impl From<String> for SolveError {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

struct Solved {
    output: Output,
    revision: String,
    preview: Option<Result<(model_preview::PreviewScene, String), String>>,
    bake: Option<BakedFile>,
}

struct BakedFile(Option<PathBuf>);

impl Drop for BakedFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct Background {
    requests: Sender<Work>,
    results: Receiver<WorkerEvent>,
    cancel: Cancellation,
    busy: Option<ViewerKey>,
}

impl Drop for Background {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Default)]
pub(super) struct Viewers {
    animation: Option<animation::Animation>,
    views: HashMap<ViewerKey, View>,
    background: Option<Background>,
    next_instance: u64,
    graph: links::Graph,
    links_dirty: bool,
    source_poll: Option<source_watch::Watch>,
    image_revisions: links::image::Revisions,
}

impl Viewers {
    fn remove(&mut self, key: ViewerKey) {
        if let Some(view) = self.views.remove(&key) {
            self.source_poll = None;
            self.links_dirty = true;
            if let Some(background) = &self.background {
                let _ = background
                    .requests
                    .send(Work::Close(viewer_id(key.0, view.instance)));
            }
        }
    }

    fn take_completed_bakes(&mut self, tab: u64) -> Vec<(NodeId, BakedFile)> {
        self.views
            .iter_mut()
            .filter_map(|(&(doc, id), view)| {
                if doc != tab {
                    return None;
                }
                view.completed_bake.take().map(|bake| (id, bake))
            })
            .collect()
    }
}
