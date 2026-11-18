//! Typed outputs belong to a specific page, navigation and workbook.
use super::*;
#[cfg(test)]
use slate_doc::dashboard_outputs::parse;
use slate_doc::dashboard_outputs::{
    controller::{observation, ControllerEdit},
    Snapshot,
};

pub(super) mod controller;
use std::sync::mpsc::{self, Receiver, SyncSender};

#[derive(Default)]
pub(super) struct PageOutputs {
    authored: String,
    controller_script: Option<(Option<slate_doc::scene::GrasshopperPortId>, String)>,
    url: Option<url::Url>,
    page: Option<(u64, u64)>,
    pub(super) inspected: Option<(u64, u64)>,
    pub(super) authored_input: bool,
    sampled: bool,
    raw: Option<String>,
    queued: Option<String>,
    pending: Option<u64>,
    snapshot: Option<Snapshot>,
    error: Option<&'static str>,
}

impl PageOutputs {
    pub(super) fn bind(&mut self, authored: &str) {
        if authored == self.authored {
            return;
        }
        let url = if Path::new(authored).is_absolute() {
            url::Url::from_file_path(authored).ok()
        } else {
            url::Url::parse(authored).ok()
        };
        *self = Self {
            authored: authored.into(),
            url: url.map(without_fragment),
            ..Default::default()
        };
    }

    pub(super) fn append_controller_script(
        &mut self,
        script: &mut String,
        state: Option<&slate_doc::dashboard_outputs::controller::ControllerState>,
    ) {
        let revision = state.map(|s| &s.revision);
        if self
            .controller_script
            .as_ref()
            .is_none_or(|(cached, _)| cached.as_ref() != revision)
        {
            self.controller_script = Some((
                revision.cloned(),
                controller::install_script(&self.authored, state),
            ));
        }
        if let Some((_, install)) = &self.controller_script {
            script.push_str(install)
        }
    }

    fn reset_page(&mut self, page: Option<(u64, u64)>) -> bool {
        if page == self.page {
            return false;
        }
        self.page = page;
        self.authored_input = false;
        self.sampled = false;
        self.pending = None;
        self.raw = None;
        self.queued = None;
        self.error = None;
        self.snapshot.take().is_some()
    }
}

fn without_fragment(mut url: url::Url) -> url::Url {
    url.set_fragment(None);
    url
}

type Job = (NodeId, u64, String, egui::Context);
type Reply = (
    NodeId,
    u64,
    Result<(Snapshot, Option<ControllerEdit>), &'static str>,
);

