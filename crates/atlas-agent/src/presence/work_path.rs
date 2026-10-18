use super::{PresenceError, WorldPoint, HELPER_LIMIT};

pub const WORK_PATH_POINT_LIMIT: usize = 4_096;
pub const DRAWING_HELPER_LIMIT: usize = HELPER_LIMIT;
const TRAVEL_MS_PER_WORLD_UNIT: f64 = 10.0;

/// How the witnessed drawing motion spends its work interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    /// Travel at a steady rate proportional to arclength.
    Continuous,
    /// Pause visibly at each authored vertex while still travelling each leg.
    VertexByVertex { dwell_ms: u32 },
}

/// Bounded authored drawing strokes. Strokes remain separate; pen-up travel
/// between them is sampled as motion and is never drawn as a connecting chord.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkPath {
    strokes: Vec<Vec<WorldPoint>>,
    stroke_modes: Vec<PathMode>,
    /// Compatibility mode for callers which historically supplied one mode.
    /// For mixed paths this is the first stroke's mode; use `stroke_mode`.
    mode: PathMode,
    point_count: usize,
    duration_weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathProgress {
    pub position: WorldPoint,
    pub progress: f32,
    pub stroke_index: usize,
    pub vertex_index: usize,
    pub pen_down: bool,
    pub dwelling: bool,
}

impl WorkPath {
    pub fn new(strokes: Vec<Vec<WorldPoint>>, mode: PathMode) -> Result<Self, PresenceError> {
        let modes = vec![mode; strokes.len()];
        Self::new_with_modes(strokes, modes)
    }

    /// Create a path with an independent motion mode for each authored stroke.
    /// The legacy `mode()` accessor reports the first stroke's mode; callers
    /// inspecting mixed paths should use `stroke_mode`.
    pub fn new_with_modes(
        strokes: Vec<Vec<WorldPoint>>,
        stroke_modes: Vec<PathMode>,
    ) -> Result<Self, PresenceError> {
        if strokes.is_empty()
            || strokes.iter().any(Vec::is_empty)
            || strokes.len() != stroke_modes.len()
            || stroke_modes
                .iter()
                .any(|mode| matches!(mode, PathMode::VertexByVertex { dwell_ms: 0 }))
        {
            return Err(PresenceError::InvalidPath);
        }
        let point_count = strokes.iter().map(Vec::len).sum::<usize>();
        if point_count > WORK_PATH_POINT_LIMIT {
            return Err(PresenceError::PathLimit);
        }
        validate_points(&strokes)?;
        let mut path = Self {
            strokes,
            mode: stroke_modes[0],
            stroke_modes,
            point_count,
            duration_weight: 0.0,
        };
        path.duration_weight = path.measure();
        Ok(path)
    }

    pub fn strokes(&self) -> &[Vec<WorldPoint>] {
        &self.strokes
    }
    pub fn mode(&self) -> PathMode {
        self.mode
    }
    pub fn stroke_mode(&self, stroke_index: usize) -> Option<PathMode> {
        self.stroke_modes.get(stroke_index).copied()
    }
    pub fn point_count(&self) -> usize {
        self.point_count
    }

    /// Sample normalized work progress. Inter-stroke pen-up travel has its own
    /// arclength interval, so a renderer can lift the pen without teleporting.
    pub fn position(&self, progress: f32) -> PathProgress {
        self.position_at(f64::from(progress))
    }

    /// Sample from elapsed-time progress without first rounding it to f32.
    /// Motion samples expose f32 progress for compatibility, but retaining
    /// f64 here keeps a witnessed world point stable when cancellation settles
    /// that exact displayed sample.
    pub(crate) fn position_at(&self, progress: f64) -> PathProgress {
        let normalized = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.sample_weight(normalized * self.duration_weight, normalized as f32)
    }

    fn measure(&self) -> f64 {
        let mut total = 0.0;
        for (index, stroke) in self.strokes.iter().enumerate() {
            if index > 0 {
                total += distance(self.strokes[index - 1].last().unwrap(), &stroke[0])
                    * TRAVEL_MS_PER_WORLD_UNIT;
            }
            total += stroke
                .windows(2)
                .map(|pair| distance(&pair[0], &pair[1]))
                .sum::<f64>()
                * TRAVEL_MS_PER_WORLD_UNIT;
            if let PathMode::VertexByVertex { dwell_ms } = self.stroke_modes[index] {
                total += f64::from(dwell_ms) * stroke.len() as f64;
            }
        }
        total.max(f64::EPSILON)
    }

