//! Gesture begin, pick, and press setup.

use super::*;

impl SlateApp {
    // ----- gesture handling ------------------------------------------------------

    pub(crate) fn begin_gesture(
        &mut self,
        screen: Pos2,
        world: Pos2,
        mods: egui::Modifiers,
    ) -> Option<BoardDrag> {
        self.freehand_resume = false;
        // A dropped toolbar is a node: press-drag anywhere on it (icons
        // included) moves the strip. Create tools stay armed; they do not
        // start a draw from the toolbar.
        if let Some(drag) = self.begin_dock_strip_drag(screen, world) {
            return Some(drag);
        }
        match self.board_tool {
            BoardTool::Select => {
                // Crop mode intercepts everything on its node: handles move
                // the crop window, interior drags pan the content, presses
                // outside exit crop mode and fall through to normal behavior.
                if self.board_crop.is_some() {
                    if let Some(drag) = self.begin_crop_blister(screen) {
                        return Some(drag);
                    }
                    let on_image = self.press_on_selected_crop(world);
                    if !on_image {
                        self.board_crop = None;
                    }
                }
                // Match hover priority: the visible fillet grip wins any
                // overlap with wire/resize bands, and a curve grip only
                // where it is nearer (`fillet_grip_hit_at`).
                if let Some(drag) = self.begin_fillet_drag(screen, world) {
                    return Some(drag);
                }
                // Endpoint grips on a selected simple line — these replace
                // the resize bbox entirely (P1.curve.grips, contract D13).
                if self.board_sel.len() == 1 {
                    let id = *self.board_sel.iter().next().unwrap();
                    if let Some(n) = self.doc().scene.node(id).cloned() {
                        if Self::node_uses_curve_grips(&n) {
                            let xf = self.board_xf();
                            if let Some(end) = self.line_grip_at(id, screen, &xf) {
                                if !self.direct.grip_points.is_picked(id, end as usize) {
                                    self.direct.grip_points.pick(id, end as usize, mods.shift);
                                }
                                return Some(BoardDrag::LineGrip { id, before: n, end });
                            }
                        }
                    }
                }
                // Vertex / handle / arc grips on a selected curve.
                if let Some(drag) = self.begin_curve_grip_drag(screen, mods) {
                    return Some(BoardDrag::Direct(drag));
                }
                if let Some(drag) = self.begin_picked_edge_drag(world) {
                    return Some(BoardDrag::Direct(drag));
                }
                // Wire grip at the press origin beats edge resize. The rest
                // of the edge is Windows-style resize (no selection needed).
                if let Some(wd) = self.try_begin_wire_drag(screen, world, mods) {
                    return Some(BoardDrag::Wire(wd));
                }
                if let Some(drag) = self.begin_transform_drag(screen, world) {
                    return Some(drag);
                }
                // Dragging inside an unlocked 3D viewport orbits its camera
                // instead of moving the node (Alt still duplicates, so the
                // node itself can be grabbed by locking or Alt-dragging).
                if !self.alt_down {
                    if let Some(id) = self.live_model_at(world.x, world.y) {
                        // Orbiting also selects the node (egui suppresses the
                        // click after a drag), so its resize handles appear
                        // and win the next press — live viewports resize
                        // exactly like images.
                        if !self.board_sel.contains(&id) {
                            self.board_sel.clear();
                            self.board_sel.insert(id);
                        }
                        let tool = self
                            .model3d
                            .live
                            .get(&id)
                            .map(|vp| vp.tool)
                            .unwrap_or(model3d::ModelViewportTool::Navigate);
                        if tool == model3d::ModelViewportTool::MeasureDistance && !self.shift_down {
                            return Some(BoardDrag::ModelMeasure {
                                id,
                                start_screen: screen,
                            });
                        }
                        return Some(BoardDrag::ModelOrbit {
                            id,
                            last_screen: screen,
                        });
                    }
                }
                // Locked nodes are unpickable, except one already force-
                // selected via Ctrl+Shift+click (the one-off edit hatch).
                let mut picked = self.board_pick_node(world.x, world.y);
                if picked.is_none() {
                    let forced = board_path::board_pick_node_routed(
                        &self.doc().scene,
                        world.x,
                        world.y,
                        self.tab().cam.z,
                        true,
                        self.board_wire_routing,
                    );
                    if let Some(f) = forced {
                        if self.board_sel.contains(&f) {
                            picked = Some(f);
                        }
                    }
                }
                match picked {
                    Some(hit) if self.frame_body_selects_contents(hit) => {
                        Some(BoardDrag::Marquee {
                            start_screen: screen,
                            frame: Some(hit),
                        })
                    }
                    Some(hit) => {
                        self.apply_select_pick(hit, mods, true);
                        let sel: Vec<NodeId> = self.board_sel.iter().copied().collect();
                        if self.alt_down {
                            // Alt-drag duplicate: insert copies (journaled on release).
                            let expanded = self.expand_with_members(&sel);
                            let sources: Vec<Node> = expanded
                                .iter()
                                .filter_map(|i| self.doc().scene.node(*i).cloned())
                                .collect();
                            let (ids, before) = self.stage_unjournaled_duplicates(&sources);
                            Some(BoardDrag::Move {
                                ids,
                                before,
                                start_world: world,
                                dup: true,
                            })
                        } else {
                            let expanded = self.expand_with_members(&sel);
                            let before: Vec<Node> = expanded
                                .iter()
                                .filter_map(|i| self.doc().scene.node(*i).cloned())
                                .collect();
                            let ids = before.iter().map(|n| n.id).collect();
                            Some(BoardDrag::Move {
                                ids,
                                before,
                                start_world: world,
                                dup: false,
                            })
                        }
                    }
                    None => Some(BoardDrag::Marquee {
                        start_screen: screen,
                        frame: None,
                    }),
                }
            }
            BoardTool::Text => {
                let start = self.resolve_point_snap(world, &[], None, false, false);
                Some(BoardDrag::Draw {
                    start_world: start,
                    start_screen: screen,
                    tool: BoardTool::Text,
                })
            }
            BoardTool::Pan => None, // drag pans the canvas
            BoardTool::Pen => Some(BoardDrag::FreehandPen {
                stroke: board_path::FreehandTips::new(
                    world,
                    self.placed_tip(slate_doc::StrokeTool::Pen),
                ),
            }),
            BoardTool::Brush => {
                if self.alt_down || self.shift_down {
                    // Alt samples; Shift draws a straight segment. The
                    // ordered press owns both. Neither starts freehand ink.
                    None
                } else {
                    Some(BoardDrag::FreehandBrush {
                        stroke: board_path::FreehandTips::new(world, self.tip_now().span()),
                    })
                }
            }
            BoardTool::Eraser => Some(self.begin_erase(world, mods.shift)),
            BoardTool::Smooth => Some(self.begin_smooth(world, mods.shift)),
            BoardTool::Eyedropper | BoardTool::Sticky | BoardTool::Trim | BoardTool::Split => None, // click tools
            BoardTool::Deck => Some(BoardDrag::DeckStroke {
                start_screen: screen,
                points: vec![world],
            }),
            BoardTool::DirectSelect => self
                .begin_direct_drag(screen, world, mods)
                .map(BoardDrag::Direct),
            BoardTool::BezierSpan => {
                if let Some(hit) = self.bezier_draft_hit(screen) {
                    // Past the close delay the start anchor closes the span;
                    // before it, the press edits the anchor as usual.
                    if hit == super::super::path_edit_overlay::PathEditHit::Anchor(0)
                        && self.bezier_close_ready()
                    {
                        self.close_bezier_draft();
                        return None;
                    }
                    self.bezier_note_edit_press();
                    if let Some(board_path::BoardPathDraft::Bezier { anchors, .. }) =
                        &self.board_path_draft
                    {
                        return Some(BoardDrag::BezierEdit {
                            hit,
                            start: world,
                            anchors0: anchors.clone(),
                        });
                    }
                }
                let from = self.pending_segment_origin();
                let press = self.resolve_segment_point(from, world, mods.shift);
                self.draft_lock = None;
                self.bezier_anchor_press(press);
                Some(BoardDrag::BezierAnchor { press })
            }
            BoardTool::Polyline | BoardTool::Arc | BoardTool::Line => None,
            // The DragRect family. Named rather than caught by a wildcard so a
            // new tool cannot fall into a press-drag-release it never asked for;
            // `kits::tests` checks this list against `BoardTool::grammar`.
            tool @ (BoardTool::Frame
            | BoardTool::RectShape
            | BoardTool::Ellipse
            | BoardTool::Polygon
            | BoardTool::AgentPortal
            | BoardTool::WebPortal
            | BoardTool::AtlasPortal
            | BoardTool::SlatePortal) => {
                let start = self.resolve_point_snap(world, &[], None, false, false);
                Some(BoardDrag::Draw {
                    start_world: start,
                    start_screen: screen,
                    tool,
                })
            }
        }
    }

