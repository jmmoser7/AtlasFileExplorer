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

mod links;
mod packages;
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
    views: HashMap<ViewerKey, View>,
    background: Option<Background>,
    next_instance: u64,
    graph: links::Graph,
    links_dirty: bool,
    next_source_poll: Option<Instant>,
    source_poll: Option<Receiver<Vec<SourceRevisionUpdate>>>,
}

impl Viewers {
    fn remove(&mut self, key: ViewerKey) {
        if let Some(view) = self.views.remove(&key) {
            self.links_dirty = true;
            if let Some(background) = &self.background {
                let _ = background
                    .requests
                    .send(Work::Close(viewer_id(key.0, view.instance)));
            }
        }
    }

    fn poll_sources(&mut self, ctx: &egui::Context, tab: u64) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.source_poll {
            match rx.try_recv() {
                Ok(revisions) => {
                    self.source_poll = None;
                    for revision in revisions {
                        let Some(view) = self.views.get_mut(&revision.key) else {
                            continue;
                        };
                        if view.instance == revision.instance
                            && view.source.key == revision.source_key
                            && view.source_revision == revision.before
                            && revision.before != revision.current
                        {
                            view.source_revision = revision.current;
                            if revision.before.is_some() || view.due.is_none() {
                                view.invalidate(Duration::ZERO);
                                changed = true;
                            }
                        }
                    }
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => self.source_poll = None,
                Err(crossbeam_channel::TryRecvError::Empty) => return false,
            }
        }
        let now = Instant::now();
        if let Some(due) = self.next_source_poll.filter(|due| *due > now) {
            if self.views.keys().any(|(doc, _)| *doc == tab) {
                ctx.request_repaint_after(due - now);
            }
            return changed;
        }
        let active: Vec<_> = self
            .views
            .iter()
            .filter(|(key, _)| key.0 == tab)
            .map(|(&key, view)| {
                (
                    key,
                    view.instance,
                    view.source_revision.clone(),
                    view.source.clone(),
                )
            })
            .collect();
        if active.is_empty() {
            return changed;
        }
        self.next_source_poll = Some(now + Duration::from_millis(750));
        let (tx, rx) = crossbeam_channel::bounded(1);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let revisions = active
                .into_iter()
                .map(|(key, instance, before, source)| {
                    let current = source_revision(&source).ok();
                    SourceRevisionUpdate {
                        key,
                        instance,
                        source_key: source.key,
                        before,
                        current,
                    }
                })
                .collect();
            let _ = tx.send(revisions);
            ctx.request_repaint();
        });
        self.source_poll = Some(rx);
        changed
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

fn grasshopper_item(tab: &SlateTab, id: NodeId) -> Option<(&ImageNode, &slate_doc::SlateItem)> {
    let NodeKind::Image(image) = &tab.doc.scene.node(id)?.kind else {
        return None;
    };
    let item = tab.doc.item(image.item)?;
    slate_doc::media::is_grasshopper(&item.path).then_some((image, item))
}

impl SlateApp {
    pub(crate) fn grasshopper_viewer(&self, id: NodeId) -> bool {
        grasshopper_item(self.tab(), id).is_some()
    }

    pub(crate) fn request_grasshopper(&mut self, id: NodeId, ctx: &egui::Context) {
        let tab = self.tab();
        let key = (tab.id, id);
        let Some((_, item)) = grasshopper_item(tab, id) else {
            return;
        };
        if self
            .grasshopper
            .views
            .get(&key)
            .is_some_and(|view| view.source.matches(tab, item))
        {
            return;
        }
        let source = Source {
            path: slate_doc::scene::resolve_source(
                tab.path.as_deref(),
                &item.path.to_string_lossy(),
            ),
            key: item.cache_key.clone(),
            item_path: item.path.clone(),
            workbook_path: tab.path.clone(),
        };
        if let Some(view) = self.grasshopper.views.get_mut(&key) {
            if view.source.path == source.path && view.source.workbook_path == source.workbook_path
            {
                view.source = source;
                view.source_revision = None;
                view.invalidate(Duration::ZERO);
                self.grasshopper.links_dirty = true;
                ctx.request_repaint();
                return;
            }
        }
        if self.grasshopper.views.contains_key(&key) {
            self.clear_grasshopper_preview(key.0, id);
            self.grasshopper.remove(key);
        }
        self.grasshopper.next_instance += 1;
        self.grasshopper
            .views
            .insert(key, View::new(source, self.grasshopper.next_instance));
        self.grasshopper.links_dirty = true;
        ctx.request_repaint();
    }