    fn sample_weight(&self, mut remaining: f64, progress: f32) -> PathProgress {
        for (stroke_index, stroke) in self.strokes.iter().enumerate() {
            if stroke_index > 0 {
                let previous = self.strokes[stroke_index - 1].last().unwrap();
                let gap = distance(previous, &stroke[0]);
                let gap_weight = gap * TRAVEL_MS_PER_WORLD_UNIT;
                if remaining <= gap_weight {
                    return pen_up(
                        previous,
                        &stroke[0],
                        remaining / TRAVEL_MS_PER_WORLD_UNIT,
                        gap,
                        stroke_index,
                        progress,
                    );
                }
                remaining -= gap_weight;
            }
            let mode = self.stroke_modes[stroke_index];
            if let Some(sample) = sample_stroke(stroke, stroke_index, mode, remaining, progress) {
                return sample;
            }
            remaining -= stroke_measure(stroke, mode);
        }
        let (stroke_index, stroke) = self.strokes.iter().enumerate().last().unwrap();
        let vertex_index = stroke.len() - 1;
        PathProgress {
            position: stroke[vertex_index],
            progress,
            stroke_index,
            vertex_index,
            pen_down: true,
            dwelling: false,
        }
    }
}

fn validate_points(strokes: &[Vec<WorldPoint>]) -> Result<(), PresenceError> {
    if strokes
        .iter()
        .flatten()
        .any(|point| point.validate().is_err())
    {
        return Err(PresenceError::InvalidPoint);
    }
    Ok(())
}

fn stroke_measure(stroke: &[WorldPoint], mode: PathMode) -> f64 {
    let travel = stroke
        .windows(2)
        .map(|pair| distance(&pair[0], &pair[1]))
        .sum::<f64>()
        * TRAVEL_MS_PER_WORLD_UNIT;
    match mode {
        PathMode::Continuous => travel,
        PathMode::VertexByVertex { dwell_ms } => travel + f64::from(dwell_ms) * stroke.len() as f64,
    }
}

fn sample_stroke(
    stroke: &[WorldPoint],
    stroke_index: usize,
    mode: PathMode,
    mut remaining: f64,
    progress: f32,
) -> Option<PathProgress> {
    let dwell = match mode {
        PathMode::Continuous => 0.0,
        PathMode::VertexByVertex { dwell_ms } => f64::from(dwell_ms),
    };
    for (vertex_index, point) in stroke.iter().enumerate() {
        if dwell > 0.0 {
            if remaining <= dwell {
                return Some(PathProgress {
                    position: *point,
                    progress,
                    stroke_index,
                    vertex_index,
                    pen_down: true,
                    dwelling: true,
                });
            }
            remaining -= dwell;
        }
        if let Some(next) = stroke.get(vertex_index + 1) {
            let leg = distance(point, next);
            let leg_weight = leg * TRAVEL_MS_PER_WORLD_UNIT;
            if remaining <= leg_weight {
                return Some(interpolate(
                    point,
                    next,
                    remaining / TRAVEL_MS_PER_WORLD_UNIT,
                    leg,
                    stroke_index,
                    vertex_index,
                    progress,
                ));
            }
            remaining -= leg_weight;
        }
    }
    None
}

fn pen_up(
    from: &WorldPoint,
    to: &WorldPoint,
    offset: f64,
    length: f64,
    stroke_index: usize,
    progress: f32,
) -> PathProgress {
    let mut result = interpolate(from, to, offset, length, stroke_index, 0, progress);
    result.pen_down = false;
    result
}

fn interpolate(
    from: &WorldPoint,
    to: &WorldPoint,
    offset: f64,
    length: f64,
    stroke_index: usize,
    vertex_index: usize,
    progress: f32,
) -> PathProgress {
    let t = if length <= f64::EPSILON {
        1.0
    } else {
        (offset / length).clamp(0.0, 1.0)
    };
    PathProgress {
        position: WorldPoint {
            x: from.x + (to.x - from.x) * t,
            y: from.y + (to.y - from.y) * t,
        },
        progress,
        stroke_index,
        vertex_index,
        pen_down: true,
        dwelling: false,
    }
}