    /// True when `id` is a frame large enough on screen that a body drag
    /// should marquee its members instead of moving the frame.
    pub(super) fn frame_body_selects_contents(&self, id: NodeId) -> bool {
        let Some(n) = self.doc().scene.node(id) else {
            return false;
        };
        if !n.is_frame() {
            return false;
        }
        let screen = self.board_xf().rect_w2s(n.rect).size();
        let view = self.canvas_rect.size();
        screen.x >= view.x * FRAME_CONTENTS_SELECT_COVER
            && screen.y >= view.y * FRAME_CONTENTS_SELECT_COVER
    }

    pub(crate) fn board_pick_node(&self, x: f32, y: f32) -> Option<NodeId> {
        board_path::board_pick_node_routed(
            &self.doc().scene,
            x,
            y,
            self.tab().cam.z,
            false,
            self.board_wire_routing,
        )
    }

    /// Keep a picture being resized mirrored on exactly the axes its drag
    /// has crossed. Rebuilt from the gesture-start node, so dragging back
    /// restores it; the release commits one Patch like any resize.
    pub(super) fn sync_resize_mirror(&mut self, id: NodeId, crossed: [bool; 2]) {
        let Some(BoardDrag::Resize { before, .. }) = &self.board_drag else {
            return;
        };
        let (NodeKind::Image(start), Some(NodeKind::Image(live))) =
            (&before.kind, self.doc().scene.node(id).map(|n| &n.kind))
        else {
            return;
        };
        if [live.flip_x != start.flip_x, live.flip_y != start.flip_y] == crossed {
            return;
        }
        let mut node = before.clone();
        if crossed[0] {
            slate_doc::mirror::mirror_local(&mut node, slate_doc::mirror::MirrorAxis::Horizontal);
        }
        if crossed[1] {
            slate_doc::mirror::mirror_local(&mut node, slate_doc::mirror::MirrorAxis::Vertical);
        }
        if let Some(live) = self.doc_mut().scene.node_mut(id) {
            live.kind = node.kind;
            live.clip = node.clip;
        }
    }

    #[cfg(test)]
    pub(crate) fn update_gesture_for_test(&mut self, world: Pos2, mods: egui::Modifiers) {
        self.update_gesture(world, mods);
    }

    #[cfg(test)]
    pub(crate) fn begin_gesture_for_test(
        &mut self,
        screen: Pos2,
        world: Pos2,
        mods: egui::Modifiers,
    ) -> Option<BoardDrag> {
        self.begin_gesture(screen, world, mods)
    }

    #[cfg(test)]
    pub(crate) fn end_gesture_for_test(
        &mut self,
        world: Pos2,
        pointer: Option<Pos2>,
        mods: egui::Modifiers,
    ) {
        self.end_gesture(world, pointer, mods)
    }
}
