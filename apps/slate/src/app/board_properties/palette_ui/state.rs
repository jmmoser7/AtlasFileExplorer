//! Prepared property chrome state shared by the palette and its media owner.
use super::*;

#[derive(Clone)]
pub(in crate::app::board_properties) struct LastChrome {
    pub(in crate::app::board_properties) host: Option<(u64, NodeId)>,
    pub(in crate::app::board_properties) bounds: WorldRect,
    pub(in crate::app::board_properties) items: Vec<StripItem>,
    pub(in crate::app::board_properties) count: usize,
    pub(super) generation: (u64, u64),
    pub(super) picked_vertices: bool,
    pub(in crate::app::board_properties) bottom: Pos2,
    pub(in crate::app::board_properties) blister: bool,
    pub(in crate::app::board_properties) image: bool,
    pub(in crate::app::board_properties) media: Option<media::PreparedBlister>,
}

impl SlateApp {
    pub(super) fn update_live_property_chrome(
        &mut self,
        ctx: &egui::Context,
        vertex_bounds: Option<WorldRect>,
    ) {
        let items = live_property_strip_items(self, &self.shape_properties.nodes);
        let count = match self.shape_properties.nodes.as_slice() {
            [node] => image_palette::control_count(self, node).unwrap_or(items.len()),
            _ => items.len(),
        };
        // D13 / D11: a panel whose squircle a vertex pick took off the
        // strip commits its pending edits and closes.
        if self
            .shape_properties
            .panel
            .is_some_and(|panel| !items.contains(&StripItem::Panel(panel)))
            && !self.shape_property_points().is_empty()
        {
            self.apply_shape_preview(ctx, true);
        }
        if let Some(bounds) = vertex_bounds.or(self.shape_properties.bounds) {
            let generation = self.shape_properties.generation;
            let bottom = self
                .shape_properties
                .last_chrome
                .as_ref()
                .filter(|last| {
                    last.bounds == bounds
                        && last.count == count
                        && last.generation == generation
                        && last.picked_vertices == vertex_bounds.is_some()
                })
                .map(|last| last.bottom)
                .unwrap_or_else(|| {
                    let outlines: Vec<_> = self
                        .shape_properties
                        .nodes
                        .iter()
                        .filter(|_| vertex_bounds.is_none())
                        .filter_map(|node| slate_doc::geom::node_closed_poly(node, 0.25))
                        .flatten()
                        .collect();
                    chrome::strip_bottom(
                        Rect::from_min_size(
                            Pos2::new(bounds.x, bounds.y),
                            Vec2::new(bounds.w, bounds.h),
                        ),
                        &outlines,
                        chrome::strip_size(count).x,
                    )
                });
            self.shape_properties.last_chrome = Some(LastChrome {
                host: match self.shape_properties.ids.as_slice() {
                    [host] => Some((self.tab().id, *host)),
                    _ => None,
                },
                bounds,
                items: items.clone(),
                count,
                generation,
                picked_vertices: vertex_bounds.is_some(),
                bottom,
                blister: property_strip::rectangular_blister(
                    self,
                    &self.shape_properties.nodes,
                    items.len(),
                ),
                image: image_palette::attached(self, true),
                media: None,
            });
        }
    }
}
