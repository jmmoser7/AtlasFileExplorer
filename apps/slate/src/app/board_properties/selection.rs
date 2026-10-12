//! Selection revisions and cached property measurements.

use super::*;

impl SlateApp {
    pub(super) fn property_pointer_probe(ctx: &egui::Context) -> Option<Pos2> {
        ctx.input(|i| {
            i.pointer
                .press_origin()
                .filter(|_| i.pointer.any_down())
                .or(i.pointer.latest_pos())
        })
    }
    pub(crate) fn sync_shape_properties(&mut self) {
        // Sorted, so the strip resets on membership only: `board_sel` is a
        // HashSet whose iteration order can change without its contents
        // changing (a rehash on insert of an already selected id).
        let mut ids: Vec<_> = self.board_sel.iter().copied().collect();
        ids.sort_unstable_by_key(|id| id.0);
        let changed =
            self.shape_properties.tab != self.tab().id || self.shape_properties.ids != ids;
        let keep_text = self.text_edit.as_ref().is_some_and(|(id, _)| {
            ids.len() == 1
                && ids[0] == *id
                && self.doc().scene.node(*id).is_some_and(|n| match &n.kind {
                    NodeKind::Text(_) => true,
                    NodeKind::Shape(s) => scene::shape_hosts_text(s),
                    _ => false,
                })
        });
        // A sticky opens on the caret. The text/color capsule stays closed
        // until the user opens it from the strip.
        let sticky_edit = keep_text
            && self
                .doc()
                .scene
                .node(ids[0])
                .is_some_and(|n| matches!(&n.kind, NodeKind::Text(t) if t.fill.is_some()));
        let open_text = keep_text && !sticky_edit;
        if changed {
            if let ([old], Some(panel)) = (
                self.shape_properties.ids.as_slice(),
                self.shape_properties.panel,
            ) {
                if self.shape_properties.tab == self.tab().id && self.image_palette_eligible(*old) {
                    self.shape_properties
                        .remembered_image_panels
                        .insert((self.shape_properties.tab, *old), panel);
                }
            }
            let remembered_image_panels =
                std::mem::take(&mut self.shape_properties.remembered_image_panels);
            let restored = if ids.len() == 1 && self.image_palette_eligible(ids[0]) {
                remembered_image_panels
                    .get(&(self.tab().id, ids[0]))
                    .copied()
            } else {
                None
            };
            let last_chrome = ids
                .is_empty()
                .then(|| self.shape_properties.last_chrome.clone())
                .flatten();
            self.shape_properties = ShapeProperties {
                tab: self.tab().id,
                ids: ids.clone(),
                last_chrome,
                panel: open_text.then_some(Panel::Text).or(restored),
                remembered_image_panels,
                ..Default::default()
            };
        } else if open_text && self.shape_properties.panel.is_none() {
            self.shape_properties.panel = Some(Panel::Text);
        }
        let generation = (self.scene_gen, self.doc().scene.scene_gen());
        if changed || self.shape_properties.generation != generation {
            self.shape_properties.preview.clear();
            self.shape_properties.edits.clear();
            let nodes: Vec<_> = ids
                .iter()
                .filter_map(|id| self.doc().scene.node(*id).cloned())
                .collect();
            let (bounds, dimensions) = dimensions(&nodes);
            self.shape_properties.nodes = nodes;
            self.shape_properties.bounds = bounds;
            self.shape_properties.dimensions = dimensions;
            self.shape_properties.generation = generation;
        }
    }

    /// A painted path-edit grip under `screen` (P1.curve.grips): an anchor
    /// square or handle knob of the Select tool's grip target, an end point
    /// of the selected simple line, or any anchor or knob Direct Select
    /// paints on its target, including selected closed-form vertices.
    pub(crate) fn property_grip_under(&self, screen: Pos2) -> bool {
        match self.board_tool {
            BoardTool::Select => {
                self.hovered_vertex(screen).is_some()
                    || self
                        .polygon_sides_glyph_at(screen, &self.board_xf())
                        .is_some()
                    || self.fillet_grip_hit_at(screen).is_some()
                    || (self.board_sel.len() == 1
                        && self.board_sel.iter().next().is_some_and(|&id| {
                            self.line_grip_at(id, screen, &self.board_xf()).is_some()
                        }))
            }
            BoardTool::DirectSelect => self.hovered_vertex(screen).is_some(),
            _ => false,
        }
    }
}
