//! Image/media and picked-vertex property-strip policy.

use super::*;

impl SlateApp {
    /// Reserve the final strip footprint while it fades in, so nearby controls
    /// cannot move away from the pointer between hover and press.
    pub(crate) fn external_property_strip_rect(&self, xf: &BoardXf) -> Option<Rect> {
        let last = self.shape_properties.last_chrome.as_ref()?;
        (!last.blister && !last.image)
            .then(|| chrome::strip_rect_below(xf.w2s(last.bottom), last.count, xf.z, 1.0))
    }
}

pub(super) fn node_strip_items(app: &SlateApp, nodes: &[Node]) -> Vec<StripItem> {
    if nodes.is_empty() {
        return Vec::new();
    }
    if let [node] = nodes {
        if let Some(panels) = image_palette::panels(app, node) {
            return panels.into_iter().map(StripItem::Panel).collect();
        }
    }
    if nodes
        .iter()
        .all(|n| slate_doc::agent_chat::agent(n).is_some())
    {
        let mut items = vec![
            StripItem::Panel(Panel::Fill),
            StripItem::Panel(Panel::Stroke),
        ];
        let ids: Vec<_> = nodes.iter().map(|n| n.id).collect();
        if slate_doc::agent_chat::bundle_run(&app.doc().scene, &ids).is_some() {
            items.push(StripItem::Agent(false));
            return items;
        }
        if nodes.len() == 1
            && slate_doc::agent_chat::agent(&nodes[0])
                .is_some_and(|a| a.chat.train && !a.chat.bundled.is_empty())
        {
            items.push(StripItem::Agent(true));
            return items;
        }
        // A generated picture or text is media too; a picture takes photo
        // filters like any placed image.
        if nodes.len() == 1 && app.is_agent_media(nodes[0].id) {
            if SlateApp::supports_image_paint(nodes[0].id, app) {
                items.push(StripItem::Panel(Panel::Filter));
            }
            items.push(StripItem::Panel(Panel::Agent));
        }
        return items;
    }
    // Crosswires own their red styling and mid-span capsule (crosstalk D16).
    if nodes
        .iter()
        .any(|n| slate_doc::crosstalk::crosstalk(n).is_some())
    {
        return Vec::new();
    }
    let mut items = property_strip_items(nodes);
    if nodes.iter().any(|n| image_is_text(app, n)) {
        items.retain(|item| *item != StripItem::Panel(Panel::Filter));
    }
    if nodes.len() == 1 && app.model_has_viewport(nodes[0].id) {
        items.retain(|item| *item != StripItem::Panel(Panel::Filter));
        items.extend([
            StripItem::Panel(Panel::ModelDisplay),
            StripItem::ModelMeasure,
            StripItem::ModelScreenshot,
            StripItem::Panel(Panel::Filter),
        ]);
    }
    if nodes.len() == 1 && image_is_model(app, &nodes[0]) && !app.model_has_viewport(nodes[0].id) {
        items.retain(|item| *item != StripItem::Panel(Panel::Filter));
    }
    if nodes.len() == 1 && image_has_pages(app, &nodes[0]) {
        items.push(StripItem::Panel(Panel::Pages));
    }
    if hosted_text_on_strip(app, nodes) {
        items.push(StripItem::Panel(Panel::Text));
    }
    if nodes.len() == 1 && app.is_agent_media(nodes[0].id) {
        items.push(StripItem::Panel(Panel::Agent));
    }
    if app.settings.optional_bumper_cars && nodes.iter().all(slate_doc::bumper::supports_bumper) {
        items.push(StripItem::Panel(Panel::Bumper));
    }
    items
}

/// Portal property presentation changes; capability categories stay in the
/// existing node strip registry, including 3D/PDF/video image-backed portals.
pub(super) fn rectangular_blister(app: &SlateApp, nodes: &[Node], count: usize) -> bool {
    // The image owner decides between its blister and external squircles.
    // A smaller generic category count must not re-admit a rejected image.
    if matches!(nodes, [node] if app.image_palette_eligible(node.id)
        || app.portal_capture_owns_viewport(node.id))
    {
        return false;
    }
    matches!(nodes, [node] if (matches!(node.kind, NodeKind::Portal(_) | NodeKind::Image(_))
        || matches!(node.kind, NodeKind::Text(_)) && app.is_text_block(node.id))
        && !matches!(&node.kind, NodeKind::Portal(portal) if portal.kind == slate_doc::scene::PortalKind::Agent)
        && media_blister_fits(node,count))
}

