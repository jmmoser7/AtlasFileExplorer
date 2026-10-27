//! Selection revisions and cached property measurements.

use super::*;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;

const OBJECT_PANEL_RETURN_GRACE: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub(crate) struct ObjectPanelMemory {
    panels: BTreeMap<(u64, NodeId), RememberedPanel>,
}

#[derive(Clone, Copy)]
struct RememberedPanel {
    panel: Panel,
    presentation: Option<chrome::generator::GeneratorPresentation>,
    dismissed_at: Option<Instant>,
}

impl ObjectPanelMemory {
    pub(super) fn open(
        &mut self,
        tab: u64,
        image: NodeId,
        panel: Panel,
        presentation: Option<chrome::generator::GeneratorPresentation>,
    ) {
        self.panels.insert(
            (tab, image),
            RememberedPanel {
                panel,
                presentation,
                dismissed_at: None,
            },
        );
    }

    pub(super) fn dismiss(
        &mut self,
        tab: u64,
        image: NodeId,
        panel: Panel,
        presentation: Option<chrome::generator::GeneratorPresentation>,
        now: Instant,
    ) {
        self.panels.insert(
            (tab, image),
            RememberedPanel {
                panel,
                presentation,
                dismissed_at: Some(now),
            },
        );
    }

    fn selection_changed(&mut self, tab: u64, ids: &[NodeId], now: Instant) {
        for ((owner, image), entry) in &mut self.panels {
            if *owner != tab || ids != [*image] {
                entry.dismissed_at.get_or_insert(now);
            }
        }
        self.panels.retain(|_, entry| {
            entry
                .dismissed_at
                .is_none_or(|at| now.saturating_duration_since(at) <= OBJECT_PANEL_RETURN_GRACE)
        });
    }

    fn restore(&mut self, tab: u64, image: NodeId, now: Instant) -> Option<RememberedPanel> {
        let entry = self.panels.get(&(tab, image))?;
        if entry
            .dismissed_at
            .is_some_and(|at| now.saturating_duration_since(at) > OBJECT_PANEL_RETURN_GRACE)
        {
            self.forget(tab, image);
            return None;
        }
        let entry = self.panels.get_mut(&(tab, image))?;
        entry.dismissed_at = None;
        Some(*entry)
    }

    pub(super) fn forget(&mut self, tab: u64, image: NodeId) {
        self.panels.remove(&(tab, image));
    }
}

impl SlateApp {
    fn remembered_panel_eligible(&self, id: NodeId) -> bool {
        self.image_palette_eligible(id) || self.is_artifact_node(id)
    }

    pub(crate) fn remember_object_panel_open(&mut self, id: NodeId, panel: Panel) {
        if self.remembered_panel_eligible(id) {
            let presentation = (panel == Panel::Agent).then(|| self.generator_presentation(id));
            self.shape_properties.remembered_image_panels.open(
                self.tab().id,
                id,
                panel,
                presentation,
            );
        }
    }

    pub(crate) fn dismiss_object_panel(&mut self, id: NodeId, panel: Panel, now: Instant) {
        if self.remembered_panel_eligible(id) {
            let presentation =
                (panel == Panel::Agent).then(|| self.generator_presentation(id).for_reselection());
            self.shape_properties.remembered_image_panels.dismiss(
                self.tab().id,
                id,
                panel,
                presentation,
                now,
            );
            self.forget_generator_presentation(id);
        }
    }

    pub(crate) fn forget_object_panel(&mut self, id: NodeId) {
        self.shape_properties
            .remembered_image_panels
            .forget(self.tab().id, id);
        self.forget_generator_presentation(id);
    }

    pub(crate) fn dismiss_active_object_panel(&mut self, now: Instant) {
        if let (Some(id), Some(panel)) = (
            self.shape_properties
                .ids
                .first()
                .copied()
                .filter(|_| self.shape_properties.ids.len() == 1),
            self.shape_properties.panel,
        ) {
            self.dismiss_object_panel(id, panel, now);
        }
    }

