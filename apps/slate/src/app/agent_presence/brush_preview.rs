use super::*;
use crate::app::board::BoardXf;
use crate::app::board_path::{self, BrushLiveCanvas};
use eframe::egui::{self, Pos2, Rect};
use slate_doc::scene::{NodeId, StrokeSpan};
use std::collections::BTreeMap;

const PREVIEW_ROUTE_LIMIT: usize = 4_096;
const PREVIEW_CACHE_LIMIT: usize = 64;
const PREVIEW_POINT_BUDGET: usize = 32_768;
/// Both live-canvas textures (coverage and endpoint) are accounted for. Keep
/// the total bounded even when many helper lanes draw at once.
const PREVIEW_PIXEL_BUDGET: u64 = 16_000_000;

#[cfg(test)]
mod diagnostics;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BrushCoverage {
    Active,
    Held,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct BrushKey {
    tab: u64,
    node: NodeId,
}

struct BrushEntry {
    key: BrushKey,
    motion: String,
    lane: String,
    content_key: u64,
    points: Vec<Pos2>,
    tips: Vec<StrokeSpan>,
    world_bounds: [f32; 4],
    padding: f32,
    traced: usize,
    vertex: usize,
    tip_position: Pos2,
    tip: StrokeSpan,
    node_opacity: f32,
    finished: bool,
    seen: bool,
    rejected: bool,
    canvas: Option<BrushLiveCanvas>,
}

#[derive(Default)]
pub(crate) struct DrawingBrushPreview {
    entries: BTreeMap<BrushKey, BrushEntry>,
    synced_frame: Option<u64>,
    painted: BTreeMap<BrushKey, bool>,
    pixel_budget: u64,
}

impl DrawingBrushPreview {
    fn point_count(&self) -> usize {
        self.entries.values().map(|entry| entry.points.len()).sum()
    }

    fn can_admit(
        &self,
        tab: u64,
        node: NodeId,
        content_key: u64,
        motion: &str,
        lane: &str,
        points: usize,
    ) -> bool {
        let key = BrushKey { tab, node };
        self.entries.get(&key).is_some_and(|entry| {
            !entry.rejected
                && entry.content_key == content_key
                && entry.motion == motion
                && entry.lane == lane
        }) || (!self.entries.contains_key(&key)
            && self.entries.len() < PREVIEW_CACHE_LIMIT
            && points <= PREVIEW_ROUTE_LIMIT
            && self.point_count().saturating_add(points) <= PREVIEW_POINT_BUDGET)
    }

    fn covered(&self, tab: u64, node: NodeId, content_key: Option<u64>) -> Option<BrushCoverage> {
        let entry = self.entries.get(&BrushKey { tab, node })?;
        if entry.rejected || entry.canvas.is_none() {
            return None;
        }
        if content_key != Some(entry.content_key) {
            return None;
        }
        Some(BrushCoverage::Held)
    }

    fn release(&mut self, tab: u64, node: NodeId, content_key: u64) {
        self.entries.retain(|key, entry| {
            !(key.tab == tab && key.node == node && entry.content_key == content_key)
        });
    }

    fn owned_lane(&self, tab: u64, node: NodeId, motion: &str) -> Option<&str> {
        let entry = self.entries.get(&BrushKey { tab, node })?;
        (!entry.rejected && entry.motion == motion).then_some(entry.lane.as_str())
    }

    fn sync(&mut self, app: &mut SlateApp) {
        let frame = app.frame_no;
        if self.synced_frame == Some(frame) {
            return;
        }
        self.synced_frame = Some(frame);
        self.pixel_budget = PREVIEW_PIXEL_BUDGET;
        self.painted.clear();
        let tab = app.tab().id;
        for entry in self.entries.values_mut() {
            entry.seen = false;
            // A per-frame pixel-budget miss must retry on the next frame.
            // The node painter keeps the canonical fallback if that retry fails.
            entry.rejected = false;
        }

        {
            let (agents, roster) = (&app.presence.agents, &app.presence.roster);
            for agent in agents.iter().filter(|agent| agent.tab == tab) {
                let Some(work) = agent.work.as_ref() else {
                    continue;
                };
                let Some(sample) = roster.session(&agent.motion).map(PresenceSession::sample)
                else {
                    continue;
                };
                let motion = agent.motion.as_str();
                work.for_each_drawing_brush_path(sample, |path| {
                    self.observe(tab, motion, path);
                });
            }
        }

        // Drop canvases for inactive tabs. Their routes can be replayed from
        // the still-live witness when the tab becomes visible again.
        self.entries.retain(|key, _| key.tab == tab);
        self.entries.retain(|_, entry| {
            if entry.seen {
                return true;
            }
            let still_running = app.presence.agents.iter().any(|agent| {
                agent.tab == tab
                    && agent.motion.as_str() == entry.motion.as_str()
                    && agent.work.as_ref().is_some_and(|work| {
                        work.owns_drawing_brush_lane(&entry.lane, entry.key.node)
                    })
            });
            if still_running {
                return true;
            }
            let Some(node) = app.doc().scene.node(entry.key.node).cloned() else {
                return false;
            };
            if board_path::node_stamp_key(&node) != Some(entry.content_key) {
                return false;
            }
            entry.node_opacity = node.opacity;
            entry.finished = true;
            entry.traced = entry.points.len();
            entry.vertex = entry.points.len().saturating_sub(1);
            entry.tip_position = *entry.points.last().unwrap_or(&Pos2::ZERO);
            if let Some(canvas) = &mut entry.canvas {
                canvas.add_freehand(&entry.points, &entry.tips);
                canvas.clear_live_line();
                canvas.hold(entry.key.node, Some(entry.content_key));
            }
            true
        });
    }

