//! Live board gesture updates. Press routing remains in the parent module.

use super::super::*;

impl SlateApp {
    pub(in crate::app::board) fn update_gesture(&mut self, world: Pos2, mods: egui::Modifiers) {
        let deck_zoom = self.tab().cam.z;
        if let Some(BoardDrag::DeckStroke { points, .. }) = &mut self.board_drag {
            let far = points
                .last()
                .is_none_or(|p| (*p - world).length() * deck_zoom >= 0.5);
            if far {
                points.push(world);
            }
            return;
        }
        if matches!(
            self.board_drag,
            Some(BoardDrag::FreehandPen { .. } | BoardDrag::FreehandBrush { .. })
        ) {
            // A tip chord's scrub is not ink, nor is the frame it closes on.
            if self.brush_hud.is_some() || std::mem::take(&mut self.freehand_resume) {
                return;
            }
            let zoom = self.tabs[self.active_tab].cam.z;
            let far = |last: Pos2| {
                (world - last).length() * zoom >= board_path::FREEHAND_SAMPLE_SPACING_PX
            };
            let pen_tip = self.placed_tip(slate_doc::StrokeTool::Pen);
            let brush_tip = self.tip_now().span();
            match &mut self.board_drag {
                Some(BoardDrag::FreehandPen { stroke }) if far(stroke.last()) => {
                    stroke.push(world, pen_tip, zoom);
                }
                Some(BoardDrag::FreehandBrush { stroke }) if far(stroke.last()) => {
                    stroke.push(world, brush_tip, zoom);
                }
                _ => {}
            }
            return;
        }
        if let Some(BoardDrag::BezierAnchor { press }) = &self.board_drag {
            let mut w = world;
            if board_snap::effective_ortho(self.board_ortho, mods.shift) {
                w = board_snap::ortho_snap_point(*press, world);
            }
            self.bezier_anchor_move(*press, w, mods.alt);
            return;
        }
        if let Some(BoardDrag::BezierEdit {
            hit,
            start,
            anchors0,
        }) = &self.board_drag
        {
            let (hit, start, anchors0) = (*hit, *start, anchors0.clone());
            self.bezier_draft_edit(hit, start, &anchors0, world, mods);
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::LineDraw { .. })) {
            self.line_hover(world, mods.shift);
            return;
        }
        if let Some(BoardDrag::Draw {
            start_world, tool, ..
        }) = &self.board_drag
        {
            let start = *start_world;
            let tool = *tool;
            let _ = self.resolve_draw_rect(
                start,
                world,
                tool,
                mods.shift,
                board_place::draws_from_center(tool, mods.ctrl),
            );
            return;
        }
        if let Some(BoardDrag::LineGrip { id, end, .. }) = &self.board_drag {
            let (id, end) = (*id, *end);
            self.line_grip_update(id, end, world, mods.shift);
            return;
        }
        // Eraser scrub: accumulate strokes under the circle (removed on
        // release; Esc cancels with no journal).
        if matches!(self.board_drag, Some(BoardDrag::Erase { .. })) {
            self.update_erase(world);
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::Smooth { .. })) {
            self.update_smooth(world);
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::Wire(_))) {
            let Some(BoardDrag::Wire(mut wd)) = self.board_drag.take() else {
                unreachable!();
            };
            self.wire_drag_update(&mut wd, world, mods.shift);
            self.board_drag = Some(BoardDrag::Wire(wd));
            return;
        }
        if matches!(self.board_drag, Some(BoardDrag::Direct(_))) {
            self.update_direct_drag(world, mods);
            return;
        }
        match &self.board_drag {
            Some(BoardDrag::Move {
                ids,
                before,
                start_world,
                dup,
                ..
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let start = *start_world;
                let dup = *dup;
                let mut d = world - start;
                // Ortho (F8, Shift inverts): the drag vector snaps to 45°
                // steps from the gesture origin.
                let ortho = board_snap::effective_ortho(self.board_ortho, mods.shift);
                let ortho_axis = board_snap::ortho_axis(d);
                if ortho {
                    d = board_snap::ortho_snap_vec(d);
                    self.ortho_feedback = Some((start, ortho_axis));
                }
                let mut pairs: Vec<(NodeId, WorldRect)> = ids
                    .iter()
                    .zip(before.iter())
                    .map(|(id, b)| (*id, b.rect.translated(d.x, d.y)))
                    .collect();

                // Object snaps beat smart guides; both yield to Alt (Rhino).
                let snap_off = mods.alt || dup;
                let mut osnap_moved = false;
                if !snap_off {
                    let rects: Vec<WorldRect> = pairs.iter().map(|(_, r)| *r).collect();
                    if let Some(union) = board_snap::union_rect(&rects) {
                        let set = self.board_osnap;
                        let radius = self.osnap_radius_world();
                        if let Some((snapped, hit)) = board_osnap::snap_bbox_osnap(
                            union,
                            &self.doc().scene,
                            &ids,
                            set,
                            radius,
                        ) {
                            let mut ax = snapped.x - union.x;
                            let mut ay = snapped.y - union.y;
                            if ortho {
                                let along = ax * ortho_axis.x + ay * ortho_axis.y;
                                ax = ortho_axis.x * along;
                                ay = ortho_axis.y * along;
                            }
                            if ax != 0.0 || ay != 0.0 {
                                for (_, r) in pairs.iter_mut() {
                                    r.x += ax;
                                    r.y += ay;
                                }
                            }
                            self.board_osnap_hit = Some(hit);
                            osnap_moved = true;
                        } else if self.board_smart_guides {
                            let all = self.board_node_rects();
                            let (snapped, guides) =
                                board_snap::snap_bbox_scoped(union, &ids, &all, self.snap_scope());
                            let mut ax = snapped.x - union.x;
                            let mut ay = snapped.y - union.y;
                            if ortho {
                                let along = ax * ortho_axis.x + ay * ortho_axis.y;
                                ax = ortho_axis.x * along;
                                ay = ortho_axis.y * along;
                            }
                            if ax != 0.0 || ay != 0.0 {
                                for (_, r) in pairs.iter_mut() {
                                    r.x += ax;
                                    r.y += ay;
                                }
                            }
                            self.board_snap_guides = guides;
                        }
                    }
                }
                // Grid snap would pull the origin off the constrained axis,
                // so ortho suspends it. Object snap overrides grid.
                if self.board_snap_grid && !snap_off && !ortho && !osnap_moved {
                    for (_, r) in pairs.iter_mut() {
                        *r = board_snap::snap_rect_origin(*r, true);
                    }
                }

                if !snap_off
                    && !ortho
                    && before
                        .iter()
                        .all(|n| slate_doc::agent_chat::agent(n).is_some())
                {
                    if let Some((_, first)) = pairs.first() {
                        let snapped = board_snap::agent_datum(
                            *first,
                            &ids,
                            &self.doc().scene,
                            self.board_xf().z,
                        );
                        let delta = Vec2::new(snapped.x - first.x, snapped.y - first.y);
                        for (_, rect) in &mut pairs {
                            rect.x += delta.x;
                            rect.y += delta.y;
                        }
                    }
                }
                self.bumper_drag(&ids, &before, &mut pairs, mods.alt);
                let scene = &mut self.doc_mut().scene;
                for ((id, r), b) in pairs.into_iter().zip(before.iter()) {
                    if let Some(n) = scene.node_mut(id) {
                        // Free connector endpoints travel with the drag
                        // (anchored ends stay glued — geometry is derived).
                        if let (NodeKind::Connector(c), NodeKind::Connector(cb)) =
                            (&mut n.kind, &b.kind)
                        {
                            let dd = Vec2::new(r.x - b.rect.x, r.y - b.rect.y);
                            for (end, end_b) in [(&mut c.a, &cb.a), (&mut c.b, &cb.b)] {
                                if let slate_doc::scene::ConnectorEnd::Free { point } = end_b {
                                    *end = slate_doc::scene::ConnectorEnd::Free {
                                        point: [point[0] + dd.x, point[1] + dd.y],
                                    };
                                }
                            }
                        }
                        n.rect = r;
                    }
                }
                self.update_image_drop_offer(world, &ids);
            }
            Some(BoardDrag::ModelOrbit {
                id, last_screen, ..
            }) => {
                let id = *id;
                let last = *last_screen;
                let xf = self.board_xf();
                let screen = xf.w2s(world);
                let delta = screen - last;
                if delta != Vec2::ZERO {
                    self.drag_live_model(id, delta, xf.z);
                    if let Some(BoardDrag::ModelOrbit { last_screen, .. }) = &mut self.board_drag {
                        *last_screen = screen;
                    }
                }
            }
            Some(BoardDrag::ModelMeasure { .. }) | Some(BoardDrag::DeckStroke { .. }) => {}
            Some(BoardDrag::Resize { .. }) => self.update_node_resize(world, mods),
            Some(BoardDrag::CropEdge {
                id,
                before,
                handle,
                peers,
            }) => {
                let node_id = *id;
                let handle = *handle;
                let peers = peers.clone();
                let NodeKind::Image(image) = &before.kind else {
                    return;
                };
                let before_crop = image.crop;
                let Some(after) = board_crop::gesture_geometry(
                    before,
                    board_crop::CropGesture::Handle {
                        handle,
                        world: (world.x, world.y),
                    },
                ) else {
                    return;
                };
                let crop = after.crop;
                if let Some(node) = self.doc_mut().scene.node_mut(node_id) {
                    after.apply_to(node);
                }
                for peer in peers {
                    if let Some(after) = board_crop::peer_crop_geometry(&peer, before_crop, crop) {
                        if let Some(node) = self.doc_mut().scene.node_mut(peer.id) {
                            after.apply_to(node);
                        }
                    }
                }
            }
            Some(BoardDrag::CropPan {
                id,
                before,
                start_world,
            }) => {
                let node_id = *id;
                let delta = world - *start_world;
                if let Some(after) = board_crop::gesture_geometry(
                    before,
                    board_crop::CropGesture::Pan {
                        delta_world: (delta.x, delta.y),
                    },
                ) {
                    if let Some(node) = self.doc_mut().scene.node_mut(node_id) {
                        after.apply_to(node);
                    }
                }
            }
            Some(BoardDrag::Rotate {
                id,
                before,
                start_angle,
                vertex,
            }) => {
                let node_id = *id;
                let rot = Self::rotation_drag_value(before, *start_angle, *vertex, world, mods);
                if let Some(n) = self.doc_mut().scene.node_mut(node_id) {
                    n.rotation_deg = rot;
                }
            }
            Some(BoardDrag::GroupResize {
                ids,
                before,
                group_before,
                handle,
                grab_offset,
                ..
            }) => {
                let world = world + *grab_offset;
                let ids = ids.clone();
                let before = before.clone();
                let gb = *group_before;
                let minimum = MIN_DRAW.min(gb.w).min(gb.h).max(f32::EPSILON);
                let handle = *handle;
                // Same convention as single-node resize: corners scale
                // proportionally by default, Shift distorts. Ctrl+Alt+Shift
                // is a dedicated group mode: the box still follows the
                // handle, but members keep their size and only translate.
                let is_corner = matches!(handle, 0 | 2 | 4 | 6);
                let reposition = mods.ctrl && mods.alt && mods.shift;
                let lock_aspect = if reposition {
                    is_corner
                } else if is_corner {
                    !mods.shift
                } else {
                    mods.shift
                };
                let from_center = !reposition && mods.ctrl;
                let mut pointer = world;
                if !mods.alt && is_corner {
                    let proposed = board_snap::resize_from_handle(
                        gb,
                        world,
                        handle,
                        minimum,
                        lock_aspect,
                        from_center,
                        0.0,
                    );
                    pointer = self.resolve_corner_scale(
                        world,
                        gb,
                        proposed,
                        handle,
                        minimum,
                        lock_aspect,
                        from_center,
                        &ids,
                    );
                }
                let mut new_group = board_snap::resize_from_handle(
                    gb,
                    pointer,
                    handle,
                    minimum,
                    lock_aspect,
                    from_center,
                    0.0,
                );
                if !mods.alt && !is_corner && self.board_smart_guides {
                    let all = self.board_node_rects();
                    let edges = board_snap::ResizeSnapEdges::for_handle(handle);
                    let (snapped, guides) = board_snap::snap_resize_rect_scoped(
                        new_group,
                        &ids,
                        &all,
                        self.snap_scope(),
                        edges,
                    );
                    new_group = snapped;
                    self.board_snap_guides = guides;
                }
                let sx = new_group.w / gb.w.max(0.001);
                let sy = new_group.h / gb.h.max(0.001);
                let (sx, sy) = if reposition {
                    (sx, sy)
                } else {
                    Self::constrain_group_scale(&before, sx, sy, lock_aspect)
                };
                let mean = (sx + sy) * 0.5;
                let rects: Vec<WorldRect> = before.iter().map(|n| n.rect).collect();
                let scaled = board_snap::apply_group_box_scale(
                    &rects,
                    gb,
                    sx,
                    sy,
                    handle,
                    from_center,
                    reposition,
                );
                let scene = &mut self.doc_mut().scene;
                for ((id, b), r) in ids.iter().zip(before.iter()).zip(scaled.iter()) {
                    if let Some(n) = scene.node_mut(*id) {
                        n.rect = *r;
                        if !reposition {
                            // Text scales with the group; stroke widths stay
                            // fixed (CSS keeps stroke width on resize).
                            if let (NodeKind::Text(t), NodeKind::Text(tb)) = (&mut n.kind, &b.kind)
                            {
                                t.size = (tb.size * mean).max(4.0);
                            }
                        }
                    }
                }
            }
            Some(BoardDrag::GroupRotate {
                ids,
                before,
                center,
                start_angle,
            }) => {
                let ids = ids.clone();
                let before = before.clone();
                let (cx, cy) = *center;
                let start = *start_angle;
                let angle = (world.y - cy).atan2(world.x - cx);
                let mut delta = (angle - start).to_degrees();
                if !mods.alt {
                    delta = board_snap::snap_rotation_deg(delta, board_snap::ROTATION_SNAP_DEG);
                }
                let scene = &mut self.doc_mut().scene;
                for (id, b) in ids.iter().zip(before.iter()) {
                    if let Some(n) = scene.node_mut(*id) {
                        if Self::node_allows_rotation(b) {
                            let mut rot = b.rotation_deg + delta;
                            while rot > 180.0 {
                                rot -= 360.0;
                            }
                            while rot < -180.0 {
                                rot += 360.0;
                            }
                            n.rotation_deg = rot;
                        }
                        // Orbit the rect center around the group center;
                        // width/height are unchanged by rotation. Portals
                        // stay axis-aligned while they follow the group.
                        let (bx, by) = b.rect.center();
                        let (nx, ny) = board_snap::orbit_point((cx, cy), (bx, by), delta);
                        n.rect.x = nx - b.rect.w * 0.5;
                        n.rect.y = ny - b.rect.h * 0.5;
                    }
                }
            }
            Some(BoardDrag::FilletRadius { .. }) => self.update_fillet_drag(world, mods.shift),
            _ => {}
        }
    }
}
