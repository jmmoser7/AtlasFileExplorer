//! Connector painting and transient host geometry, using the shared route owner.
use super::*;

impl SlateApp {
    pub(crate) fn connector_path_visible(
        &self,
        id: NodeId,
        conn: &ConnectorNode,
    ) -> Option<ConnectorPath> {
        slate_doc::wire::connector_route_in_scene_with_hosts(
            &self.doc().scene,
            Some(id),
            &conn.a,
            &conn.b,
            conn.effective_routing(self.board_wire_routing),
            |id| self.visible_wire_host(id),
        )
    }

    pub(crate) fn visible_wire_host(&self, id: NodeId) -> Option<WireHost> {
        self.presence_preview(id)
            .or_else(|| self.doc().scene.node(id))
            .filter(|node| !node.hidden)
            .map(|node| self.wire_host(node))
    }

    // ----- painting -----

    /// Wire-drag preview: rubber-band bezier (solid when snapped), the snap
    /// highlight ring, and the modifier glyph (+ add / − detach).
    pub(crate) fn paint_wire_drag(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        wd: &WireDrag,
        mods: egui::Modifiers,
    ) {
        let palette = self.palette();
        if let WireMode::Add { from } = &wd.mode {
            let a = ConnectorEnd::Anchored {
                node: from.0,
                side: from.1,
                t: from.2,
            };
            let b = match wd.snap {
                Some((node, side, t)) => ConnectorEnd::Anchored { node, side, t },
                None => ConnectorEnd::Free {
                    point: [wd.cursor.x, wd.cursor.y],
                },
            };
            let scene = &self.doc().scene;
            if let Some(path) = slate_doc::wire::connector_route_in_scene_with_hosts(
                scene,
                None,
                &a,
                &b,
                self.board_wire_routing,
                |id| self.visible_wire_host(id),
            ) {
                let crosstalk = self.is_crosstalk_port(*from);
                let mut stroke = connector_drawn_stroke(if crosstalk {
                    slate_doc::crosstalk::wire_stroke()
                } else {
                    self.default_wire_stroke()
                });
                if !crosstalk {
                    stroke.color = super::super::board::to_rgba(palette.wire);
                }
                let path = retreat_off_hosts(
                    path,
                    &a,
                    &b,
                    |id| self.visible_wire_host(id),
                    stroke.width * 0.5,
                );
                let color =
                    rgba32(stroke.color).gamma_multiply(if wd.snap.is_some() { 1.0 } else { 0.55 });
                paint_route_preview(
                    painter,
                    xf,
                    &path,
                    EStroke::new(atlas_shell::canvas_scale::px(stroke.width, xf.z), color),
                );
            }
        }
        // Snap highlight.
        if let Some((node, side, t)) = wd.snap {
            if let Some(n) = self
                .presence_preview(node)
                .or_else(|| self.doc().scene.node(node))
            {
                let p = connector_anchor_on(n, side, t);
                let s = xf.w2s(Pos2::new(p[0], p[1]));
                painter.circle_stroke(s, 8.0, EStroke::new(2.0_f32, palette.accent));
            }
        }
        // Modifier glyph near the pointer.
        let glyph = if mods.ctrl && !mods.shift {
            Some("−")
        } else if mods.shift && !mods.ctrl {
            Some("+")
        } else {
            None
        };
        if let Some(g) = glyph {
            let at = xf.w2s(wd.cursor) + Vec2::new(14.0, -14.0);
            painter.circle_filled(at, 8.0, palette.card);
            painter.text(
                at,
                Align2::CENTER_CENTER,
                g,
                FontId::proportional(13.0),
                palette.ink,
            );
        }
    }