pub(super) struct Worker {
    send: SyncSender<Job>,
    recv: Receiver<Reply>,
    last: u64,
}
impl Worker {
    fn new() -> Option<Self> {
        let (send, jobs) = mpsc::sync_channel::<Job>(LIVE_POOL);
        let (results, recv) = mpsc::channel();
        std::thread::Builder::new()
            .name("slate-html-outputs".into())
            .spawn(move || {
                while let Ok((id, ticket, raw, ctx)) = jobs.recv() {
                    if results.send((id, ticket, observation(&raw))).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Self {
            send,
            recv,
            last: 0,
        })
    }

    fn submit(&mut self, id: NodeId, raw: &str, ctx: &egui::Context) -> Option<u64> {
        self.last = self.last.wrapping_add(1);
        self.send
            .try_send((id, self.last, raw.to_owned(), ctx.clone()))
            .ok()?;
        Some(self.last)
    }
}

impl SlateApp {
    #[cfg(test)]
    pub(crate) fn set_dashboard_test_document(&mut self, id: NodeId, raw: &str) {
        let tab = self.tab().id;
        let view = self
            .web
            .views
            .entry(id)
            .or_insert_with(|| WebView::new("test".into()));
        view.outputs.page = Some((tab, 1));
        view.outputs.snapshot = parse(raw).ok();
        view.outputs.sampled = view.outputs.snapshot.is_some();
        self.dashboard_outputs_changed(true);
    }

    pub(crate) fn dashboard_was_sampled(&self, id: NodeId) -> bool {
        self.web.views.get(&id).is_some_and(|v| {
            v.outputs.sampled && v.outputs.page.is_some_and(|p| p.0 == self.tab().id)
        })
    }

    pub(crate) fn dashboard_output_snapshot(&self, id: NodeId) -> Option<&Snapshot> {
        let output = &self.web.views.get(&id)?.outputs;
        (output.page?.0 == self.tab().id)
            .then_some(output.snapshot.as_ref())
            .flatten()
    }

    pub(crate) fn dashboard_output_error(&self, id: NodeId) -> Option<&'static str> {
        let output = &self.web.views.get(&id)?.outputs;
        (output.page?.0 == self.tab().id)
            .then_some(output.error)
            .flatten()
    }

    pub(super) fn pump_dashboard_outputs(&mut self, ctx: &egui::Context) {
        let tab = self.tab().id;
        let mut changed = false;
        let mut ports_changed = false;
        // Both the probe work and the number of outstanding reads are limited by the live pool.
        for id in self.web.host_open.iter().copied() {
            let Some(view) = self.web.views.get_mut(&id) else {
                continue;
            };
            let output = &mut view.outputs;
            let identity = (tab, self.web.host.control_epoch(id));
            if output.inspected != Some(identity) {
                let current = self.web.host.current_url(id);
                if current.is_some() {
                    output.inspected = Some(identity);
                }
                let matched = current
                    .and_then(|s| url::Url::parse(&s).ok())
                    .map(without_fragment)
                    .is_some_and(|u| Some(&u) == output.url.as_ref());
                ports_changed |= output.reset_page(matched.then_some(identity));
            }
            if output.page.is_none() || output.pending.is_some() {
                continue;
            }
            if let Some(reply) = self.web.host.take_dashboard_outputs(id) {
                match reply {
                    None => {
                        output.sampled = true;
                        let removed = output.snapshot.take().is_some();
                        ports_changed |= removed;
                        changed |= output.error.take().is_some();
                        output.raw = None;
                        output.queued = None;
                    }
                    Some(raw) if output.raw.as_ref() != Some(&raw) => {
                        output.queued = Some(raw);
                    }
                    _ => {}
                }
            }
            if let Some(raw) = output.queued.as_ref() {
                if self.web.output_worker.is_none() {
                    self.web.output_worker = Worker::new();
                }
                output.pending = self
                    .web
                    .output_worker
                    .as_mut()
                    .and_then(|w| w.submit(id, raw, ctx));
                if output.pending.is_some() {
                    output.sampled = true;
                    output.raw = output.queued.take();
                } else {
                    // A full parsing queue must retain the latest observed value.
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
            if output.queued.is_none() {
                // An unchanged browser read stays pending until the DOM changes.
                self.web.host.request_dashboard_outputs(id);
            }
        }
        let mut edits = Vec::new();
        if let Some(worker) = self.web.output_worker.as_ref() {
            while let Ok((id, ticket, result)) = worker.recv.try_recv() {
                let Some(view) = self.web.views.get_mut(&id) else {
                    continue;
                };
                let output = &mut view.outputs;
                if output.pending != Some(ticket) || output.page.map(|p| p.0) != Some(tab) {
                    continue;
                }
                output.pending = None;
                let (snapshot, error) = match result {
                    Ok((snapshot, edit)) => {
                        if let Some(edit) = edit {
                            if output.authored_input
                                && output
                                    .url
                                    .as_ref()
                                    .is_some_and(|url| url.scheme() == "file")
                            {
                                edits.push((id, edit));
                            }
                        }
                        (Some(snapshot), None)
                    }
                    Err(error) => (None, Some(error)),
                };
                ports_changed |= snapshot.as_ref().map(|s| &s.ports)
                    != output.snapshot.as_ref().map(|s| &s.ports);
                changed |= snapshot != output.snapshot || error != output.error;
                output.snapshot = snapshot;
                output.error = error;
                // A newer browser reply may already be waiting behind this parse.
                ctx.request_repaint();
            }
        }
        for (id, edit) in edits {
            self.commit_dashboard_parameters(id, edit);
        }
        if changed || ports_changed {
            self.dashboard_outputs_changed(ports_changed);
        }
    }
}
