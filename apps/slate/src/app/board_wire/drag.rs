//! Extracted adapter on the existing owner.
use super::*;

impl SlateApp {
    /// Snap target for a live wire drag: topmost visible non-connector node
    /// (excluding `exclude`) whose grip or edge is within 14 px screen.
    pub(crate) fn wire_snap_target(
        &self,
        world: Pos2,
        from: Option<(NodeId, Side, f32)>,
    ) -> Option<(NodeId, Side, f32)> {
        let z = self.tab().cam.z.max(0.05);
        let snap_w = WIRE_SNAP_PX / z;
        if let Some(from) = from.filter(|f| self.is_crosstalk_port(*f)) {
            return self.crosstalk_snap(world, from.0, snap_w);
        }
        let exclude = from.map(|f| f.0);
        let doc = self.doc();
        let scene = &doc.scene;
        for n in scene.nodes.iter().rev() {
            if n.hidden || matches!(n.kind, NodeKind::Connector(_)) || Some(n.id) == exclude {
                continue;
            }
            if self.portal_capture_owns_viewport(n.id) {
                if n.rect.contains_rotated(world.x, world.y, n.rotation_deg) {
                    return None;
                }
                continue;
            }
            let host = self.wire_input_host(n);
            if !host.offers_ports() {
                continue;
            }
            if let Some(port) = host
                .ports()
                .into_iter()
                .find(|port| Pos2::new(port.point[0], port.point[1]).distance(world) <= snap_w)
            {
                return Some((n.id, port.side, port.t));
            }
            // Dropped anywhere on a generator or text block, a wire lands on
            // the port that reads it; drawn back from an input, on the output.
            if host.is_flow()
                && (n.rect.contains_rotated(world.x, world.y, n.rotation_deg)
                    || host.snap([world.x, world.y]).dist <= snap_w)
            {
                let Some((source, side, _)) = from else {
                    continue;
                };
                if side == Side::Left && slate_doc::agent_inputs::is_flow_node(scene, source) {
                    return Some((n.id, Side::Right, slate_doc::agent_inputs::OUTPUT_T));
                }
                if let Some(port) =
                    slate_doc::agent_inputs::default_port(scene, n.id, source, &|id| {
                        doc.item(id).map(|item| item.path.as_path())
                    })
                {
                    return Some((n.id, Side::Left, port.t));
                }
                continue;
            }
            let snap = host.snap([world.x, world.y]);
            if snap.dist <= snap_w {
                return Some((n.id, snap.side, snap.t));
            }
        }
        None
    }

    /// Live wire drag: track the (ortho-adjusted) cursor, resolve the snap
    /// target, and let detached ends follow (live scene mutation; the net
    /// effect journals on release, per the board's gesture convention).
    pub(crate) fn wire_drag_update(&mut self, wd: &mut WireDrag, world: Pos2, shift: bool) {
        let mut cursor = world;
        // Ortho on wire drags uses the F8 toggle only: Shift already means
        // "add" in the wire grammar (deliberate deviation, documented).
        if self.board_ortho {
            let origin = match &wd.mode {
                WireMode::Add { from } => self
                    .doc()
                    .scene
                    .node(from.0)
                    .map(|n| port_point(n, from.1, from.2)),
                _ => None,
            };
            if let Some(o) = origin {
                cursor = crate::app::board_snap::ortho_snap_point(o, world);
                self.ortho_feedback = Some((o, crate::app::board_snap::ortho_axis(world - o)));
            }
        }
        let _ = shift;
        wd.cursor = cursor;
        let from = match &wd.mode {
            WireMode::Add { from } => Some(*from),
            _ => None,
        };
        let exclude = from.map(|f| f.0);
        wd.snap = self.wire_snap_target(cursor, from);
        if wd.snap.is_none() {
            let from = match &wd.mode {
                WireMode::Add { from } => self
                    .doc()
                    .scene
                    .node(from.0)
                    .map(|n| port_point(n, from.1, from.2)),
                _ => None,
            };
            let skip: Vec<NodeId> = exclude.into_iter().collect();
            cursor = self.resolve_point_snap(cursor, &skip, from, false, from.is_some());
            wd.cursor = cursor;
        }

        // Detached ends follow the cursor (or preview-anchor onto the snap).
        let live_end = |snap: Option<(NodeId, Side, f32)>, cursor: Pos2| match snap {
            Some((node, side, t)) => ConnectorEnd::Anchored { node, side, t },
            None => ConnectorEnd::Free {
                point: [cursor.x, cursor.y],
            },
        };
        match &wd.mode {
            WireMode::Detach { conn, end_b, .. } => {
                let (conn, end_b) = (*conn, *end_b);
                let end = live_end(wd.snap, cursor);
                if let Some(NodeKind::Connector(c)) =
                    self.doc_mut().scene.node_mut(conn).map(|n| &mut n.kind)
                {
                    if end_b {
                        c.b = end;
                    } else {
                        c.a = end;
                    }
                }
            }
            WireMode::MoveAll { items } => {
                let items: Vec<(NodeId, bool)> = items.iter().map(|i| (i.0, i.1)).collect();
                let end = live_end(wd.snap, cursor);
                for (id, end_b) in items {
                    if let Some(NodeKind::Connector(c)) =
                        self.doc_mut().scene.node_mut(id).map(|n| &mut n.kind)
                    {
                        if end_b {
                            c.b = end;
                        } else {
                            c.a = end;
                        }
                    }
                }
            }
            WireMode::Add { .. } => {}
        }
    }