fn distance(a: &WorldPoint, b: &WorldPoint) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// A worker's stable assignment for one ephemeral drawing run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerAssignment {
    pub role: WorkerRole,
    pub stroke_indexes: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerRole {
    Parent,
    Helper,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkerProgress {
    pub position: WorldPoint,
    pub progress: f32,
    pub pen_down: bool,
    pub dwelling: bool,
}

/// Deterministic ephemeral assignment plan; it describes workers and never
/// creates provider sessions or requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawingCrewPlan {
    assignments: Vec<WorkerAssignment>,
    budget_ms: u32,
}

impl DrawingCrewPlan {
    pub fn build(path: &WorkPath, helper_budget: usize, budget_ms: u32) -> Self {
        let helper_count = worker_count(
            path,
            helper_budget.min(DRAWING_HELPER_LIMIT),
            budget_ms.min(10_000),
        );
        let mut assignments = (0..=helper_count)
            .map(|index| WorkerAssignment {
                role: if index == 0 {
                    WorkerRole::Parent
                } else {
                    WorkerRole::Helper
                },
                stroke_indexes: Vec::new(),
            })
            .collect::<Vec<_>>();
        assign_strokes(path, &mut assignments);
        Self {
            assignments,
            budget_ms: budget_ms.min(10_000),
        }
    }

    pub fn assignments(&self) -> &[WorkerAssignment] {
        &self.assignments
    }
    pub fn budget_ms(&self) -> u32 {
        self.budget_ms
    }

    /// Prepare a worker route once, before frame sampling. The route begins
    /// with a synthetic one-point stroke at `birth`; authored route strokes
    /// follow in the order listed by `assignments()[worker_index].stroke_indexes`.
    /// Per-frame callers should retain this value and sample `position`.
    pub fn worker_path(
        &self,
        path: &WorkPath,
        worker_index: usize,
        birth: WorldPoint,
    ) -> Option<WorkPath> {
        let assignment = self.assignments.get(worker_index)?;
        birth.validate().ok()?;
        if assignment
            .stroke_indexes
            .iter()
            .any(|index| *index >= path.strokes.len())
        {
            return None;
        }
        assigned_route(path, assignment, birth)
    }
}

fn worker_count(path: &WorkPath, helper_budget: usize, budget_ms: u32) -> usize {
    if budget_ms == 0 || path.strokes.len() < 2 {
        return 0;
    }
    let estimated_ms = path.duration_weight;
    let needed_helpers = (estimated_ms / f64::from(budget_ms)).ceil() as usize;
    helper_budget
        .min(needed_helpers.saturating_sub(1))
        .min(path.strokes.len().saturating_sub(1))
        .min(DRAWING_HELPER_LIMIT)
}

fn assign_strokes(path: &WorkPath, assignments: &mut [WorkerAssignment]) {
    let mut loads = vec![0.0; assignments.len()];
    for stroke_index in 0..path.strokes.len() {
        let worker = loads
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        let prior_end = assignments[worker]
            .stroke_indexes
            .last()
            .map(|prior| path.strokes[*prior].last().unwrap());
        let stroke = &path.strokes[stroke_index];
        let pen_up = prior_end
            .map(|end| distance(end, &stroke[0]) * TRAVEL_MS_PER_WORLD_UNIT)
            .unwrap_or(0.0);
        loads[worker] += stroke_measure(stroke, path.stroke_modes[stroke_index]) + pen_up;
        assignments[worker].stroke_indexes.push(stroke_index);
    }
}

fn assigned_route(
    path: &WorkPath,
    assignment: &WorkerAssignment,
    birth: WorldPoint,
) -> Option<WorkPath> {
    let mut strokes = vec![vec![birth]];
    let mut stroke_modes = vec![PathMode::Continuous];
    for index in &assignment.stroke_indexes {
        strokes.push(path.strokes[*index].clone());
        stroke_modes.push(path.stroke_modes[*index]);
    }
    let point_count = strokes.iter().map(Vec::len).sum();
    if point_count > WORK_PATH_POINT_LIMIT {
        return None;
    }
    let mut route = WorkPath {
        strokes,
        mode: stroke_modes[0],
        stroke_modes,
        point_count,
        duration_weight: 0.0,
    };
    route.duration_weight = route.measure();
    Some(route)
}

#[cfg(test)]
mod tests;