    fn paint_one(
        &mut self,
        app: &mut SlateApp,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: NodeId,
    ) -> bool {
        self.sync(app);
        let key = BrushKey {
            tab: app.tab().id,
            node,
        };
        if let Some(painted) = self.painted.get(&key) {
            return *painted;
        }
        let Some(entry) = self.entries.get_mut(&key) else {
            self.painted.insert(key, false);
            return false;
        };
        let cost = paint_entry(
            entry,
            &mut app.brush_tiles,
            painter,
            xf,
            painter.clip_rect(),
            painter.ctx().pixels_per_point().max(0.25),
            self.pixel_budget,
        );
        self.pixel_budget = self.pixel_budget.saturating_sub(cost);
        let painted = cost > 0;
        self.painted.insert(key, painted);
        painted
    }

    fn observe(&mut self, tab: u64, motion: &str, path: super::witness::BrushPathSample<'_>) {
        if path.points.is_empty()
            || path.points.len() > PREVIEW_ROUTE_LIMIT
            || path.points.len() != path.tips.len()
        {
            return;
        }
        let Some(content_key) = board_path::node_stamp_key(path.node) else {
            return;
        };
        let key = BrushKey {
            tab,
            node: path.node.id,
        };
        if !self.entries.contains_key(&key) {
            let total = self.point_count();
            if self.entries.len() >= PREVIEW_CACHE_LIMIT
                || total.saturating_add(path.points.len()) > PREVIEW_POINT_BUDGET
            {
                return;
            }
            let mut points = Vec::with_capacity(path.points.len());
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            for point in path.points {
                let x = point.x as f32;
                let y = point.y as f32;
                if !x.is_finite() || !y.is_finite() {
                    return;
                }
                points.push(Pos2::new(x, y));
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
            }
            let width = path
                .tips
                .iter()
                .map(|tip| tip.width.max(0.0))
                .fold(0.0, f32::max);
            let softness = path
                .tips
                .iter()
                .map(|tip| tip.softness.max(0.0))
                .fold(0.0, f32::max);
            let padding = width * (0.5 + softness) + 5.0;
            let tip_position = Pos2::new(path.position.x as f32, path.position.y as f32);
            self.entries.insert(
                key,
                BrushEntry {
                    key,
                    motion: motion.to_owned(),
                    lane: path.lane_id.to_owned(),
                    content_key,
                    points,
                    tips: path.tips.to_vec(),
                    world_bounds: bounds,
                    padding,
                    traced: 0,
                    vertex: 0,
                    tip_position,
                    tip: path.tip,
                    node_opacity: path.node.opacity,
                    finished: false,
                    seen: false,
                    rejected: false,
                    canvas: None,
                },
            );
        }
        if self.entries.get(&key).is_some_and(|entry| {
            entry.content_key != content_key
                || entry.motion.as_str() != motion
                || entry.lane.as_str() != path.lane_id
        }) {
            self.entries.remove(&key);
            return;
        }
        let Some(entry) = self.entries.get_mut(&key) else {
            return;
        };
        entry.node_opacity = path.node.opacity;
        entry.seen = true;
        entry.finished |= path.finished;
        entry.vertex = path.vertex_index.min(entry.points.len().saturating_sub(1));
        entry.tip_position = Pos2::new(path.position.x as f32, path.position.y as f32);
        entry.tip = path.tip;
        if entry.finished {
            entry.traced = entry.points.len();
        } else if path.vertex_index + 1 > entry.traced {
            entry.traced = (path.vertex_index + 1).min(entry.points.len());
        }
    }
}

fn paint_entry(
    entry: &mut BrushEntry,
    tiles: &mut board_path::tiles::BrushTiles,
    painter: &egui::Painter,
    xf: &BoardXf,
    clip: Rect,
    ppp: f32,
    pixel_budget: u64,
) -> u64 {
    let screen = entry_screen(entry, xf, clip);
    if screen.width() <= 0.0 || screen.height() <= 0.0 {
        entry.canvas = None;
        entry.rejected = false;
        return 0;
    }
    let pixels = ((screen.width() * ppp).ceil().max(1.0) as u64)
        .saturating_mul((screen.height() * ppp).ceil().max(1.0) as u64);
    // `ensure` owns two GPU textures plus the CPU coverage image per pixel.
    let cost = pixels.saturating_mul(3);
    if pixels == 0 || cost > PREVIEW_PIXEL_BUDGET || cost > pixel_budget {
        entry.canvas = None;
        entry.rejected = true;
        return 0;
    }
    entry.rejected = false;
    let canvas = BrushLiveCanvas::ensure(
        &mut entry.canvas,
        tiles,
        painter,
        xf,
        screen,
        None,
        None,
        Vec::new,
    );
    let count = if entry.finished {
        entry.points.len()
    } else {
        entry.traced.max(1).min(entry.points.len())
    };
    canvas.add_freehand(&entry.points[..count], &entry.tips[..count]);
    if entry.finished {
        canvas.clear_live_line();
        canvas.hold(entry.key.node, Some(entry.content_key));
    } else {
        let from = entry.points[entry.vertex];
        let a = vector_ink::TipPoint {
            pos: [from.x, from.y],
            tip: entry.tips[entry.vertex].stamp_style(),
        };
        let b = vector_ink::TipPoint {
            pos: [entry.tip_position.x, entry.tip_position.y],
            tip: entry.tip.stamp_style(),
        };
        if a.pos != b.pos {
            canvas.set_line(a, b);
        } else {
            canvas.clear_live_line();
        }
    }
    let mut live_painter = painter.clone();
    live_painter.multiply_opacity(entry.node_opacity);
    canvas.paint(&live_painter, xf);
    cost
}

fn entry_screen(entry: &BrushEntry, xf: &BoardXf, clip: Rect) -> Rect {
    let pad = entry.padding * xf.z + 2.0;
    let min = xf.w2s(Pos2::new(entry.world_bounds[0], entry.world_bounds[1]));
    let max = xf.w2s(Pos2::new(entry.world_bounds[2], entry.world_bounds[3]));
    Rect::from_min_max(min - egui::vec2(pad, pad), max + egui::vec2(pad, pad)).intersect(clip)
}

impl SlateApp {
    /// Refresh paths and cache lifecycle once before scene-node painting.
    pub(crate) fn sync_agent_brush_previews(&mut self) {
        let mut cache = std::mem::take(&mut self.presence.brush_preview);
        cache.sync(self);
        self.presence.brush_preview = cache;
    }

