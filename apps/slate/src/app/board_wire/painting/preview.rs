//! Rubber-band wire previews and snap feedback.
use super::*;

impl SlateApp {
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
                let s = xf.w2s(self.wire_port_point(n, side, t));
                painter.circle_stroke(
                    s,
                    atlas_shell::canvas_scale::px(8.0, xf.z),
                    EStroke::new(atlas_shell::canvas_scale::px(2.0, xf.z), palette.accent),
                );
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
