//! Named slide sequences are ordered Frame references, separate from canvas placement.
use crate::scene::{NodeId, Scene, SceneCmd, WorldRect};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presentation {
    pub id: u64,
    pub name: String,
    pub frames: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaSettings {
    pub video_start: Option<bool>,
    pub video_loop: Option<bool>,
    pub web_last_view: bool,
    pub web_bookmarks: std::collections::BTreeMap<NodeId, String>,
    pub interactive_models: bool,
}

impl Default for MediaSettings {
    fn default() -> Self {
        Self {
            video_start: None,
            video_loop: None,
            web_last_view: false,
            web_bookmarks: Default::default(),
            interactive_models: true,
        }
    }
}

impl MediaSettings {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl Scene {
    pub fn presentations(&self) -> &[Presentation] {
        &self.presentations
    }

    /// No chosen presentation preserves the legacy order. Hidden/deleted frames
    /// stay referenced so visibility changes and node undo restore their positions.
    pub fn presentation_frames(&self, id: Option<u64>) -> Vec<NodeId> {
        if let Some(deck) = id.and_then(|id| self.presentations.iter().find(|deck| deck.id == id)) {
            let mut seen = HashSet::new();
            deck.frames
                .iter()
                .copied()
                .filter(|id| {
                    seen.insert(*id) && self.node(*id).is_some_and(|n| n.is_frame() && !n.hidden)
                })
                .collect()
        } else {
            self.frames_in_order()
                .into_iter()
                .filter(|n| !n.hidden)
                .map(|n| n.id)
                .collect()
        }
    }

    pub fn presentation_command(
        &self,
        edit: impl FnOnce(&mut Vec<Presentation>),
    ) -> Option<SceneCmd> {
        let mut after = self.presentations.clone();
        edit(&mut after);
        (after != self.presentations).then(|| SceneCmd::Presentations {
            before: self.presentations.clone(),
            after,
        })
    }

    /// Move each frame and its geometrically owned contents exactly once.
    /// Sorting changes canvas placement, leaving every presentation sequence intact.
    pub fn sort_presentation_on_canvas(&self, id: Option<u64>) -> Vec<SceneCmd> {
        let ids = self.presentation_frames(id);
        let frames = ids
            .iter()
            .filter_map(|id| self.node(*id))
            .collect::<Vec<_>>();
        if frames.iter().any(|n| n.locked) {
            return Vec::new();
        }
        let mut positions = frames
            .iter()
            .map(|n| (n.rect.x, n.rect.y))
            .collect::<Vec<_>>();
        positions.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.total_cmp(&b.0)));
        let deltas = frames
            .iter()
            .zip(positions)
            .map(|(n, (x, y))| (n.id, (x - n.rect.x, y - n.rect.y)))
            .collect::<HashMap<_, _>>();
        self.nodes
            .iter()
            .filter_map(|node| {
                if node.locked {
                    return None;
                }
                let owner = if node.is_frame() {
                    node.id
                } else {
                    self.frame_of(node.id)?
                };
                let (dx, dy) = *deltas.get(&owner)?;
                if dx == 0.0 && dy == 0.0 {
                    return None;
                }
                let mut after = node.clone();
                after.rect = after.rect.translated(dx, dy);
                Some(SceneCmd::Patch {
                    before: Box::new(node.clone()),
                    after: Box::new(after),
                })
            })
            .collect()
    }
}

pub fn insert_frames(
    order: &[NodeId],
    selected: &HashSet<NodeId>,
    target: NodeId,
    after: bool,
) -> Vec<NodeId> {
    if selected.contains(&target) {
        return order.to_vec();
    }
    let (moved, mut rest): (Vec<_>, Vec<_>) =
        order.iter().copied().partition(|id| selected.contains(id));
    let Some(index) = rest.iter().position(|id| *id == target) else {
        return order.to_vec();
    };
    let at = index + usize::from(after);
    rest.splice(at..at, moved);
    rest
}

/// Frames a stroke enters in first-contact order, with topmost winning ties.
pub fn frames_along(scene: &Scene, points: &[[f32; 2]]) -> Vec<NodeId> {
    let mut hit = Vec::new();
    let segments = points.windows(2).map(|w| (w[0], w[1]));
    for (a, b) in segments.chain(
        points
            .first()
            .filter(|_| points.len() == 1)
            .map(|p| (*p, *p)),
    ) {
        let mut cursor = 0.0;
        loop {
            let mut best: Option<(f32, usize, NodeId)> = None;
            for (index, node) in scene.nodes.iter().enumerate() {
                if node.hidden || !node.is_frame() || hit.contains(&node.id) {
                    continue;
                }
                let Some(t) = segment_entry(a, b, node.rect) else {
                    continue;
                };
                if t + 1.0e-4 < cursor {
                    continue;
                }
                if best.is_none_or(|(bt, bi, _)| {
                    t < bt - 1.0e-4 || ((t - bt).abs() <= 1.0e-4 && index > bi)
                }) {
                    best = Some((t, index, node.id));
                }
            }
            let Some((t, _, id)) = best else {
                break;
            };
            hit.push(id);
            cursor = t + 1.0e-3;
            if cursor > 1.0 {
                break;
            }
        }
    }
    hit
}

fn segment_entry(a: [f32; 2], b: [f32; 2], r: WorldRect) -> Option<f32> {
    let [dx, dy] = [b[0] - a[0], b[1] - a[1]];
    let (mut first, mut last) = (0.0_f32, 1.0_f32);
    for (p, q) in [
        (-dx, a[0] - r.x),
        (dx, r.x + r.w - a[0]),
        (-dy, a[1] - r.y),
        (dy, r.y + r.h - a[1]),
    ] {
        if p.abs() < 1.0e-8 {
            if q < 0.0 {
                return None;
            }
        } else if p < 0.0 {
            first = first.max(q / p);
        } else {
            last = last.min(q / p);
        }
        if first > last {
            return None;
        }
    }
    Some(first)
}