    /// Whether selection properties currently own an editor or numeric entry.
    pub(crate) fn property_editor_open(&self) -> bool {
        self.shape_properties.panel.is_some() || self.property_inline_editor_open()
    }

    pub(crate) fn property_inline_editor_open(&self) -> bool {
        self.shape_properties.number.is_some() || self.shape_properties.corner_entry.is_some()
    }

    pub(crate) fn open_object_panel(&mut self, panel: Panel) -> bool {
        self.sync_shape_properties();
        let item = if panel == Panel::Capture {
            StripItem::ModelScreenshot
        } else {
            StripItem::Panel(panel)
        };
        if !live_property_strip_items(self, &self.shape_properties.nodes).contains(&item) {
            return false;
        }
        let id = self
            .shape_properties
            .ids
            .first()
            .copied()
            .filter(|_| self.shape_properties.ids.len() == 1);
        if self.shape_properties.panel.is_some_and(|old| old != panel) {
            if let Some(id) = id {
                self.forget_object_panel(id);
            }
        }
        self.shape_properties.number = None;
        self.shape_properties.panel = Some(panel);
        if let Some(id) = id {
            self.remember_object_panel_open(id, panel);
        }
        true
    }

    pub(crate) fn open_object_properties(&mut self) -> bool {
        self.sync_shape_properties();
        let panel = live_property_strip_items(self, &self.shape_properties.nodes)
            .into_iter()
            .find_map(|item| match item {
                StripItem::Panel(panel) => Some(panel),
                _ => None,
            });
        panel.is_some_and(|panel| self.open_object_panel(panel))
    }

    pub(crate) fn object_properties_available(&self, ids: &[NodeId]) -> bool {
        let nodes: Vec<_> = ids
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        live_property_strip_items(self, &nodes)
            .iter()
            .any(|item| matches!(item, StripItem::Panel(_)))
    }

    pub(super) fn property_pointer_probe(ctx: &egui::Context) -> Option<Pos2> {
        ctx.input(|i| {
            i.pointer
                .press_origin()
                .filter(|_| i.pointer.any_down())
                .or(i.pointer.latest_pos())
        })
    }
    pub(crate) fn sync_shape_properties(&mut self) {
        self.sync_shape_properties_at(Instant::now());
    }

    fn sync_shape_properties_at(&mut self, now: Instant) {
        // Sorted, so the strip resets on membership only: `board_sel` is a
        // HashSet whose iteration order can change without its contents
        // changing (a rehash on insert of an already selected id).
        // Maximized model chrome belongs to its visible source, even when a
        // capture selects a newly placed image elsewhere on the board.
        let mut ids: Vec<_> = self
            .portal_chrome
            .maximized
            .filter(|id| self.model_node_info(*id).is_some())
            .map(|id| vec![id])
            .unwrap_or_else(|| self.board_sel.iter().copied().collect());
        ids.sort_unstable_by_key(|id| id.0);
        let changed =
            self.shape_properties.tab != self.tab().id || self.shape_properties.ids != ids;
        if changed {
            if self.shape_properties.tab == self.tab().id {
                self.dismiss_active_object_panel(now);
            }
            let mut remembered_image_panels =
                std::mem::take(&mut self.shape_properties.remembered_image_panels);
            remembered_image_panels.selection_changed(self.tab().id, &ids, now);
            let restored = if ids.len() == 1 && self.remembered_panel_eligible(ids[0]) {
                remembered_image_panels.restore(self.tab().id, ids[0], now)
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
                panel: restored.map(|entry| entry.panel),
                remembered_image_panels,
                ..Default::default()
            };
            if let Some(id) = ids.first().copied().filter(|_| ids.len() == 1) {
                if let Some(presentation) = restored.and_then(|entry| entry.presentation) {
                    self.restore_generator_presentation(id, presentation);
                } else if self.remembered_panel_eligible(id) {
                    self.forget_generator_presentation(id);
                }
            }
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
        if self.curve_subselect_under(screen) || self.tiny_body_owns_pointer(screen).is_some() {
            return true;
        }
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
