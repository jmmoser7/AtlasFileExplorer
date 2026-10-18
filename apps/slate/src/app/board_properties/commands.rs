//! Accepted property edits feed the same journal and document color history.

use super::*;

impl SlateApp {
    pub(crate) fn shape_property_command(&mut self, detail: Option<&str>) -> bool {
        let Some(req) = detail.and_then(|s| serde_json::from_str::<PropertyRequest>(s).ok()) else {
            return false;
        };
        if req.ids.is_empty()
            || req
                .ids
                .iter()
                .any(|id| self.doc().scene.node(*id).is_none_or(|node| node.locked))
            || self.refuse_read_only_edit()
        {
            return false;
        }
        self.last_board_edit = None;
        self.seed_document_colors();
        let color_clock = self.doc().view.recent_color_clock;
        let item_paths: std::collections::BTreeMap<_, _> = req
            .ids
            .iter()
            .filter_map(|id| {
                let node = self.doc().scene.node(*id)?;
                self.node_item_path(node)
                    .map(|path| (*id, path.to_path_buf()))
            })
            .collect();
        let points: &[usize] = if req.ids.len() == 1 { &req.points } else { &[] };
        let field = self.field_selection();
        self.patch_nodes(&req.ids, |node| {
            for edit in &req.edits {
                if field == Some(node.id)
                    && super::super::board_direct::apply_field_property(node, points, edit)
                {
                    continue;
                }
                edit.apply_at(
                    node,
                    item_paths.get(&node.id).map(|path| path.as_path()),
                    points,
                );
            }
        });
        // Vertex ink leaves the global stroke unchanged; unchanged RGB is
        // deliberate reuse too. Record it only if the journal path did not.
        if color_clock == self.doc().view.recent_color_clock {
            let last = req.edits.iter().rev().find_map(|edit| {
                let (rgb, supported): (_, fn(&Node) -> bool) = match edit {
                    Property::FillRgb(rgb) => (rgb, supports_fill),
                    Property::StrokeRgb(rgb) => (rgb, scene::supports_stroke),
                    Property::TextRgb(rgb) => (rgb, |node| match &node.kind {
                        NodeKind::Text(_) => true,
                        NodeKind::Shape(shape) => scene::shape_hosts_text(shape),
                        _ => false,
                    }),
                    _ => return None,
                };
                req.ids
                    .iter()
                    .any(|id| self.doc().scene.node(*id).is_some_and(supported))
                    .then_some(*rgb)
            });
            if let Some(rgb) = last {
                self.remember_document_colors(vec![rgb]);
            }
        }
        self.last_board_edit = None;
        true
    }
}
