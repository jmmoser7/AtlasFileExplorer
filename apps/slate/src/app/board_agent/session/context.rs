//! Wired context metadata for the session pump.
use super::*;
use std::path::Path;
use std::rc::Rc;

const PUBLICATIONS_PER_FRAME: usize = 32;
const BACKGROUND_POLLS_PER_FRAME: usize = 64;
const PRIORITY_EDGE: f32 = 24.0;

#[derive(PartialEq, Eq)]
struct ContextRevision {
    doc: u64,
    scene: u64,
    document_scene: u64,
    workbook: Option<PathBuf>,
    workspace: PathBuf,
}

/// Each source directory is published once per sweep, with its full train.
/// Preserve the cursor across revisions so an active edit can't starve later links.
#[derive(Default)]
pub(in crate::app::board_agent) struct ContextPublications {
    revision: Option<ContextRevision>,
    groups: Vec<Rc<[NodeId]>>,
    cursor: usize,
    remaining: usize,
    links: Rc<SessionLinks>,
    poll_cursor: usize,
    poll_remaining: usize,
    poll_tick: Option<Instant>,
}

pub(super) struct SessionLink {
    pub(super) dir: PathBuf,
    bounds: Option<slate_doc::WorldRect>,
}

#[derive(Default)]
pub(super) struct SessionLinks {
    pub(super) by_card: HashMap<NodeId, SessionLink>,
    pub(super) dirs: HashSet<PathBuf>,
    order: Vec<NodeId>,
}

pub(super) struct SessionBatch {
    pub(super) links: Rc<SessionLinks>,
    pub(super) publish: HashMap<NodeId, Rc<[NodeId]>>,
    pub(super) changed_dirs: HashSet<NodeId>,
    background: Option<HashSet<NodeId>>,
    viewport: Option<(slate_doc::WorldRect, f32)>,
}

impl SessionBatch {
    pub(super) fn poll_due(&self, id: NodeId, link: &SessionLink) -> bool {
        self.background.as_ref().is_none_or(|ids| ids.contains(&id))
            || self.publish.contains_key(&id)
            || self
                .viewport
                .zip(link.bounds)
                .is_some_and(|((view, zoom), bounds)| {
                    bounds.w.max(bounds.h) * zoom >= PRIORITY_EDGE && bounds.intersects(&view)
                })
    }
}

impl SlateApp {
    pub(super) fn agent_session_batch(
        &mut self,
        ctx: &egui::Context,
        workspace: &Path,
        cards: &[(NodeId, Option<slate_doc::scene::AgentPortalRef>)],
    ) -> SessionBatch {
        let revision = ContextRevision {
            doc: self.tab().id,
            scene: self.scene_gen,
            document_scene: self.doc().scene.scene_gen(),
            workbook: self.tab().path.clone(),
            workspace: workspace.to_path_buf(),
        };
        let mut changed_dirs = HashSet::new();
        if self.agents.context_publications.revision.as_ref() != Some(&revision) {
            let _span = atlas_core::session_log::span("slate.agents.context.plan");
            let mut groups = Vec::<Vec<NodeId>>::new();
            let mut by_dir = HashMap::new();
            let mut links = SessionLinks::default();
            for (id, agent) in cards {
                let Some(agent) = agent.as_ref().filter(|a| !a.provider.is_empty()) else {
                    continue;
                };
                let dir = self
                    .agent_link_dir(*id, workspace)
                    .unwrap_or_else(|| atlas_ai::agent::agent_dir(workspace, &agent.session));
                let index = *by_dir.entry(dir.clone()).or_insert_with(|| {
                    groups.push(Vec::new());
                    groups.len() - 1
                });
                groups[index].push(*id);
                let bounds = self
                    .doc()
                    .scene
                    .node(*id)
                    .filter(|node| !node.hidden)
                    .map(slate_doc::node_aabb);
                links.by_card.insert(*id, SessionLink { dir, bounds });
                links.order.push(*id);
            }
            links.dirs = by_dir.into_keys().collect();
            let schedule = &mut self.agents.context_publications;
            changed_dirs.extend(schedule.links.by_card.iter().filter_map(|(&id, old)| {
                links
                    .by_card
                    .get(&id)
                    .is_none_or(|new| new.dir != old.dir)
                    .then_some(id)
            }));
            schedule.revision = Some(revision);
            schedule.groups = groups.into_iter().map(Rc::from).collect();
            schedule.remaining = schedule.remaining.min(schedule.groups.len());
            schedule.poll_remaining = schedule.poll_remaining.min(links.order.len());
            schedule.links = Rc::new(links);
        }
        let viewport = (!self.at_home && self.canvas_rect.is_positive())
            .then(|| (self.board_paint_view(self.canvas_rect), self.board_xf().z));
        let schedule = &mut self.agents.context_publications;
        let background = schedule.background_polls(ctx);
        let publish = if workspace.as_os_str().is_empty() {
            HashMap::new()
        } else {
            schedule.publications(ctx, &mut self.agents.context_tick)
        };
        SessionBatch {
            links: schedule.links.clone(),
            publish,
            changed_dirs,
            background,
            viewport,
        }
    }

