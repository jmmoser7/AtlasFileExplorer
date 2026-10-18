//! Actual drawing routes and partial geometry, prepared once per witnessed call.
use super::*;
use crate::app::board_color;
use crate::app::board_color::EraserSweepQuery;
use crate::app::board_path::witness::StrokePreview;
use atlas_agent::presence::{DrawingCrewPlan, PathMode, WorkPath};
use slate_doc::scene::StrokeSpan;

const DRAWING_BUDGET_MS: u32 = 6_000;
const VERTEX_DWELL_MS: u32 = 120;

struct InkStroke {
    node: Node,
    geometry: StrokePreview,
    vertices: bool,
}
struct Lane {
    route: WorkPath,
    strokes: Vec<usize>,
    birth_marker: usize,
    id: String,
    position: WorldPoint,
}

struct ErasePreview {
    span: StrokeSpan,
    pairs: Vec<(Node, Option<Node>)>,
    vector_first_hit: Vec<Option<f32>>,
}

pub(super) struct DrawingRun {
    path: WorkPath,
    ink: Vec<InkStroke>,
    erase: Option<ErasePreview>,
    lanes: Vec<Lane>,
    helper_limit: usize,
    key: String,
    last_sample: Option<(MotionPhase, u32)>,
    helpers_visible: bool,
}

pub(in crate::app::agent_presence) struct BrushPathSample<'a> {
    pub lane_id: &'a str,
    pub node: &'a Node,
    pub points: &'a [WorldPoint],
    pub tips: &'a [StrokeSpan],
    pub vertex_index: usize,
    pub position: WorldPoint,
    pub tip: StrokeSpan,
    pub finished: bool,
}

impl DrawingRun {
    pub(super) fn owns_brush_lane(&self, lane_id: &str, id: slate_doc::NodeId) -> bool {
        self.lanes.iter().any(|lane| lane.id == lane_id)
            && self.ink.iter().any(|stroke| stroke.node.id == id)
    }