pub(super) fn media_blister_fits(node: &Node, count: usize) -> bool {
    let outline = if node.clip.is_some() {
        slate_doc::geom::node_closed_poly(node, 0.25)
            .and_then(|p| p.into_iter().next())
            .unwrap_or_default()
    } else {
        slate_doc::scene::resolved_corner(node, None)
            .outline(node.rect, 0.25)
            .into_iter()
            .map(|p| node.rect.rotate_point(p, node.rotation_deg))
            .collect()
    };
    let outline = outline.into_iter().map(Pos2::from).collect::<Vec<_>>();
    chrome::blister_fits_outline(&outline, 1.0, count)
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
            | Panel::Capture
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
    if matches!(nodes, [node] if matches!(node.kind, NodeKind::Portal(_))) {
        items.push(StripItem::ModelScreenshot);
    }
    if app.field_selection().is_some() {
        items.retain(|item| *item == StripItem::Panel(Panel::Fill));
        return items;
    }
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
    /// The selected object's shared Agent control owns prompt/run interaction.
    pub(crate) fn agent_property_controls_own(&self, id: NodeId) -> bool {
        self.shape_properties.ids == [id]
            && self.board_sel.len() == 1
            && self.board_sel.contains(&id)
            && self
                .shape_properties
                .last_chrome
                .as_ref()
                .is_some_and(|state| state.items.contains(&StripItem::Panel(Panel::Agent)))
    }

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
                "Capture: still image or video to File or Canvas",
                Icon::Camera,
                self.shape_properties.panel == Some(Panel::Capture)
                    || self
                        .shape_properties
                        .ids
                        .first()
                        .is_some_and(|id| self.portal_capture_status(*id).is_some()),
            ),
            StripItem::Panel(Panel::Capture) => (
                "Capture: still image or video to File or Canvas",
                Icon::Camera,
                self.shape_properties.panel == Some(Panel::Capture),
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

#[cfg(test)]
impl SlateApp {
    /// Screen rect of the shared Capture control, last painted.
    pub(crate) fn model_screenshot_button(&self) -> Option<Rect> {
        self.strip_button(|item| matches!(item, StripItem::ModelScreenshot))
    }

    /// Screen rect of the selection strip's Measure button, last frame.
    #[cfg(test)]
    pub(crate) fn model_measure_button(&self) -> Option<Rect> {
        self.strip_button(|item| matches!(item, StripItem::ModelMeasure))
    }

    fn strip_button(&self, want: fn(&StripItem) -> bool) -> Option<Rect> {
        let items = &self.shape_properties.last_chrome.as_ref()?.items;
        let index = items.iter().position(want)?;
        self.shape_properties.chrome_hits.get(index).copied()
    }
}

#[cfg(test)]
#[path = "property_strip/text_media_tests.rs"]
mod text_media_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;

    fn image(h: &mut Harness, path: &str, width: f32, height: f32) -> Node {
        let item = h.app.doc_mut().add_item(path.into(), path, 0, 0, path);
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(0.0, 0.0, width, height),
            NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.doc().scene.node(id).unwrap().clone()
    }

    #[test]
    fn image_owner_rejection_cannot_fall_back_to_a_smaller_generic_blister() {
        let mut h = Harness::new("image-blister-one-owner");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let rejected = image(&mut h, "picture.png", 220.0, 260.0);
        assert!(h.app.image_palette_eligible(rejected.id));
        assert!(!h.app.image_blister_eligible(&rejected));
        assert!(
            chrome::blister_fits(
                Rect::from_min_size(Pos2::ZERO, egui::vec2(220.0, 260.0)),
                slate_doc::scene::resolved_corner_radius(&rejected, None),
                1.0,
                1
            ),
            "the smaller generic count would have re-admitted this image"
        );
        assert!(!rectangular_blister(&h.app, &[rejected], 1));
        let accepted = image(&mut h, "large.png", 900.0, 600.0);
        assert!(h.app.image_blister_eligible(&accepted));
        assert!(
            !rectangular_blister(&h.app, &[accepted], 1),
            "accepted images also remain with the image owner"
        );
    }

    #[test]
    fn image_backed_nonimage_media_keep_the_generic_portal_chrome() {
        let mut h = Harness::new("nonimage-blister-categories");
        h.app.leave_home();
        h.app.ensure_work_tab();
        for path in ["model.obj", "document.pdf", "clip.mp4", "snippet.txt"] {
            let node = image(&mut h, path, 900.0, 600.0);
            assert!(!h.app.image_palette_eligible(node.id), "{path}");
            assert!(rectangular_blister(&h.app, &[node], 1), "{path}");
        }
    }
}