    pub(crate) fn grasshopper_preview(
        &self,
        doc: u64,
        id: NodeId,
        source_key: &str,
    ) -> Option<(&str, &Path)> {
        let view = self.grasshopper.views.get(&(doc, id))?;
        if view.source.key != source_key {
            return None;
        }
        let preview = view.preview.as_ref()?;
        Some((&preview.key, &preview.path))
    }

    pub(crate) fn grasshopper_status(&self, id: NodeId) -> Option<&str> {
        self.grasshopper
            .views
            .get(&(self.tab().id, id))?
            .status
            .as_deref()
    }

    pub(crate) fn grasshopper_working_label(&self, id: NodeId) -> Option<&'static str> {
        let key = (self.tab().id, id);
        let view = self.grasshopper.views.get(&key)?;
        if view.bake.is_some() {
            return Some("Baking geometry");
        }
        let solving = self
            .grasshopper
            .background
            .as_ref()
            .is_some_and(|worker| worker.busy == Some(key));
        if view.due.is_none() && !solving {
            return None;
        }
        if view.wait.is_some() {
            return view
                .wait
                .as_deref()
                .filter(|status| status.starts_with("Waiting"))
                .map(|_| "Waiting for connected inputs");
        }
        Some(match view.status.as_deref() {
            Some(status) if status.starts_with("Baking") => "Baking geometry",
            Some(status) if status.starts_with("Starting") => "Starting Grasshopper",
            _ => "Solving Grasshopper",
        })
    }

    /// Keep one rendered poster per viewer while a replacement is being drawn.
    pub(crate) fn grasshopper_displayed_poster(
        &mut self,
        id: NodeId,
        fresh: Option<egui::TextureHandle>,
    ) -> Option<egui::TextureId> {
        let key = (self.tab().id, id);
        let view = self.grasshopper.views.get_mut(&key)?;
        if let Some(fresh) = fresh {
            view.displayed_poster = Some(fresh);
        }
        view.displayed_poster.as_ref().map(egui::TextureHandle::id)
    }

    pub(crate) fn grasshopper_panel(&self, id: NodeId) -> Option<PanelState> {
        let view = self.grasshopper.views.get(&(self.tab().id, id))?;
        let mut controls = view.controls.clone();
        for control in &mut controls {
            if let Some(value) = view.desired.get(&control.id) {
                control.value = Some(value.clone());
            }
        }
        Some(PanelState {
            status: view.status.clone(),
            controls,
            can_bake: view.ready_revision.is_some()
                && view.preview.is_some()
                && view.bake.is_none()
                && !self.tab().read_only,
            can_retry: view.ready_revision.is_none()
                && view.status.is_some()
                && view.due.is_none()
                && self
                    .grasshopper
                    .background
                    .as_ref()
                    .is_none_or(|bg| bg.busy != Some((self.tab().id, id))),
        })
    }

    pub(crate) fn grasshopper_control_count(&self, id: NodeId) -> usize {
        self.grasshopper
            .views
            .get(&(self.tab().id, id))
            .map_or(0, |view| view.controls.len())
    }

    pub(crate) fn grasshopper_control(
        &self,
        node: NodeId,
        id: &GrasshopperControlId,
    ) -> Option<&GrasshopperControl> {
        self.grasshopper
            .views
            .get(&(self.tab().id, node))?
            .controls
            .iter()
            .find(|control| &control.id == id)
    }

    pub(crate) fn grasshopper_accepts_plugin_drop(&self, id: NodeId) -> bool {
        let key = (self.tab().id, id);
        self.grasshopper.views.get(&key).is_some_and(|view| {
            view.needs_plugins && view.due.is_none() && view.local_plugins.is_empty()
        }) && self
            .grasshopper
            .background
            .as_ref()
            .is_none_or(|bg| bg.busy != Some(key))
    }

    pub(crate) fn grasshopper_drop_plugins(&mut self, id: NodeId, mut paths: Vec<PathBuf>) -> bool {
        if !self.grasshopper_accepts_plugin_drop(id) || !self.grasshopper_viewer(id) {
            return false;
        }
        paths.retain(|path| is_grasshopper_plugin(path));
        paths.sort_unstable_by(|a, b| {
            packages::is_raw_plugin(a)
                .cmp(&packages::is_raw_plugin(b))
                .then_with(|| a.cmp(b))
        });
        paths.dedup();
        if paths.is_empty() {
            return false;
        }
        let key = (self.tab().id, id);
        let view = self
            .grasshopper
            .views
            .get_mut(&key)
            .expect("missing plug-in viewer");
        view.generation += 1;
        view.local_plugins = paths;
        view.retry_startup = true;
        view.due = Some(Instant::now());
        view.status = Some("Checking dropped Grasshopper plug-ins…".into());
        true
    }

    pub(crate) fn grasshopper_edit(
        &mut self,
        id: NodeId,
        control: GrasshopperControlId,
        value: GrasshopperValue,
        phase: EditPhase,
    ) {
        if self.tab().read_only {
            return;
        }
        let key = (self.tab().id, id);
        let Some(view) = self.grasshopper.views.get_mut(&key) else {
            return;
        };
        view.draft_changed = true;
        match phase {
            EditPhase::Preview => view.draft = Some((control, value)),
            EditPhase::Cancel => view.draft = None,
            EditPhase::Commit => {
                view.draft = None;
                self.last_board_edit = None;
                self.patch_nodes(&[id], |node| {
                    if let NodeKind::Image(image) = &mut node.kind {
                        let overrides = image.grasshopper.get_or_insert_with(Box::default);
                        let _ = overrides.set(control.clone(), value.clone());
                    }
                });
                self.last_board_edit = None;
            }
        }
    }

    pub(crate) fn grasshopper_retry(&mut self, id: NodeId) {
        if let Some(view) = self.grasshopper.views.get_mut(&(self.tab().id, id)) {
            view.invalidate(Duration::ZERO);
            view.retry_startup = true;
            view.status = Some("Retrying Grasshopper…".into());
            self.grasshopper.links_dirty = true;
        }
    }

    pub(crate) fn dispatch_grasshopper_bake(&mut self, detail: Option<&str>) -> bool {
        let id = match detail {
            Some(id) => id.parse().ok().map(NodeId),
            None if self.board_sel.len() == 1 => self.board_sel.iter().next().copied(),
            None => None,
        };
        let Some(id) = id else {
            return false;
        };
        let tab = self.tab();
        let Some((_, item)) = grasshopper_item(tab, id) else {
            return false;
        };
        let key = (tab.id, id);
        if tab.read_only
            || !self.grasshopper.views.get(&key).is_some_and(|view| {
                view.source.matches(tab, item)
                    && view.preview.is_some()
                    && view.bake.is_none()
                    && view.draft.is_none()
                    && view.ready_revision.is_some()
            })
        {
            return false;
        }
        let destination = output::capture_path(tab.path.as_deref(), id, "3dm");
        let view = self.grasshopper.views.get_mut(&key).expect("active viewer");
        let Some(revision) = view.ready_revision.take() else {
            return false;
        };
        view.bake = Some(BakeRequest {
            destination,
            revision,
            overrides: view.desired.clone(),
            inputs: view.linked.clone().unwrap_or_default(),
            generation: view.generation,
        });
        view.status = Some("Baking visible geometry…".into());
        true
    }

    pub(super) fn grasshopper_frame(&mut self, ctx: &egui::Context) {
        if self.grasshopper.views.is_empty() && self.grasshopper.background.is_none() {
            return;
        }
        let active_doc = self.tab().id;
        self.request_grasshopper_dependencies(ctx);
        let mut links_changed = std::mem::take(&mut self.grasshopper.links_dirty);
        links_changed |= self.grasshopper.poll_sources(ctx, active_doc);
        let keys: Vec<_> = self.grasshopper.views.keys().copied().collect();
        for key in keys {
            let image = self
                .tabs
                .iter()
                .find(|tab| tab.id == key.0)
                .and_then(|tab| {
                    let (image, item) = grasshopper_item(tab, key.1)?;
                    self.grasshopper.views[&key]
                        .source
                        .matches(tab, item)
                        .then_some(image)
                });
            if let Some(image) = image {
                let view = self.grasshopper.views.get_mut(&key).expect("viewer key");
                let generation = view.generation;
                if key.0 != active_doc && view.draft.take().is_some() {
                    view.draft_changed = true;
                }
                view.update_inputs(image.grasshopper.as_deref());
                links_changed |= generation != view.generation;
            } else {
                self.clear_grasshopper_preview(key.0, key.1);
                self.grasshopper.remove(key);
                links_changed = true;
            }
        }
        if links_changed {
            self.update_grasshopper_links();
        }
        let mut finished = false;
        while let Some(event) = self
            .grasshopper
            .background
            .as_ref()
            .and_then(|bg| bg.results.try_recv().ok())
        {
            match event {
                WorkerEvent::Stage {
                    viewer,
                    instance,
                    generation,
                    message,
                } => {
                    if let Some(view) = self.grasshopper.views.get_mut(&viewer) {
                        if (view.instance, view.generation) == (instance, generation)
                            && view.wait.is_none()
                        {
                            view.status = Some(message.into());
                        }
                    }
                }
                WorkerEvent::Finished(completion) => {
                    self.grasshopper.background.as_mut().expect("worker").busy = None;
                    if completion.was_bake {
                        ctx.request_repaint();
                    }
                    self.complete_grasshopper(*completion);
                    finished = true;
                }
            }
        }
        if finished {
            self.update_grasshopper_links();
        }
        for (id, bake) in self.grasshopper.take_completed_bakes(active_doc) {
            self.place_grasshopper_bake(id, bake);
        }

        if self
            .grasshopper
            .background
            .as_ref()
            .is_some_and(|bg| bg.busy.is_some())
        {
            return;
        }
        let now = Instant::now();
        let next = self
            .grasshopper
            .views
            .iter()
            .filter(|(key, view)| {
                key.0 == active_doc
                    && view.wait.is_none()
                    && (view.due.is_some() || view.bake.is_some())
            })
            .min_by_key(|(_, view)| (view.bake.is_none(), view.due.unwrap_or(now)))
            .map(|(key, view)| {
                (
                    *key,
                    if view.bake.is_some() {
                        now
                    } else {
                        view.due.unwrap_or(now)
                    },
                )
            });
        let Some((key, due)) = next else {
            return;
        };
        if due > now {
            ctx.request_repaint_after(due - now);
            return;
        }
        let view = self.grasshopper.views.get_mut(&key).expect("viewer key");
        let bake = view.bake.take();
        if bake.is_none() {
            view.due = None;
        }
        let job = Job {
            viewer: key,
            instance: view.instance,
            source: view.source.clone(),
            overrides: bake
                .as_ref()
                .map_or(&view.desired, |bake| &bake.overrides)
                .clone(),
            inputs: bake.as_ref().map_or_else(
                || view.linked.clone().unwrap_or_default(),
                |bake| bake.inputs.clone(),
            ),
            generation: bake
                .as_ref()
                .map_or(view.generation, |bake| bake.generation),
            bake,
            retry_startup: std::mem::take(&mut view.retry_startup),
            local_plugins: std::mem::take(&mut view.local_plugins),
        };
        let background = self
            .grasshopper
            .background
            .get_or_insert_with(|| Background::start(ctx));
        if background.requests.send(Work::Solve(Box::new(job))).is_ok() {
            background.busy = Some(key);
        } else {
            view.status = Some("The Grasshopper worker stopped. Retry to start it again.".into());
            self.grasshopper.background = None;
        }
    }

    fn complete_grasshopper(&mut self, completion: Completion) {
        let key = completion.viewer;
        let Some(view) = self.grasshopper.views.get_mut(&key) else {
            return;
        };
        if view.instance != completion.instance
            || view.source != completion.source
            || view.generation != completion.generation
        {
            return;
        }
        if let Ok(solved) = &completion.result {
            view.source_revision = Some(solved.revision.clone());
        }
        if completion.was_bake {
            match completion.result {
                Ok(solved) if solved.bake.is_some() => {
                    view.completed_bake = solved.bake;
                    view.ready_revision = Some(solved.revision);
                    view.status = solved.output.recovery_warning().map(str::to_owned);
                }
                Ok(solved) => view.status = Some(response_status(&solved.output)),
                Err(SolveError::Failed(error)) => view.status = Some(error),
                Err(SolveError::SourceChanged) => view.invalidate(Duration::ZERO),
            }
            return;
        }
        let solved = match completion.result {
            Ok(solved) => solved,
            Err(SolveError::Failed(error)) => {
                view.status = Some(error);
                return;
            }
            Err(SolveError::SourceChanged) => {
                view.invalidate(Duration::ZERO);
                return;
            }
        };
        if let Ok(controls) = serde_json::from_value(solved.output.response.controls.clone()) {
            view.controls = controls;
        }
        if let Ok(ports) = serde_json::from_value(solved.output.response.ports.clone()) {
            if view.ports != ports {
                self.connector_pick_bounds = None;
            }
            view.ports = ports;
            view.port_revision = Some(solved.revision.clone());
        }
        view.needs_plugins = solved.output.response.status == "missing_packages";
        if solved.output.response.status != "ready" {
            view.status = Some(response_status(&solved.output));
            return;
        }
        view.outputs = Some(
            serde_json::from_value(solved.output.response.outputs.clone()).unwrap_or_default(),
        );
        match solved.preview {
            Some(Ok((scene, hash))) => {
                let cache_key = format!("grasshopper-{}-{}-{hash}", key.0, view.instance);
                let path = solved.output.preview_path().expect("preview path");
                view.status = solved.output.response.diagnostics.first().cloned();
                view.ready_revision = Some(solved.revision);
                view.preview = Some(Preview {
                    key: cache_key.clone(),
                    path,
                    _owner: solved.output,
                });
                self.install_grasshopper_preview(key.0, key.1, cache_key, scene, hash);
            }
            Some(Err(error)) => view.status = Some(error),
            None => {
                view.preview = None;
                view.displayed_poster = None;
                view.status = Some(response_status(&solved.output));
                self.clear_grasshopper_preview(key.0, key.1);
            }
        }
    }

    fn place_grasshopper_bake(&mut self, source: NodeId, mut bake: BakedFile) {
        if self.tab().read_only || !self.grasshopper_viewer(source) {
            return;
        }
        let Some(rect) = self.doc().scene.node(source).map(|node| node.rect) else {
            return;
        };
        let previous = self
            .doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find_map(|node| match &node.kind {
                NodeKind::Image(image) if image.grasshopper_bake_source == Some(source) => {
                    Some(node.rect)
                }
                _ => None,
            });
        let (x, y) = previous.map_or((rect.x + rect.w + 24.0, rect.y), |previous| {
            (previous.x, previous.y + previous.h + 24.0)
        });
        let Some(item) = bake.0.as_deref().and_then(|path| self.item_for_path(path)) else {
            self.toast("Could not add the Grasshopper bake to the workbook");
            return;
        };
        let mut image = ImageNode::new(item);
        image.grasshopper_bake_source = Some(source);
        let node = self
            .doc()
            .scene
            .node_preparation()
            .build_node(WorldRect::new(x, y, rect.w, rect.h), NodeKind::Image(image));
        if !self.add_nodes(vec![node]).is_empty() {
            bake.0.take();
            self.toast("Baked Grasshopper preview to a 3D model");
        }
    }
}