    /// Paint one cached stroke at its normal scene-node z-order slot.
    pub(crate) fn paint_agent_brush_preview(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: NodeId,
    ) -> bool {
        let mut cache = std::mem::take(&mut self.presence.brush_preview);
        let painted = cache.paint_one(self, painter, xf, node);
        self.presence.brush_preview = cache;
        painted
    }

    pub(crate) fn agent_brush_coverage(
        &self,
        node: NodeId,
        content_key: Option<u64>,
    ) -> Option<BrushCoverage> {
        let tab = self.tab().id;
        // An active witness takes precedence over a held canvas: helper lanes
        // may finish before their parent command, while their commit is still
        // withheld. Admit only a route the bounded preview cache can own.
        let active = self
            .presence
            .agents
            .iter()
            .filter(|agent| agent.tab == tab)
            .any(|agent| {
                let Some(work) = agent.work.as_ref() else {
                    return false;
                };
                if self
                    .presence
                    .brush_preview
                    .owned_lane(tab, node, agent.motion.as_str())
                    .is_some_and(|lane| work.owns_drawing_brush_lane(lane, node))
                {
                    return true;
                }
                let Some(sample) = self
                    .presence
                    .roster
                    .session(&agent.motion)
                    .map(PresenceSession::sample)
                else {
                    return false;
                };
                let mut admitted = false;
                work.for_each_drawing_brush_path(sample, |path| {
                    if path.node.id == node {
                        if let Some(content_key) = board_path::node_stamp_key(path.node) {
                            admitted = self.presence.brush_preview.can_admit(
                                tab,
                                node,
                                content_key,
                                agent.motion.as_str(),
                                path.lane_id,
                                path.points.len(),
                            );
                        }
                    }
                });
                admitted
            });
        if active {
            return Some(BrushCoverage::Active);
        }
        let key = self
            .doc()
            .scene
            .node(node)
            .and_then(board_path::node_stamp_key);
        if content_key.is_some_and(|requested| Some(requested) != key) {
            return None;
        }
        if let Some(coverage) = self.presence.brush_preview.covered(tab, node, key) {
            return Some(coverage);
        }
        None
    }

    pub(crate) fn release_agent_brush_preview(&mut self, node: NodeId, content_key: u64) {
        let tab = self.tab().id;
        self.presence.brush_preview.release(tab, node, content_key);
    }

    pub(crate) fn update_agent_brush_opacity(&mut self, node: NodeId, opacity: f32) {
        let tab = self.tab().id;
        if let Some(entry) = self
            .presence
            .brush_preview
            .entries
            .get_mut(&BrushKey { tab, node })
        {
            entry.node_opacity = opacity;
        }
    }
}
