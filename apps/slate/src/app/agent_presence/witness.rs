use super::super::dispatch::PreparedBoardCommand;
use super::*;
use slate_doc::scene::{Node, NodeKind, SceneCmd};

#[cfg(test)]
#[path = "crop_witness_tests.rs"]
mod crop_tests;
mod drawing;
mod pairs;

pub(super) struct WitnessedCommand {
    pub(super) purpose: WitnessPurpose,
    pub call: BoardCall,
    pub(super) action: WitnessAction,
    pub(super) steps: Vec<(Option<Node>, Option<Node>)>,
    pub(super) index: usize,
    pub previews: HashMap<slate_doc::NodeId, Node>,
    pub(super) started: bool,
    pub(super) checked_generation: Option<u64>,
    cohorts: HashMap<usize, Vec<usize>>,
    skipped: std::collections::HashSet<usize>,
    wire_ends: HashMap<slate_doc::NodeId, (WorldPoint, WorldPoint)>,
    crop_contact: Option<WorldPoint>,
    drawing: Option<drawing::DrawingRun>,
}

pub(super) enum WitnessAction {
    Board(PreparedBoardCommand),
    Relay(messaging::PreparedRelay),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WitnessPurpose {
    Interaction,
    ProposalPreview,
    Autonomous,
    ApprovedProposal(String),
}
impl WitnessPurpose {
    pub(super) fn replaces_objects(&self) -> bool {
        matches!(self, Self::Autonomous | Self::ApprovedProposal(_))
    }
}

impl WitnessedCommand {
    pub(super) fn set_drawing_helpers(&mut self, limit: u8) {
        if let Some(drawing) = &mut self.drawing {
            drawing.set_helper_limit(limit);
        }
    }
    pub(super) fn drawing_helpers(&self) -> impl Iterator<Item = (&str, WorldPoint)> {
        self.drawing.iter().flat_map(|drawing| drawing.helpers())
    }
    pub(super) fn relay(call: BoardCall, relay: messaging::PreparedRelay) -> Self {
        Self {
            purpose: WitnessPurpose::Interaction,
            call,
            action: WitnessAction::Relay(relay),
            steps: Vec::new(),
            index: 0,
            previews: HashMap::new(),
            started: false,
            checked_generation: None,
            cohorts: HashMap::new(),
            skipped: Default::default(),
            wire_ends: HashMap::new(),
            crop_contact: None,
            drawing: None,
        }
    }
    pub(super) fn board_prepared(&self) -> Option<&PreparedBoardCommand> {
        match &self.action {
            WitnessAction::Board(p) => Some(p),
            WitnessAction::Relay(_) => None,
        }
    }
    pub(super) fn command(&self) -> &str {
        self.board_prepared()
            .map_or("presence.send", |p| p.command.as_str())
    }
    pub(super) fn targets(&self) -> &[slate_doc::NodeId] {
        self.board_prepared().map_or(&[], |p| p.targets.as_slice())
    }
    pub(super) fn total_steps(&self) -> usize {
        if self.drawing.is_some() || matches!(self.action, WitnessAction::Relay(_)) {
            1
        } else {
            self.steps.len()
        }
    }
    pub(super) fn tab_id(&self) -> u64 {
        match &self.action {
            WitnessAction::Board(p) => p.tab_id(),
            WitnessAction::Relay(r) => r.tab,
        }
    }
    pub(super) fn expected_matches(&self, scene: &slate_doc::Scene) -> Result<(), String> {
        match &self.action {
            WitnessAction::Board(p) => p.expected_matches(scene),
            WitnessAction::Relay(r) => r.expected_matches(scene),
        }
    }
    pub(super) fn relay_epoch_changed(&self, epoch: u64) -> bool {
        matches!(&self.action, WitnessAction::Relay(r) if r.control_epoch != epoch)
    }
    pub(super) fn look_at(&self) -> Option<WorldPoint> {
        match &self.action {
            WitnessAction::Relay(r) => Some(r.contact),
            _ => self.drawing.as_ref().and_then(drawing::DrawingRun::look_at),
        }
    }
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
        let drawing = drawing::DrawingRun::prepare(&prepared, &steps, &call.id)?;
        Ok(Self {
            purpose: WitnessPurpose::Autonomous,
            call,
            action: WitnessAction::Board(prepared),
            steps,
            index,
            previews: HashMap::new(),
            started: false,
            checked_generation: None,
            wire_ends,
            cohorts,
            skipped,
            crop_contact: None,
            drawing,
        })
    }
    pub(super) fn begin_step(&mut self, motion: &mut PresenceSession) -> Result<(), String> {
        if let Some(drawing) = &mut self.drawing {
            drawing.begin(motion)?;
            self.started = true;
            return Ok(());
        }
        if let WitnessAction::Relay(relay) = &self.action {
            enqueue_motion(motion, &relay.witness_id(), relay.contact, relay.contact)?;
            self.started = true;
            return Ok(());
        }
        let (before, after) = &self.steps[self.index];
        let reference = before.as_ref().or(after.as_ref()).ok_or("Missing target")?;
        let (cx, cy) = reference.rect.center();
        let (ex, ey) = after.as_ref().unwrap_or(reference).rect.center();
        let (contact, placement) = if let Some(ends) = self.wire_ends.get(&reference.id) {
            *ends
        } else if let Some(requested) = self
            .board_prepared()
            .and_then(PreparedBoardCommand::crop_gesture)
        {
            use crate::app::board_crop::{effective_gesture, gesture_contact, CropGesture};
            let gesture =
                effective_gesture(reference, requested).ok_or("Missing effective crop gesture")?;
            match gesture {
                CropGesture::Handle { world, .. } => {
                    let (x, y) =
                        gesture_contact(reference, gesture).ok_or("Missing crop handle")?;
                    (point(x, y), point(world.0, world.1))
                }
                CropGesture::Pan { delta_world } => {
                    let contact = body_contact(reference)?;
                    (
                        contact,
                        WorldPoint {
                            x: contact.x + f64::from(delta_world.0),
                            y: contact.y + f64::from(delta_world.1),
                        },
                    )
                }
            }
        } else {
            let contact = body_contact(reference)?;
            (
                contact,
                WorldPoint {
                    x: contact.x + f64::from(ex - cx),
                    y: contact.y + f64::from(ey - cy),
                },
            )
        };
        self.crop_contact = self
            .board_prepared()
            .and_then(PreparedBoardCommand::crop_gesture)
            .map(|_| contact);
        enqueue_motion(motion, &reference.id.0.to_string(), contact, placement)?;
        self.started = true;
        Ok(())
    }
    /// Control samples drive canonical crop math; actual grips own the avatar.
    pub(super) fn display_position(&self, sample: MotionSample) -> WorldPoint {
        if matches!(sample.phase, MotionPhase::Work | MotionPhase::ReadyToCommit) {
            if let Some(gesture @ crate::app::board_crop::CropGesture::Handle { .. }) = self
                .board_prepared()
                .and_then(PreparedBoardCommand::crop_gesture)
            {
                if let Some((_, Some(after))) = self.steps.get(self.index) {
                    if let Some(preview) = self.previews.get(&after.id) {
                        if let Some((x, y)) =
                            crate::app::board_crop::gesture_contact(preview, gesture)
                        {
                            return point(x, y);
                        }
                    }
                }
            }
        }
        sample.position
    }
    pub(super) fn cancel_motion(&self, motion: &mut PresenceSession) -> Result<(), String> {
        let displayed = self.display_position(motion.sample());
        displayed.validate().map_err(|error| error.to_string())?;
        motion.cancel_motion();
        motion
            .settle_idle_position(displayed)
            .map_err(|error| error.to_string())
    }
    pub(super) fn complete_step(&mut self) {
        self.index += 1;
        while self.skipped.contains(&self.index) {
            self.index += 1;
        }
        self.started = false;
    }
    pub(super) fn preview(&mut self, sample: MotionSample) -> Result<(), String> {
        if let Some(drawing) = &mut self.drawing {
            drawing.preview(sample, &mut self.previews);
            return Ok(());
        }
        if matches!(self.action, WitnessAction::Relay(_))
            || !matches!(sample.phase, MotionPhase::Work | MotionPhase::ReadyToCommit)
        {
            return Ok(());
        }
        if let Some(gesture) = self
            .board_prepared()
            .and_then(PreparedBoardCommand::crop_gesture)
        {
            use crate::app::board_crop::{gesture_geometry, CropGesture};
            let before = self.steps[self.index]
                .0
                .as_ref()
                .ok_or("Missing crop before-state")?;
            if sample.phase == MotionPhase::ReadyToCommit {
                // Preparation already used the canonical geometry owner. Keep
                // its exact endpoint instead of a world/local float roundtrip.
                let after = self.steps[self.index]
                    .1
                    .as_ref()
                    .ok_or("Missing crop endpoint")?;
                self.previews.insert(before.id, after.clone());
                return Ok(());
            }
            let contact = self.crop_contact.ok_or("Missing crop contact")?;
            // Motion already supplies the eased pointer position. Replay that
            // input against the immutable start, never interpolate UV endpoints.
            let input = match gesture {
                CropGesture::Handle { handle, .. } => CropGesture::Handle {
                    handle,
                    world: (sample.position.x as f32, sample.position.y as f32),
                },
                CropGesture::Pan { .. } => CropGesture::Pan {
                    delta_world: (
                        (sample.position.x - contact.x) as f32,
                        (sample.position.y - contact.y) as f32,
                    ),
                },
            };
            let geometry =
                gesture_geometry(before, input).ok_or("Crop preview no longer applies")?;
            let preview = self
                .previews
                .entry(before.id)
                .or_insert_with(|| before.clone());
            if !geometry.apply_to(preview) {
                return Err("Crop preview identity no longer applies".into());
            }
            return Ok(());
        }
        pairs::preview_pair(&self.steps[self.index], sample, &mut self.previews);
        if let Some(children) = self.cohorts.get(&self.index) {
            for index in children {
                pairs::preview_pair(&self.steps[*index], sample, &mut self.previews);
            }
        }
        Ok(())
    }
}

