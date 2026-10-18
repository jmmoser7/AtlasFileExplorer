use super::*;
use crate::{ActivityTarget, AgentActivitySemantics, ACTIVITY_TARGET_ID_LIMIT};
use std::collections::{HashSet, VecDeque};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceEvent {
    pub session_id: PresenceId,
    pub request_id: PresenceId,
    pub sequence: u64,
    pub activity: AgentActivitySemantics,
}

#[derive(Debug)]
struct ActiveMotion {
    plan: MotionPlan,
    elapsed_ms: f64,
}

impl ActiveMotion {
    fn sample(&self) -> MotionSample {
        let times = self.plan.durations;
        let travel = f64::from(times.travel_ms);
        let phase = if self.elapsed_ms < travel {
            MotionPhase::Travel
        } else if self.elapsed_ms < travel + f64::from(times.grab_ms) {
            MotionPhase::Grab
        } else if self.elapsed_ms < times.total() {
            MotionPhase::Work
        } else {
            MotionPhase::ReadyToCommit
        };
        let t = (self.elapsed_ms / travel).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        let mut position = if t == 1.0 {
            self.plan.end
        } else {
            WorldPoint {
                x: self.plan.start.x + (self.plan.end.x - self.plan.start.x) * smooth,
                y: self.plan.start.y + (self.plan.end.y - self.plan.start.y) * smooth,
            }
        };
        let phase_start = match phase {
            MotionPhase::Travel => 0.0,
            MotionPhase::Grab => travel,
            _ => travel + f64::from(times.grab_ms),
        };
        let phase_duration = match phase {
            MotionPhase::Travel => travel,
            MotionPhase::Grab => f64::from(times.grab_ms),
            _ => f64::from(times.work_ms),
        };
        let progress = ((self.elapsed_ms - phase_start) / phase_duration).clamp(0.0, 1.0);
        if matches!(phase, MotionPhase::Work | MotionPhase::ReadyToCommit) {
            if let Some(end) = self.plan.work_end {
                let s = progress * progress * (3.0 - 2.0 * progress);
                position = WorldPoint {
                    x: self.plan.end.x + (end.x - self.plan.end.x) * s,
                    y: self.plan.end.y + (end.y - self.plan.end.y) * s,
                };
            }
        }
        MotionSample {
            position,
            phase,
            progress: progress as f32,
        }
    }
}

/// Runtime only: deliberately neither Serialize nor Clone. A restored profile
/// must receive a fresh session and request; copying a session could replay a gate.
#[derive(Debug)]
pub struct PresenceSession {
    id: PresenceId,
    profile_id: PresenceId,
    parent_id: Option<PresenceId>,
    request_id: Option<PresenceId>,
    position: WorldPoint,
    sequence: u64,
    activity: Option<AgentActivitySemantics>,
    queue: VecDeque<MotionPlan>,
    active: Option<ActiveMotion>,
    actions: HashSet<PresenceId>,
    requests: HashSet<PresenceId>,
}

impl PresenceSession {
    pub(super) fn new(
        profile_id: PresenceId,
        parent_id: Option<PresenceId>,
        position: WorldPoint,
    ) -> Result<Self, PresenceError> {
        position.validate()?;
        Ok(Self {
            id: PresenceId::new(crate::request_id())?,
            profile_id,
            parent_id,
            request_id: None,
            position,
            sequence: 0,
            activity: None,
            queue: VecDeque::new(),
            active: None,
            actions: HashSet::new(),
            requests: HashSet::new(),
        })
    }