    /// Release: journal the net effect (Add / Patch / Patch group).
    /// An add that misses every node keeps a free end at the release
    /// point, drawn the same way a detached end is.
    pub(crate) fn finish_wire_drag(&mut self, wd: WireDrag) {
        match wd.mode {
            // Released anywhere but a partner, a crosstalk drag journals nothing.
            WireMode::Add { from } if self.is_crosstalk_port(from) => {
                if let Some((to, _, _)) = wd.snap {
                    self.crosstalk_link(from.0, to);
                }
            }
            WireMode::Add { from } => match wd.snap {
                Some((node, side, t)) => {
                    self.add_connector(
                        ConnectorEnd::Anchored {
                            node: from.0,
                            side: from.1,
                            t: from.2,
                        },
                        ConnectorEnd::Anchored { node, side, t },
                    );
                }
                None => {
                    // Agent media offers Text / Image / Agent here.
                    if self.flow_wire_released(from, wd.cursor) {
                        return;
                    }
                    // Releasing back on the source node cancels quietly.
                    let on_source = self
                        .doc()
                        .scene
                        .node(from.0)
                        .is_some_and(|n| n.rect.contains(wd.cursor.x, wd.cursor.y));
                    if on_source {
                        return;
                    }
                    self.add_connector(
                        ConnectorEnd::Anchored {
                            node: from.0,
                            side: from.1,
                            t: from.2,
                        },
                        ConnectorEnd::Free {
                            point: [wd.cursor.x, wd.cursor.y],
                        },
                    );
                }
            },
            WireMode::Detach { conn, before, .. } => {
                if let Some(mut after) = self.doc().scene.node(conn).cloned() {
                    if let (NodeKind::Connector(old), NodeKind::Connector(new)) =
                        (&before.kind, &mut after.kind)
                    {
                        new.binding = self.rebind_wire(old, new);
                    }
                    if after != before {
                        let promotion = self.image_wire_target_command(&after);
                        let mut commands = vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }];
                        commands.extend(promotion);
                        self.commit_scene(commands);
                        self.push_history(
                            atlas_commands::CommandId("board.wire.detach"),
                            Some(
                                if wd.snap.is_some() {
                                    "rewired"
                                } else {
                                    "freed"
                                }
                                .into(),
                            ),
                        );
                    }
                }
            }
            WireMode::MoveAll { items } => {
                if wd.snap.is_none() {
                    // Release on empty cancels: restore every end.
                    self.restore_wire_nodes(&items);
                    return;
                }
                let mut cmds: Vec<SceneCmd> = items
                    .iter()
                    .filter_map(|(id, _, before)| {
                        let mut after = self.doc().scene.node(*id)?.clone();
                        if let (NodeKind::Connector(old), NodeKind::Connector(new)) =
                            (&before.kind, &mut after.kind)
                        {
                            new.binding = self.rebind_wire(old, new);
                        }
                        (after != *before).then(|| SceneCmd::Patch {
                            before: Box::new(before.clone()),
                            after: Box::new(after),
                        })
                    })
                    .collect();
                if !cmds.is_empty() {
                    let n = cmds.len();
                    let promotions: std::collections::BTreeMap<_, _> = cmds
                        .iter()
                        .filter_map(|cmd| match cmd {
                            SceneCmd::Patch { after, .. } => self.image_wire_target_command(after),
                            _ => None,
                        })
                        .map(|cmd| match &cmd {
                            SceneCmd::Patch { after, .. } => (after.id, cmd),
                            _ => unreachable!(),
                        })
                        .collect();
                    cmds.extend(promotions.into_values());
                    self.commit_scene(cmds);
                    self.push_history(
                        atlas_commands::CommandId("board.wire.move_all"),
                        Some(format!("{n} wire(s)")),
                    );
                }
            }
        }
    }

    /// Esc during a wire drag: restore live-mutated ends, journal nothing.
    pub(crate) fn cancel_wire_drag(&mut self, wd: WireDrag) {
        match wd.mode {
            WireMode::Add { .. } => {}
            WireMode::Detach { before, conn, .. } => {
                self.restore_wire_nodes(&[(conn, false, before)]);
            }
            WireMode::MoveAll { items } => self.restore_wire_nodes(&items),
        }
    }

    fn restore_wire_nodes(&mut self, items: &[(NodeId, bool, Node)]) {
        let tab = self.tab_mut();
        for (id, _, before) in items {
            if let Some(n) = tab.doc.scene.node_mut(*id) {
                *n = before.clone();
            }
        }
    }
}
