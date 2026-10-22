//! Ephemeral agent presence and witnessed motion; never document commands.
//!
//! Only profiles are durable. A host paints samples, checks command applicability
//! in `resolve_ready`, and journals any resulting command itself. A ready signal
//! is permission to attempt that host operation, never a claim it executed.
use serde::{Deserialize, Serialize};
use std::fmt;

mod roster;
mod session;
mod work_path;
pub use roster::PresenceRoster;
pub use session::{PresenceEvent, PresenceSession};
pub use work_path::{
    DrawingCrewPlan, PathMode, PathProgress, WorkPath, WorkerAssignment, WorkerProgress, WorkerRole,
};

pub const ID_LIMIT: usize = 128;
pub const PROFILE_LIMIT: usize = 128;
pub const SESSION_LIMIT: usize = 32;
pub const HELPER_LIMIT: usize = 8;
pub const HELPER_DEPTH_LIMIT: usize = 2;
pub const HELPERS_PER_PARENT: usize = 4;
pub const QUEUE_LIMIT: usize = 8;
pub const TARGET_LIMIT: usize = 16;
/// Bound retained replay IDs while allowing a 4096-call task to witness up to
/// 16 target motions per call. This cap is independent of caller grant limits.
pub const ACTION_LIMIT: usize = 65_536;
pub const REQUEST_LIMIT: usize = 128;
pub const PHASE_MS_LIMIT: u32 = 30_000;
pub const VISIBLE_STEP_MS: f64 = 50.0;
pub const TRAVEL_MIN_MS: u32 = (VISIBLE_STEP_MS * 3.0) as u32;
pub const GRAB_MIN_MS: u32 = VISIBLE_STEP_MS as u32;
pub const WORK_MIN_MS: u32 = (VISIBLE_STEP_MS * 3.0) as u32;
pub const WORLD_COORD_LIMIT: f64 = 1_000_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceError {
    InvalidId,
    InvalidProfile,
    InvalidPoint,
    InvalidDuration,
    InvalidTargets,
    ProfileExists,
    UnknownProfile,
    RetiredProfile,
    NotRetired,
    AlreadyPresent,
    UnknownSession,
    ProfileLimit,
    SessionLimit,
    HelperLimit,
    SessionMismatch,
    RequestMismatch,
    StaleSequence,
    QueueFull,
    DuplicateAction,
    ActionLimit,
    DuplicateRequest,
    RequestLimit,
    DiscontinuousStart,
    InvalidPath,
    PathLimit,
}

impl fmt::Display for PresenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PresenceError {}

/// Opaque, bounded identity. A coordinate or document title is not a node ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PresenceId(String);

impl PresenceId {
    pub fn new(value: impl Into<String>) -> Result<Self, PresenceError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > ID_LIMIT
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        {
            return Err(PresenceError::InvalidId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PresenceId {
    type Error = PresenceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<PresenceId> for String {
    fn from(value: PresenceId) -> Self {
        value.0
    }
}

/// Authored identity and appearance only; no restored job, cursor or lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: PresenceId,
    pub name: String,
    pub hat: Option<String>,
    pub retired: bool,
}

impl AgentProfile {
    pub fn validate(&self) -> Result<(), PresenceError> {
        fn text_ok(value: &str, limit: usize) -> bool {
            value.len() <= limit * 4
                && !value.trim().is_empty()
                && value.chars().count() <= limit
                && !value.chars().any(char::is_control)
        }
        if !text_ok(&self.name, 80) || self.hat.as_ref().is_some_and(|hat| !text_ok(hat, 64)) {
            return Err(PresenceError::InvalidProfile);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPoint {
    pub x: f64,
    pub y: f64,
}

impl WorldPoint {
    pub fn new(x: f64, y: f64) -> Result<Self, PresenceError> {
        let point = Self { x, y };
        point.validate()?;
        Ok(point)
    }

    pub fn validate(self) -> Result<(), PresenceError> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || self.x.abs() > WORLD_COORD_LIMIT
            || self.y.abs() > WORLD_COORD_LIMIT
        {
            return Err(PresenceError::InvalidPoint);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionDurations {
    pub travel_ms: u32,
    pub grab_ms: u32,
    pub work_ms: u32,
}

impl MotionDurations {
    fn validate(self) -> Result<(), PresenceError> {
        if self.travel_ms < TRAVEL_MIN_MS
            || self.grab_ms < GRAB_MIN_MS
            || self.work_ms < WORK_MIN_MS
            || [self.travel_ms, self.grab_ms, self.work_ms]
                .iter()
                .any(|duration| *duration > PHASE_MS_LIMIT)
        {
            return Err(PresenceError::InvalidDuration);
        }
        Ok(())
    }
    fn total(self) -> f64 {
        f64::from(self.travel_ms + self.grab_ms + self.work_ms)
    }
}

/// Witness metadata only. The host retains the actual command and checks its
/// stable targets again immediately before journaling; this is not a command.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionPlan {
    pub session_id: PresenceId,
    pub request_id: PresenceId,
    pub action_id: PresenceId,
    pub target_nodes: Vec<PresenceId>,
    pub start: WorldPoint,
    pub end: WorldPoint,
    /// Placement after contact; Work moves continuously to this point.
    pub work_end: Option<WorldPoint>,
    pub durations: MotionDurations,
}

impl MotionPlan {
    pub fn validate(&self) -> Result<(), PresenceError> {
        self.start.validate()?;
        self.end.validate()?;
        if let Some(end) = self.work_end {
            end.validate()?;
        }
        self.durations.validate()?;
        if self.target_nodes.is_empty()
            || self.target_nodes.len() > TARGET_LIMIT
            || self
                .target_nodes
                .iter()
                .enumerate()
                .any(|(i, target)| self.target_nodes[..i].contains(target))
        {
            return Err(PresenceError::InvalidTargets);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionPhase {
    Idle,
    Travel,
    Grab,
    Work,
    ReadyToCommit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionSample {
    pub position: WorldPoint,
    pub phase: MotionPhase,
    pub progress: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresenceSignal {
    ReadyToCommit {
        session_id: PresenceId,
        request_id: PresenceId,
        action_id: PresenceId,
    },
    Cancelled {
        session_id: PresenceId,
        request_id: PresenceId,
        action_id: PresenceId,
    },
}

impl PresenceSignal {
    fn from_plan(plan: &MotionPlan, ready: bool) -> Self {
        let (session_id, request_id, action_id) = (
            plan.session_id.clone(),
            plan.request_id.clone(),
            plan.action_id.clone(),
        );
        if ready {
            Self::ReadyToCommit {
                session_id,
                request_id,
                action_id,
            }
        } else {
            Self::Cancelled {
                session_id,
                request_id,
                action_id,
            }
        }
    }
}

#[cfg(test)]
mod tests;