    pub fn id(&self) -> &PresenceId {
        &self.id
    }
    pub fn profile_id(&self) -> &PresenceId {
        &self.profile_id
    }
    pub fn parent_id(&self) -> Option<&PresenceId> {
        self.parent_id.as_ref()
    }
    pub fn request_id(&self) -> Option<&PresenceId> {
        self.request_id.as_ref()
    }
    pub fn activity(&self) -> Option<&AgentActivitySemantics> {
        self.activity.as_ref()
    }
    pub fn current_plan(&self) -> Option<&MotionPlan> {
        self.active.as_ref().map(|a| &a.plan)
    }
    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }

    pub fn sample(&self) -> MotionSample {
        self.active
            .as_ref()
            .map(ActiveMotion::sample)
            .unwrap_or(MotionSample {
                position: self.position,
                phase: MotionPhase::Idle,
                progress: 0.0,
            })
    }

    /// Local reservation hints only. This acquires no document or filesystem lease.
    pub fn leased_targets(&self) -> &[PresenceId] {
        match self.sample().phase {
            MotionPhase::Grab | MotionPhase::Work | MotionPhase::ReadyToCommit => {
                &self
                    .active
                    .as_ref()
                    .expect("active phase")
                    .plan
                    .target_nodes
            }
            _ => &[],
        }
    }

    pub fn begin_request(&mut self, id: PresenceId) -> Result<Vec<PresenceSignal>, PresenceError> {
        if self.requests.contains(&id) {
            return Err(PresenceError::DuplicateRequest);
        }
        if self.requests.len() == REQUEST_LIMIT {
            return Err(PresenceError::RequestLimit);
        }
        let cancelled = self.cancel_pending();
        self.requests.insert(id.clone());
        self.request_id = Some(id);
        self.sequence = 0;
        self.activity = None;
        self.actions.clear();
        Ok(cancelled)
    }

    pub fn reduce(&mut self, event: PresenceEvent) -> Result<(), PresenceError> {
        if event.session_id != self.id {
            return Err(PresenceError::SessionMismatch);
        }
        if Some(&event.request_id) != self.request_id.as_ref() {
            return Err(PresenceError::RequestMismatch);
        }
        if event.sequence <= self.sequence {
            return Err(PresenceError::StaleSequence);
        }
        if event.activity.target.as_ref().is_some_and(|target| {
            target.node_id.len() > ACTIVITY_TARGET_ID_LIMIT * 4
                || ActivityTarget::new(&target.node_id).is_none()
        }) {
            return Err(PresenceError::InvalidTargets);
        }
        self.sequence = event.sequence;
        self.activity = Some(event.activity);
        Ok(())
    }

    fn validate_new(&self, plan: &MotionPlan) -> Result<(), PresenceError> {
        plan.validate()?;
        if plan.session_id != self.id {
            return Err(PresenceError::SessionMismatch);
        }
        if Some(&plan.request_id) != self.request_id.as_ref() {
            return Err(PresenceError::RequestMismatch);
        }
        if self.actions.contains(&plan.action_id) {
            return Err(PresenceError::DuplicateAction);
        }
        if self.actions.len() == ACTION_LIMIT {
            return Err(PresenceError::ActionLimit);
        }
        Ok(())
    }

    pub fn enqueue(&mut self, plan: MotionPlan) -> Result<(), PresenceError> {
        self.validate_new(&plan)?;
        if self.queue.len() == QUEUE_LIMIT {
            return Err(PresenceError::QueueFull);
        }
        let expected = self
            .queue
            .back()
            .map(|p| p.work_end.unwrap_or(p.end))
            .or_else(|| {
                self.active
                    .as_ref()
                    .map(|a| a.plan.work_end.unwrap_or(a.plan.end))
            })
            .unwrap_or(self.position);
        if plan.start != expected {
            return Err(PresenceError::DiscontinuousStart);
        }
        self.actions.insert(plan.action_id.clone());
        self.queue.push_back(plan);
        Ok(())
    }

    /// Cancels dependent plans and starts the replacement at the last witnessed
    /// point, rather than jumping to its supplied start or an unwitnessed endpoint.
    pub fn interrupt(
        &mut self,
        mut replacement: MotionPlan,
    ) -> Result<Vec<PresenceSignal>, PresenceError> {
        self.validate_new(&replacement)?;
        let cancelled = self.cancel_pending();
        replacement.start = self.position;
        self.enqueue(replacement)?;
        Ok(cancelled)
    }

    /// Call only for visible elapsed time. Discard excess time after a background
    /// gap: one delivery advances at most 50ms and stops at the next phase
    /// boundary. Overshoot is discarded so Grab and Work cannot be skipped.
    pub fn advance(&mut self, elapsed: Duration) -> MotionSample {
        if self.active.is_none() {
            self.active = self.queue.pop_front().map(|plan| ActiveMotion {
                plan,
                elapsed_ms: 0.0,
            });
        }
        if let Some(active) = &mut self.active {
            let step = (elapsed.as_secs_f64() * 1000.0).min(VISIBLE_STEP_MS);
            let travel_end = f64::from(active.plan.durations.travel_ms);
            let grab_end = travel_end + f64::from(active.plan.durations.grab_ms);
            let boundary = if active.elapsed_ms < travel_end {
                travel_end
            } else if active.elapsed_ms < grab_end {
                grab_end
            } else {
                active.plan.durations.total()
            };
            active.elapsed_ms = (active.elapsed_ms + step).min(boundary);
            self.position = active.sample().position;
        }
        self.sample()
    }

    /// The host must check its real command/targets here and journal immediately
    /// on ReadyToCommit. Neither result asserts that any command was executed.
    /// Taking the plan makes this terminal signal exactly once, even if called twice.
    pub fn resolve_ready(
        &mut self,
        applicable: impl FnOnce(&MotionPlan) -> bool,
    ) -> Option<PresenceSignal> {
        let active = self.active.as_ref()?;
        if active.sample().phase != MotionPhase::ReadyToCommit {
            return None;
        }
        let ready = applicable(&active.plan);
        let active = self.active.take().expect("checked active plan");
        Some(PresenceSignal::from_plan(&active.plan, ready))
    }

    pub fn cancel_request(&mut self) -> Vec<PresenceSignal> {
        let signals = self.cancel_pending();
        self.request_id = None;
        self.sequence = 0;
        self.activity = None;
        signals
    }
    /// Cancel only witnessed motion after a conflict; keep the task identity and
    /// replay fences so the provider can read again and propose a fresh action.
    pub fn cancel_motion(&mut self) -> Vec<PresenceSignal> {
        self.cancel_pending()
    }

    pub(super) fn cancel_pending(&mut self) -> Vec<PresenceSignal> {
        let mut signals = Vec::with_capacity(self.queue.len() + usize::from(self.active.is_some()));
        if let Some(active) = self.active.take() {
            self.position = active.sample().position;
            signals.push(PresenceSignal::from_plan(&active.plan, false));
        }
        signals.extend(
            self.queue
                .drain(..)
                .map(|plan| PresenceSignal::from_plan(&plan, false)),
        );
        signals
    }
}