impl Background {
    fn start(ctx: &egui::Context) -> Self {
        let (requests, jobs) = crossbeam_channel::unbounded();
        let (done, results) = crossbeam_channel::unbounded();
        let cancel = Cancellation::default();
        let stop = cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut worker = None;
            let mut startup_failure = None;
            while let Ok(work) = jobs.recv() {
                if stop.is_cancelled() {
                    break;
                }
                match work {
                    Work::Close(viewer) => {
                        if let Some(warm) = worker.as_mut() {
                            if close_viewer(warm, &viewer, &stop).is_err() {
                                worker = None;
                            }
                        }
                    }
                    Work::Solve(job) => {
                        if job.retry_startup {
                            startup_failure = None;
                            if worker.as_ref().is_some_and(Worker::is_recovery) {
                                worker = None;
                            }
                        }
                        let stage = |message| {
                            let _ = done.send(WorkerEvent::Stage {
                                viewer: job.viewer,
                                instance: job.instance,
                                generation: job.generation,
                                message,
                            });
                            ctx.request_repaint();
                        };
                        let result = run_job(&mut worker, &mut startup_failure, &job, &stop, stage);
                        let _ = done.send(WorkerEvent::Finished(Box::new(Completion {
                            viewer: job.viewer,
                            instance: job.instance,
                            source: job.source,
                            generation: job.generation,
                            was_bake: job.bake.is_some(),
                            result,
                        })));
                        ctx.request_repaint();
                    }
                }
            }
        });
        Self {
            requests,
            results,
            cancel,
            busy: None,
        }
    }
}

