//! Gesture release and commit.

use super::*;

impl SlateApp {
    pub(super) fn end_gesture(
        &mut self,
        world: Pos2,
        pointer: Option<Pos2>,
        mods: egui::Modifiers,
    ) {
        // Any gesture may have journaled; one generation bump per gesture
        // end keeps the minimap/search caches fresh without per-frame cost.
        self.note_scene_change();
        let sources_sel = self.staged_dup_sel.take();
        let drag = self.board_drag.take();
        // A saved-view picture released over a model commits a camera patch
        // instead of the move.
        let view_drop = match (&drag, pointer) {
            (Some(BoardDrag::Move { ids, .. }), Some(p)) if !self.bumper.dragging() => {
                self.node_view_drop_target(ids, self.board_xf().s2w(p))
            }
            _ => None,
        };
        match drag {
            Some(BoardDrag::Move {
                ids, before, dup, ..
            }) if self.bumper.dragging() => {
                self.bumper_release(&ids, &before, dup);
            }
            Some(BoardDrag::Move { before, dup, .. }) if view_drop.is_some() => {
                self.restore_press_nodes(before, dup, sources_sel);
                if let Some(target) = view_drop {
                    self.commit_node_view_drop(target);
                }
            }
            Some(BoardDrag::Move { before, dup, .. }) if self.image_drop_armed() => {
                // Read the picture while a staged copy still holds it.
                let item = self.image_drop_item();
                self.restore_press_nodes(before, dup, sources_sel);
                // An Alt copy drops its picture and keeps the original.
                self.commit_image_drop(item, dup);
            }
            Some(BoardDrag::Move {
                ids, before, dup, ..
            }) => {
                self.image_drop = None;
                // Whole-node compare: a connector move also translates its
                // Free endpoints (kind change), not just the rect.
                let moved = ids
                    .iter()
                    .zip(before.iter())
                    .any(|(id, b)| self.doc().scene.node(*id) != Some(b));
                if dup {
                    self.journal_alt_copies(&ids, format!("{} node(s), Alt-drag", ids.len()));
                } else if moved {
                    let cmds: Vec<SceneCmd> = ids
                        .iter()
                        .zip(before)
                        .filter_map(|(id, b)| {
                            let after = self.doc().scene.node(*id)?.clone();
                            (after != b).then(|| SceneCmd::Patch {
                                before: Box::new(b),
                                after: Box::new(after),
                            })
                        })
                        .collect();
                    self.tab_mut().journal.record(cmds);
                    self.tab_mut().dirty = true;
                    // Dropping images into a tagged frame assigns its tags.
                    self.inherit_frame_tags_after_move(&ids);
                }
            }
            Some(BoardDrag::Resize {
                id, before, dup, ..
            }) => {
                if dup {
                    self.journal_alt_copies(&[id], "1 node(s), Alt-scale".into());
                } else if let Some(mut after) = self.doc().scene.node(id).cloned() {
                    if after.rect != before.rect || after.rotation_deg != before.rotation_deg {
                        // The size lands in the same undo step as the rect.
                        if after.rect != before.rect
                            && super::super::board_agent::record_agent_resize(&mut after)
                        {
                            if let Some(live) = self.doc_mut().scene.node_mut(id) {
                                *live = after.clone();
                            }
                        }
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                    }
                }
            }
            Some(BoardDrag::FilletRadius {
                id,
                vertex,
                before,
                peers,
                max_px,
                ..
            }) if max_px <= board_place::place_tokens::DRAG_THRESHOLD => {
                for before in std::iter::once(before).chain(peers) {
                    if let Some(n) = self.doc_mut().scene.node_mut(before.id) {
                        *n = before;
                    }
                }
                self.open_corner_entry(id, vertex);
            }
            Some(BoardDrag::FilletRadius { before, peers, .. }) => {
                let cmds: Vec<SceneCmd> = std::iter::once(before)
                    .chain(peers)
                    .filter_map(|before| {
                        let after = self.doc().scene.node(before.id)?.clone();
                        (after != before).then(|| SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        })
                    })
                    .collect();
                if !cmds.is_empty() {
                    self.tab_mut().journal.record(cmds);
                    self.tab_mut().dirty = true;
                }
            }
            // Crop gestures: one Patch for the whole drag — both the rect
            // (window) and the image crop may differ between before/after.
            Some(BoardDrag::CropEdge {
                id, before, peers, ..
            }) => {
                let mut cmds = Vec::new();
                if let Some(after) = self.doc().scene.node(id).cloned() {
                    if after != before {
                        cmds.push(SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        });
                    }
                }
                for peer in peers {
                    if let Some(after) = self.doc().scene.node(peer.id).cloned() {
                        if after != peer {
                            cmds.push(SceneCmd::Patch {
                                before: Box::new(peer),
                                after: Box::new(after),
                            });
                        }
                    }
                }
                if !cmds.is_empty() {
                    let detail = format!("{} image(s)", cmds.len());
                    self.tab_mut().journal.record(cmds);
                    self.tab_mut().dirty = true;
                    // P0.4: a finished crop is what Space / Enter repeat on
                    // the next selection, however crop mode was entered.
                    self.push_history(atlas_commands::CommandId("board.crop"), Some(detail));
                }
            }
            Some(BoardDrag::CropPan { id, before, .. }) => {
                if let Some(after) = self.doc().scene.node(id).cloned() {
                    if after != before {
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                    }
                }
            }
            Some(BoardDrag::Rotate { id, before, .. }) => {
                if let Some(after) = self.doc().scene.node(id).cloned() {
                    if (after.rotation_deg - before.rotation_deg).abs() > f32::EPSILON {
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                    }
                }
            }
            // One Patch group for the whole gesture, like the Move arm.
            Some(BoardDrag::GroupResize {
                ids, before, dup, ..
            }) => {
                if dup {
                    self.journal_alt_copies(&ids, format!("{} node(s), Alt-scale", ids.len()));
                } else {
                    self.journal_resize_patches(&ids, before);
                }
            }
            Some(BoardDrag::GroupRotate { ids, before, .. }) => {
                self.journal_resize_patches(&ids, before);
            }
            Some(BoardDrag::Draw {
                start_world,
                start_screen,
                tool,
            }) => {
                if tool == BoardTool::Text {
                    self.finish_text_box_place_gesture(
                        start_world,
                        world,
                        start_screen,
                        pointer,
                        mods,
                    );
                } else {
                    let rect = self.resolve_draw_rect(
                        start_world,
                        world,
                        tool,
                        mods.shift,
                        board_place::draws_from_center(tool, mods.ctrl),
                    );
                    // D04: cursor travel in *screen* px. World units made a
                    // zoomed-out click look like a drag (and a zoomed-in snap
                    // pull look like ClickPlace).
                    let travel_px = pointer
                        .map(|p| (p - start_screen).length())
                        .unwrap_or_else(|| (world - start_world).length() * self.board_xf().z);
                    if travel_px <= board_place::place_tokens::DRAG_THRESHOLD {
                        self.place_default_at(tool, start_world);
                    } else {
                        self.commit_draw_rect(rect, tool);
                    }
                }
            }
            Some(BoardDrag::FreehandPen { mut stroke }) => {
                stroke.end_at(world, self.tabs[self.active_tab].cam.z);
                self.finish_freehand_pen_stroke(&stroke);
            }
            Some(BoardDrag::FreehandBrush { mut stroke }) => {
                stroke.end_at(world, self.tabs[self.active_tab].cam.z);
                self.finish_freehand_brush_stroke(&stroke);
            }
            Some(BoardDrag::Erase {
                touched,
                points,
                spot,
                straight,
                ..
            }) => {
                self.finish_erase_pass(touched, points, spot, straight);
            }
            Some(drag @ BoardDrag::Smooth { .. }) => {
                self.finish_smooth(drag);
            }
            Some(BoardDrag::Wire(wd)) => {
                self.finish_wire_drag(wd);
            }
            Some(BoardDrag::Direct(d)) => {
                self.finish_direct_drag(d, pointer);
            }
            Some(BoardDrag::BezierAnchor { press }) => {
                self.bezier_anchor_release(press, world, mods.alt);
            }
            Some(BoardDrag::BezierEdit { .. }) => {}
            Some(BoardDrag::LineDraw { started }) => {
                self.line_release(world, started, mods.shift);
            }
            Some(BoardDrag::DeckStroke {
                start_screen,
                points,
            }) => {
                self.finish_deck_stroke(start_screen, points, world, pointer);
            }
            Some(BoardDrag::LineGrip { id, before, .. }) => {
                self.line_grip_record(id, before);
            }
            // Camera poses journal on lock, not per orbit gesture.
            Some(BoardDrag::ModelOrbit { .. }) => {}
            Some(BoardDrag::ModelMeasure { id, .. }) => {
                if let Some(p) = pointer {
                    if let Some(n) = self.doc().scene.node(id) {
                        let xf = self.board_xf();
                        let srect = xf.rect_w2s(n.rect);
                        self.model_measure_pick(id, p, srect);
                    }
                }
            }
            Some(BoardDrag::Marquee {
                start_screen,
                frame,
            }) => {
                if let Some(p) = pointer {
                    let xf = self.board_xf();
                    let r = wr(Rect::from_two_pos(xf.s2w(start_screen), xf.s2w(p)));
                    let hits: Vec<NodeId> = self
                        .doc()
                        .scene
                        .nodes
                        .iter()
                        .filter(|n| !n.is_frame() && !n.hidden && !n.locked)
                        .filter(|n| {
                            frame.is_none_or(|id| self.doc().scene.frame_of(n.id) == Some(id))
                        })
                        .filter(|n| {
                            let mode = board_path::marquee_mode(start_screen.x, p.x);
                            board_path::marquee_selects_node(
                                n,
                                r,
                                self.tab().cam.z,
                                &self.doc().scene,
                                self.board_wire_routing,
                                mode,
                            )
                        })
                        .map(|n| n.id)
                        .collect();
                    if mods.shift || mods.ctrl {
                        self.board_sel.extend(hits);
                    } else {
                        self.board_sel = hits.into_iter().collect();
                    }
                    // A member inside the rect selects its whole group.
                    self.expand_board_selection();
                }
            }
            None => {}
        }
    }
}