    pub(super) fn agent_context_for(
        &self,
        session: &str,
        provider: &str,
        _scope: AgentContextScope,
    ) -> AgentContext {
        let tab = self.tab();
        AgentContext {
            app: "slate",
            session: session.to_string(),
            provider: provider.to_string(),
            workbook: tab.path.clone(),
            format_version: tab.doc.format_version,
            scope: "wired".into(),
            selection: vec![],
            viewport: None,
            board_summary: String::new(),
            generated_at: atlas_ai::context::now_secs(),
        }
    }
}

impl ContextPublications {
    fn publications(
        &mut self,
        ctx: &egui::Context,
        tick: &mut Option<Instant>,
    ) -> HashMap<NodeId, Rc<[NodeId]>> {
        let schedule = self;
        let count = schedule.groups.len();
        if count == 0 {
            return HashMap::new();
        }
        if schedule.remaining == 0
            && tick.is_none_or(|tick| tick.elapsed() >= Duration::from_secs(1))
        {
            schedule.remaining = count;
            *tick = Some(Instant::now());
        }
        let take = schedule.remaining.min(PUBLICATIONS_PER_FRAME);
        if take == 0 {
            return HashMap::new();
        }
        let start = schedule.cursor % count;
        let batch = (0..take)
            .map(|offset| {
                let group = &schedule.groups[(start + offset) % count];
                (group[0], group.clone())
            })
            .collect();
        schedule.cursor = (start + take) % count;
        schedule.remaining -= take;
        if schedule.remaining != 0 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        batch
    }

    /// Keep external file links observable even when their cards are offscreen.
    /// A large cold board admits a rotating slice each frame instead of queueing
    /// every metadata read before the links the person can interact with.
    fn background_polls(&mut self, ctx: &egui::Context) -> Option<HashSet<NodeId>> {
        let count = self.links.order.len();
        if count <= BACKGROUND_POLLS_PER_FRAME {
            self.poll_remaining = 0;
            return None;
        }
        if self.poll_remaining == 0
            && self
                .poll_tick
                .is_none_or(|tick| tick.elapsed() >= Duration::from_secs(1))
        {
            self.poll_remaining = count;
            self.poll_tick = Some(Instant::now());
        }
        let take = self.poll_remaining.min(BACKGROUND_POLLS_PER_FRAME);
        let start = self.poll_cursor % count;
        let batch = (0..take)
            .map(|offset| self.links.order[(start + offset) % count])
            .collect();
        self.poll_cursor = (start + take) % count;
        self.poll_remaining -= take;
        if self.poll_remaining != 0 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        Some(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_polls_complete_a_rotating_sweep_before_restarting() {
        let ctx = egui::Context::default();
        let count = BACKGROUND_POLLS_PER_FRAME * 2 + 1;
        let mut schedule = ContextPublications {
            links: Rc::new(SessionLinks {
                order: (1..=count as u64).map(NodeId).collect(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut seen = HashSet::new();
        for expected in [BACKGROUND_POLLS_PER_FRAME, BACKGROUND_POLLS_PER_FRAME, 1] {
            let batch = schedule.background_polls(&ctx).unwrap();
            assert_eq!(batch.len(), expected);
            assert!(batch.is_disjoint(&seen));
            seen.extend(batch);
        }
        assert_eq!(seen.len(), count);
        schedule.poll_tick = Some(Instant::now() + Duration::from_secs(60));
        assert!(schedule.background_polls(&ctx).unwrap().is_empty());
    }
}