    pub(super) fn prepare(
        prepared: &PreparedBoardCommand,
        steps: &[(Option<Node>, Option<Node>)],
        key: &str,
    ) -> Result<Option<Self>, String> {
        let mut ink = Vec::new();
        let mut routes = Vec::new();
        let erase = if prepared.command == "board.eraser.stroke" {
            let points: Vec<_> = prepared
                .erase_path()
                .iter()
                .map(|p| point(p[0] as f32, p[1] as f32))
                .collect();
            routes.push(points.clone());
            let pairs: Vec<(Node, Option<Node>)> = steps
                .iter()
                .filter_map(|(before, after)| Some((before.clone()?, after.clone())))
                .collect();
            let span = prepared.erase_span().ok_or("Missing eraser span")?;
            let sweep_points: Vec<_> = points
                .iter()
                .map(|point| egui::pos2(point.x as f32, point.y as f32))
                .collect();
            let sample_count =
                board_color::eraser_sweep_sample_count(&sweep_points, span.width * 0.5);
            if sample_count > board_color::MAX_ERASER_SWEEP_SAMPLES {
                return Err("Eraser preview exceeds the per-request sample budget".into());
            }
            let mut queries = Vec::with_capacity(pairs.len());
            let mut sweep_work = 0usize;
            for (before, after) in &pairs {
                let query = after
                    .is_none()
                    .then(|| {
                        EraserSweepQuery::prepare(before, span.width * 0.5, prepared.camera_zoom())
                    })
                    .flatten();
                if let Some(query) = &query {
                    sweep_work = board_color::next_eraser_sweep_work(
                        sweep_work,
                        sample_count,
                        query.segment_count(),
                    )
                    .ok_or(
                        "Eraser preview exceeds the bounded geometry-work budget; split the sweep",
                    )?;
                }
                queries.push(query);
            }
            let vector_first_hit = pairs
                .iter()
                .zip(&queries)
                .map(|((_, after), query)| {
                    after
                        .is_none()
                        .then(|| {
                            query
                                .as_ref()
                                .and_then(|query| first_vector_hit(query, &points))
                        })
                        .flatten()
                })
                .collect();
            Some(ErasePreview {
                span,
                pairs,
                vector_first_hit,
            })
        } else if prepared.command == "board.paths.draw" {
            for (_, after) in steps {
                let node = after.as_ref().ok_or("Missing drawing result")?;
                let tolerance =
                    f64::from(node.rect.w.abs().max(node.rect.h.abs()) * 0.0005).max(0.1);
                let geometry = StrokePreview::new(node, tolerance)
                    .ok_or("Drawing route exceeds the bounded preview budget")?;
                if geometry.contours().len() != 1 {
                    return Err("Split compound drawing paths into individual strokes".into());
                }
                routes.push(geometry.contours()[0].clone());
                ink.push(InkStroke {
                    node: node.clone(),
                    geometry,
                    vertices: prepared.vertex_drawing(node.id),
                });
            }
            None
        } else {
            return Ok(None);
        };
        let modes = if ink.is_empty() {
            vec![PathMode::Continuous; routes.len()]
        } else {
            ink.iter()
                .map(|stroke| {
                    if stroke.vertices {
                        PathMode::VertexByVertex {
                            dwell_ms: VERTEX_DWELL_MS,
                        }
                    } else {
                        PathMode::Continuous
                    }
                })
                .collect()
        };
        let path = WorkPath::new_with_modes(routes, modes).map_err(|e| e.to_string())?;
        Ok(Some(Self {
            path,
            ink,
            erase,
            lanes: Vec::new(),
            helper_limit: 0,
            key: key.into(),
            last_sample: None,
            helpers_visible: false,
        }))
    }
    pub(super) fn set_helper_limit(&mut self, limit: u8) {
        self.helper_limit = usize::from(limit);
    }
    pub(super) fn begin(&mut self, motion: &mut PresenceSession) -> Result<(), String> {
        let helpers = if self.erase.is_some() {
            0
        } else {
            self.helper_limit
        };
        let crew = DrawingCrewPlan::build(&self.path, helpers, DRAWING_BUDGET_MS);
        let contact = self.path.strokes()[0][0];
        for (index, assignment) in crew.assignments().iter().enumerate() {
            let (route, birth_marker) = if index == 0 {
                let strokes = assignment
                    .stroke_indexes
                    .iter()
                    .map(|i| self.path.strokes()[*i].clone())
                    .collect();
                let modes = assignment
                    .stroke_indexes
                    .iter()
                    .map(|i| self.path.stroke_mode(*i).expect("assigned stroke mode"))
                    .collect();
                (
                    WorkPath::new_with_modes(strokes, modes).map_err(|e| e.to_string())?,
                    0,
                )
            } else {
                (
                    crew.worker_path(&self.path, index, contact)
                        .ok_or("Drawing helper route exceeds budget")?,
                    1,
                )
            };
            self.lanes.push(Lane {
                route,
                strokes: assignment.stroke_indexes.clone(),
                birth_marker,
                id: format!("{}:ink:{index}", self.key),
                position: contact,
            });
        }
        let work_ms = self
            .lanes
            .iter()
            .map(|lane| route_duration(&lane.route))
            .fold(600.0_f64, f64::max)
            .clamp(600.0, f64::from(DRAWING_BUDGET_MS)) as u32;
        let start = motion.sample().position;
        let distance = ((start.x - contact.x).powi(2) + (start.y - contact.y).powi(2)).sqrt();
        let target = self
            .ink
            .first()
            .map_or_else(|| self.erase.as_ref().unwrap().pairs[0].0.id, |s| s.node.id);
        let plan = MotionPlan {
            session_id: motion.id().clone(),
            request_id: motion
                .request_id()
                .cloned()
                .ok_or("Missing drawing request")?,
            action_id: id(&atlas_agent::request_id())?,
            target_nodes: vec![id(&target.0.to_string())?],
            start,
            end: contact,
            work_end: None,
            durations: MotionDurations {
                travel_ms: (distance / 1.8).clamp(180.0, 650.0) as u32,
                grab_ms: 90,
                work_ms,
            },
        };
        motion
            .enqueue_path(plan, self.lanes[0].route.clone())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(super) fn preview(
        &mut self,
        sample: MotionSample,
        previews: &mut HashMap<slate_doc::NodeId, Node>,
    ) {
        if !matches!(sample.phase, MotionPhase::Work | MotionPhase::ReadyToCommit) {
            return;
        }
        if self.last_sample == Some((sample.phase, sample.progress.to_bits())) {
            return;
        }
        self.last_sample = Some((sample.phase, sample.progress.to_bits()));
        self.helpers_visible = true;
        if let Some(erase) = &self.erase {
            let span = erase.span;
            let at = self.path.position(sample.progress);
            let points = &self.path.strokes()[0];
            let mut prefix: Vec<_> = points
                .iter()
                .take(at.vertex_index + 1)
                .map(|p| egui::pos2(p.x as f32, p.y as f32))
                .collect();
            prefix.push(egui::pos2(at.position.x as f32, at.position.y as f32));
            for (index, (before, after)) in erase.pairs.iter().enumerate() {
                let node = if sample.phase == MotionPhase::ReadyToCommit {
                    after.clone().unwrap_or_else(|| {
                        let mut n = before.clone();
                        n.opacity = 0.;
                        n
                    })
                } else if after.is_some() {
                    crate::app::board_color::with_erase_mark(before, &prefix, span)
                } else {
                    let mut n = before.clone();
                    if erase.vector_first_hit[index]
                        .is_some_and(|threshold| sample.progress >= threshold)
                    {
                        n.opacity = 0.;
                    }
                    n
                };
                previews.insert(before.id, node);
            }
            return;
        }
        for lane in &mut self.lanes {
            let at = lane.route.position(sample.progress);
            lane.position = at.position;
            let Some(current) = at.stroke_index.checked_sub(lane.birth_marker) else {
                continue;
            };
            for (order, index) in lane.strokes.iter().enumerate() {
                let stroke = &self.ink[*index];
                if sample.phase == MotionPhase::ReadyToCommit || order < current {
                    previews.insert(stroke.node.id, stroke.node.clone());
                } else if order == current && at.pen_down {
                    previews.insert(
                        stroke.node.id,
                        stroke
                            .geometry
                            .through(0, at.vertex_index, at.position, true, false),
                    );
                }
            }
        }
    }
    pub(super) fn helpers(&self) -> impl Iterator<Item = (&str, WorldPoint)> {
        self.lanes
            .iter()
            .skip(1)
            .filter(|_| self.helpers_visible)
            .map(|l| (l.id.as_str(), l.position))
    }

    /// Visit the already-flattened stamped paths that this lane has reached.
    /// Completed contours remain available for the live-canvas handoff;
    /// the current contour carries its exact route prefix and interpolated
    /// authored tip without rebuilding path geometry.
    pub(super) fn for_each_brush_path(
        &self,
        sample: MotionSample,
        mut visit: impl FnMut(BrushPathSample<'_>),
    ) {
        if !matches!(sample.phase, MotionPhase::Work | MotionPhase::ReadyToCommit) {
            return;
        }
        let ready = sample.phase == MotionPhase::ReadyToCommit;
        for lane in &self.lanes {
            let at = lane.route.position(sample.progress);
            let Some(current) = at.stroke_index.checked_sub(lane.birth_marker) else {
                continue;
            };
            for (order, index) in lane.strokes.iter().enumerate() {
                let stroke = &self.ink[*index];
                let NodeKind::Shape(shape) = &stroke.node.kind else {
                    continue;
                };
                if !shape.stroke.paints_as_stamp() || shape.stroke.is_none() {
                    continue;
                }
                let is_finished = ready || order < current;
                if !is_finished && (order != current || !at.pen_down) {
                    continue;
                }
                let Some((points, tips)) = stroke.geometry.brush_contour(0) else {
                    continue;
                };
                if points.len() != tips.len() || points.is_empty() {
                    continue;
                }
                let (vertex_index, position) = if is_finished {
                    (points.len() - 1, *points.last().expect("nonempty points"))
                } else {
                    (at.vertex_index.min(points.len() - 1), at.position)
                };
                let Some(tip) = stroke.geometry.brush_tip_at(0, position) else {
                    continue;
                };
                visit(BrushPathSample {
                    lane_id: &lane.id,
                    node: &stroke.node,
                    points,
                    tips,
                    vertex_index,
                    position,
                    tip,
                    finished: is_finished,
                });
            }
        }
    }
    pub(super) fn look_at(&self) -> Option<WorldPoint> {
        self.lanes.first().map(|lane| lane.position)
    }
}

fn first_vector_hit(query: &EraserSweepQuery, points: &[WorldPoint]) -> Option<f32> {
    let total: f64 = points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum();
    if total <= f64::EPSILON {
        return None;
    }
    let mut walked = 0.0_f64;
    for pair in points.windows(2) {
        let length = distance(pair[0], pair[1]);
        let from = egui::pos2(pair[0].x as f32, pair[0].y as f32);
        let to = egui::pos2(pair[1].x as f32, pair[1].y as f32);
        if query.hits(from, to) {
            let mut low = 0.0_f32;
            let mut high = 1.0_f32;
            for _ in 0..8 {
                let middle = (low + high) * 0.5;
                if query.hits(from, from.lerp(to, middle)) {
                    high = middle;
                } else {
                    low = middle;
                }
            }
            return Some(((walked + length * f64::from(high)) / total).clamp(0.0, 1.0) as f32);
        }
        walked += length;
    }
    None
}

fn distance(a: WorldPoint, b: WorldPoint) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn route_duration(route: &WorkPath) -> f64 {
    let mut previous: Option<WorldPoint> = None;
    let mut duration = 0.0;
    for (index, stroke) in route.strokes().iter().enumerate() {
        for point in stroke {
            if let Some(start) = previous {
                duration += distance(start, *point) * 10.0;
            }
            previous = Some(*point);
        }
        if let Some(PathMode::VertexByVertex { dwell_ms }) = route.stroke_mode(index) {
            duration += stroke.len() as f64 * f64::from(dwell_ms);
        }
    }
    duration
}