fn close_viewer(worker: &mut Worker, viewer_id: &str, stop: &Cancellation) -> Result<(), String> {
    worker
        .request(
            &Request {
                action: "close",
                viewer_id,
                source_path: None,
                source_revision: None,
                overrides: None,
                inputs: None,
            },
            stop,
        )
        .map(|_| ())
}

fn viewer_id(doc: u64, instance: u64) -> String {
    format!("{doc:016x}{instance:016x}")
}

fn source_revision(source: &Source) -> Result<String, String> {
    let metadata = std::fs::metadata(&source.path)
        .map_err(|error| format!("Could not read the Grasshopper definition: {error}"))?;
    Ok(format!(
        "{}:{}:{:?}",
        source.key,
        metadata.len(),
        metadata.modified()
    ))
}

fn run_job(
    worker: &mut Option<Worker>,
    startup_failure: &mut Option<String>,
    job: &Job,
    stop: &Cancellation,
    stage: impl Fn(&'static str),
) -> Result<Solved, SolveError> {
    if let Some(error) = startup_failure {
        return Err(error.clone().into());
    }
    if atlas_core::cloud::is_dehydrated(&job.source.path) {
        return Err(
            "Make the Grasshopper definition available locally, then retry."
                .to_owned()
                .into(),
        );
    }
    let revision = source_revision(&job.source)?;
    if job
        .bake
        .as_ref()
        .is_some_and(|bake| bake.revision != revision)
    {
        return Err(SolveError::SourceChanged);
    }
    let config = application_connector::headless_rhino_configuration()?;
    if worker.is_none() {
        stage("Starting headless Rhino and loading Grasshopper components…");
        *worker = Some(start_worker(&config, stop, startup_failure)?);
    }
    stage(if job.bake.is_some() {
        "Baking visible geometry…"
    } else {
        "Solving Grasshopper definition…"
    });
    let viewer = viewer_id(job.viewer.0, job.instance);
    let overrides = serde_json::to_value(&job.overrides).map_err(|error| error.to_string())?;
    let inputs = serde_json::to_value(&job.inputs).map_err(|error| error.to_string())?;
    let request = Request {
        action: if job.bake.is_some() { "bake" } else { "solve" },
        viewer_id: &viewer,
        source_path: Some(&job.source.path),
        source_revision: Some(&revision),
        overrides: Some(&overrides),
        inputs: Some(&inputs),
    };
    let mut output = match worker
        .as_mut()
        .expect("worker initialized")
        .request(&request, stop)
    {
        Ok(output) => output,
        Err(error) => {
            *worker = None;
            return Err(error.into());
        }
    };
    if output.response.status == "missing_packages" && job.bake.is_none() {
        packages::Restorer {
            worker,
            config: &config,
            startup_failure,
            stop,
            stage: &stage,
        }
        .resolve(&mut output, &request, &job.local_plugins);
    }
    let preview = if job.bake.is_none() {
        output.preview_path().map(|path| {
            stage("Preparing the 3D preview…");
            super::model3d::parse_grasshopper_preview(&path)
        })
    } else {
        None
    };
    let bake = if let (Some(path), Some(bake)) = (output.bake_path(), &job.bake) {
        let draft = output::Draft::reserve(bake.destination.clone())?;
        std::fs::copy(path, draft.path())
            .map_err(|error| format!("Could not copy the Grasshopper bake: {error}"))?;
        Some(BakedFile(Some(draft.commit_unique()?)))
    } else {
        None
    };
    if source_revision(&job.source).as_deref() != Ok(revision.as_str()) {
        return Err(SolveError::SourceChanged);
    }
    Ok(Solved {
        output,
        revision,
        preview,
        bake,
    })
}

fn start_worker(
    config: &atlas_connector_runtime::headless_rhino::HeadlessRhino,
    stop: &Cancellation,
    failure: &mut Option<String>,
) -> Result<Worker, String> {
    Worker::start(
        &config.executable,
        &config.rhino_system,
        &config.workspace,
        stop,
    )
    .inspect_err(|error| *failure = Some(error.clone()))
}

fn response_status(output: &Output) -> String {
    output
        .response
        .diagnostics
        .first()
        .cloned()
        .or_else(|| output.response.error.clone())
        .unwrap_or_else(|| match output.response.status.as_str() {
            "ready" => "No visible preview geometry".into(),
            "missing_packages" => "Grasshopper needs additional packages".into(),
            _ => "Grasshopper did not complete the solve".into(),
        })
}
