//! Two independent arrowhead grips: base length and wing width.
use super::*;

impl SlateApp {
    fn arrow_handles(&self) -> Vec<(NodeId, usize, usize, Pos2, [f32; 2], [f32; 2])> {
        if self.board_sel.len() != 1 {
            return Vec::new();
        }
        let id = *self.board_sel.iter().next().unwrap();
        let Some(node) = self.doc().scene.node(id).filter(|n| !n.locked && !n.hidden) else {
            return Vec::new();
        };
        let NodeKind::Shape(shape) = &node.kind else {
            return Vec::new();
        };
        let Some((anchors, false)) = self.direct_anchors_of(id) else {
            return Vec::new();
        };
        let bez = bezpath_from_anchors(&anchors, false);
        let tips = vertex_style::vertex_styles(shape.path.as_deref(), &shape.stroke, anchors.len());
        let mut out = Vec::new();
        for end in (0..2).filter(|i| shape.stroke.arrows()[*i]) {
            let width = if end == 0 { tips.first() } else { tips.last() }
                .map_or(shape.stroke.width, |t| t.tip.width);
            let size = shape.stroke.arrow_size(end, width);
            let Some((tip, into)) = slate_doc::geom::path_arrow_sized(&bez, end, size) else {
                continue;
            };
            let [_, a, b] = slate_doc::geom::arrow_head_sized(tip, into, size);
            out.push((
                id,
                end,
                0,
                Pos2::new((a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5),
                into,
                size,
            ));
            out.push((id, end, 1, Pos2::new(a[0], a[1]), into, size));
        }
        out
    }

    pub(crate) fn arrow_size_handle_under(&self, screen: Pos2) -> bool {
        let xf = self.board_xf();
        self.arrow_handles()
            .iter()
            .any(|(_, _, _, p, _, _)| xf.w2s(*p).distance(screen) <= 7.0)
    }

    pub(super) fn begin_arrow_size_drag(&self, screen: Pos2) -> Option<DirectDrag> {
        let xf = self.board_xf();
        let (node, end, axis, _, into, original) = self
            .arrow_handles()
            .into_iter()
            .find(|(_, _, _, p, _, _)| xf.w2s(*p).distance(screen) <= 7.0)?;
        Some(DirectDrag::ArrowSize {
            node,
            before: self.doc().scene.node(node)?.clone(),
            end,
            axis,
            start: xf.s2w(screen),
            into,
            original,
        })
    }

    pub(super) fn update_arrow_size_drag(&mut self, world: Pos2) -> bool {
        let Some(super::super::board::BoardDrag::Direct(DirectDrag::ArrowSize {
            node,
            end,
            axis,
            start,
            into,
            original,
            ..
        })) = &self.board_drag
        else {
            return false;
        };
        let (node, end, axis, start, into, mut size) =
            (*node, *end, *axis, *start, *into, *original);
        let delta = world - start;
        let direction = if axis == 0 {
            into
        } else {
            [-into[1] * 2.0, into[0] * 2.0]
        };
        size[axis] = (size[axis] + delta.x * direction[0] + delta.y * direction[1]).max(0.1);
        if let Some(NodeKind::Shape(shape)) =
            self.doc_mut().scene.node_mut(node).map(|n| &mut n.kind)
        {
            shape.stroke.arrow_sizes[end] = Some(size);
        }
        self.note_scene_change();
        true
    }

    pub(super) fn paint_arrow_size_handles(&self, painter: &egui::Painter, xf: &BoardXf) {
        let palette = self.palette();
        for (_, _, _, world, _, _) in self.arrow_handles() {
            painter.circle(
                xf.w2s(world),
                3.5_f32,
                palette.bg,
                egui::Stroke::new(1.0_f32, palette.sub),
            );
        }
    }
}
