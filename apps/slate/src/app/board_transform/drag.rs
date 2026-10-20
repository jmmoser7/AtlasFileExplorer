//! Transform press routing and rotation gesture arithmetic.

use super::super::{board::MIN_DRAW, board_snap};
use super::*;

impl SlateApp {
    /// Preserve minimum member dimensions while resizing the group box.
    pub(crate) fn constrain_group_scale(
        before: &[Node],
        mut sx: f32,
        mut sy: f32,
        aspect: bool,
    ) -> (f32, f32) {
        let minimum = super::super::board::MIN_DRAW;
        for node in before
            .iter()
            .filter(|n| !matches!(n.kind, NodeKind::Connector(_)))
        {
            if node.rect.w >= minimum {
                sx = sx.max(minimum / node.rect.w);
            }
            if node.rect.h >= minimum {
                sy = sy.max(minimum / node.rect.h);
            }
        }
        if aspect {
            sx = sx.max(sy);
            sy = sx;
        }
        (sx, sy)
    }

    pub(crate) fn rotation_drag_value(
        before: &Node,
        start_angle: f32,
        vertex: u8,
        world: Pos2,
        mods: egui::Modifiers,
    ) -> f32 {
        let (cx, cy) = before.rect.center();
        let angle = (world.y - cy).atan2(world.x - cx);
        let mut rotation = before.rotation_deg + (angle - start_angle).to_degrees();
        if !mods.alt {
            let cardinal = match &before.kind {
                NodeKind::Shape(s) if mods.shift && s.shape == ShapeKind::RegularPolygon => {
                    scene::regular_polygon_vertex(
                        before.rect,
                        s.sides,
                        s.phase_deg,
                        vertex as usize,
                    )
                    .map(|p| {
                        let bearing = (p[1] - cy).atan2(p[0] - cx).to_degrees();
                        ((rotation + bearing) / 90.0).round() * 90.0 - bearing
                    })
                }
                _ => None,
            };
            rotation = cardinal.unwrap_or_else(|| {
                super::super::board_snap::snap_rotation_deg(
                    rotation,
                    super::super::board_snap::ROTATION_SNAP_DEG,
                )
            });
        }
        while rotation > 180.0 {
            rotation -= 360.0;
        }
        while rotation < -180.0 {
            rotation += 360.0;
        }
        rotation
    }
    /// A wire-grip press is not a resize, even though it sits on the edge.
    pub(crate) fn begin_transform_drag(&mut self, screen: Pos2, world: Pos2) -> Option<BoardDrag> {
        let xf = self.board_xf();
        if self.pointer_on_portal_maximize(screen, &xf) {
            return None;
        }
        if self.wire_grip_at(screen, &xf).is_some() {
            return None;
        }
        let Some((node, hit)) = self.transform_hit_at(screen) else {
            return self.move_selection_in_bounds(world);
        };
        match (node, hit) {
            (None, board_handles::BoardHitTarget::Resize(h)) => {
                let gb = self.board_group_bounds()?;
                let geom = board_handles::selection_geom(&xf, gb, 0.0);
                let grab_offset = Self::control_grab_offset(&geom, h, screen, &xf);
                let before: Vec<Node> = self
                    .board_sel
                    .iter()
                    .filter_map(|i| self.doc().scene.node(*i).cloned())
                    .collect();
                if self.alt_scale_copies() {
                    let (ids, before) = self.stage_unjournaled_duplicates(&before);
                    let gb = self.board_group_bounds().unwrap_or(gb);
                    return Some(BoardDrag::GroupResize {
                        ids,
                        before,
                        group_before: gb,
                        handle: h as u8,
                        grab_offset,
                        dup: true,
                    });
                }
                let ids: Vec<NodeId> = before.iter().map(|n| n.id).collect();
                Some(BoardDrag::GroupResize {
                    ids,
                    before,
                    group_before: gb,
                    handle: h as u8,
                    grab_offset,
                    dup: false,
                })
            }
            (None, board_handles::BoardHitTarget::Rotate(_)) => {
                let gb = self.board_group_bounds()?;
                let before: Vec<Node> = self
                    .board_sel
                    .iter()
                    .filter_map(|i| self.doc().scene.node(*i).cloned())
                    .collect();
                let ids: Vec<NodeId> = before.iter().map(|n| n.id).collect();
                let (cx, cy) = gb.center();
                let start_angle = (world.y - cy).atan2(world.x - cx);
                Some(BoardDrag::GroupRotate {
                    ids,
                    before,
                    center: (cx, cy),
                    start_angle,
                })
            }
            (Some(id), board_handles::BoardHitTarget::Resize(h)) => {
                // Shift/Ctrl on an unselected node is add-to-selection, not
                // a hover-resize steal (P1.node.select). Shift on a selected
                // node still means free-aspect resize.
                if (self.shift_down || self.ctrl_down) && !self.board_sel.contains(&id) {
                    return None;
                }
                let n = self.doc().scene.node(id).cloned()?;
                let geom = self.node_interaction_geom(&n, &xf);
                let grab_offset = Self::control_grab_offset(&geom, h, screen, &xf);
                if !self.board_sel.contains(&id) {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                if self.alt_scale_copies() {
                    let (ids, before) = self.stage_unjournaled_duplicates(std::slice::from_ref(&n));
                    let (id, before) = (ids.into_iter().next()?, before.into_iter().next()?);
                    return Some(BoardDrag::Resize {
                        id,
                        before,
                        handle: h as u8,
                        grab_offset,
                        dup: true,
                    });
                }
                Some(BoardDrag::Resize {
                    id,
                    before: n,
                    handle: h as u8,
                    grab_offset,
                    dup: false,
                })
            }
            (Some(id), board_handles::BoardHitTarget::Rotate(vertex)) => {
                let n = self.doc().scene.node(id).cloned()?;
                if !Self::node_allows_rotation(&n) {
                    return None;
                }
                if (self.shift_down || self.ctrl_down) && !self.board_sel.contains(&id) {
                    return None;
                }
                if !self.board_sel.contains(&id) {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                let (cx, cy) = n.rect.center();
                let start_angle = (world.y - cy).atan2(world.x - cx);
                Some(BoardDrag::Rotate {
                    id,
                    before: n,
                    start_angle,
                    vertex,
                })
            }
            (Some(id), board_handles::BoardHitTarget::Body) => {
                let node = self.doc().scene.node(id)?;
                if node
                    .rect
                    .contains_rotated(world.x, world.y, node.rotation_deg)
                {
                    return None;
                }
                self.begin_select_body_drag(
                    id,
                    world,
                    egui::Modifiers {
                        alt: self.alt_down,
                        ctrl: self.ctrl_down,
                        shift: self.shift_down,
                        ..Default::default()
                    },
                )
            }
            (None, board_handles::BoardHitTarget::Body) => None,
            (_, board_handles::BoardHitTarget::FilletRadius) => None,
        }
    }

    /// Adapt captured selection-control motion to authored resize/snap geometry.
    pub(crate) fn update_node_resize(&mut self, world: Pos2, mods: egui::Modifiers) {
        let Some(BoardDrag::Resize {
            id,
            before,
            handle,
            grab_offset,
            ..
        }) = &self.board_drag
        else {
            return;
        };
        let world = world + *grab_offset;
        let node_id = *id;
        let handle = *handle;
        let before_rect = before.rect;
        let rotation_deg = before.rotation_deg;
        // A picture dragged past its opposite edge mirrors instead of
        // clamping (P1.node.transform); other kinds keep the clamp.
        let mirrors = matches!(before.kind, NodeKind::Image(_))
            && slate_doc::mirror::node_mirrors(self.doc(), before);
        // Corner drags scale proportionally by default; Shift frees
        // the aspect (distortion). Edge drags are single-axis, with
        // Shift locking the aspect instead.
        let is_corner = matches!(handle, 0 | 2 | 4 | 6);
        let lock_aspect = if is_corner { !mods.shift } else { mods.shift };
        let from_center = mods.ctrl;
        let resize = |pointer: Pos2| {
            if mirrors {
                board_snap::resize_from_handle_mirroring(
                    before_rect,
                    pointer,
                    handle,
                    MIN_DRAW,
                    lock_aspect,
                    from_center,
                    rotation_deg,
                )
            } else {
                let r = board_snap::resize_from_handle(
                    before_rect,
                    pointer,
                    handle,
                    MIN_DRAW,
                    lock_aspect,
                    from_center,
                    rotation_deg,
                );
                (r, [false, false])
            }
        };
        let (mut r, mut crossed) = resize(world);

        if !mods.alt {
            if is_corner {
                let rotated = rotation_deg.abs() > f32::EPSILON;
                if !rotated && crossed == [false, false] {
                    let pointer = self.resolve_corner_scale(
                        world,
                        before_rect,
                        r,
                        handle,
                        MIN_DRAW,
                        lock_aspect,
                        from_center,
                        &[node_id],
                    );
                    (r, crossed) = resize(pointer);
                } else {
                    let pointer = self.resolve_point_snap(world, &[node_id], None, false, false);
                    (r, crossed) = resize(pointer);
                }
            } else if self.board_smart_guides {
                let all = self.board_node_rects();
                // Past the far edge, the moving edge is the opposite one.
                let moving = if crossed[0] || crossed[1] {
                    (handle + 4) % 8
                } else {
                    handle
                };
                let edges = board_snap::ResizeSnapEdges::for_handle(moving);
                let (snapped, guides) = board_snap::snap_resize_rect_scoped(
                    r,
                    &[node_id],
                    &all,
                    self.snap_scope(),
                    edges,
                );
                r = snapped;
                self.board_snap_guides = guides;
            }
        }

        if mirrors {
            self.sync_resize_mirror(node_id, crossed);
        }
        if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
            n.rect = r;
        }
    }

    fn control_grab_offset(
        geom: &board_handles::SelectionGeom,
        handle: board_handles::ResizeHandle,
        screen: Pos2,
        xf: &BoardXf,
    ) -> Vec2 {
        if geom.corners == geom.control_corners {
            return Vec2::ZERO;
        }
        let i = handle as usize;
        let anchor = if i.is_multiple_of(2) {
            geom.corners[i / 2]
        } else {
            geom.edges[i / 2]
        };
        xf.s2w(anchor) - xf.s2w(screen)
    }

    /// Gap inside a multi-selection's box: nothing was picked, so the press
    /// moves the group instead of starting a marquee.
    fn move_selection_in_bounds(&self, world: Pos2) -> Option<BoardDrag> {
        if !self.selection_bounds_claim_press(world) {
            return None;
        }
        if self.board_pick_node(world.x, world.y).is_some() {
            return None;
        }
        let selected: Vec<NodeId> = self.board_sel.iter().copied().collect();
        let expanded = self.expand_with_members(&selected);
        let before: Vec<Node> = expanded
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        if before.is_empty() {
            return None;
        }
        let ids = before.iter().map(|n| n.id).collect();
        Some(BoardDrag::Move {
            ids,
            before,
            start_world: world,
            dup: self.alt_down,
        })
    }
}
