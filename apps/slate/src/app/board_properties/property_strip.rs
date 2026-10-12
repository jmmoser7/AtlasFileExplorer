//! Image/media and picked-vertex property-strip policy.

use super::*;

/// Portal property presentation changes; capability categories stay in the
/// existing node strip registry, including 3D/PDF/video image-backed portals.
pub(super) fn rectangular_blister(nodes: &[Node]) -> bool {
    matches!(nodes, [node] if matches!(node.kind, NodeKind::Portal(_) | NodeKind::Image(_)))
}

pub(super) fn image_is_model(app: &SlateApp, n: &Node) -> bool {
    let NodeKind::Image(img) = &n.kind else {
        return false;
    };
    app.doc()
        .item(img.item)
        .is_some_and(|item| slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Model)
}

pub(super) fn image_is_text(app: &SlateApp, n: &Node) -> bool {
    let NodeKind::Image(img) = &n.kind else {
        return false;
    };
    app.doc()
        .item(img.item)
        .is_some_and(|item| slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Text)
}

pub(super) fn image_has_pages(app: &SlateApp, n: &Node) -> bool {
    let NodeKind::Image(img) = &n.kind else {
        return false;
    };
    app.doc()
        .item(img.item)
        .is_some_and(|item| slate_doc::media::has_pages(&item.path))
}

/// Whether a strip item edits picked vertices. With vertices picked only
/// these show (user, 28 September 2026, ed9); every other squircle edits
/// the whole node. A new item must be classified here.
pub(super) fn edits_picked_vertices(item: &StripItem) -> bool {
    match item {
        // Width, color and opacity per vertex; the corner a vertex turns.
        StripItem::Panel(Panel::Stroke | Panel::Corners) => true,
        StripItem::Panel(
            Panel::Fill
            | Panel::Wire
            | Panel::Filter
            | Panel::Pages
            | Panel::AtlasFormat
            | Panel::Text
            | Panel::Agent
            | Panel::Brush
            | Panel::Layers
            | Panel::Bumper
            | Panel::ModelDisplay,
        )
        | StripItem::Frame(_)
        | StripItem::Agent(_)
        | StripItem::ModelMeasure
        | StripItem::ModelScreenshot => false,
    }
}

/// The strip for `nodes`. With vertices picked on the one curve, only the
/// per-vertex squircles, and Corners only when a picked vertex turns one.
pub(super) fn live_property_strip_items(app: &SlateApp, nodes: &[Node]) -> Vec<StripItem> {
    let mut items = node_strip_items(app, nodes);
    let points = app.shape_property_points();
    if let ([node], false) = (nodes, points.is_empty()) {
        if app.shape_properties.ids.as_slice() != [node.id] {
            return items;
        }
        let corner = !picked_corners(node, &points).is_empty();
        items.retain(|item| {
            edits_picked_vertices(item) && (corner || *item != StripItem::Panel(Panel::Corners))
        });
    }
    items
}

impl SlateApp {
    pub(super) fn property_strip_button_info(
        &self,
        item: &StripItem,
    ) -> (&'static str, Icon, bool) {
        match item {
            StripItem::Agent(false) => ("Bundle selected messages", Icon::ChatBundle, false),
            StripItem::Agent(true) => ("Unbundle messages", Icon::ChatUnbundle, false),
            StripItem::Panel(Panel::Fill) => (
                "Fill",
                Icon::Fill,
                self.shape_properties.panel == Some(Panel::Fill),
            ),
            StripItem::Panel(Panel::Stroke) => (
                "Stroke",
                Icon::Ellipse,
                self.shape_properties.panel == Some(Panel::Stroke),
            ),
            StripItem::Panel(Panel::Corners) => (
                if self.corners_include_crop() {
                    "Corners and crop"
                } else {
                    "Corners: fillet / chamfer"
                },
                Icon::Corners,
                self.shape_properties.panel == Some(Panel::Corners)
                    || (self.corners_include_crop() && self.board_crop.is_some()),
            ),
            StripItem::Panel(Panel::Wire) => (
                "Wire: routing / weight / dashes / arrows",
                Icon::Bezier,
                self.shape_properties.panel == Some(Panel::Wire),
            ),
            StripItem::Panel(Panel::Filter) => (
                "Photo filters",
                Icon::Filters,
                self.shape_properties.panel == Some(Panel::Filter),
            ),
            StripItem::Panel(Panel::Pages) => (
                "Pages: browse the deck or unbundle onto the board",
                Icon::Pages,
                self.shape_properties.panel == Some(Panel::Pages),
            ),
            StripItem::Panel(Panel::AtlasFormat) => (
                "Formatting: filters and zoom to fit",
                Icon::Display,
                self.shape_properties.panel == Some(Panel::AtlasFormat),
            ),
            StripItem::Panel(Panel::Text) => (
                "Text",
                Icon::Text,
                self.shape_properties.panel == Some(Panel::Text),
            ),
            StripItem::Panel(Panel::Agent) => (
                "Agent: write or picture from this",
                Icon::Agent,
                self.shape_properties.panel == Some(Panel::Agent),
            ),
            StripItem::Panel(Panel::Brush) => (
                "Brush or lasso the selected image",
                Icon::Brush,
                self.shape_properties.panel == Some(Panel::Brush),
            ),
            StripItem::Panel(Panel::Layers) => (
                "Image results and paint layers",
                Icon::Pages,
                self.shape_properties.panel == Some(Panel::Layers),
            ),
            StripItem::Panel(Panel::ModelDisplay) => (
                "Viewport display",
                Icon::Model,
                self.shape_properties.panel == Some(Panel::ModelDisplay),
            ),
            StripItem::ModelMeasure => (
                "Measure: pick two points on the model",
                Icon::Ruler,
                self.shape_properties
                    .ids
                    .first()
                    .is_some_and(|id| self.model_measuring(*id)),
            ),
            StripItem::ModelScreenshot => (
                "Screenshot: export the current viewport",
                Icon::View,
                self.model_shot_popup.is_some(),
            ),
            StripItem::Panel(Panel::Bumper) => (
                "Bumper cars",
                Icon::Bumper,
                self.shape_properties.panel == Some(Panel::Bumper)
                    || self
                        .shape_properties
                        .nodes
                        .iter()
                        .all(|n| n.bumper.is_some()),
            ),
            StripItem::Frame(FrameAction::Prev) => {
                ("Move earlier in the deck", Icon::ChevronLeft, false)
            }
            StripItem::Frame(FrameAction::Next) => {
                ("Move later in the deck", Icon::ChevronRight, false)
            }
            StripItem::Frame(FrameAction::Present) => {
                ("Present from this slide", Icon::View, false)
            }
            StripItem::Frame(FrameAction::Deck) => (
                "Order frames: click each one, or draw a stroke through them",
                Icon::Deck,
                self.board_tool == BoardTool::Deck,
            ),
        }
    }
}
