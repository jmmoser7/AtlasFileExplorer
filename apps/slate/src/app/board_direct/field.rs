//! Fill sub-selection is ephemeral; authored colors belong to slate-doc.
use super::super::board_properties::Property;
use super::*;

pub(super) struct CachedFieldPaint {
    tab: u64,
    source: Node,
    painted: std::sync::Arc<Node>,
    outline: Vec<Pos2>,
}

pub(crate) fn apply_field_property(node: &mut Node, points: &[usize], property: &Property) -> bool {
    let NodeKind::Shape(shape) = &mut node.kind else {
        return false;
    };
    match *property {
        Property::FillRgb(rgb) | Property::StrokeRgb(rgb) => {
            slate_doc::field::edit_colors(shape, points, |c| c.0[..3].copy_from_slice(&rgb))
        }
        Property::FillAlpha(a) | Property::StrokeAlpha(a) => {
            slate_doc::field::edit_colors(shape, points, |c| c.0[3] = a)
        }
        _ => false,
    }
}

impl SlateApp {
    pub(crate) fn paint_field_interaction_shadow(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
    ) {
        if self.slate_nesting() || self.field_selection() != Some(node.id) {
            return;
        }
        let Some(cached) = &self.direct.field_paint_cache else {
            return;
        };
        if cached.tab != self.tab().id || cached.source.id != node.id {
            return;
        }
        atlas_shell::selection_tools::paint_outline_interaction_shadow(
            painter,
            egui::Id::new(("solid_field_shadow", self.tab().id, node.id)),
            &cached.outline,
            egui::emath::TSTransform::new(xf.center.to_vec2() - xf.offset * xf.z, xf.z),
            self.palette(),
        );
    }

    pub(crate) fn field_selection(&self) -> Option<NodeId> {
        let id = self.direct.field?;
        (self.board_sel.len() == 1
            && self.board_sel.contains(&id)
            && self.doc().scene.node(id).is_some())
        .then_some(id)
    }

    pub(super) fn solid_field_under(&self, world: Pos2) -> Option<NodeId> {
        let id = self.board_pick_node(world.x, world.y)?;
        let node = self.doc().scene.node(id)?;
        let NodeKind::Shape(shape) = &node.kind else {
            return None;
        };
        if shape.fill.is_none() || self.curve_edge_at(id, world).is_some() {
            return None;
        }
        self.direct_anchors_of(id)
            .filter(|(_, closed)| *closed)
            .map(|_| id)
    }

    pub(super) fn pick_solid_field(&mut self, world: Pos2) -> bool {
        let Some(id) = self.solid_field_under(world) else {
            return false;
        };
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.direct.field = Some(id);
        self.direct.grip_points = GripPoints {
            node: Some(id),
            ..GripPoints::default()
        };
        true
    }

    pub(crate) fn field_paint_node(&mut self, node: &Node) -> Option<std::sync::Arc<Node>> {
        if self.slate_nesting() || self.field_selection() != Some(node.id) {
            return None;
        }
        let tab = self.tab().id;
        if self
            .direct
            .field_paint_cache
            .as_ref()
            .is_none_or(|c| c.tab != tab || c.source != *node)
        {
            let mut painted = node.clone();
            if let NodeKind::Shape(shape) = &mut painted.kind {
                shape.stroke.width = 0.0;
            }
            let outline = slate_doc::geom::node_closed_poly(node, 0.25)
                .and_then(|p| p.into_iter().next())
                .unwrap_or_default()
                .into_iter()
                .map(|p| Pos2::new(p[0], p[1]))
                .collect();
            self.direct.field_paint_cache = Some(CachedFieldPaint {
                tab,
                source: node.clone(),
                painted: std::sync::Arc::new(painted),
                outline,
            });
        }
        self.direct
            .field_paint_cache
            .as_ref()
            .map(|c| c.painted.clone())
    }

    pub(crate) fn selected_field_tip(
        &self,
        points: &[usize],
    ) -> Option<slate_doc::scene::StrokeSpan> {
        let node = self.doc().scene.node(self.field_selection()?)?;
        let NodeKind::Shape(shape) = &node.kind else {
            return None;
        };
        slate_doc::field::tip(shape, points)
    }
}