    /// Paint one connector node: tessellated through the path-mesh cache
    /// (geometry hash in the key), Faint = 40% opacity, filled-triangle
    /// arrowheads sized like the artifact, label centered at the midpoint.
    pub(crate) fn paint_connector(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        conn: &ConnectorNode,
    ) {
        // Hidden-anchor rule: unresolvable connectors are skipped entirely
        // (matches the artifact writer).
        let Some(path) = self.connector_path_visible(node.id, conn) else {
            return;
        };
        if conn.crosstalk.is_some() && self.crosstalk_duplicate(node.id) {
            return;
        }
        let (path, mut stroke) =
            drawn_connector_with_hosts(path, conn, |id| self.visible_wire_host(id));
        stroke.color = conn.paint_color(self.theme_wire_rgba());
        let opacity = (node.opacity
            * match conn.display {
                WireDisplay::Faint => FAINT_OPACITY,
                WireDisplay::Default => 1.0,
            })
        .clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(opacity);
        let base = fade(rgba32(stroke.color));

        if !stroke.is_none() {
            let bucket = board_path::zoom_bucket(xf.z);
            let key = connector_cache_key(
                &path,
                self.board_wire_routing,
                &stroke,
                conn.display,
                bucket,
            );
            let style = board_path::stroke_style_world(&stroke, xf.z);
            let feather = board_path::FEATHER_PX / xf.z.max(0.05);
            let kurbo = connector_path_kurbo(&path);
            let cached = self.path_mesh_cache.get_or_tessellate(node.id, key, || {
                vector_ink::stroke_mesh(&kurbo, &style, feather, board_path::curve_tolerance(xf.z))
            });
            let mesh = board_path::ink_mesh_to_epaint(&cached, xf, base, |c| c);
            painter.add(egui::Shape::mesh(mesh));
        }

        // Arrowheads: filled triangles, tip at the endpoint, base back along
        // the tangent into the curve; size matches the artifact.
        let arrow = |tip: [f32; 2], into: [f32; 2]| {
            let [a, b, c] = slate_doc::geom::arrow_head(tip, into, stroke.width);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    xf.w2s(Pos2::new(a[0], a[1])),
                    xf.w2s(Pos2::new(b[0], b[1])),
                    xf.w2s(Pos2::new(c[0], c[1])),
                ],
                base,
                EStroke::NONE,
            ));
        };
        if conn.arrow_a {
            arrow(path.start(), path.start_dir());
        }
        if conn.arrow_b {
            arrow(path.end(), path.end_dir());
        }

        // Label at the curve midpoint (skipped while its inline edit is up).
        if self
            .wire_label_edit
            .as_ref()
            .is_some_and(|(id, _)| *id == node.id)
        {
            return;
        }
        if let Some(label) = conn.label.as_deref().filter(|l| !l.is_empty()) {
            let size = atlas_shell::canvas_scale::px(CONNECTOR_LABEL_SIZE, xf.z);
            if atlas_shell::canvas_text::legible(size) {
                let m = path.midpoint();
                atlas_shell::canvas_text::text(
                    painter,
                    xf.w2s(Pos2::new(m[0], m[1])),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(size),
                    base,
                );
            }
        }
    }

    /// Selection adornment for a connector: curve highlight + endpoint dots
    /// (draggable — detach), instead of the rect outline/handles.
    pub(crate) fn paint_connector_selection(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
    ) {
        let node = self
            .shape_properties
            .preview
            .iter()
            .find(|n| n.id == node.id)
            .unwrap_or(node);
        let NodeKind::Connector(conn) = &node.kind else {
            return;
        };
        let Some(path) = self.connector_path_visible(node.id, conn) else {
            return;
        };
        let anchors = [path.start(), path.end()];
        let (path, _) = drawn_connector_with_hosts(path, conn, |id| self.visible_wire_host(id));
        let palette = self.palette();
        paint_route_preview(
            painter,
            xf,
            &path,
            EStroke::new(atlas_shell::canvas_scale::px(1.5, xf.z), palette.select),
        );
        let r = atlas_shell::canvas_scale::px(4.5, xf.z);
        let ends = anchors.map(|p| xf.w2s(Pos2::new(p[0], p[1])));
        for p in ends {
            painter.circle_filled(p, r, palette.bg);
            painter.circle_stroke(
                p,
                r,
                EStroke::new(atlas_shell::canvas_scale::px(2.0, xf.z), palette.select),
            );
        }
    }
}

pub(crate) fn paint_route_preview(
    painter: &egui::Painter,
    xf: &BoardXf,
    path: &ConnectorPath,
    stroke: EStroke,
) {
    match path {
        ConnectorPath::Bezier(bez) => {
            let pts = [
                xf.w2s(Pos2::new(bez.p0[0], bez.p0[1])),
                xf.w2s(Pos2::new(bez.c1[0], bez.c1[1])),
                xf.w2s(Pos2::new(bez.c2[0], bez.c2[1])),
                xf.w2s(Pos2::new(bez.p3[0], bez.p3[1])),
            ];
            painter.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    pts,
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ),
            ));
        }
        ConnectorPath::Orthogonal(pts) => {
            let screen: Vec<Pos2> = pts.iter().map(|p| xf.w2s(Pos2::new(p[0], p[1]))).collect();
            atlas_shell::dock::rounded_route(
                painter,
                &screen,
                atlas_shell::canvas_scale::px(ORTHO_CORNER_RADIUS, xf.z),
                stroke,
            );
        }
    }
}
