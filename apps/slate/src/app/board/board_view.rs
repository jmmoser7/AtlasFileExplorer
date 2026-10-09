//! Camera fit, tool arming, and the active kit recipe.

use super::*;

// ---------- SlateApp: board state helpers ----------

/// Node `n` paints in `view` ([`SlateApp::board_paint_view`]): it is not
/// hidden, and its ink meets the view or it is rotated.
pub(crate) fn paints_in_view(n: &Node, view: &WorldRect) -> bool {
    if n.hidden {
        return false;
    }
    let ink = match &n.kind {
        NodeKind::Shape(s) if !s.stroke.is_none() => s.stroke.width.max(0.0) * 0.5,
        _ => 0.0,
    };
    let r = n.rect.normalized();
    let visible = r.x - ink <= view.x + view.w
        && r.x + r.w + ink >= view.x
        && r.y - ink <= view.y + view.h
        && r.y + r.h + ink >= view.y;
    visible || n.rotation_deg.abs() > 0.01
}

/// The broad-phase rect [`SlateApp::board_paint_nodes`] queries the scene
/// index with for `view`. The index holds centerline bounds and ink reaches
/// half a stroke width past them, so it is wide enough for the thickest
/// stroke; [`paints_in_view`] then tests each node's own ink bounds.
pub(crate) fn paint_query(view: &WorldRect) -> WorldRect {
    let reach = super::super::settings::STROKE_WIDTH_MAX * 0.5;
    WorldRect::new(
        view.x - reach,
        view.y - reach,
        view.w + reach * 2.0,
        view.h + reach * 2.0,
    )
}

/// Node `n` is among the candidates [`paint_query`] returns for `view`.
pub(crate) fn in_paint_query(n: &Node, view: &WorldRect) -> bool {
    slate_doc::node_aabb(n).intersects(&paint_query(view))
}

impl SlateApp {
    pub fn board_xf(&self) -> BoardXf {
        let cam = self.tab().cam;
        BoardXf {
            center: self.canvas_rect.center(),
            offset: cam.offset,
            z: cam.z,
        }
    }

    /// The world rect [`Self::board_paint_nodes`] culls against for
    /// `screen`: the view plus a margin for strokes.
    pub(crate) fn board_paint_view(&self, screen: Rect) -> WorldRect {
        let xf = self.board_xf();
        let a = xf.s2w(screen.min);
        let b = xf.s2w(screen.max);
        let pad = 80.0 / xf.z.max(0.05);
        WorldRect::new(
            a.x.min(b.x) - pad,
            a.y.min(b.y) - pad,
            (a.x - b.x).abs() + pad * 2.0,
            (a.y - b.y).abs() + pad * 2.0,
        )
    }

    /// Nodes whose AABB intersects `screen` (plus a margin for strokes).
    pub(crate) fn board_paint_nodes(&self, screen: Rect) -> Vec<Node> {
        let view = self.board_paint_view(screen);
        self.doc()
            .scene
            .query_rect(paint_query(&view))
            .into_iter()
            .filter_map(|id| {
                let n = self.doc().scene.node(id)?;
                paints_in_view(n, &view).then(|| {
                    if let Some(p) = self.smooth_preview.get(&id) {
                        return p.clone();
                    }
                    self.shape_properties
                        .preview
                        .iter()
                        .find(|p| p.id == id)
                        .unwrap_or(n)
                        .clone()
                })
            })
            .collect()
    }

    pub(super) fn scene_bounds(&self) -> Option<Rect> {
        let nodes = &self.doc().scene.nodes;
        if nodes.is_empty() {
            return None;
        }
        let mut b = Rect::NOTHING;
        for n in nodes {
            b = b.union(Rect::from_min_size(
                Pos2::new(n.rect.x, n.rect.y),
                Vec2::new(n.rect.w, n.rect.h),
            ));
        }
        Some(b)
    }

    pub fn fit_board(&mut self) {
        let Some(bounds) = self.scene_bounds() else {
            return;
        };
        let canvas = self.canvas_rect;
        let z = ((canvas.width() / bounds.width().max(1.0))
            .min(canvas.height() / bounds.height().max(1.0))
            * 0.9)
            .clamp(ZOOM_MIN, ZOOM_MAX);
        let cam = &mut self.tab_mut().cam;
        cam.z = z;
        cam.offset = bounds.center().to_vec2();
    }

    /// Frame one world rect in the viewport, with a little breathing room.
    pub(crate) fn zoom_to_rect(&mut self, r: WorldRect) {
        let canvas = self.canvas_rect;
        let z = ((canvas.width() / r.w.max(1.0)).min(canvas.height() / r.h.max(1.0)) * 0.9)
            .clamp(ZOOM_MIN, ZOOM_MAX);
        let (cx, cy) = r.center();
        let cam = &mut self.tab_mut().cam;
        cam.z = z;
        cam.offset = Vec2::new(cx, cy);
    }

    /// Switch the board tool through one place: the brush chain breaks on
    /// every re-arm, and direct-selection state clears when leaving A.
    pub(crate) fn set_board_tool(&mut self, tool: BoardTool) {
        if let Some(draft) = self.text_box_draft.take() {
            if draft.buffer.is_empty() {
                // Tool switch discards an empty compose (no journal).
            } else {
                self.text_box_draft = Some(draft);
                self.commit_text_box_draft();
            }
        }
        self.shape_properties = Default::default();
        self.desktop_sample = None;
        self.armed_kit_id = None;
        self.brush_straight = None;
        self.brush_chain = Default::default();
        self.pen_straight = None;
        self.draft_lock = None;
        if tool != BoardTool::DirectSelect {
            self.direct.node = None;
            self.direct.anchors.clear();
        }
        // Any tool switch (including re-arming L) restarts the line draft.
        self.line_draft = None;
        if self.board_tool == BoardTool::Deck
            && tool != BoardTool::Deck
            && matches!(self.board_drag, Some(BoardDrag::DeckStroke { .. }))
        {
            self.board_drag = None;
        }
        if tool == BoardTool::Trim {
            self.trim_arm();
        } else if tool == BoardTool::Split {
            self.split_arm();
        } else {
            self.trim = None;
            self.board_tool = tool;
            self.sync_image_paint_for_tool();
        }
        if tool == BoardTool::Eyedropper {
            self.start_tool_desktop_sample(self.alt_down, false);
        }
        // Dock clicks arm through here without dispatch. Record the tool so
        // Space/Enter repeat the tool the user just chose, not an older one.
        // Select is the cancel destination and must not become that target.
        if tool != BoardTool::Select {
            if let Some(id) = tool.command_id() {
                let latest = self.cmd_history.iter().last().map(|e| e.id.0);
                if latest != Some(id) {
                    self.push_history(atlas_commands::CommandId(id), None);
                }
            }
        }
    }

    pub(crate) fn disarm_create(&mut self) {
        self.board_tool = BoardTool::Select;
        self.armed_kit_id = None;
        self.clear_image_paint_session();
    }

    pub(super) fn active_recipe(&self, tool: BoardTool) -> Option<slate_kit::Recipe> {
        if let Some(id) = self.armed_kit_id.as_deref() {
            if let Some(recipe) = self.kits.recipe_for_id(id) {
                return Some(recipe.clone());
            }
        }
        self.kits.recipe_for(tool).cloned()
    }
}
