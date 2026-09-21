//! Selection property adapter. Chrome is atlas-shell; authored style is slate-doc.
//! A preview never changes the document. A completed edit dispatches one journal group.
use super::{
    board::{BoardTool, BoardXf},
    board_line, board_path, board_snap, board_transform, SlateApp,
};
use atlas_commands::CommandId;
use atlas_shell::{canvas_scale, icons::Icon, selection_tools as chrome};
use eframe::egui::{self, Id, Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};
use slate_doc::{
    scene::{
        self, Corner, Dash, ImageAdjust, Node, NodeKind, PhotoFilter, Rgba, ShapeKind, StrokeCap,
        StrokeJoin, WorldRect,
    },
    NodeId,
};
use vector_ink::kurbo::Shape;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Fill,
    Stroke,
    Corners,
    Wire,
    Filter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameAction {
    Prev,
    Next,
    Images,
    Tags,
    Present,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StripItem {
    Panel(Panel),
    Frame(FrameAction),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Property {
    FillRgb([u8; 3]),
    FillAlpha(u8),
    Filled(bool),
    StrokeRgb([u8; 3]),
    StrokeAlpha(u8),
    StrokeWidth(f32),
    Dash(Dash),
    Cap(StrokeCap),
    Join(StrokeJoin),
    CornerTreatment(bool),
    CornerMode(bool),
    CornerAmount(f32),
    WireRouting(slate_doc::WireRouting),
    WireArrows(bool),
    ImageAdjust(ImageAdjust),
}

impl Property {
    pub(crate) fn apply(&self, node: &mut Node) {
        match *self {
            Self::WireRouting(routing) => {
                if let NodeKind::Connector(c) = &mut node.kind {
                    c.routing = Some(routing);
                }
            }
            Self::WireArrows(enabled) => {
                if let NodeKind::Connector(c) = &mut node.kind {
                    if !enabled {
                        c.arrow_a = false;
                        c.arrow_b = false;
                    } else if !c.arrow_a && !c.arrow_b {
                        c.arrow_b = true;
                    }
                }
            }
            Self::FillRgb(_) | Self::FillAlpha(_) | Self::Filled(_) => {
                let mut c = scene::fill_of(node).unwrap_or(Rgba([128, 128, 128, 255]));
                match *self {
                    Self::FillRgb(rgb) => c.0[..3].copy_from_slice(&rgb),
                    Self::FillAlpha(a) => c.0[3] = a,
                    Self::Filled(false) => {
                        scene::set_fill(node, None);
                        return;
                    }
                    _ => {}
                }
                scene::set_fill(node, Some(c));
            }
            Self::CornerTreatment(_) | Self::CornerMode(_) | Self::CornerAmount(_) => {
                let Some(c) = scene::corner_of(node) else {
                    return;
                };
                let (mut chamfer, percent, mut amount) = c.parameters();
                match *self {
                    Self::CornerTreatment(v) => chamfer = v,
                    Self::CornerMode(v) => {
                        scene::set_corner(node, c.with_mode(v, node.rect.w, node.rect.h));
                        return;
                    }
                    Self::CornerAmount(v) => amount = v,
                    _ => {}
                }
                scene::set_corner(node, Corner::from_parameters(chamfer, percent, amount));
            }
            Self::ImageAdjust(adjust) => scene::set_adjust(node, adjust),
            _ => {
                let Some(mut s) = scene::stroke_of(node) else {
                    return;
                };
                match *self {
                    Self::StrokeRgb(rgb) => s.color.0[..3].copy_from_slice(&rgb),
                    Self::StrokeAlpha(a) => s.color.0[3] = a,
                    Self::StrokeWidth(v) if v.is_finite() => s.width = v.max(0.0),
                    Self::Dash(v) => s.dash = v,
                    Self::Cap(v) => s.cap = v,
                    Self::Join(v) => s.join = v,
                    _ => {}
                }
                scene::set_stroke(node, s);
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct PropertyRequest {
    pub ids: Vec<NodeId>,
    pub edits: Vec<Property>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) enum DimensionKind {
    Width,
    Height,
    Length,
    Diameter,
}
#[derive(Serialize, Deserialize)]
pub(crate) struct DimensionRequest {
    pub ids: Vec<NodeId>,
    pub kind: DimensionKind,
    pub value: f32,
}

#[derive(Clone)]
struct Dimension {
    kind: DimensionKind,
    value: f32,
    ends: [Pos2; 2],
    offset: Vec2,
}
struct NumberEdit {
    kind: DimensionKind,
    input: Option<chrome::NumberEdit>,
}

#[derive(Default)]
pub struct ShapeProperties {
    pub(super) tab: u64,
    pub(super) ids: Vec<NodeId>,
    generation: u64,
    nodes: Vec<Node>,
    dimensions: Vec<Dimension>,
    bounds: Option<WorldRect>,
    pub panel: Option<Panel>,
    pub preview: Vec<Node>,
    edits: Vec<Property>,
    number: Option<NumberEdit>,
    color: chrome::ColorState,
    frame_menu: Option<FrameAction>,
    last_chrome: Option<LastChrome>,
}

#[derive(Clone)]
struct LastChrome {
    bounds: WorldRect,
    items: Vec<StripItem>,
}

pub(crate) fn supports_fill(n: &Node) -> bool {
    scene::supports_fill(n)
}

fn property_strip_items(nodes: &[Node]) -> Vec<StripItem> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let mut items = Vec::new();
    if nodes.iter().all(scene::supports_fill) {
        items.push(StripItem::Panel(Panel::Fill));
    }
    if nodes.iter().all(scene::supports_stroke) {
        items.push(StripItem::Panel(Panel::Stroke));
    }
    if nodes.iter().all(scene::supports_corners) {
        items.push(StripItem::Panel(Panel::Corners));
    }
    if nodes
        .iter()
        .all(|n| matches!(n.kind, NodeKind::Connector(_)))
    {
        items.push(StripItem::Panel(Panel::Wire));
    }
    if nodes.iter().all(scene::supports_image_adjust) {
        items.push(StripItem::Panel(Panel::Filter));
    }
    if nodes.len() == 1 && matches!(nodes[0].kind, NodeKind::Frame(_)) {
        items.extend([
            StripItem::Frame(FrameAction::Prev),
            StripItem::Frame(FrameAction::Next),
            StripItem::Frame(FrameAction::Images),
            StripItem::Frame(FrameAction::Tags),
            StripItem::Frame(FrameAction::Present),
        ]);
    }
    items
}

fn image_is_model(app: &SlateApp, n: &Node) -> bool {
    let NodeKind::Image(img) = &n.kind else {
        return false;
    };
    app.doc()
        .item(img.item)
        .is_some_and(|item| slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Model)
}

fn live_property_strip_items(app: &SlateApp, nodes: &[Node]) -> Vec<StripItem> {
    let mut items = property_strip_items(nodes);
    if nodes.iter().any(|n| image_is_model(app, n)) {
        items.retain(|item| *item != StripItem::Panel(Panel::Filter));
    }
    items
}

fn photo_filter_radios() -> [chrome::FilterRadio; 5] {
    PhotoFilter::ALL.map(|kind| {
        let (fill, fill_b) = kind.swatch();
        chrome::FilterRadio {
            label: kind.label(),
            fill,
            fill_b,
        }
    })
}

fn dimension_editable(n: &Node) -> bool {
    !matches!(n.kind, NodeKind::Connector(_) | NodeKind::DockStrip(_))
}

/// Tight centerline bounds, before node rotation. Cached on scene/selection change.
pub(crate) fn measured_bounds(n: &Node) -> WorldRect {
    if let NodeKind::Shape(s) = &n.kind {
        if let Some(path) = &s.path {
            let bounds = board_path::path_data_to_world_bez(path, n.rect, 0.0).bounding_box();
            return WorldRect::new(
                bounds.x0 as f32,
                bounds.y0 as f32,
                bounds.width() as f32,
                bounds.height() as f32,
            );
        }
    }
    n.rect
}

fn dimensions(nodes: &[Node]) -> (Option<WorldRect>, Vec<Dimension>) {
    if nodes.is_empty() {
        return (None, vec![]);
    }
    if nodes
        .iter()
        .any(|n| matches!(n.kind, NodeKind::Connector(_)))
    {
        // Anchored wire length follows its hosts; it is not an editable box size.
        return (
            board_snap::union_rect(
                &nodes
                    .iter()
                    .map(|n| n.rect.rotated_bounds(n.rotation_deg))
                    .collect::<Vec<_>>(),
            ),
            vec![],
        );
    }
    if nodes.len() == 1 {
        let n = &nodes[0];
        if let Some((a, b)) = board_line::line_endpoints(n) {
            return (
                Some(n.rect.rotated_bounds(n.rotation_deg)),
                vec![Dimension {
                    kind: DimensionKind::Length,
                    value: (b - a).length(),
                    ends: [a, b],
                    offset: (b - a).normalized().rot90() * chrome::STRINGER_GAP,
                }],
            );
        }
    }
    let local = if nodes.len() == 1 {
        measured_bounds(&nodes[0])
    } else {
        board_snap::union_rect(
            &nodes
                .iter()
                .map(|n| n.rect.rotated_bounds(n.rotation_deg))
                .collect::<Vec<_>>(),
        )
        .unwrap()
    };
    let mut pts = local.corners_rotated(0.0).map(|(x, y)| Pos2::new(x, y));
    if nodes.len() == 1 {
        for p in &mut pts {
            let q =
                board_snap::orbit_point(nodes[0].rect.center(), (p.x, p.y), nodes[0].rotation_deg);
            *p = Pos2::new(q.0, q.1);
        }
    }
    let bounds = WorldRect::new(
        pts.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
        pts.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
        0.0,
        0.0,
    );
    let bounds = WorldRect::new(
        bounds.x,
        bounds.y,
        pts.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max) - bounds.x,
        pts.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max) - bounds.y,
    );
    let circle = nodes.len() == 1
        && matches!(&nodes[0].kind,NodeKind::Shape(s) if s.shape==ShapeKind::Ellipse)
        && (local.w - local.h).abs() < local.w.max(local.h) * 1e-5;
    let mut result = vec![Dimension {
        kind: if circle {
            DimensionKind::Diameter
        } else {
            DimensionKind::Width
        },
        value: local.w,
        ends: [pts[3], pts[2]],
        offset: -(pts[2] - pts[3]).normalized().rot90() * chrome::STRINGER_GAP,
    }];
    if !circle {
        result.push(Dimension {
            kind: DimensionKind::Height,
            value: local.h,
            ends: [pts[1], pts[2]],
            offset: (pts[2] - pts[1]).normalized().rot90() * chrome::STRINGER_GAP,
        });
    }
    (Some(bounds), result)
}

impl SlateApp {
    fn sync_shape_properties(&mut self) {
        let ids: Vec<_> = self.board_sel.iter().copied().collect();
        let changed =
            self.shape_properties.tab != self.tab().id || self.shape_properties.ids != ids;
        if changed {
            let last_chrome = ids
                .is_empty()
                .then(|| self.shape_properties.last_chrome.clone())
                .flatten();
            self.shape_properties = ShapeProperties {
                tab: self.tab().id,
                ids: ids.clone(),
                last_chrome,
                ..Default::default()
            };
        }
        if changed || self.shape_properties.generation != self.scene_gen {
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
            self.shape_properties.generation = self.scene_gen;
        }
    }

    pub(crate) fn shape_property_command(&mut self, detail: Option<&str>) -> bool {
        let Some(req) = detail.and_then(|s| serde_json::from_str::<PropertyRequest>(s).ok()) else {
            return false;
        };
        if req.ids.is_empty()
            || req
                .ids
                .iter()
                .any(|id| self.doc().scene.node(*id).is_none_or(|n| n.locked))
            || self.refuse_read_only_edit()
        {
            return false;
        }
        self.last_board_edit = None;
        let generation = self.scene_gen;
        self.patch_nodes(&req.ids, |n| {
            for edit in &req.edits {
                edit.apply(n);
            }
        });
        // Choosing a recent color already on the target is still a deliberate
        // reuse. Promote it without manufacturing a scene/undo mutation.
        if generation == self.scene_gen {
            let mut colors = Vec::new();
            for edit in &req.edits {
                match edit {
                    Property::FillRgb(rgb)
                        if req
                            .ids
                            .iter()
                            .any(|id| self.doc().scene.node(*id).is_some_and(supports_fill)) =>
                    {
                        colors.push(*rgb)
                    }
                    Property::StrokeRgb(rgb) => colors.push(*rgb),
                    _ => {}
                }
            }
            // Only the final explicit color is promoted for an unchanged batch.
            if let Some(last) = colors.last().copied() {
                self.remember_document_colors(vec![last]);
            }
        }
        self.last_board_edit = None;
        true
    }

    pub(crate) fn shape_dimension_command(&mut self, detail: Option<&str>) -> bool {
        let Some(req) = detail.and_then(|s| serde_json::from_str::<DimensionRequest>(s).ok())
        else {
            return false;
        };
        if !req.value.is_finite()
            || req.value <= 0.0
            || req.ids.is_empty()
            || self.refuse_read_only_edit()
        {
            return false;
        }
        let nodes: Vec<_> = req
            .ids
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        if nodes.len() != req.ids.len() || nodes.iter().any(|n| n.locked || !dimension_editable(n))
        {
            return false;
        }
        let (bounds, dims) = dimensions(&nodes);
        let Some(d) = dims.iter().find(|d| d.kind == req.kind) else {
            return false;
        };
        if d.value <= f32::EPSILON {
            return false;
        }
        let factor = req.value / d.value;
        if !factor.is_finite() {
            return false;
        }
        // A rotated group must scale uniformly: independent world-axis scaling
        // would require shear, which node rect + rotation cannot represent.
        let (sx, sy) = if nodes.len() > 1 {
            (factor, factor)
        } else {
            match req.kind {
                DimensionKind::Width => (factor, 1.0),
                DimensionKind::Height => (1.0, factor),
                _ => (factor, factor),
            }
        };
        self.last_board_edit = None;
        self.patch_nodes(&req.ids, |n| {
            if nodes.len() == 1 {
                board_transform::resize_measured_node(n, measured_bounds(n), sx, sy);
            } else {
                n.rect = board_snap::remap_group_scale(n.rect, sx, sy, bounds.unwrap().center());
            }
        });
        self.last_board_edit = None;
        true
    }

    pub(crate) fn preview_shape_property(&mut self, edit: Property) {
        self.record_shape_property(edit);
        self.rebuild_shape_preview(None);
    }

    fn record_shape_property(&mut self, edit: Property) {
        if self
            .shape_properties
            .edits
            .last()
            .is_some_and(|last| std::mem::discriminant(last) == std::mem::discriminant(&edit))
        {
            *self.shape_properties.edits.last_mut().unwrap() = edit;
        } else {
            self.shape_properties.edits.push(edit);
        }
    }

    fn rebuild_shape_preview(&mut self, peek: Option<Property>) {
        if self.shape_properties.edits.is_empty() && peek.is_none() {
            self.shape_properties.preview.clear();
            return;
        }
        let mut nodes = self.shape_properties.nodes.clone();
        for edit in &self.shape_properties.edits {
            for n in &mut nodes {
                edit.apply(n);
            }
        }
        if let Some(edit) = peek {
            for n in &mut nodes {
                edit.apply(n);
            }
        }
        self.shape_properties.preview = nodes;
    }

    pub(crate) fn apply_shape_preview(&mut self, ctx: &egui::Context, close: bool) {
        let edits = std::mem::take(&mut self.shape_properties.edits);
        if !edits.is_empty() && self.shape_properties.tab == self.tab().id {
            let req = PropertyRequest {
                ids: self.shape_properties.ids.clone(),
                edits,
            };
            self.dispatch(
                ctx,
                CommandId(
                    if self
                        .shape_properties
                        .nodes
                        .iter()
                        .all(|n| matches!(n.kind, NodeKind::Connector(_)))
                    {
                        "board.wire.edit"
                    } else {
                        "board.shape.edit"
                    },
                ),
                serde_json::to_string(&req).ok(),
            );
        }
        self.shape_properties.preview.clear();
        if close {
            self.shape_properties.panel = None;
            self.shape_properties.color = Default::default();
        }
        self.sync_shape_properties();
    }

    pub(crate) fn shape_property_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.shape_properties.number.is_some()
            || self.shape_properties.panel.is_some()
            || self.shape_properties.frame_menu.is_some()
        {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.shape_properties.preview.clear();
                self.shape_properties.edits.clear();
                self.shape_properties.number = None;
                self.shape_properties.panel = None;
                self.shape_properties.frame_menu = None;
                self.shape_properties.color = Default::default();
                return true;
            }
            // Numeric/color text fields own typing; global shortcuts must not arm tools.
            return ctx.wants_keyboard_input();
        }
        false
    }

    /// Runs before board input. Layout is recomputed from the host transform every frame.
    pub(crate) fn shape_properties_ui(&mut self, ui: &mut egui::Ui, xf: &BoardXf) -> bool {
        let ctx = ui.ctx().clone();
        self.sync_shape_properties();
        self.seed_document_colors();
        let live = self.board_tool == BoardTool::Select
            && self.board_drag.is_none()
            && self.text_edit.is_none()
            && !self.shape_properties.nodes.is_empty();
        if live {
            if let Some(bounds) = self.shape_properties.bounds {
                self.shape_properties.last_chrome = Some(LastChrome {
                    bounds,
                    items: live_property_strip_items(self, &self.shape_properties.nodes),
                });
            }
        }
        let fade = chrome::strip_fade(&ctx, Id::new("selection_property_strip"), live);
        if fade <= 0.0 {
            if !live {
                self.shape_properties.last_chrome = None;
            }
            return false;
        }
        let z = xf.z;
        if canvas_scale::too_small(12.0 * z) {
            return false;
        }
        let Some(chrome_state) = self.shape_properties.last_chrome.clone() else {
            return false;
        };
        let theme = self.palette();
        let bounds = xf.rect_w2s(chrome_state.bounds);
        let enabled =
            live && !self.tab().read_only && self.shape_properties.nodes.iter().all(|n| !n.locked);
        if live && !enabled {
            self.shape_properties.preview.clear();
            self.shape_properties.edits.clear();
            self.shape_properties.number = None;
        }
        let items = chrome_state.items;
        let strip = if items.is_empty() {
            Rect::from_center_size(
                Pos2::new(bounds.center().x, bounds.top() - chrome::OBJECT_GAP * z),
                Vec2::ZERO,
            )
        } else {
            chrome::strip_rect(
                Pos2::new(bounds.center().x, bounds.top()),
                items.len(),
                z,
                fade,
            )
        };
        let mut requested_panel = None;
        let mut requested_frame = None;
        let mut captures = false;
        let mut tags_rect = None;
        for (index, item) in items.iter().enumerate() {
            let r = chrome::strip_button_rect(strip, index, z);
            let (label, icon, active) = match item {
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
                    "Corners: fillet / chamfer",
                    Icon::Corners,
                    self.shape_properties.panel == Some(Panel::Corners),
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
                StripItem::Frame(FrameAction::Prev) => {
                    ("Move earlier in the deck", Icon::ChevronLeft, false)
                }
                StripItem::Frame(FrameAction::Next) => {
                    ("Move later in the deck", Icon::ChevronRight, false)
                }
                StripItem::Frame(FrameAction::Images) => {
                    ("Add image files into this frame", Icon::Image, false)
                }
                StripItem::Frame(FrameAction::Tags) => (
                    "Frame tags — dropped images inherit these",
                    Icon::Tags,
                    self.shape_properties.frame_menu == Some(FrameAction::Tags),
                ),
                StripItem::Frame(FrameAction::Present) => {
                    ("Present from this slide", Icon::View, false)
                }
            };
            if matches!(item, StripItem::Frame(FrameAction::Tags)) {
                tags_rect = Some(r);
            }
            let response = chrome::button(
                ui,
                r,
                Id::new(("shape_property", index)),
                label,
                icon,
                active,
                z,
                theme,
                fade,
                live,
            );
            if live && response.clicked() {
                match item {
                    StripItem::Panel(panel) => requested_panel = Some(*panel),
                    StripItem::Frame(action) => requested_frame = Some(*action),
                }
            }
            captures |= ctx.pointer_latest_pos().is_some_and(|p| r.contains(p));
        }
        if let Some(panel) = requested_panel {
            let was = self.shape_properties.panel;
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            self.shape_properties.frame_menu = None;
            if was != Some(panel) {
                self.shape_properties.panel = Some(panel);
            }
        }
        if let Some(action) = requested_frame {
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            self.run_frame_strip_action(action);
        }
        // The inline editor is attached to a dimension kind, never a cached screen position.
        let dims = self.shape_properties.dimensions.clone();
        let mut dimension_commit = None;
        for (index, d) in dims.iter().enumerate() {
            let mut edit = self.shape_properties.number.take();
            let is_current = edit.as_ref().is_some_and(|e| e.kind == d.kind);
            let mut input = if is_current {
                edit.as_mut().unwrap().input.take()
            } else {
                None
            };
            let was_editing = input.is_some();
            let result = ui
                .add_enabled_ui(live && enabled && d.value > f32::EPSILON, |ui| {
                    chrome::stringer(
                        ui,
                        Id::new(("shape_stringer", index)),
                        d.ends.map(|p| xf.w2s(p)),
                        d.offset * z,
                        d.value,
                        if d.kind == DimensionKind::Diameter {
                            "Ø "
                        } else {
                            ""
                        },
                        z,
                        theme,
                        &mut input,
                    )
                })
                .inner;
            captures |=
                result.response.contains_pointer() || (was_editing && ctx.wants_keyboard_input());
            if let Some(value) = result.value {
                dimension_commit = Some((d.kind, value));
            }
            if input.is_some() {
                if !was_editing {
                    self.apply_shape_preview(&ctx, true);
                }
                self.shape_properties.number = Some(NumberEdit {
                    kind: d.kind,
                    input,
                });
            } else if !is_current {
                self.shape_properties.number = edit;
            }
        }
        if let Some((kind, value)) = dimension_commit {
            let request = DimensionRequest {
                ids: self.shape_properties.ids.clone(),
                kind,
                value,
            };
            self.dispatch(
                &ctx,
                CommandId("board.shape.dimension"),
                serde_json::to_string(&request).ok(),
            );
        }
        if live {
            if let Some(panel) = self.shape_properties.panel {
                let height = match panel {
                    Panel::Fill => chrome::FILL_HEIGHT,
                    Panel::Stroke => chrome::STROKE_HEIGHT,
                    Panel::Corners => chrome::CORNER_HEIGHT,
                    Panel::Wire => chrome::WIRE_HEIGHT,
                    Panel::Filter => chrome::FILTER_HEIGHT,
                };
                let rect = chrome::editor_rect(strip, height, z);
                let mut sample = false;
                let canvas = self.canvas_rect;
                egui::Area::new(Id::new("shape_property_editor"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(rect.min)
                    .constrain(false)
                    .movable(false)
                    .fade_in(false)
                    .show(&ctx, |ui| {
                        ui.set_clip_rect(canvas);
                        ui.set_min_size(rect.size());
                        ui.add_enabled_ui(enabled, |ui| {
                            sample = self.shape_property_body(ui, rect, panel, z);
                        });
                    });
                captures |= ctx
                    .pointer_latest_pos()
                    .is_some_and(|p| rect.contains(p) && canvas.contains(p));
                if sample {
                    self.start_property_desktop_sample(panel);
                }
            }
            if self.shape_properties.frame_menu == Some(FrameAction::Tags) {
                if let (Some(id), Some(anchor)) =
                    (self.shape_properties.ids.first().copied(), tags_rect)
                {
                    let canvas = self.canvas_rect;
                    let pos = Pos2::new(anchor.left(), anchor.top() - 8.0 * z);
                    let popup = egui::Area::new(Id::new("frame_tags_menu"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(pos)
                        .constrain(false)
                        .movable(false)
                        .fade_in(false)
                        .show(&ctx, |ui| {
                            ui.set_clip_rect(canvas);
                            ui.set_max_width(220.0 * z);
                            self.frame_tags_menu(ui, id);
                        });
                    captures |= popup
                        .response
                        .rect
                        .contains(ctx.pointer_latest_pos().unwrap_or(pos));
                }
            }
            let overlay_open = self.shape_properties.panel.is_some()
                || self.shape_properties.frame_menu.is_some()
                || self.shape_properties.number.is_some();
            if overlay_open && !captures && ctx.input(|i| i.pointer.any_pressed()) {
                self.apply_shape_preview(&ctx, true);
                self.shape_properties.frame_menu = None;
                if let Some(p) = ctx.pointer_latest_pos() {
                    if self.canvas_rect.contains(p) {
                        let world = xf.s2w(p);
                        match self.board_pick_node(world.x, world.y) {
                            Some(id) => {
                                self.board_sel = super::board_flags::expand_selection_to_groups(
                                    &self.doc().scene,
                                    &[id],
                                )
                                .into_iter()
                                .collect();
                            }
                            None => self.board_sel.clear(),
                        }
                    }
                }
                captures = true;
            }
        }
        if captures && ctx.input(|i| i.pointer.any_pressed()) {
            self.board_align_eat_press = true;
        }
        captures
    }

    fn run_frame_strip_action(&mut self, action: FrameAction) {
        let Some(id) = self.shape_properties.ids.first().copied() else {
            return;
        };
        if !matches!(
            self.doc().scene.node(id).map(|n| &n.kind),
            Some(NodeKind::Frame(_))
        ) {
            return;
        }
        match action {
            FrameAction::Tags => {
                self.shape_properties.frame_menu = (self.shape_properties.frame_menu
                    != Some(FrameAction::Tags))
                .then_some(FrameAction::Tags);
            }
            FrameAction::Images => self.add_to_frame_dialog(id),
            FrameAction::Present => self.start_present(Some(id)),
            FrameAction::Prev | FrameAction::Next => {
                let frames: Vec<NodeId> = self
                    .doc()
                    .scene
                    .frames_in_order()
                    .iter()
                    .map(|n| n.id)
                    .collect();
                let Some(pos) = frames.iter().position(|f| *f == id) else {
                    return;
                };
                let other = match action {
                    FrameAction::Prev if pos > 0 => Some(frames[pos - 1]),
                    FrameAction::Next if pos + 1 < frames.len() => Some(frames[pos + 1]),
                    _ => None,
                };
                if let Some(other) = other {
                    self.swap_frame_order(id, other);
                }
            }
        }
    }

    fn shape_property_body(&mut self, ui: &mut egui::Ui, rect: Rect, panel: Panel, z: f32) -> bool {
        let theme = self.palette();
        let nodes = if self.shape_properties.preview.is_empty() {
            &self.shape_properties.nodes
        } else {
            &self.shape_properties.preview
        };
        let first = &nodes[0];
        if panel == Panel::Filter {
            let Some(current) = scene::adjust_of(first) else {
                return false;
            };
            let common = nodes.iter().all(|n| scene::adjust_of(n) == Some(current));
            let recognized = common.then(|| PhotoFilter::recognize(&current)).flatten();
            let selected = recognized.map(|(kind, _)| {
                PhotoFilter::ALL
                    .iter()
                    .position(|k| *k == kind)
                    .unwrap_or(0)
            });
            let amount = recognized.map(|(_, amount)| amount).unwrap_or(1.0);
            let edit =
                chrome::filter_editor(ui, rect, &photo_filter_radios(), selected, amount, z, theme);
            if let Some(index) = edit.clicked {
                let kind = PhotoFilter::ALL[index];
                let next = if selected == Some(index) {
                    ImageAdjust::default()
                } else {
                    kind.at(if amount < 0.05 { 1.0 } else { amount })
                };
                self.preview_shape_property(Property::ImageAdjust(next));
            } else if let Some(t) = edit.amount {
                if let Some(index) = edit.hovered.or(selected) {
                    self.preview_shape_property(Property::ImageAdjust(
                        PhotoFilter::ALL[index].at(t),
                    ));
                }
            } else if let Some(index) = edit.hovered {
                let t = if amount < 0.05 { 1.0 } else { amount };
                self.rebuild_shape_preview(Some(Property::ImageAdjust(
                    PhotoFilter::ALL[index].at(t),
                )));
            } else {
                self.rebuild_shape_preview(None);
            }
            return false;
        }
        if panel == Panel::Wire {
            let NodeKind::Connector(first) = &first.kind else {
                return false;
            };
            let square = first.effective_routing(self.board_wire_routing)
                == slate_doc::WireRouting::Orthogonal;
            let dashed = match first.stroke.dash {
                Dash::Solid => Some(false),
                Dash::Dashed => Some(true),
                Dash::Dotted => None,
            };
            let arrows = first.arrow_a || first.arrow_b;
            let common = |test: &dyn Fn(&scene::ConnectorNode) -> bool| {
                nodes
                    .iter()
                    .all(|n| matches!(&n.kind, NodeKind::Connector(c) if test(c)))
            };
            let edit = chrome::wire_editor(
                ui,
                rect,
                common(&|c| {
                    (c.effective_routing(self.board_wire_routing)
                        == slate_doc::WireRouting::Orthogonal)
                        == square
                })
                .then_some(square),
                common(&|c| c.stroke.dash == first.stroke.dash)
                    .then_some(dashed)
                    .flatten(),
                common(&|c| (c.arrow_a || c.arrow_b) == arrows).then_some(arrows),
                first.stroke.width,
                z,
                theme,
            );
            if let Some(square) = edit.square {
                self.preview_shape_property(Property::WireRouting(if square {
                    slate_doc::WireRouting::Orthogonal
                } else {
                    slate_doc::WireRouting::Bezier
                }));
            }
            if let Some(dashed) = edit.dashed {
                self.preview_shape_property(Property::Dash(if dashed {
                    Dash::Dashed
                } else {
                    Dash::Solid
                }));
            }
            if let Some(arrows) = edit.arrows {
                self.preview_shape_property(Property::WireArrows(arrows));
            }
            if let Some(width) = edit.width {
                self.preview_shape_property(Property::StrokeWidth(width));
            }
            return false;
        }
        if panel == Panel::Corners {
            let corner = scene::corner_of(first).unwrap_or_default();
            let (chamfer, percent, amount) = corner.parameters();
            let maximum = if percent {
                100.0
            } else {
                first.rect.w.min(first.rect.h) * 0.5
            };
            let edit = chrome::corner_editor(ui, rect, chamfer, percent, amount, maximum, z, theme);
            if edit.chamfer != chamfer {
                self.preview_shape_property(Property::CornerTreatment(edit.chamfer));
            }
            if edit.percent != percent {
                self.preview_shape_property(Property::CornerMode(edit.percent));
            }
            if let Some(amount) = edit.amount {
                self.preview_shape_property(Property::CornerAmount(amount));
            }
            return false;
        }
        let get_color = |n: &Node| {
            if panel == Panel::Fill {
                scene::fill_of(n).unwrap_or(Rgba([128, 128, 128, 0]))
            } else {
                scene::stroke_of(n).unwrap().color
            }
        };
        let color = get_color(first);
        let mixed = nodes.iter().any(|n| get_color(n) != color);
        let width = (panel == Panel::Stroke).then(|| scene::stroke_of(first).unwrap().width);
        let recent = self.doc().view.recent_colors.clone().unwrap_or_default();
        let edit = chrome::color_editor(
            ui,
            rect,
            color.0,
            width,
            mixed,
            &recent,
            &mut self.shape_properties.color,
            z,
            theme,
        );
        if let Some(rgb) = edit.rgb {
            self.preview_shape_property(if panel == Panel::Fill {
                Property::FillRgb(rgb)
            } else {
                Property::StrokeRgb(rgb)
            });
        }
        if let Some(alpha) = edit.alpha {
            self.preview_shape_property(if panel == Panel::Fill {
                Property::FillAlpha(alpha)
            } else {
                Property::StrokeAlpha(alpha)
            });
        }
        if let Some(width) = edit.width {
            self.preview_shape_property(Property::StrokeWidth(width));
        }
        edit.sample
    }
}

impl SlateApp {
    pub(crate) fn start_property_desktop_sample(&mut self, panel: Panel) {
        self.begin_desktop_sample(
            super::board_color::DesktopDestination::Nodes {
                ids: self.shape_properties.ids.clone(),
                panel,
                preview: true,
            },
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::Harness;
    use super::*;

    fn board() -> Harness {
        let mut h = Harness::new("shape_properties");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.set_board_tool(BoardTool::Select);
        h.app.tab_mut().cam.z = 1.0;
        h
    }

    fn wire(h: &mut Harness, y: f32) -> NodeId {
        h.app
            .add_connector(
                scene::ConnectorEnd::Free { point: [0.0, y] },
                scene::ConnectorEnd::Free { point: [300.0, y] },
            )
            .unwrap()
    }

    #[test]
    fn wire_properties_batch_preview_commit_undo_and_persistence() {
        let mut h = board();
        let a = wire(&mut h, 0.0);
        let b = wire(&mut h, 80.0);
        let untouched = wire(&mut h, 160.0);
        h.app.sync_connector_rects();
        h.app.board_sel = [a, b].into_iter().collect();
        h.app.sync_shape_properties();
        let before = h.app.doc().scene.nodes.clone();
        h.app
            .preview_shape_property(Property::WireRouting(slate_doc::WireRouting::Orthogonal));
        h.app.preview_shape_property(Property::StrokeWidth(6.0));
        h.app.preview_shape_property(Property::Dash(Dash::Dashed));
        h.app.preview_shape_property(Property::WireArrows(true));
        assert_eq!(
            h.app.doc().scene.nodes,
            before,
            "preview must not author the scene"
        );
        let preview = &h.app.shape_properties.preview[0];
        let NodeKind::Connector(c) = &preview.kind else {
            panic!("wire")
        };
        assert!(matches!(
            h.app.connector_path_visible(preview.id, c),
            Some(slate_doc::ConnectorPath::Orthogonal(_))
        ));
        assert!(h.app.shape_properties.dimensions.is_empty());
        h.app.apply_shape_preview(&h.ctx, true);
        for id in [a, b] {
            let node = h.app.doc().scene.node(id).unwrap();
            let NodeKind::Connector(c) = &node.kind else {
                panic!("wire")
            };
            assert_eq!(c.routing, Some(slate_doc::WireRouting::Orthogonal));
            assert_eq!(c.stroke.width, 6.0);
            assert_eq!(c.stroke.dash, Dash::Dashed);
            assert!(!c.arrow_a && c.arrow_b);
            let saved = serde_json::to_string(node).unwrap();
            assert_eq!(&serde_json::from_str::<Node>(&saved).unwrap(), node);
        }
        assert_eq!(
            h.app.doc().scene.node(untouched),
            before.iter().find(|n| n.id == untouched)
        );
        h.app.board_undo();
        assert_eq!(
            h.app.doc().scene.nodes,
            before,
            "one undo restores the entire batch"
        );
    }

    #[test]
    fn wire_shift_and_crossing_marquee_selection() {
        let mut h = board();
        let a = wire(&mut h, 0.0);
        let b = wire(&mut h, 80.0);
        h.frame();
        h.app.board_sel.clear();
        h.app
            .board_click_for_test(Pos2::new(150.0, 0.0), egui::Modifiers::NONE);
        h.app
            .board_click_for_test(Pos2::new(150.0, 80.0), egui::Modifiers::SHIFT);
        assert_eq!(h.app.board_sel, [a, b].into_iter().collect());
        h.app
            .board_click_for_test(Pos2::new(150.0, 0.0), egui::Modifiers::SHIFT);
        assert_eq!(h.app.board_sel, [b].into_iter().collect());
        let start = Pos2::new(130.0, -25.0);
        let end = Pos2::new(170.0, 105.0);
        let xf = h.app.board_xf();
        h.app.board_drag =
            h.app
                .begin_gesture_for_test(xf.w2s(start), start, egui::Modifiers::NONE);
        assert!(matches!(
            h.app.board_drag,
            Some(super::super::board::BoardDrag::Marquee { .. })
        ));
        h.app
            .end_gesture_for_test(end, Some(xf.w2s(end)), egui::Modifiers::NONE);
        assert_eq!(
            h.app.board_sel,
            [a, b].into_iter().collect(),
            "crossing a wire suffices; its full AABB need not fit"
        );
    }

    #[test]
    fn wire_palette_opens_on_selection_and_cancel_preserves_style() {
        let mut h = board();
        let a = wire(&mut h, 0.0);
        h.app.board_sel.insert(a);
        h.frame();
        h.frame();
        let before = h.app.doc().scene.node(a).unwrap().clone();
        let r = h.app.board_xf().rect_w2s(before.rect);
        let p = Pos2::new(r.center().x + 18.5, r.top() - 29.0);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
        for pressed in [true, false] {
            h.frame_with(|i| {
                i.events.push(egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                })
            });
        }
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Wire));
        h.app.preview_shape_property(Property::StrokeWidth(12.0));
        h.app.set_board_tool(BoardTool::Line);
        assert!(h.app.shape_properties.preview.is_empty());
        assert_eq!(h.app.doc().scene.node(a).unwrap(), &before);
    }
    fn rectangle(h: &mut Harness, rect: WorldRect, angle: f32) -> NodeId {
        let mut node = h.app.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(scene::ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(Rgba([10, 20, 30, 77])),
                stroke: scene::Stroke {
                    width: 2.0,
                    color: Rgba([90, 80, 70, 255]),
                    ..Default::default()
                },
                corner: Corner::Square,
                flip: false,
                path: None,
            }),
        );
        node.rotation_deg = angle;
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }
    fn apply(h: &mut Harness, ids: Vec<NodeId>, edits: Vec<Property>) {
        assert!(h.app.dispatch(
            &h.ctx,
            CommandId("board.shape.edit"),
            Some(serde_json::to_string(&PropertyRequest { ids, edits }).unwrap())
        ));
    }
    fn size(h: &mut Harness, ids: Vec<NodeId>, kind: DimensionKind, value: f32) {
        assert!(h.app.dispatch(
            &h.ctx,
            CommandId("board.shape.dimension"),
            Some(serde_json::to_string(&DimensionRequest { ids, kind, value }).unwrap())
        ));
    }

    #[test]
    fn shape_property_batch_alpha_preserves_rgb_and_stroke_and_undo_is_one_group() {
        let mut h = board();
        let a = rectangle(&mut h, WorldRect::new(0.0, 0.0, 100.0, 80.0), 0.0);
        let b = rectangle(&mut h, WorldRect::new(200.0, 0.0, 100.0, 80.0), 0.0);
        h.app.patch_nodes(&[b], |n| {
            scene::set_fill(n, Some(Rgba([100, 110, 120, 88])))
        });
        let before = h.app.doc().scene.nodes.clone();
        apply(&mut h, vec![a, b], vec![Property::FillAlpha(128)]);
        for (old, new) in before.iter().zip(&h.app.doc().scene.nodes) {
            assert_eq!(
                scene::fill_of(old).unwrap().0[..3],
                scene::fill_of(new).unwrap().0[..3]
            );
            assert_eq!(scene::fill_of(new).unwrap().0[3], 128);
            assert_eq!(scene::stroke_of(old), scene::stroke_of(new));
        }
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.nodes, before);
    }
    #[test]
    fn shape_property_rotated_dimensions_preserve_center_and_corner_intent() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(10.0, 20.0, 4.0, 2.0), 30.0);
        apply(
            &mut h,
            vec![id],
            vec![Property::CornerMode(true), Property::CornerAmount(50.0)],
        );
        let before = h.app.doc().scene.node(id).unwrap().clone();
        size(&mut h, vec![id], DimensionKind::Height, 4.0);
        let n = h.app.doc().scene.node(id).unwrap();
        assert_eq!(n.rect.center(), before.rect.center());
        assert_eq!(n.rect.w, 4.0);
        assert_eq!(n.rect.h, 4.0);
        assert_eq!(n.rotation_deg, 30.0);
        assert_eq!(
            scene::corner_of(n).unwrap().effective(n.rect.w, n.rect.h),
            (false, 1.0)
        );
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    }
    #[test]
    fn shape_property_mode_conversion_is_per_host_and_rgb_pick_preserves_alpha() {
        let mut h = board();
        let a = rectangle(&mut h, WorldRect::new(0.0, 0.0, 2.0, 2.0), 0.0);
        let b = rectangle(&mut h, WorldRect::new(10.0, 0.0, 4.0, 4.0), 0.0);
        apply(
            &mut h,
            vec![a, b],
            vec![
                Property::CornerMode(true),
                Property::CornerAmount(50.0),
                Property::FillRgb([3, 4, 5]),
            ],
        );
        apply(&mut h, vec![a, b], vec![Property::CornerMode(false)]);
        assert_eq!(
            scene::corner_of(h.app.doc().scene.node(a).unwrap()),
            Some(Corner::Rounded { radius: 0.5 })
        );
        assert_eq!(
            scene::corner_of(h.app.doc().scene.node(b).unwrap()),
            Some(Corner::Rounded { radius: 1.0 })
        );
        assert_eq!(
            scene::fill_of(h.app.doc().scene.node(a).unwrap()),
            Some(Rgba([3, 4, 5, 77]))
        );
    }
    #[test]
    fn shape_property_line_length_scales_about_midpoint_and_circle_keeps_diameter() {
        let mut h = board();
        let id = h
            .app
            .commit_line(Pos2::new(0.0, 0.0), Pos2::new(3.0, 4.0))
            .unwrap();
        size(&mut h, vec![id], DimensionKind::Length, 10.0);
        let (a, b) = board_line::line_endpoints(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert!(((b - a).length() - 10.0).abs() < 1e-4);
        assert!((a.lerp(b, 0.5) - Pos2::new(1.5, 2.0)).length() < 1e-4);
        let circle = rectangle(&mut h, WorldRect::new(50.0, 50.0, 10.0, 10.0), 30.0);
        h.app.patch_nodes(&[circle], |n| {
            if let NodeKind::Shape(s) = &mut n.kind {
                s.shape = ShapeKind::Ellipse;
            }
        });
        size(&mut h, vec![circle], DimensionKind::Diameter, 20.0);
        let r = h.app.doc().scene.node(circle).unwrap().rect;
        assert_eq!(r.w, 20.0);
        assert_eq!(r.h, 20.0);
        assert_eq!(r.center(), (55.0, 55.0));
    }
    #[test]
    fn shape_property_preview_escape_and_locked_targets_never_mutate_document() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
        h.frame();
        h.app.sync_shape_properties();
        let before = h.app.doc().scene.node(id).unwrap().clone();
        h.app.shape_properties.panel = Some(Panel::Fill);
        h.app.preview_shape_property(Property::FillRgb([255, 0, 0]));
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
        assert_ne!(h.app.shape_properties.preview[0], before);
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert!(h.app.shape_properties.preview.is_empty());
        assert!(h.app.board_sel.contains(&id));
        h.app.patch_nodes(&[id], |n| n.locked = true);
        let request = serde_json::to_string(&PropertyRequest {
            ids: vec![id],
            edits: vec![Property::FillAlpha(0)],
        })
        .unwrap();
        assert!(!h.app.shape_property_command(Some(&request)));
        assert_eq!(
            scene::fill_of(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .0[3],
            77
        );
    }
    #[test]
    fn shape_property_rotated_group_dimension_preserves_union_center() {
        let mut h = board();
        let a = rectangle(&mut h, WorldRect::new(0.0, 0.0, 100.0, 20.0), 90.0);
        let b = rectangle(&mut h, WorldRect::new(200.0, 60.0, 30.0, 80.0), 37.0);
        let old = h.app.doc().scene.nodes.clone();
        let before = dimensions(&old).0.unwrap();
        size(&mut h, vec![a, b], DimensionKind::Width, before.w * 1.7);
        let after = dimensions(&h.app.doc().scene.nodes).0.unwrap();
        assert!((after.w - before.w * 1.7).abs() < 1e-3);
        assert!((after.center().0 - before.center().0).abs() < 1e-3);
        assert!((after.center().1 - before.center().1).abs() < 1e-3);
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.nodes, old);
    }
    #[test]
    fn shape_property_tool_switch_discards_pending_preview() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(0.0, 0.0, 100.0, 80.0), 0.0);
        h.app.sync_shape_properties();
        let before = h.app.doc().scene.node(id).unwrap().clone();
        h.app.shape_properties.panel = Some(Panel::Fill);
        h.app.preview_shape_property(Property::FillAlpha(20));
        h.app.set_board_tool(BoardTool::Line);
        assert!(h.app.shape_properties.preview.is_empty());
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    }
    #[test]
    fn shape_property_toolbar_opens_from_real_pointer_without_moving_selection() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
        h.frame();
        h.frame();
        let before = h.app.doc().scene.node(id).unwrap().rect;
        let r = h.app.board_xf().rect_w2s(before);
        let p = Pos2::new(r.center().x - 37.0, r.top() - 29.0);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            })
        });
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect, before);
        assert!(h.app.board_sel.contains(&id));
    }

    #[test]
    fn shape_property_stringers_are_exterior_in_host_axes() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(0.0, 0.0, 180.0, 120.0), 0.0);
        let (_, dims) = dimensions(&[h.app.doc().scene.node(id).unwrap().clone()]);
        assert!(dims[0].offset.y > 0.0);
        assert!(dims[1].offset.x > 0.0);
        h.app.patch_nodes(&[id], |n| n.rotation_deg = 37.0);
        let n = h.app.doc().scene.node(id).unwrap();
        let (_, dims) = dimensions(std::slice::from_ref(n));
        let center = Pos2::new(n.rect.center().0, n.rect.center().1);
        for d in dims {
            let midpoint = d.ends[0].lerp(d.ends[1], 0.5);
            assert!((midpoint - center).dot(d.offset) > 0.0);
        }
    }

    #[test]
    fn shape_property_adjustment_fades_selection_paint_and_restores_after_cancel() {
        for dark in [false, true] {
            for connector in [false, true] {
                let mut h = board();
                h.app.dark_mode = dark;
                let id = if connector {
                    let id = wire(&mut h, 0.0);
                    h.app.board_sel.insert(id);
                    id
                } else {
                    rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0)
                };
                h.frame();
                let before = h.app.doc().scene.node(id).unwrap().clone();
                let mut time = h.ctx.input(|i| i.time);
                let mut render = |h: &mut Harness, events: Vec<egui::Event>| {
                    time += 1.0 / 60.0;
                    h.ctx.clone().run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(1440.0, 900.0),
                            )),
                            time: Some(time),
                            events,
                            ..Default::default()
                        },
                        |ctx| h.app.update_app(ctx),
                    )
                };
                let theme = h.app.palette();
                let tint = theme.select.gamma_multiply(
                    atlas_shell::tokens::current().board_preview.select_opacity * 0.16,
                );
                // These are the actual board's filled selection silhouette and wire grips,
                // not a readback of the animation value or a substitute widget fixture.
                let highlight_count = |out: &egui::FullOutput| {
                    out.shapes
                        .iter()
                        .filter(|s| match &s.shape {
                            egui::Shape::Path(p) => !connector && p.fill == tint,
                            egui::Shape::Circle(c) => {
                                connector && c.radius == 4.5 && c.stroke.color == theme.select
                            }
                            _ => false,
                        })
                        .count()
                };
                let rest = render(&mut h, vec![]);
                let count = highlight_count(&rest);
                assert!(count > 0, "selection must be visible before editing");
                let panels = if connector {
                    vec![Panel::Stroke, Panel::Wire]
                } else {
                    vec![Panel::Fill, Panel::Stroke, Panel::Corners]
                };
                for panel in panels {
                    h.app.shape_properties.panel = Some(panel);
                    for _ in 0..12 {
                        render(&mut h, vec![]);
                    }
                    assert_eq!(highlight_count(&render(&mut h, vec![])), 0);
                    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
                    assert!(h.app.board_sel.contains(&id));
                }
                render(
                    &mut h,
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                assert_eq!(h.app.shape_properties.panel, None);
                for _ in 0..12 {
                    render(&mut h, vec![]);
                }
                assert_eq!(highlight_count(&render(&mut h, vec![])), count);
                assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
                assert!(h.app.board_sel.contains(&id));
            }
        }
    }

    #[test]
    fn shape_property_rotated_inline_dimension_types_at_new_camera_position_in_both_themes() {
        for dark in [false, true] {
            let mut h = board();
            h.app.dark_mode = dark;
            let id = rectangle(&mut h, WorldRect::new(-90.0, -60.0, 180.0, 120.0), 37.0);
            h.frame();
            h.frame();
            let d = h.app.shape_properties.dimensions[1].clone();
            let xf = h.app.board_xf();
            let p = xf.w2s(d.ends[0].lerp(d.ends[1], 0.5) + d.offset);
            h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
            for pressed in [true, false] {
                h.frame_with(|i| {
                    i.events.push(egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                });
            }
            assert!(
                h.app.shape_properties.number.is_some(),
                "rotated value must accept a real click"
            );
            h.app.tab_mut().cam.z = 1.6;
            h.app.tab_mut().cam.offset += Vec2::new(30.0, 15.0);
            h.frame_with(|i| i.events.push(egui::Event::Text("240".into())));
            h.frame_with(|i| {
                i.events.push(egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
            });
            assert!(h.app.shape_properties.number.is_none());
            let n = h.app.doc().scene.node(id).unwrap();
            assert_eq!(n.rect.h, 240.0);
            assert_eq!(n.rect.w, 180.0);
            assert_eq!(n.rect.center(), (0.0, 0.0));
            assert_eq!(n.rotation_deg, 37.0);
        }
    }

    #[test]
    fn shape_property_recent_colors_ignore_preview_cancel_and_geometry_commits() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(0.0, 0.0, 180.0, 120.0), 0.0);
        h.app.sync_shape_properties();
        let before = h.app.doc().view.recent_colors.clone();
        h.app.preview_shape_property(Property::FillRgb([1, 2, 3]));
        assert_eq!(h.app.doc().view.recent_colors, before);
        h.app.set_board_tool(BoardTool::Line);
        size(&mut h, vec![id], DimensionKind::Width, 200.0);
        assert_eq!(h.app.doc().view.recent_colors, before);
        apply(&mut h, vec![id], vec![Property::FillRgb([4, 5, 6])]);
        assert_eq!(
            h.app.doc().view.recent_colors.as_ref().unwrap()[0],
            [4, 5, 6]
        );
        h.app.board_undo();
        assert_eq!(
            h.app.doc().view.recent_colors.as_ref().unwrap()[0],
            [4, 5, 6]
        );
        apply(&mut h, vec![id], vec![Property::FillRgb([4, 5, 6])]);
        assert_eq!(
            h.app
                .doc()
                .view
                .recent_colors
                .as_ref()
                .unwrap()
                .iter()
                .filter(|c| **c == [4, 5, 6])
                .count(),
            1
        );
    }

    fn frame_node(h: &mut Harness, rect: WorldRect) -> NodeId {
        let node = h.app.doc_mut().scene.build_node(
            rect,
            NodeKind::Frame(scene::FrameNode {
                title: "Slide".into(),
                order: 0,
                fill: Rgba([20, 30, 40, 255]),
                assignments: Default::default(),
            }),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    fn text_node(h: &mut Harness, rect: WorldRect) -> NodeId {
        let node = h.app.doc_mut().scene.build_node(
            rect,
            NodeKind::Text(scene::TextNode {
                text: "Note".into(),
                family: Default::default(),
                size: 18.0,
                color: Rgba([0, 0, 0, 255]),
                align: Default::default(),
                fill: None,
            }),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    fn image_node(h: &mut Harness, rect: WorldRect) -> NodeId {
        let item =
            h.app
                .doc_mut()
                .add_item(std::path::PathBuf::from("pic.png"), "pic.png", 1, 0, "pic");
        let node = h
            .app
            .doc_mut()
            .scene
            .build_node(rect, NodeKind::Image(scene::ImageNode::new(item)));
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    fn portal_node(h: &mut Harness, rect: WorldRect) -> NodeId {
        let node = h.app.doc_mut().scene.build_node(
            rect,
            NodeKind::Portal(scene::PortalNode::unbound_web("Page")),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    fn item_kinds(items: &[StripItem]) -> Vec<&'static str> {
        items
            .iter()
            .map(|item| match item {
                StripItem::Panel(Panel::Fill) => "fill",
                StripItem::Panel(Panel::Stroke) => "stroke",
                StripItem::Panel(Panel::Corners) => "corners",
                StripItem::Panel(Panel::Wire) => "wire",
                StripItem::Panel(Panel::Filter) => "filter",
                StripItem::Frame(FrameAction::Prev) => "prev",
                StripItem::Frame(FrameAction::Next) => "next",
                StripItem::Frame(FrameAction::Images) => "images",
                StripItem::Frame(FrameAction::Tags) => "tags",
                StripItem::Frame(FrameAction::Present) => "present",
            })
            .collect()
    }

    #[test]
    fn property_strip_gates_on_scene_capabilities() {
        let mut h = board();
        let rect = WorldRect::new(0.0, 0.0, 120.0, 80.0);
        let frame = frame_node(&mut h, rect);
        let image = image_node(&mut h, rect);
        let text = text_node(&mut h, rect);
        let portal = portal_node(&mut h, rect);
        let shape = rectangle(&mut h, rect, 0.0);
        let node = |id| h.app.doc().scene.node(id).unwrap().clone();
        assert_eq!(
            item_kinds(&property_strip_items(&[node(frame)])),
            ["fill", "prev", "next", "images", "tags", "present"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(image)])),
            ["stroke", "corners", "filter"]
        );
        assert_eq!(item_kinds(&property_strip_items(&[node(text)])), ["fill"]);
        assert_eq!(item_kinds(&property_strip_items(&[node(portal)])), ["fill"]);
        assert_eq!(
            item_kinds(&property_strip_items(&[node(shape)])),
            ["fill", "stroke", "corners"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(frame), node(shape)])),
            ["fill"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(image), node(shape)])),
            ["stroke", "corners"]
        );
    }

    #[test]
    fn image_filter_hover_previews_without_commit_and_click_journals() {
        let mut h = board();
        let image = image_node(&mut h, WorldRect::new(0.0, 0.0, 80.0, 50.0));
        h.app.board_sel = [image].into_iter().collect();
        h.app.sync_shape_properties();
        let before = h.app.doc().scene.node(image).unwrap().clone();
        h.app
            .rebuild_shape_preview(Some(Property::ImageAdjust(PhotoFilter::Mono.at(1.0))));
        assert_eq!(
            h.app.doc().scene.node(image).unwrap(),
            &before,
            "hover must not author the scene"
        );
        assert_eq!(
            scene::adjust_of(&h.app.shape_properties.preview[0]),
            Some(PhotoFilter::Mono.at(1.0))
        );
        h.app.rebuild_shape_preview(None);
        assert!(h.app.shape_properties.preview.is_empty());
        h.app
            .preview_shape_property(Property::ImageAdjust(PhotoFilter::Clarendon.at(0.6)));
        assert_eq!(h.app.doc().scene.node(image).unwrap(), &before);
        h.app.apply_shape_preview(&h.ctx, true);
        let after = scene::adjust_of(h.app.doc().scene.node(image).unwrap()).unwrap();
        let (kind, amount) = PhotoFilter::recognize(&after).expect("clarendon");
        assert_eq!(kind, PhotoFilter::Clarendon);
        assert!((amount - 0.6).abs() < 0.02);
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(image).unwrap(), &before);
    }

    #[test]
    fn frame_image_text_and_portal_share_the_shape_property_journal() {
        let mut h = board();
        let frame = frame_node(&mut h, WorldRect::new(0.0, 0.0, 200.0, 120.0));
        let text = text_node(&mut h, WorldRect::new(220.0, 0.0, 80.0, 40.0));
        let portal = portal_node(&mut h, WorldRect::new(0.0, 140.0, 200.0, 120.0));
        let image = image_node(&mut h, WorldRect::new(220.0, 140.0, 80.0, 50.0));
        apply(&mut h, vec![frame], vec![Property::FillRgb([9, 8, 7])]);
        apply(&mut h, vec![text], vec![Property::FillRgb([1, 2, 3])]);
        apply(&mut h, vec![portal], vec![Property::FillAlpha(64)]);
        apply(
            &mut h,
            vec![image],
            vec![Property::StrokeRgb([4, 5, 6]), Property::CornerAmount(8.0)],
        );
        assert_eq!(
            scene::fill_of(h.app.doc().scene.node(frame).unwrap()),
            Some(Rgba([9, 8, 7, 255]))
        );
        assert_eq!(
            scene::fill_of(h.app.doc().scene.node(text).unwrap()),
            Some(Rgba([1, 2, 3, 255]))
        );
        assert_eq!(
            scene::fill_of(h.app.doc().scene.node(portal).unwrap())
                .unwrap()
                .0[3],
            64
        );
        let image_node = h.app.doc().scene.node(image).unwrap();
        assert_eq!(
            scene::stroke_of(image_node).unwrap().color.0[..3],
            [4, 5, 6]
        );
        assert_eq!(
            scene::corner_of(image_node).unwrap(),
            Corner::from_parameters(false, false, 8.0)
        );
        size(&mut h, vec![frame], DimensionKind::Width, 260.0);
        assert_eq!(h.app.doc().scene.node(frame).unwrap().rect.w, 260.0);
        assert_eq!(h.app.doc().scene.node(frame).unwrap().rect.h, 120.0);
    }

    #[test]
    fn canvas_press_collapses_the_editor_and_deselects_in_one_click() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
        h.frame();
        h.frame();
        h.app.shape_properties.panel = Some(Panel::Fill);
        let xf = h.app.board_xf();
        let r = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let canvas = h.app.canvas_rect;
        let p = Pos2::new(
            (r.max.x + 90.0).min(canvas.max.x - 24.0),
            (r.max.y + 70.0).min(canvas.max.y - 24.0),
        );
        assert!(canvas.contains(p));
        assert!(!r.expand(20.0).contains(p));
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert!(h.app.shape_properties.panel.is_none());
        assert!(h.app.board_sel.is_empty());
        assert!(!h.app.board_sel.contains(&id));
    }
}