/// One body-contact policy for ordinary work and crop pan. A known outline
/// never falls back into a hole; the fixed candidate budget can refuse contact.
pub(super) fn body_contact(node: &Node) -> Result<WorldPoint, String> {
    let (cx, cy) = node.rect.center();
    let Some(outline) = slate_doc::geom::node_closed_poly(node, 0.05) else {
        if node.clip.is_some() {
            return Err("The authored mask has no visible body contact".into());
        }
        return Ok(point(cx, cy));
    };
    if vector_ink::point_in_polygon(&outline, [cx, cy]) {
        return Ok(point(cx, cy));
    }
    for row in 0..8 {
        for column in 0..8 {
            let local = [
                node.rect.x + node.rect.w * (column as f32 + 0.5) / 8.0,
                node.rect.y + node.rect.h * (row as f32 + 0.5) / 8.0,
            ];
            let candidate = node.rect.rotate_point(local, node.rotation_deg);
            if vector_ink::point_in_polygon(&outline, candidate) {
                return Ok(point(candidate[0], candidate[1]));
            }
        }
    }
    Err("No visible body contact was found within the bounded outline search".into())
}

/// One motion construction policy for board actions and conversation contact.
pub(super) fn enqueue_motion(
    motion: &mut PresenceSession,
    target: &str,
    contact: WorldPoint,
    placement: WorldPoint,
) -> Result<(), String> {
    let start = motion.sample().position;
    let distance = ((start.x - contact.x).powi(2) + (start.y - contact.y).powi(2)).sqrt();
    motion
        .enqueue(MotionPlan {
            session_id: motion.id().clone(),
            request_id: motion.request_id().cloned().ok_or("Missing request")?,
            action_id: id(&atlas_agent::request_id())?,
            target_nodes: vec![id(target)?],
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
    Ok(())
}
