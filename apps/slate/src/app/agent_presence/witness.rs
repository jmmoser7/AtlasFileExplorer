use super::super::dispatch::PreparedBoardCommand;
use super::*;
use slate_doc::scene::{Node, NodeKind, SceneCmd};

pub(super) struct WitnessedCommand {
    pub call: BoardCall,
    pub prepared: PreparedBoardCommand,
    pub(super) steps: Vec<(Option<Node>, Option<Node>)>,
    pub(super) index: usize,
    pub previews: HashMap<slate_doc::NodeId, Node>,
    pub(super) started: bool,
    pub(super) checked_generation: Option<u64>,
    cohorts: HashMap<usize, Vec<usize>>,
    skipped: std::collections::HashSet<usize>,
    wire_ends: HashMap<slate_doc::NodeId, (WorldPoint, WorldPoint)>,
}

impl WitnessedCommand {
    pub(super) fn new(
        call: BoardCall,
        prepared: PreparedBoardCommand,
        scene: &slate_doc::Scene,
        routing: slate_doc::WireRouting,
    ) -> Result<Self, String> {
        let mut steps: Vec<_> = prepared
            .cmds
            .iter()
            .filter_map(|c| match c {
                SceneCmd::Patch { before, after } => {
                    Some((Some(before.as_ref().clone()), Some(after.as_ref().clone())))
                }
                SceneCmd::Add { node, .. } => Some((None, Some(node.clone()))),
                SceneCmd::Remove { node, .. } => Some((Some(node.clone()), None)),
                _ => None,
            })
            .collect();
        if steps.is_empty() {
            steps = prepared
                .witness_nodes(scene)
                .into_iter()
                .map(|n| (Some(n.clone()), Some(n)))
                .collect();
        }
        if steps.is_empty() || steps.len() > 128 {
            return Err("This command has no bounded witnessed preview".into());
        }
        let mut cohorts: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut skipped = std::collections::HashSet::new();
        if prepared.command == "board.objects.move" {
            for (index, (before, _)) in steps.iter().enumerate() {
                if let Some(frame) = before.as_ref().filter(|n| n.is_frame()) {
                    let children: Vec<_> = steps
                        .iter()
                        .enumerate()
                        .filter(|(_, pair)| {
                            pair.0
                                .as_ref()
                                .is_some_and(|n| scene.frame_of(n.id) == Some(frame.id))
                        })
                        .map(|(i, _)| i)
                        .collect();
                    skipped.extend(children.iter().copied());
                    cohorts.insert(index, children);
                }
            }
        }
        let wire_ends = steps
            .iter()
            .filter_map(|(_, after)| after.as_ref())
            .filter_map(|node| {
                if let NodeKind::Connector(c) = &node.kind {
                    slate_doc::wire::connector_route_in_scene(
                        scene,
                        None,
                        &c.a,
                        &c.b,
                        c.effective_routing(routing),
                    )
                    .map(|path| {
                        let a = path.start();
                        let b = path.end();
                        (node.id, (point(a[0], a[1]), point(b[0], b[1])))
                    })
                } else {
                    None
                }
            })
            .collect();
        let index = (0..steps.len())
            .find(|i| !skipped.contains(i))
            .unwrap_or(steps.len());
        Ok(Self {
            call,
            prepared,
            steps,
            index,
            previews: HashMap::new(),
            started: false,
            checked_generation: None,
            wire_ends,
            cohorts,
            skipped,
        })
    }
    pub(super) fn begin_step(&mut self, motion: &mut PresenceSession) -> Result<(), String> {
        let (before, after) = &self.steps[self.index];
        let reference = before.as_ref().or(after.as_ref()).ok_or("Missing target")?;
        let (cx, cy) = reference.rect.center();
        let (ex, ey) = after.as_ref().unwrap_or(reference).rect.center();
        let start = motion.sample().position;
        let distance =
            ((start.x - f64::from(cx)).powi(2) + (start.y - f64::from(cy)).powi(2)).sqrt();
        let (contact, placement) = self
            .wire_ends
            .get(&reference.id)
            .copied()
            .unwrap_or((point(cx, cy), point(ex, ey)));
        motion
            .enqueue(MotionPlan {
                session_id: motion.id().clone(),
                request_id: motion.request_id().cloned().ok_or("Missing request")?,
                action_id: id(&atlas_agent::request_id())?,
                target_nodes: vec![id(&reference.id.0.to_string())?],
                start,
                end: contact,
                work_end: Some(placement),
                durations: MotionDurations {
                    travel_ms: (distance / 1.8).clamp(180.0, 650.0) as u32,
                    grab_ms: 90,
                    work_ms: 360,
                },
            })
            .map_err(|e| e.to_string())?;
        self.started = true;
        Ok(())
    }
    pub(super) fn complete_step(&mut self) {
        self.index += 1;
        while self.skipped.contains(&self.index) {
            self.index += 1;
        }
        self.started = false;
    }
    pub(super) fn preview(&mut self, sample: MotionSample) {
        if !matches!(sample.phase, MotionPhase::Work | MotionPhase::ReadyToCommit) {
            return;
        }
        Self::preview_pair(&self.steps[self.index], sample, &mut self.previews);
        if let Some(children) = self.cohorts.get(&self.index) {
            for index in children {
                Self::preview_pair(&self.steps[*index], sample, &mut self.previews);
            }
        }
    }
    fn preview_pair(
        pair: &(Option<Node>, Option<Node>),
        sample: MotionSample,
        previews: &mut HashMap<slate_doc::NodeId, Node>,
    ) {
        let (before, after) = pair;
        let t = sample.progress;
        let t = t * t * (3.0 - 2.0 * t);
        match (before, after) {
            (Some(before), Some(after)) => {
                let node = previews.entry(after.id).or_insert_with(|| after.clone());
                node.rect = slate_doc::scene::WorldRect::new(
                    before.rect.x + (after.rect.x - before.rect.x) * t,
                    before.rect.y + (after.rect.y - before.rect.y) * t,
                    before.rect.w + (after.rect.w - before.rect.w) * t,
                    before.rect.h + (after.rect.h - before.rect.h) * t,
                );
                node.opacity = after.opacity;
            }
            (None, Some(after)) => {
                let node = previews.entry(after.id).or_insert_with(|| after.clone());
                if let NodeKind::Connector(conn) = &mut node.kind {
                    conn.b = if sample.phase == MotionPhase::ReadyToCommit {
                        if let NodeKind::Connector(c) = &after.kind {
                            c.b
                        } else {
                            unreachable!()
                        }
                    } else {
                        slate_doc::scene::ConnectorEnd::Free {
                            point: [sample.position.x as f32, sample.position.y as f32],
                        }
                    };
                    node.opacity = after.opacity * 0.8;
                } else {
                    node.opacity = after.opacity * t;
                }
            }
            (Some(before), None) => {
                let node = previews.entry(before.id).or_insert_with(|| before.clone());
                node.opacity = before.opacity * (1.0 - t);
            }
            _ => {}
        }
    }
}
