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
    Pages,
    AtlasFormat,
    Text,
    /// Text and image agents started from a piece of media.
    Agent,
    /// Bumper cars: On / Off, buffer, friction (optional tool).
    Bumper,
    /// 3D viewport display pass: Shaded / Arctic / Material mask / Z-buffer.
    ModelDisplay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameAction {
    Prev,
    Next,
    Present,
    Deck,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StripItem {
    Panel(Panel),
    Frame(FrameAction),
    Agent(bool),
    /// Arms point-to-point measure in a 3D viewport, like Deck arms a tool.
    ModelMeasure,
    /// Viewport screenshot export menu (pointer-attached).
    ModelScreenshot,
}

/// Display passes offered on a 3D viewport, in strip order.
const MODEL_DISPLAYS: [(scene::ModelDisplay, &str); 4] = [
    (scene::ModelDisplay::Shaded, "Shaded"),
    (scene::ModelDisplay::Arctic, "Arctic"),
    (scene::ModelDisplay::Material, "Material mask"),
    (scene::ModelDisplay::Depth, "Z-buffer"),
];
fn model_display_glyph(mode: scene::ModelDisplay) -> ([u8; 3], Option<[u8; 3]>) {
    match mode {
        scene::ModelDisplay::Shaded => ([108, 118, 132], None),
        scene::ModelDisplay::Arctic => ([238, 238, 234], None),
        scene::ModelDisplay::Material => ([196, 88, 72], Some([72, 132, 188])),
        scene::ModelDisplay::Depth => ([32, 32, 32], Some([228, 228, 228])),
    }
}

fn model_display_radios(thumbs: [Option<egui::TextureId>; 4]) -> [chrome::FilterRadio; 4] {
    std::array::from_fn(|i| {
        let (mode, label) = MODEL_DISPLAYS[i];
        let (fill, fill_b) = model_display_glyph(mode);
        chrome::FilterRadio {
            label,
            fill,
            fill_b,
            thumb: thumbs[i],
        }
    })
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
    TextFamily(scene::Typeface),
    TextSize(f32),
    TextAlign(scene::TextAlign),
    TextRgb([u8; 3]),
    TextAlpha(u8),
    BumperOn(bool),
    BumperBuffer(f32),
    BumperFriction(f32),
    /// Regular polygon side count (3–12).
    RegularSides(u8),
}

impl Property {
    pub(crate) fn apply(&self, node: &mut Node, item_path: Option<&std::path::Path>) {
        match *self {
            Self::BumperOn(on) => {
                node.bumper = (on && slate_doc::bumper::supports_bumper(node))
                    .then(|| node.bumper.unwrap_or_default());
            }
            Self::BumperBuffer(v) | Self::BumperFriction(v) => {
                if !slate_doc::bumper::supports_bumper(node) {
                    return;
                }
                let mut b = node.bumper.unwrap_or_default();
                if matches!(self, Self::BumperBuffer(_)) {
                    b.buffer = v;
                } else {
                    b.friction = v;
                }
                node.bumper = Some(b.clamped());
            }
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
                let theme_relative = match &node.kind {
                    NodeKind::Portal(p) => p.fill_follows_theme() || p.slate_fill_follows_theme(),
                    NodeKind::Frame(f) => f.fill_follows_theme(),
                    _ => false,
                };
                let mut c = scene::fill_of(node).unwrap_or(Rgba([128, 128, 128, 255]));
                match *self {
                    Self::FillRgb(rgb) => {
                        c.0[..3].copy_from_slice(&rgb);
                        if theme_relative {
                            c.0[3] = 255;
                        }
                    }
                    Self::FillAlpha(a) => c.0[3] = a,
                    Self::Filled(false) => {
                        scene::set_fill(node, None);
                        return;
                    }
                    _ => {}
                }
                scene::set_fill(node, Some(c));
            }
            Self::RegularSides(sides) => {
                if let NodeKind::Shape(s) = &mut node.kind {
                    if s.shape == ShapeKind::RegularPolygon {
                        let old = s.sides;
                        s.sides = scene::clamp_regular_sides(sides);
                        slate_doc::vertex_style::reside_closed_form_style(s, old, s.phase_deg);
                    }
                }
            }
            Self::CornerTreatment(_) | Self::CornerMode(_) | Self::CornerAmount(_) => {
                if !scene::supports_corners(node) {
                    return;
                }
                let (w, h) = (node.rect.w, node.rect.h);
                scene::edit_corner(node, item_path, |corner| {
                    let (mut chamfer, percent, mut amount) = corner.parameters();
                    match *self {
                        Self::CornerTreatment(v) => chamfer = v,
                        Self::CornerMode(v) => return corner.with_mode(v, w, h),
                        Self::CornerAmount(v) => amount = v,
                        _ => {}
                    }
                    Corner::from_parameters(chamfer, percent, amount)
                });
                if matches!(self, Self::CornerAmount(_)) {
                    scene::clear_vertex_corner_amounts(node);
                }
            }
            Self::ImageAdjust(adjust) => scene::set_adjust(node, adjust),
            Self::TextFamily(family) => {
                map_text_style(node, |face, _, _, _| *face = family);
            }
            Self::TextSize(size) => {
                if size.is_finite() {
                    let size = size.clamp(1.0, 512.0);
                    map_text_style(node, |_, stored, _, _| *stored = size);
                }
            }
            Self::TextAlign(align) => {
                map_text_style(node, |_, _, _, stored| *stored = align);
            }
            Self::TextRgb(rgb) => {
                map_text_style(node, |_, _, color, _| color.0[..3].copy_from_slice(&rgb));
            }
            Self::TextAlpha(alpha) => {
                map_text_style(node, |_, _, color, _| color.0[3] = alpha);
            }
            _ => {
                let theme_relative =
                    matches!(&node.kind, NodeKind::Portal(p) if p.stroke_follows_theme());
                let Some(mut s) = scene::stroke_of(node) else {
                    return;
                };
                match *self {
                    Self::StrokeRgb(rgb) => {
                        s.color.0[..3].copy_from_slice(&rgb);
                        if theme_relative {
                            s.color.0[3] = 255;
                            if s.width <= 0.0 {
                                s.width = 1.0;
                            }
                        }
                    }
                    Self::StrokeAlpha(a) => s.color.0[3] = a,
                    Self::StrokeWidth(v) if v.is_finite() => s.width = v.max(0.0),
                    Self::Dash(v) => s.dash = v,
                    Self::Cap(v) => s.cap = v,
                    Self::Join(v) => s.join = v,
                    _ => {}
                }
                scene::set_stroke(node, s);
                if matches!(*self, Self::StrokeRgb(_) | Self::StrokeAlpha(_)) {
                    if let NodeKind::Connector(c) = &mut node.kind {
                        c.set_color(Some(s.color));
                    }
                }
            }
        }
    }

    /// [`Self::apply`] scoped to the picked grips of a curve
    /// (P1.curve.vertex-style): a stroke width edits those vertices only.
    /// With no grip picked, or on a node without grips, the whole node.
    pub(crate) fn apply_at(
        &self,
        node: &mut Node,
        item_path: Option<&std::path::Path>,
        points: &[usize],
    ) {
        if !points.is_empty() {
            let edited = match *self {
                Self::StrokeWidth(v) if v.is_finite() => {
                    edit_vertex_tips(node, points, |t| t.width = v.max(0.0))
                }
                Self::StrokeRgb(rgb) => {
                    edit_vertex_tips(node, points, |t| t.color.0[..3].copy_from_slice(&rgb))
                }
                Self::StrokeAlpha(a) => edit_vertex_tips(node, points, |t| t.color.0[3] = a),
                Self::CornerAmount(v) => {
                    set_picked_corner_amounts(node, item_path, points, v);
                    return;
                }
                _ => false,
            };
            if edited {
                return;
            }
        }
        if let Self::StrokeWidth(v) = *self {
            if v.is_finite() && scale_stamped_curve(node, None, v.max(0.0), None) {
                return;
            }
        }
        self.apply(node, item_path);
        match *self {
            Self::StrokeRgb(rgb) => {
                edit_every_tip(node, |t| t.color.0[..3].copy_from_slice(&rgb));
            }
            Self::StrokeAlpha(a) => edit_every_tip(node, |t| t.color.0[3] = a),
            _ => {}
        }
    }
}

/// Picked grips of `node` that turn a corner, in pick order: the turning
/// vertices of a line polyline (`polyline_vertex_grip_edges`), or any
/// vertex of a rectangle or regular polygon (P1.shape.vertex-style).
fn picked_corners(node: &Node, points: &[usize]) -> Vec<usize> {
    if points.is_empty() || !scene::supports_corners(node) {
        return Vec::new();
    }
    if let NodeKind::Shape(s) = &node.kind {
        if let Some(n) = slate_doc::vertex_style::closed_form_vertex_count(s) {
            return points.iter().copied().filter(|&p| p < n).collect();
        }
    }
    let chamfer = scene::resolved_corner(node, None).parameters().0;
    let corners: Vec<usize> = slate_doc::geom::polyline_vertex_grip_edges(node, chamfer)
        .into_iter()
        .map(|(v, _)| v)
        .collect();
    points
        .iter()
        .copied()
        .filter(|p| corners.contains(p))
        .collect()
}

/// The Corners amount with corners picked (user, 28 September 2026, ep2):
/// each picked corner takes `amount` as its own override, in the shape
/// corner's units (a percentage resolves against the box like the shared
/// corner); every other corner keeps its amount.
fn set_picked_corner_amounts(
    node: &mut Node,
    item_path: Option<&std::path::Path>,
    points: &[usize],
    amount: f32,
) {
    let (chamfer, percent, _) = scene::resolved_corner(node, item_path).parameters();
    let world = if percent {
        Corner::from_parameters(chamfer, true, amount)
            .effective(node.rect.w, node.rect.h)
            .1
    } else {
        amount
    };
    for v in picked_corners(node, points) {
        scene::set_vertex_corner_amount(node, v, world);
    }
}

/// A whole-curve color or texture edit sets every vertex's tip too
/// (P1.curve.vertex-style): stored tips paint over the stroke, on a hard
/// curve and a stamped brush stroke alike.
pub(crate) fn edit_every_tip(node: &mut Node, edit: impl Fn(&mut scene::StrokeSpan)) {
    let NodeKind::Shape(s) = &mut node.kind else {
        return;
    };
    if s.shape != ShapeKind::Path {
        if let Some(mut path) = slate_doc::vertex_style::closed_form_path(s) {
            if !path.tips.is_empty() {
                slate_doc::vertex_style::edit_every_tip(&mut path, &mut s.stroke, edit);
                slate_doc::vertex_style::store_closed_form_style(s, &path);
            }
        }
        return;
    }
    let Some(path) = s.path.as_mut() else {
        return;
    };
    if path.tips.is_empty() && s.stroke.tween_from.is_none() {
        return;
    }
    slate_doc::vertex_style::edit_every_tip(std::sync::Arc::make_mut(path), &mut s.stroke, edit);
}

/// Whole-curve width (and softness, when given) of a stamped stroke with
/// stored tips: every tip scales from `base`, else from the curve as it is
/// (`vertex_style::scale_stamped_tips`). `false` when the node paints one
/// tip or is not stamped, so the stroke itself takes the edit.
pub(crate) fn scale_stamped_curve(
    node: &mut Node,
    base: Option<&Node>,
    width: f32,
    softness: Option<f32>,
) -> bool {
    let base = base.and_then(|b| match &b.kind {
        NodeKind::Shape(s) => Some((s.path.clone()?, s.stroke)),
        _ => None,
    });
    let NodeKind::Shape(s) = &mut node.kind else {
        return false;
    };
    if s.shape != ShapeKind::Path || !s.stroke.paints_as_stamp() {
        return false;
    }
    let Some(path) = s.path.as_mut() else {
        return false;
    };
    let softness = softness
        .unwrap_or_else(|| slate_doc::vertex_style::curve_tip(Some(&**path), &s.stroke).softness);
    slate_doc::vertex_style::scale_stamped_tips(
        std::sync::Arc::make_mut(path),
        &mut s.stroke,
        base.as_ref().map(|(p, st)| (&**p, st)),
        width,
        softness,
    )
}

/// Set the painted opacity of the `points` (grip indices) of a path node or
/// closed form through the share rule (`vertex_style::set_grip_opacity`),
/// node opacity included.
pub(crate) fn set_vertex_opacity(node: &mut Node, points: &[usize], opacity: f32) -> bool {
    let (rect, rotation, node_opacity) = (node.rect, node.rotation_deg, node.opacity);
    let NodeKind::Shape(s) = &mut node.kind else {
        return false;
    };
    let Some(mut path) = slate_doc::vertex_style::vertex_style_path(s) else {
        return false;
    };
    let mut stroke = s.stroke;
    let Some(top) = slate_doc::vertex_style::set_grip_opacity(
        &mut path,
        &mut stroke,
        rect,
        rotation,
        node_opacity,
        points,
        opacity,
    ) else {
        return false;
    };
    slate_doc::vertex_style::store_vertex_style_path(s, path);
    s.stroke = stroke;
    node.opacity = top;
    true
}

/// The painted tip at the first of `points` (grip indices) of a path node
/// or closed form.
pub(crate) fn picked_tip(node: &Node, points: &[usize]) -> Option<scene::StrokeSpan> {
    let first = *points.first()?;
    let NodeKind::Shape(s) = &node.kind else {
        return None;
    };
    let tips = slate_doc::vertex_style::grip_tips(
        &slate_doc::vertex_style::vertex_style_path(s)?,
        &s.stroke,
        node.rect,
        node.rotation_deg,
    )?;
    tips.get(first).copied()
}

/// Edit the stroke tips at `points` (grip indices) of a path node or closed
/// form.
pub(crate) fn edit_vertex_tips(
    node: &mut Node,
    points: &[usize],
    edit: impl Fn(&mut scene::StrokeSpan),
) -> bool {
    let (rect, rotation) = (node.rect, node.rotation_deg);
    let NodeKind::Shape(s) = &mut node.kind else {
        return false;
    };
    let Some(mut path) = slate_doc::vertex_style::vertex_style_path(s) else {
        return false;
    };
    let mut stroke = s.stroke;
    if !slate_doc::vertex_style::edit_grip_tips(
        &mut path,
        &mut stroke,
        rect,
        rotation,
        points,
        edit,
    ) {
        return false;
    }
    slate_doc::vertex_style::store_vertex_style_path(s, path);
    s.stroke = stroke;
    true
}

#[derive(Serialize, Deserialize)]
pub(crate) struct PropertyRequest {
    pub ids: Vec<NodeId>,
    pub edits: Vec<Property>,
    /// Picked grips of the one target curve; the edits apply to those
    /// vertices (`Property::apply_at`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<usize>,
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
struct CornerEntry {
    id: NodeId,
    vertex: Option<usize>,
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
    pub(super) pages_focus: u16,
    pub preview: Vec<Node>,
    edits: Vec<Property>,
    /// Last photo-filter radio under the pointer. The intensity slider sits
    /// beside the radios, so a scrub starts after hover has already ended.
    filter_aim: Option<usize>,
    number: Option<NumberEdit>,
    /// Typed corner amount opened by a click on the corner grip.
    corner_entry: Option<CornerEntry>,
    text_family_open: bool,
    text_size_open: bool,
    /// Screen rects of the strip and open editor, so a text caret can ignore them.
    pub chrome_hits: Vec<Rect>,
    color: chrome::ColorState,
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
    if nodes.len() == 1
        && matches!(
            &nodes[0].kind,
            NodeKind::Portal(p) if p.kind == slate_doc::PortalKind::FileAtlas
        )
    {
        items.push(StripItem::Panel(Panel::AtlasFormat));
    }
    if nodes.len() == 1 && matches!(nodes[0].kind, NodeKind::Frame(_)) {
        items.extend([
            StripItem::Frame(FrameAction::Prev),
            StripItem::Frame(FrameAction::Next),
            StripItem::Frame(FrameAction::Present),
            StripItem::Frame(FrameAction::Deck),
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

fn image_is_text(app: &SlateApp, n: &Node) -> bool {
    let NodeKind::Image(img) = &n.kind else {
        return false;
    };
    app.doc()
        .item(img.item)
        .is_some_and(|item| slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Text)
}

fn image_has_pages(app: &SlateApp, n: &Node) -> bool {
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
fn edits_picked_vertices(item: &StripItem) -> bool {
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
fn live_property_strip_items(app: &SlateApp, nodes: &[Node]) -> Vec<StripItem> {
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

fn node_strip_items(app: &SlateApp, nodes: &[Node]) -> Vec<StripItem> {
    if nodes.is_empty() {
        return Vec::new();
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
    // A crosswire's red is its meaning (crosstalk D16): no stroke or wire
    // styling. Its editor is the capsule at its mid-span.
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

fn hosted_text_on_strip(app: &SlateApp, nodes: &[Node]) -> bool {
    let [node] = nodes else {
        return false;
    };
    match &node.kind {
        NodeKind::Text(_) => true,
        NodeKind::Shape(shape) if scene::shape_hosts_text(shape) => {
            let editing = app.text_edit.as_ref().is_some_and(|(id, _)| *id == node.id);
            let has_body = shape
                .text
                .as_ref()
                .is_some_and(|text| !text.body.is_empty());
            editing || has_body
        }
        _ => false,
    }
}

fn map_text_style(
    node: &mut Node,
    edit: impl FnOnce(&mut scene::Typeface, &mut f32, &mut scene::Rgba, &mut scene::TextAlign),
) {
    if let NodeKind::Text(text) = &mut node.kind {
        edit(
            &mut text.family,
            &mut text.size,
            &mut text.color,
            &mut text.align,
        );
        return;
    }
    if let Some(text) = scene::ensure_shape_text(node) {
        edit(
            &mut text.family,
            &mut text.size,
            &mut text.color,
            &mut text.align,
        );
    }
}

/// Radio 0 clears the adjustment. The rest are [`PhotoFilter::ALL`] in order.
fn photo_filter_radios(thumbs: [Option<egui::TextureId>; 6]) -> [chrome::FilterRadio; 6] {
    std::array::from_fn(|i| {
        if i == 0 {
            return chrome::FilterRadio {
                label: "None",
                fill: [196, 196, 196],
                fill_b: None,
                thumb: thumbs[0],
            };
        }
        let kind = PhotoFilter::ALL[i - 1];
        let (fill, fill_b) = kind.swatch();
        chrome::FilterRadio {
            label: kind.label(),
            fill,
            fill_b,
            thumb: thumbs[i],
        }
    })
}

fn photo_filter_adjust(index: usize, amount: f32) -> ImageAdjust {
    if index == 0 {
        ImageAdjust::default()
    } else {
        PhotoFilter::ALL[index - 1].at(amount)
    }
}

enum FilterStep {
    /// Record an edit the next outside-click can journal.
    Commit(ImageAdjust),
    /// Show the look without recording it.
    Peek(ImageAdjust),
    /// Drop the peek and show recorded edits only.
    Rest,
}

/// Hover peeks. Click and the intensity slider record. `selected` is the
/// authored filter (scene plus pending edits), never the peek — otherwise
/// the click that should arm a hovered filter toggles it off.
fn photo_filter_gesture(
    selected: Option<usize>,
    amount: f32,
    aim: Option<usize>,
    edit: chrome::FilterEdit,
) -> FilterStep {
    let strength = if amount < 0.05 { 1.0 } else { amount };
    if let Some(index) = edit.clicked {
        let next = if index == 0 || selected == Some(index) {
            ImageAdjust::default()
        } else {
            photo_filter_adjust(index, strength)
        };
        return FilterStep::Commit(next);
    }
    if let Some(t) = edit.amount {
        let index = edit
            .hovered
            .filter(|index| *index > 0)
            .or(selected.filter(|index| *index > 0))
            .or(aim.filter(|index| *index > 0));
        if let Some(index) = index {
            return FilterStep::Commit(photo_filter_adjust(index, t));
        }
    }
    if let Some(index) = edit.hovered {
        return FilterStep::Peek(photo_filter_adjust(index, strength));
    }
    FilterStep::Rest
}

fn photo_filter_choice(adjust: &ImageAdjust) -> Option<(usize, f32)> {
    let (kind, amount) = PhotoFilter::recognize(adjust)?;
    let index = PhotoFilter::ALL.iter().position(|k| *k == kind)?;
    Some((index, amount))
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

/// Open curves and brush strokes carry no dimension stringers; closed shapes
/// keep theirs.
fn takes_stringers(n: &Node) -> bool {
    !slate_doc::is_open_shape(n)
        && !matches!(&n.kind, NodeKind::Shape(s) if s.stroke.paints_as_stamp())
}

fn dimensions(nodes: &[Node]) -> (Option<WorldRect>, Vec<Dimension>) {
    let (bounds, dims) = measured_dimensions(nodes);
    if nodes.iter().any(takes_stringers) {
        (bounds, dims)
    } else {
        (bounds, vec![])
    }
}

fn measured_dimensions(nodes: &[Node]) -> (Option<WorldRect>, Vec<Dimension>) {
    if nodes.is_empty() {
        return (None, vec![]);
    }
    if nodes
        .iter()
        .any(|n| slate_doc::agent_chat::agent(n).is_some())
    {
        return (
            board_snap::union_rect(&nodes.iter().map(|n| n.rect).collect::<Vec<_>>()),
            vec![],
        );
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
    let (ends, offset) = stringer_lane(
        [pts[3], pts[2]],
        -(pts[2] - pts[3]).normalized().rot90(),
        [pts[0], pts[1]],
    );
    let mut result = vec![Dimension {
        kind: if circle {
            DimensionKind::Diameter
        } else {
            DimensionKind::Width
        },
        value: local.w,
        ends,
        offset,
    }];
    if !circle {
        let (ends, offset) = stringer_lane(
            [pts[1], pts[2]],
            (pts[2] - pts[1]).normalized().rot90(),
            [pts[0], pts[3]],
        );
        result.push(Dimension {
            kind: DimensionKind::Height,
            value: local.h,
            ends,
            offset,
        });
    }
    (Some(bounds), result)
}

/// Which of an axis's two parallel edges carries its stringer. `home` is the
/// unrotated choice (local bottom for width, local right for height) and
/// `outward` its exterior normal. On screen a mostly horizontal span sits
/// below and a mostly vertical one to the right, so rotation never moves a
/// stringer into the upper lane the selection strip owns. The measured axis,
/// and so what W and H mean, never changes.
fn stringer_lane(home: [Pos2; 2], outward: Vec2, opposite: [Pos2; 2]) -> ([Pos2; 2], Vec2) {
    let along = home[1] - home[0];
    let lane = if along.x.abs() >= along.y.abs() {
        Vec2::DOWN
    } else {
        Vec2::RIGHT
    };
    if outward.dot(lane) < -1e-4 {
        (opposite, -outward * chrome::STRINGER_GAP)
    } else {
        (home, outward * chrome::STRINGER_GAP)
    }
}

impl SlateApp {
    /// An open adjustment previews authored color without the selection
    /// tint. The Text editor on text nodes is the exception: it changes the
    /// glyphs, not the box, so the outline stays (DYNAMIC_PANELS.md).
    pub(crate) fn property_panel_fades_selection(&self) -> bool {
        match self.shape_properties.panel {
            None => false,
            Some(Panel::Text) => !self.board_sel.iter().all(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Text(_)))
            }),
            Some(_) => true,
        }
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
            let last_chrome = ids
                .is_empty()
                .then(|| self.shape_properties.last_chrome.clone())
                .flatten();
            self.shape_properties = ShapeProperties {
                tab: self.tab().id,
                ids: ids.clone(),
                last_chrome,
                panel: open_text.then_some(Panel::Text),
                ..Default::default()
            };
        } else if open_text && self.shape_properties.panel.is_none() {
            self.shape_properties.panel = Some(Panel::Text);
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
        self.patch_nodes(&req.ids, |n| {
            for edit in &req.edits {
                edit.apply_at(n, item_paths.get(&n.id).map(|path| path.as_path()), points);
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

    /// Low-resolution faces for None plus the photo-filter radios. Color
    /// swatches stand in until the image thumbnail has arrived.
    fn filter_swatch_ids(
        &mut self,
        ctx: &egui::Context,
        amount: f32,
    ) -> [Option<egui::TextureId>; 6] {
        let none = [None; 6];
        let Some(item) = self
            .shape_properties
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                NodeKind::Image(img) => Some(img.item),
                _ => None,
            })
        else {
            return none;
        };
        let Some((key, _, _, _)) = self.resolved_item_preview(item) else {
            return none;
        };
        if key.is_empty() || !self.thumb_pixels.contains_key(&key) {
            self.request_thumb(item);
            return none;
        }
        if !self.filter_swatch_src.contains_key(&key) {
            let src = super::imagefx::square_swatch(
                &self.thumb_pixels[&key],
                super::imagefx::SWATCH_EDGE,
            );
            self.filter_swatch_src.insert(key.clone(), src);
        }
        if self.filter_swatch_tex.len() > 80 {
            self.filter_swatch_tex.clear();
        }
        let mut out = none;
        for (i, slot) in out.iter_mut().enumerate() {
            let adjust = photo_filter_adjust(i, amount);
            let hash = adjust.cache_hash();
            let cache_key = (key.clone(), hash);
            if !self.filter_swatch_tex.contains_key(&cache_key) {
                let image = super::imagefx::adjusted(&self.filter_swatch_src[&key], &adjust);
                let tex = ctx.load_texture(
                    format!("slate-filter-swatch-{key}-{hash}"),
                    image,
                    egui::TextureOptions::NEAREST,
                );
                self.filter_swatch_tex.insert(cache_key.clone(), tex);
            }
            *slot = Some(self.filter_swatch_tex[&cache_key].id());
        }
        out
    }

    /// Grips picked on the strip's one target curve (P1.curve.vertex-style):
    /// Select-tool grip picks or Direct Select anchors. Empty when the whole
    /// selection is the target.
    pub(crate) fn shape_property_points(&self) -> Vec<usize> {
        match (self.shape_properties.ids.as_slice(), self.picked_vertices()) {
            ([id], Some((picked, points))) if *id == picked => points,
            _ => Vec::new(),
        }
    }

    /// World bounds of the picked vertices of the strip's one curve, grown
    /// by the stroke's half width, so the strip sits beside those points.
    fn picked_vertex_bounds(&self) -> Option<WorldRect> {
        let (id, points) = self.picked_vertex_points()?;
        if self.shape_properties.ids.as_slice() != [id] {
            return None;
        }
        let half = match self.doc().scene.node(id).map(|n| &n.kind) {
            Some(NodeKind::Shape(s)) => s.stroke.width * 0.5,
            _ => 0.0,
        };
        let (mut lo, mut hi) = (points[0], points[0]);
        for p in &points[1..] {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        Some(WorldRect::new(
            lo.x - half,
            lo.y - half,
            hi.x - lo.x + 2.0 * half,
            hi.y - lo.y + 2.0 * half,
        ))
    }

    fn committed_shape_nodes(&self) -> Vec<Node> {
        let mut nodes = self.shape_properties.nodes.clone();
        let item_paths: std::collections::BTreeMap<_, _> = nodes
            .iter()
            .filter_map(|node| {
                self.node_item_path(node)
                    .map(|path| (node.id, path.to_path_buf()))
            })
            .collect();
        let points = self.shape_property_points();
        for edit in &self.shape_properties.edits {
            for n in &mut nodes {
                edit.apply_at(n, item_paths.get(&n.id).map(|path| path.as_path()), &points);
            }
        }
        nodes
    }

    fn rebuild_shape_preview(&mut self, peek: Option<Property>) {
        if self.shape_properties.edits.is_empty() && peek.is_none() {
            self.shape_properties.preview.clear();
            return;
        }
        let mut nodes = self.shape_properties.nodes.clone();
        let item_paths: std::collections::BTreeMap<_, _> = nodes
            .iter()
            .filter_map(|node| {
                self.node_item_path(node)
                    .map(|path| (node.id, path.to_path_buf()))
            })
            .collect();
        let points = self.shape_property_points();
        for edit in self.shape_properties.edits.iter().chain(peek.as_ref()) {
            for n in &mut nodes {
                edit.apply_at(n, item_paths.get(&n.id).map(|path| path.as_path()), &points);
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
                points: self.shape_property_points(),
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
        let corner_entry = self.shape_properties.corner_entry.is_some();
        if self.shape_properties.number.is_some()
            || self.shape_properties.panel.is_some()
            || corner_entry
        {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.shape_properties.preview.clear();
                self.shape_properties.edits.clear();
                self.shape_properties.number = None;
                self.shape_properties.corner_entry = None;
                self.shape_properties.panel = None;
                self.shape_properties.color = Default::default();
                return true;
            }
            // Numeric/color text fields own typing; global shortcuts must not arm tools.
            // The corner field opens on release and takes focus when it first paints.
            return ctx.wants_keyboard_input() || corner_entry;
        }
        false
    }

    /// Click on the corner grip: type the corner amount (P1.node.corner-grip).
    pub(crate) fn open_corner_entry(&mut self, id: NodeId, vertex: Option<usize>) {
        let Some(node) = self.doc().scene.node(id) else {
            return;
        };
        if node.locked || self.tab().read_only {
            return;
        }
        let amount = self.node_grip_amount(node, vertex);
        self.shape_properties.number = None;
        self.shape_properties.corner_entry = Some(CornerEntry {
            id,
            vertex,
            input: Some(chrome::NumberEdit::new(amount)),
        });
    }

    /// The corner grip's typed amount, beside the grip on the inside of its
    /// edge. Enter dispatches one `board.shape.fillet`; Esc cancels.
    pub(crate) fn corner_entry_ui(&mut self, ui: &mut egui::Ui, xf: &BoardXf) -> bool {
        let Some(mut entry) = self.shape_properties.corner_entry.take() else {
            return false;
        };
        let Some(node) = self.doc().scene.node(entry.id).cloned() else {
            return false;
        };
        if node.locked || self.board_drag.is_some() {
            return false;
        }
        let grip = self
            .corner_grips(&node, xf)
            .into_iter()
            .find_map(|(v, p)| (v == entry.vertex).then_some(p));
        let (Some(edge), Some(grip)) = (self.node_grip_edge(&node, entry.vertex), grip) else {
            return false;
        };
        let ctx = ui.ctx().clone();
        let theme = self.palette();
        let z = xf.z;
        let mut angle = Vec2::from(edge.dir).angle();
        if angle > std::f32::consts::FRAC_PI_2 - 0.001 {
            angle -= std::f32::consts::PI;
        }
        if angle < -std::f32::consts::FRAC_PI_2 {
            angle += std::f32::consts::PI;
        }
        let center = grip + Vec2::from(edge.inward) * canvas_scale::px(16.0, z);
        let amount = self.node_grip_amount(&node, entry.vertex);
        let result = chrome::inline_number(
            ui,
            Id::new(("corner_entry", entry.id, entry.vertex)),
            center,
            angle,
            "",
            " u",
            amount,
            z,
            theme,
            theme.select,
            &mut entry.input,
            0.0..=f32::MAX,
            false,
        );
        let captures = result.response.contains_pointer() || ctx.wants_keyboard_input();
        if let Some(radius) = result.value {
            let mut ids = vec![entry.id];
            if entry.vertex.is_none() {
                ids.extend(self.corner_grip_peers(entry.id).iter().map(|n| n.id));
            }
            let request = board_transform::FilletRequest {
                ids,
                radius,
                vertex: entry.vertex,
            };
            self.dispatch(
                &ctx,
                CommandId("board.shape.fillet"),
                serde_json::to_string(&request).ok(),
            );
        }
        if entry.input.is_some() {
            self.shape_properties.corner_entry = Some(entry);
        }
        if captures && ctx.input(|i| i.pointer.any_pressed()) {
            self.board_align_eat_press = true;
        }
        captures
    }

    /// This frame's press that commits an open panel's preview (D11), if
    /// any: `Some(true)` for a primary press on a crop handle, a cropping
    /// image, or a painted curve grip, which is that gesture rather than a
    /// click-away. Crop keeps the Corners panel up (D09) and the selection
    /// whole; a grip press is the grip's drag (P1.curve.grips). Neither is
    /// eaten. Right-drag and middle-drag are the
    /// canvas pan, so only a secondary or middle click without a drag counts.
    fn property_dismiss_press(&self, ctx: &egui::Context) -> Option<bool> {
        let (primary, click) = ctx.input(|i| {
            (
                i.pointer
                    .button_pressed(egui::PointerButton::Primary)
                    .then(|| i.pointer.press_origin()),
                i.pointer.button_clicked(egui::PointerButton::Secondary)
                    || i.pointer.button_clicked(egui::PointerButton::Middle),
            )
        });
        match primary {
            Some(origin) => {
                Some(origin.is_some_and(|p| self.crop_owns_pointer(p) || self.curve_knob_under(p)))
            }
            None => click.then_some(false),
        }
    }

    /// Runs before board input. Layout is recomputed from the host transform every frame.
    pub(crate) fn shape_properties_ui(&mut self, ui: &mut egui::Ui, xf: &BoardXf) -> bool {
        let ctx = ui.ctx().clone();
        self.sync_shape_properties();
        self.seed_document_colors();
        self.shape_properties.chrome_hits.clear();
        let editing_hosted_text = self.text_edit.as_ref().is_some_and(|(id, _)| {
            self.doc().scene.node(*id).is_some_and(|n| match &n.kind {
                NodeKind::Text(_) => true,
                NodeKind::Shape(s) => scene::shape_hosts_text(s),
                _ => false,
            })
        });
        // A sticky's first job is the caret. The fill/text strip returns
        // once editing ends and the note is simply selected.
        let editing_sticky = self.text_edit.as_ref().is_some_and(|(id, _)| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(|n| matches!(&n.kind, NodeKind::Text(t) if t.fill.is_some()))
        });
        let composing_text_box = self.text_box_draft.is_some();
        let vertex_bounds = self.picked_vertex_bounds();
        let direct_picks = self.board_tool == BoardTool::DirectSelect && vertex_bounds.is_some();
        let live = self.board_drag.is_none()
            && !editing_sticky
            && !composing_text_box
            && (editing_hosted_text
                || ((self.board_tool == BoardTool::Select || direct_picks)
                    && self.text_edit.is_none()))
            && !self.shape_properties.nodes.is_empty();
        if live {
            let items = live_property_strip_items(self, &self.shape_properties.nodes);
            // D13 / D11: a panel whose squircle a vertex pick took off the
            // strip commits its pending edits and closes.
            if self
                .shape_properties
                .panel
                .is_some_and(|panel| !items.contains(&StripItem::Panel(panel)))
                && !self.shape_property_points().is_empty()
            {
                self.apply_shape_preview(&ctx, true);
            }
            if let Some(bounds) = vertex_bounds.or(self.shape_properties.bounds) {
                self.shape_properties.last_chrome = Some(LastChrome { bounds, items });
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
            // The strip and its panel are not painted, but screen-constant
            // grips still take presses, so the D11 commit still runs. The
            // press keeps its board meaning.
            let overlay_open =
                self.shape_properties.panel.is_some() || self.shape_properties.number.is_some();
            if let (true, true, Some(grip)) =
                (live, overlay_open, self.property_dismiss_press(&ctx))
            {
                self.apply_shape_preview(&ctx, !grip);
            }
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
        let mut requested_agent = None;
        let mut requested_measure = false;
        let mut requested_screenshot = false;
        let mut captures = false;
        // P1.curve.grips: a painted grip under a button takes the press. The
        // board hit-tests a drag at its press origin, so a held press is
        // judged there too.
        let grip_probe = ctx.input(|i| {
            let held = i.pointer.any_down();
            i.pointer
                .press_origin()
                .filter(|_| held)
                .or(i.pointer.latest_pos())
        });
        let grip_under =
            live && grip_probe.is_some_and(|p| strip.contains(p) && self.curve_knob_under(p));
        for (index, item) in items.iter().enumerate() {
            let r = chrome::strip_button_rect(strip, index, z);
            let (label, icon, active) = match item {
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
            };
            let button_live = live && !grip_under;
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
                button_live,
            );
            if button_live && response.clicked() {
                match item {
                    StripItem::Panel(panel) => requested_panel = Some(*panel),
                    StripItem::Frame(action) => requested_frame = Some(*action),
                    StripItem::Agent(expand) => requested_agent = Some(*expand),
                    StripItem::ModelMeasure => requested_measure = true,
                    StripItem::ModelScreenshot => requested_screenshot = true,
                }
            }
            captures |= button_live && ctx.pointer_latest_pos().is_some_and(|p| r.contains(p));
            if live {
                self.shape_properties.chrome_hits.push(r);
            }
        }
        if let Some(expand) = requested_agent {
            self.dispatch(
                &ctx,
                CommandId(if expand {
                    "portal.agent.expand_chat"
                } else {
                    "portal.agent.bundle_chat"
                }),
                None,
            );
        }
        if let Some(panel) = requested_panel {
            let was = self.shape_properties.panel;
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            if was != Some(panel) {
                self.shape_properties.panel = Some(panel);
                if panel == Panel::Pages {
                    if let Some(item) = self
                        .shape_properties
                        .ids
                        .first()
                        .and_then(|id| self.node_pdf_item(*id))
                    {
                        self.shape_properties.pages_focus = item.pdf_page;
                    }
                }
            }
        }
        if let Some(action) = requested_frame {
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            self.run_frame_strip_action(&ctx, action);
        }
        if requested_measure {
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            if let Some(id) = self.shape_properties.ids.first().copied() {
                self.dispatch(
                    &ctx,
                    CommandId("board.model_measure"),
                    Some(id.0.to_string()),
                );
            }
        }
        if requested_screenshot {
            self.apply_shape_preview(&ctx, true);
            self.shape_properties.number = None;
            if let (Some(id), Some(p)) = (
                self.shape_properties.ids.first().copied(),
                ctx.pointer_latest_pos(),
            ) {
                self.open_model_screenshot_menu(id, p);
            }
        }
        // The inline editor is attached to a dimension kind, never a cached screen position.
        // Nested portal edit keeps the frame selected but hides its stringers.
        let dims = if self.selection_stringers_suppressed() {
            self.shape_properties.number = None;
            Vec::new()
        } else {
            self.shape_properties.dimensions.clone()
        };
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
        if self.shape_properties.panel != Some(Panel::Filter) {
            self.shape_properties.filter_aim = None;
        }
        if live {
            if let Some(panel) = self.shape_properties.panel {
                if panel == Panel::Pages {
                    if let Some(node_id) = self.shape_properties.ids.first().copied() {
                        if let Some(node) = self.doc().scene.node(node_id).cloned() {
                            if let Some(path) =
                                self.node_pdf_item(node_id).map(|item| item.path.clone())
                            {
                                self.documents.request(&path);
                            }
                            let host = xf.rect_w2s(node.rect);
                            let album = self.paint_pages_album(
                                ui,
                                node_id,
                                host,
                                z,
                                theme,
                                self.shape_properties.pages_focus,
                            );
                            self.shape_properties.pages_focus = album.focus;
                            captures |= album.hover;
                            if album.commit_page {
                                if let Some(item) = self.node_pdf_item(node_id) {
                                    self.dispatch(
                                        &ctx,
                                        CommandId("board.media.page"),
                                        Some(format!("{}:{}", item.id.0, album.focus)),
                                    );
                                }
                            }
                            if album.unbundle {
                                self.dispatch(
                                    &ctx,
                                    CommandId("board.media.unbundle"),
                                    Some(format!("{}:{}", node_id.0, album.focus)),
                                );
                            }
                        }
                    }
                } else {
                    let height = match panel {
                        Panel::Fill => chrome::FILL_HEIGHT,
                        Panel::Stroke => chrome::STROKE_HEIGHT,
                        Panel::Corners => chrome::CORNER_HEIGHT,
                        Panel::Wire => chrome::WIRE_HEIGHT,
                        Panel::Filter => chrome::FILTER_HEIGHT,
                        Panel::Pages => 0.0,
                        Panel::AtlasFormat => chrome::ATLAS_FORMAT_HEIGHT,
                        Panel::Text => chrome::TEXT_HEIGHT,
                        Panel::Agent => chrome::AGENT_HEIGHT,
                        Panel::Bumper => chrome::CORNER_HEIGHT,
                        Panel::ModelDisplay => chrome::FILTER_CHIPS_HEIGHT,
                    };
                    if panel != Panel::Text {
                        self.shape_properties.text_family_open = false;
                        self.shape_properties.text_size_open = false;
                    }
                    let canvas = self.canvas_rect;
                    let rect = chrome::editor_placement(strip, bounds, height, z, canvas);
                    let mut sample = None;
                    chrome::popup_area(&ctx, Id::new("shape_property_editor"), ui.layer_id())
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
                    captures |= ctx.pointer_latest_pos().is_some_and(|p| {
                        rect.contains(p) && canvas.contains(p)
                            || !grip_under
                                && self
                                    .shape_properties
                                    .chrome_hits
                                    .iter()
                                    .any(|hit| hit.contains(p))
                    });
                    if live {
                        self.shape_properties.chrome_hits.push(rect);
                    }
                    if let Some(gesture) = sample {
                        self.start_property_desktop_sample(panel, gesture);
                    }
                }
            }
            let overlay_open =
                self.shape_properties.panel.is_some() || self.shape_properties.number.is_some();
            let press = self.property_dismiss_press(&ctx);
            if overlay_open && !captures && press == Some(true) {
                self.apply_shape_preview(&ctx, false);
            } else if overlay_open && !captures && press.is_some() {
                self.apply_shape_preview(&ctx, true);
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

    fn run_frame_strip_action(&mut self, ctx: &egui::Context, action: FrameAction) {
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
            FrameAction::Deck => {
                let cmd = if self.board_tool == BoardTool::Deck {
                    "board.tool.select"
                } else {
                    "board.tool.deck"
                };
                self.dispatch(ctx, CommandId(cmd), Some("frame-strip".into()));
            }
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

    fn corners_include_crop(&self) -> bool {
        let nodes = &self.shape_properties.nodes;
        !nodes.is_empty() && nodes.iter().all(|n| self.croppable_image(n.id))
    }

    fn corner_for_editor(&self, node: &Node) -> Corner {
        scene::resolved_corner(node, self.node_item_path(node))
    }

    /// What the Corners panel shows for `first`: its treatment and units,
    /// the slider's maximum, and the amount, which is the first picked
    /// corner's when corners are picked (ep2).
    pub(crate) fn corners_panel_reading(&self, first: &Node) -> (bool, bool, f32, f32) {
        let (chamfer, percent, amount) = self.corner_for_editor(first).parameters();
        let limit = first.rect.w.min(first.rect.h) * 0.5;
        let maximum = if percent { 100.0 } else { limit };
        let points = self.shape_property_points();
        let amount = match picked_corners(first, &points).first() {
            Some(&v) => {
                let world = self.node_grip_amount(first, Some(v));
                match (percent, limit > 0.0) {
                    (false, _) => world,
                    (true, true) => (world / limit * 100.0).min(100.0),
                    (true, false) => 0.0,
                }
            }
            None => amount,
        };
        (chamfer, percent, amount, maximum)
    }

    fn shape_property_body(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        panel: Panel,
        z: f32,
    ) -> Option<chrome::SampleGesture> {
        let theme = self.palette();
        let nodes_owned: Vec<Node> = if self.shape_properties.preview.is_empty() {
            self.shape_properties.nodes.clone()
        } else {
            self.shape_properties.preview.clone()
        };
        let nodes = nodes_owned.as_slice();
        let first = &nodes[0];
        if panel == Panel::Agent {
            let id = self.shape_properties.ids.first().copied()?;
            let popups = self.agent_editor_body(ui, rect, id, z, theme);
            self.shape_properties.chrome_hits.extend(popups);
            return None;
        }
        if panel == Panel::AtlasFormat {
            let id = self.shape_properties.ids.first().copied()?;
            self.atlas_format_body(ui, rect, id, z, theme);
            return None;
        }
        if panel == Panel::ModelDisplay {
            let id = self.shape_properties.ids.first().copied()?;
            let display = self.model_display_of(id)?;
            let current = MODEL_DISPLAYS
                .iter()
                .position(|(mode, _)| *mode == display)
                .unwrap_or(0);
            self.ensure_model_display_swatches(id);
            let thumbs = self.model_display_swatch_ids(ui.ctx(), id);
            let radios = model_display_radios(thumbs);
            let chip_w = chrome::filter_chips_width(radios.len(), chrome::FILTER_CHIPS_HEIGHT, z);
            let row = Rect::from_center_size(
                rect.center(),
                Vec2::new(chip_w, chrome::FILTER_CHIPS_HEIGHT) * z,
            );
            let edit = chrome::filter_capsule(
                ui,
                row,
                &radios,
                Some(current),
                None,
                z,
                theme,
                chrome::FilterCapsuleStyle::ChipsOnly,
            );
            if let Some(picked) = edit.clicked {
                if picked != current {
                    let (mode, _) = MODEL_DISPLAYS[picked];
                    let ctx = ui.ctx().clone();
                    self.dispatch(
                        &ctx,
                        CommandId("board.model_display"),
                        Some(format!("{}:{}", id.0, mode.key())),
                    );
                }
            }
            return None;
        }
        if panel == Panel::Filter {
            let committed = self.committed_shape_nodes();
            let current = committed.first().and_then(scene::adjust_of)?;
            let common = committed
                .iter()
                .all(|n| scene::adjust_of(n) == Some(current));
            let (selected, amount) = if common {
                match photo_filter_choice(&current) {
                    Some((index, amount)) => (Some(index + 1), amount),
                    None => (Some(0), 1.0),
                }
            } else {
                (None, 1.0)
            };
            let thumbs = self.filter_swatch_ids(ui.ctx(), amount);
            let radios = photo_filter_radios(thumbs);
            let edit = chrome::filter_editor(ui, rect, &radios, selected, amount, z, theme);
            if let Some(index) = edit.hovered {
                self.shape_properties.filter_aim = Some(index);
            }
            match photo_filter_gesture(selected, amount, self.shape_properties.filter_aim, edit) {
                FilterStep::Commit(adjust) => {
                    self.preview_shape_property(Property::ImageAdjust(adjust));
                }
                FilterStep::Peek(adjust) => {
                    self.rebuild_shape_preview(Some(Property::ImageAdjust(adjust)));
                }
                FilterStep::Rest => self.rebuild_shape_preview(None),
            }
            return None;
        }
        if panel == Panel::Wire {
            let NodeKind::Connector(first) = &first.kind else {
                return None;
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
                first.stroke.width * slate_doc::scene::CONNECTOR_WIDTH_SCALE,
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
                self.preview_shape_property(Property::StrokeWidth(
                    width / slate_doc::scene::CONNECTOR_WIDTH_SCALE,
                ));
            }
            return None;
        }
        if panel == Panel::Corners {
            let (chamfer, percent, amount, maximum) = self.corners_panel_reading(first);
            let crop = self
                .corners_include_crop()
                .then(|| self.board_crop.is_some());
            let edit =
                chrome::corner_editor(ui, rect, chamfer, percent, amount, maximum, crop, z, theme);
            if let Some(on) = edit.crop {
                if !on {
                    self.board_crop = None;
                } else if let Some(id) = self
                    .shape_properties
                    .ids
                    .iter()
                    .copied()
                    .find(|id| self.croppable_image(*id))
                {
                    self.enter_crop_mode(id);
                }
            }
            if edit.chamfer != chamfer {
                self.preview_shape_property(Property::CornerTreatment(edit.chamfer));
            }
            if edit.percent != percent {
                self.preview_shape_property(Property::CornerMode(edit.percent));
            }
            if let Some(amount) = edit.amount {
                self.preview_shape_property(Property::CornerAmount(amount));
            }
            return None;
        }
        if panel == Panel::Text {
            return self.shape_text_panel(ui, rect, z, theme);
        }
        if panel == Panel::Bumper {
            let on = first.bumper.is_some();
            let common = nodes.iter().all(|n| n.bumper.is_some() == on);
            let b = first.bumper.unwrap_or_default();
            let edit = chrome::bumper_editor(
                ui,
                rect,
                common.then_some(on),
                b.buffer,
                slate_doc::bumper::tokens::MAX_BUFFER,
                b.friction,
                z,
                theme,
            );
            if let Some(on) = edit.on {
                self.preview_shape_property(Property::BumperOn(on));
            }
            if let Some(v) = edit.buffer {
                self.preview_shape_property(Property::BumperBuffer(v));
            }
            if let Some(v) = edit.friction {
                self.preview_shape_property(Property::BumperFriction(v));
            }
            return None;
        }
        let get_color = |n: &Node| {
            if panel == Panel::Fill {
                if let NodeKind::Portal(p) = &n.kind {
                    if p.fill_follows_theme() {
                        return super::board::to_rgba(theme.card);
                    }
                    if p.slate_fill_follows_theme() {
                        return super::board::to_rgba(theme.card);
                    }
                }
                if let NodeKind::Frame(f) = &n.kind {
                    if f.fill_follows_theme() {
                        return super::board::to_rgba(theme.card);
                    }
                }
                scene::fill_of(n).unwrap_or(Rgba([128, 128, 128, 0]))
            } else if let NodeKind::Portal(p) = &n.kind {
                if p.stroke_follows_theme() {
                    super::board::to_rgba(theme.border_strong)
                } else {
                    scene::stroke_of(n).unwrap().color
                }
            } else if let NodeKind::Connector(c) = &n.kind {
                c.paint_color(super::board::to_rgba(theme.wire))
            } else {
                scene::stroke_of(n).unwrap().color
            }
        };
        let points = self.shape_property_points();
        let tip = (panel == Panel::Stroke)
            .then(|| picked_tip(first, &points))
            .flatten();
        let color = tip.map_or_else(|| get_color(first), |tip| tip.color);
        let mixed = tip.is_none() && nodes.iter().any(|n| get_color(n) != color);
        let width = (panel == Panel::Stroke).then(|| {
            let width = tip
                .map(|tip| tip.width)
                .unwrap_or_else(|| scene::stroke_of(first).unwrap().width);
            if matches!(&first.kind, NodeKind::Portal(p) if p.stroke_follows_theme()) {
                width.max(1.0)
            } else {
                width
            }
        });
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
        let theme_relative_fill = panel == Panel::Fill
            && matches!(
                &first.kind,
                NodeKind::Portal(p) if p.fill_follows_theme() || p.slate_fill_follows_theme()
            );
        let theme_relative_stroke = panel == Panel::Stroke
            && matches!(&first.kind, NodeKind::Portal(p) if p.stroke_follows_theme());
        let displayed_rgb = [color.0[0], color.0[1], color.0[2]];
        if let Some(rgb) = edit.rgb {
            self.preview_shape_property(if panel == Panel::Fill {
                Property::FillRgb(rgb)
            } else {
                Property::StrokeRgb(rgb)
            });
        }
        if let Some(alpha) = edit.alpha {
            if panel == Panel::Fill {
                if theme_relative_fill && edit.rgb.is_none() {
                    self.preview_shape_property(Property::FillRgb(displayed_rgb));
                }
                self.preview_shape_property(Property::FillAlpha(alpha));
            } else {
                if theme_relative_stroke && edit.rgb.is_none() {
                    self.preview_shape_property(Property::StrokeRgb(displayed_rgb));
                }
                self.preview_shape_property(Property::StrokeAlpha(alpha));
            }
        }
        if let Some(width) = edit.width {
            self.preview_shape_property(Property::StrokeWidth(width));
        }
        edit.sample
    }
}

fn text_size_label(size: f32) -> String {
    if (size - size.round()).abs() < 0.05 {
        format!("{}", size.round() as i32)
    } else {
        format!("{size:.1}")
    }
}

impl SlateApp {
    fn shape_text_panel(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        z: f32,
        theme: atlas_shell::theme::Palette,
    ) -> Option<chrome::SampleGesture> {
        let snapshot = {
            let nodes = if self.shape_properties.preview.is_empty() {
                &self.shape_properties.nodes
            } else {
                &self.shape_properties.preview
            };
            let first = nodes.first()?;
            match &first.kind {
                NodeKind::Shape(shape) if scene::shape_hosts_text(shape) => shape
                    .text
                    .clone()
                    .unwrap_or_else(|| scene::ShapeText::new(scene::shape_text_ink(shape.fill))),
                NodeKind::Text(text) => scene::ShapeText {
                    body: text.text.clone(),
                    family: text.family,
                    size: text.size,
                    color: text.color,
                    align: text.align,
                },
                _ => return None,
            }
        };
        let families: Vec<&str> = scene::Typeface::ALL
            .iter()
            .map(|face| face.label())
            .collect();
        let family = snapshot.family.index();
        let align = match snapshot.align {
            scene::TextAlign::Left => 0,
            scene::TextAlign::Center => 1,
            scene::TextAlign::Right => 2,
        };
        let size_label = text_size_label(snapshot.size);
        let size_index = chrome::TEXT_HEIGHT_PRESETS
            .iter()
            .position(|preset| (*preset - snapshot.size).abs() < 0.5)
            .unwrap_or(usize::MAX);
        let row = Rect::from_min_size(
            rect.min,
            Vec2::new(rect.width(), chrome::TEXT_ROW_HEIGHT * z),
        );
        let edit = chrome::text_format_editor(
            ui,
            rect,
            &families,
            family,
            &mut self.shape_properties.text_family_open,
            align,
            &size_label,
            size_index,
            &mut self.shape_properties.text_size_open,
            z,
            theme,
        );
        self.shape_properties.chrome_hits.extend(edit.popups);
        if let Some(index) = edit.family {
            if let Some(family) = scene::Typeface::ALL.get(index).copied() {
                self.preview_shape_property(Property::TextFamily(family));
            }
        }
        if let Some(index) = edit.align {
            let align = match index {
                0 => scene::TextAlign::Left,
                2 => scene::TextAlign::Right,
                _ => scene::TextAlign::Center,
            };
            self.preview_shape_property(Property::TextAlign(align));
        }
        if let Some(size) = edit.size {
            self.preview_shape_property(Property::TextSize(size));
        }
        let color_rect = Rect::from_min_size(
            Pos2::new(rect.left(), row.bottom() + 6.0 * z),
            Vec2::new(rect.width(), chrome::FILL_HEIGHT * z),
        );
        let recent = self.doc().view.recent_colors.clone().unwrap_or_default();
        let color_edit = chrome::color_editor(
            ui,
            color_rect,
            snapshot.color.0,
            None,
            false,
            &recent,
            &mut self.shape_properties.color,
            z,
            theme,
        );
        if let Some(rgb) = color_edit.rgb {
            self.preview_shape_property(Property::TextRgb(rgb));
        }
        if let Some(alpha) = color_edit.alpha {
            self.preview_shape_property(Property::TextAlpha(alpha));
        }
        color_edit.sample
    }

    pub(crate) fn start_property_desktop_sample(
        &mut self,
        panel: Panel,
        gesture: chrome::SampleGesture,
    ) {
        self.begin_desktop_sample(
            super::board_color::DesktopDestination::Nodes {
                ids: self.shape_properties.ids.clone(),
                panel,
                preview: true,
            },
            match gesture {
                chrome::SampleGesture::Click => atlas_shell::desktop_color::PickMode::Click,
                chrome::SampleGesture::Drag => atlas_shell::desktop_color::PickMode::Drag,
            },
        );
    }

    /// Screen rect of the selection strip's screenshot button, last painted.
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

    /// The screenshot button and what its menu must not cover: the selection
    /// and the whole strip.
    pub(crate) fn model_screenshot_anchor(&self, xf: &BoardXf) -> Option<(Rect, Rect)> {
        let chrome = self.shape_properties.last_chrome.as_ref()?;
        let button = self.model_screenshot_button()?;
        let strip = self
            .shape_properties
            .chrome_hits
            .iter()
            .take(chrome.items.len())
            .fold(button, |strip, r| strip.union(*r));
        Some((button, xf.rect_w2s(chrome.bounds).union(strip)))
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
    fn agent_cards_offer_fill_and_zero_default_stroke() {
        let mut h = board();
        h.app.place_agent_portal_at(egui::Pos2::ZERO);
        let n = h.app.doc().scene.nodes[0].clone();
        let items = live_property_strip_items(&h.app, std::slice::from_ref(&n));
        assert!(items.contains(&StripItem::Panel(Panel::Fill)));
        assert!(items.contains(&StripItem::Panel(Panel::Stroke)));
        assert_eq!(scene::stroke_of(&n).unwrap().width, 0.0);
        let mut edited = n.clone();
        scene::set_fill(&mut edited, Some(Rgba([20, 70, 100, 255])));
        scene::set_stroke(
            &mut edited,
            scene::Stroke {
                width: 2.0,
                color: Rgba([90, 120, 150, 255]),
                ..Default::default()
            },
        );
        assert!(
            slate_doc::agent_chat::agent(&edited)
                .unwrap()
                .chat
                .custom_fill
        );
        let saved = serde_json::to_string(&edited).unwrap();
        let restored: Node = serde_json::from_str(&saved).unwrap();
        assert_eq!(scene::fill_of(&restored), Some(Rgba([20, 70, 100, 255])));
        assert_eq!(scene::stroke_of(&restored).unwrap().width, 2.0);
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
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,

                text: None,
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
            Some(
                serde_json::to_string(&PropertyRequest {
                    ids,
                    edits,
                    points: Vec::new(),
                })
                .unwrap(),
            )
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
    fn shape_property_line_length_is_not_offered_and_circle_keeps_diameter() {
        let mut h = board();
        let id = h
            .app
            .commit_line(Pos2::new(0.0, 0.0), Pos2::new(3.0, 4.0))
            .unwrap();
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let request = serde_json::to_string(&DimensionRequest {
            ids: vec![id],
            kind: DimensionKind::Length,
            value: 10.0,
        })
        .unwrap();
        assert!(!h.app.shape_dimension_command(Some(&request)));
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
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
            points: Vec::new(),
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

    /// User finding (2026-09-26): open curves and brush strokes carry no
    /// dimension stringers, alone or together; closed shapes keep theirs.
    #[test]
    fn open_curves_and_brush_strokes_have_no_dimension_stringers() {
        let mut h = board();
        let last = |h: &Harness| h.app.doc().scene.nodes.last().unwrap().id;
        let line = h
            .app
            .commit_line(Pos2::new(0.0, 0.0), Pos2::new(80.0, 40.0))
            .unwrap();
        let polyline = |h: &mut Harness, pts: &[Pos2], closed: bool| {
            let (r, d) = board_path::points_to_path_data(pts, closed);
            h.app
                .commit_path_node(slate_doc::StrokeTool::Polyline, r, d, closed);
            last(h)
        };
        let open = polyline(
            &mut h,
            &[
                Pos2::new(0.0, 100.0),
                Pos2::new(80.0, 100.0),
                Pos2::new(80.0, 160.0),
            ],
            false,
        );
        h.app.set_board_tool(BoardTool::Arc);
        for p in [
            Pos2::new(200.0, 0.0),
            Pos2::new(300.0, 0.0),
            Pos2::new(250.0, 40.0),
        ] {
            h.app.path_tool_click(p);
        }
        let arc = last(&h);
        h.app.set_board_tool(BoardTool::BezierSpan);
        let (a, b) = (Pos2::new(200.0, 100.0), Pos2::new(300.0, 140.0));
        h.app.bezier_anchor_press(a);
        h.app
            .bezier_anchor_release(a, a + Vec2::new(40.0, 0.0), false);
        h.app.bezier_anchor_press(b);
        h.app.bezier_anchor_release(b, b, false);
        assert!(h.app.path_tool_try_finish());
        let bezier = last(&h);
        h.app.finish_freehand_pen(
            (0..20)
                .map(|i| Pos2::new(400.0 + i as f32 * 6.0, (i as f32 * 0.5).sin() * 20.0))
                .collect(),
        );
        let pen = last(&h);
        h.app.set_board_tool(BoardTool::Brush);
        h.app.finish_freehand_brush(
            (0..20)
                .map(|i| Pos2::new(400.0 + i as f32 * 6.0, 100.0 + i as f32 * 2.0))
                .collect(),
        );
        let brush = last(&h);
        assert!(matches!(
            &h.app.doc().scene.node(brush).unwrap().kind,
            NodeKind::Shape(s) if s.stroke.paints_as_stamp()
        ));
        let closed = polyline(
            &mut h,
            &[
                Pos2::new(0.0, 300.0),
                Pos2::new(120.0, 300.0),
                Pos2::new(0.0, 390.0),
            ],
            true,
        );
        let rect = rectangle(&mut h, WorldRect::new(300.0, 300.0, 80.0, 60.0), 0.0);
        let node = |id: NodeId| h.app.doc().scene.node(id).unwrap().clone();
        let open_ids = [line, open, arc, bezier, pen, brush];
        for id in open_ids {
            let (_, dims) = dimensions(&[node(id)]);
            assert!(dims.is_empty(), "{id:?} has {} stringers", dims.len());
        }
        let together: Vec<Node> = open_ids.iter().map(|id| node(*id)).collect();
        assert!(dimensions(&together).1.is_empty(), "together, still none");
        for id in [closed, rect] {
            assert_eq!(dimensions(&[node(id)]).1.len(), 2, "closed shapes keep W/H");
        }
    }

    #[test]
    fn shape_property_eyedropper_click_and_drag_open_their_sampling_modes() {
        use atlas_shell::desktop_color::PickMode;
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
        h.frame();
        h.frame();
        let r = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let fill = Pos2::new(r.center().x - 37.0, r.top() - 29.0);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(fill)));
        h.frame_with(|i| i.events.push(button(fill, true)));
        h.frame_with(|i| i.events.push(button(fill, false)));
        h.frame();
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
        let editor = *h.app.shape_properties.chrome_hits.last().unwrap();
        let z = h.app.tab().cam.z;
        let dropper = Pos2::new(editor.left() + 21.0 * z, editor.bottom() - 20.0 * z);

        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(dropper)));
        h.frame_with(|i| i.events.push(button(dropper, true)));
        h.frame_with(|i| i.events.push(button(dropper, false)));
        assert_eq!(h.app.desktop_sample_requests, vec![PickMode::Click]);

        h.frame_with(|i| i.events.push(button(dropper, true)));
        for step in [30.0, 120.0, 260.0] {
            let at = dropper + Vec2::new(step, -step * 0.5);
            h.frame_with(|i| i.events.push(egui::Event::PointerMoved(at)));
        }
        let release = dropper + Vec2::new(260.0, -130.0);
        h.frame_with(|i| i.events.push(button(release, false)));
        h.frame();
        assert_eq!(
            h.app.desktop_sample_requests,
            vec![PickMode::Click, PickMode::Drag],
            "press-drag opens one drag session; its release is not a second click"
        );
    }

    /// User finding (2026-09-26): the editor avoided the shape but painted
    /// behind its dimension stringer. Popups draw above all canvas chrome and
    /// take the pointer first where the two overlap.
    #[test]
    fn a_property_editor_over_a_stringer_paints_and_hits_above_it() {
        let mut h = board();
        h.frame();
        // The rectangle's top sits just under the canvas top, so the editor has
        // no room above the strip and opens below, over the width stringer.
        let canvas = h.app.canvas_rect;
        let top = h
            .app
            .board_xf()
            .s2w(Pos2::new(canvas.center().x, canvas.top() + 70.0));
        rectangle(
            &mut h,
            WorldRect::new(top.x - 150.0, top.y, 300.0, 120.0),
            0.0,
        );
        h.frame();
        h.app.shape_properties.panel = Some(Panel::Fill);
        h.frame();
        h.frame();
        let out = h.frame_output(|_| {});
        let editor = *h.app.shape_properties.chrome_hits.last().unwrap();
        let xf = h.app.board_xf();
        let width = h
            .app
            .shape_properties
            .dimensions
            .iter()
            .find(|d| d.kind == DimensionKind::Width)
            .expect("a rectangle has a width stringer");
        let [a, b] = width.ends.map(|p| xf.w2s(p) + width.offset * xf.z);
        let label = a.lerp(b, 0.5);
        assert!(
            editor.contains(label),
            "precondition: {editor:?} covers the width stringer at {label:?}"
        );

        let layer = h.ctx.layer_id_at(label).expect("hit-testable");
        assert_eq!(
            layer.id,
            Id::new("shape_property_editor"),
            "the editor takes the pointer first: {layer:?}"
        );

        let baseline = egui::Stroke::new(0.7 * xf.z, h.app.palette().select);
        let last_stringer = out
            .shapes
            .iter()
            .rposition(|c| {
                matches!(&c.shape, egui::Shape::LineSegment { points, stroke }
                    if *stroke == baseline && points.iter().all(|p| (p.y - a.y).abs() < 0.5))
            })
            .expect("the width stringer painted");
        let panel = out
            .shapes
            .iter()
            .position(|c| matches!(&c.shape, egui::Shape::Rect(r) if r.rect == editor))
            .expect("the editor panel painted");
        assert!(
            last_stringer < panel,
            "the stringer (shape {last_stringer}) paints over the editor (shape {panel})"
        );
        assert!(
            layer.order > egui::Order::Foreground,
            "canvas chrome paints on Foreground, so a popup sits above it: {layer:?}"
        );
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
    fn rotated_frame_stringers_stay_below_and_beside_never_on_top() {
        let mut h = board();
        let id = frame_node(&mut h, WorldRect::new(0.0, 0.0, 612.0, 792.0));
        for rotation in [0.0, 90.0, -90.0, 180.0, 270.0, 30.0, -30.0, 60.0, -120.0] {
            h.app.patch_nodes(&[id], |n| n.rotation_deg = rotation);
            let n = h.app.doc().scene.node(id).unwrap();
            let center = Pos2::new(n.rect.center().0, n.rect.center().1);
            let (_, dims) = dimensions(std::slice::from_ref(n));
            assert_eq!(dims[0].kind, DimensionKind::Width, "{rotation}°");
            assert!((dims[0].value - 612.0).abs() < 0.01, "{rotation}°");
            assert_eq!(dims[1].kind, DimensionKind::Height, "{rotation}°");
            assert!((dims[1].value - 792.0).abs() < 0.01, "{rotation}°");
            for d in &dims {
                let span = d.ends[1] - d.ends[0];
                assert!(
                    (span.length() - d.value).abs() < 0.01,
                    "{rotation}° measures its own axis"
                );
                let midpoint = d.ends[0].lerp(d.ends[1], 0.5);
                assert!(
                    (midpoint - center).dot(d.offset) > 0.0,
                    "{rotation}° exterior"
                );
                let lane = if span.x.abs() >= span.y.abs() {
                    Vec2::DOWN
                } else {
                    Vec2::RIGHT
                };
                assert!(
                    d.offset.normalized().dot(lane) > 0.7,
                    "{rotation}° {:?}",
                    d.kind
                );
            }
        }
        // Portrait turned to landscape: the long side's stringer sits below.
        h.app.patch_nodes(&[id], |n| n.rotation_deg = -90.0);
        let (_, dims) = dimensions(std::slice::from_ref(h.app.doc().scene.node(id).unwrap()));
        assert!(dims[1].offset.y > 0.99, "height runs along the bottom");
        assert!(dims[0].offset.x > 0.99, "width stands on the right");
    }

    /// The screenshot case: with a 223.57 × 289.326 portrait frame turned to
    /// landscape, the 289.326 stringer was drawn along the top, through the
    /// squircle strip. No stringer baseline may rise above the frame's visual
    /// top edge, and no stringer's footprint (baseline, witness lines, ticks
    /// and its 12 u label) may meet the strip rect its owner lays out.
    #[test]
    fn landscape_turned_frame_stringers_clear_the_property_strip() {
        let mut h = board();
        for (i, (w, h_)) in [(223.57, 289.326), (612.0, 792.0)].into_iter().enumerate() {
            let id = frame_node(
                &mut h,
                WorldRect::new(40.0 + 2000.0 * i as f32, 60.0, w, h_),
            );
            for rotation in [0.0, 90.0, -90.0, 180.0, 270.0] {
                h.app.patch_nodes(&[id], |n| n.rotation_deg = rotation);
                let node = h.app.doc().scene.node(id).unwrap().clone();
                let (bounds, dims) = dimensions(std::slice::from_ref(&node));
                let bounds = bounds.unwrap();
                let items = live_property_strip_items(&h.app, std::slice::from_ref(&node));
                assert_eq!(items.len(), 7, "fill, stroke, corners, <, >, present, deck");
                let strip = chrome::strip_rect(
                    Pos2::new(bounds.x + bounds.w * 0.5, bounds.y),
                    items.len(),
                    1.0,
                    1.0,
                );
                assert_eq!(dims.len(), 2);
                for d in &dims {
                    let a = d.ends[0] + d.offset;
                    let b = d.ends[1] + d.offset;
                    let label = format!("{w}×{h_} at {rotation}° {:?}", d.kind);
                    assert!(
                        a.y >= bounds.y - 0.01 && b.y >= bounds.y - 0.01,
                        "{label} baseline above the frame"
                    );
                    let out = d.offset.normalized() * 4.0;
                    let footprint =
                        Rect::from_points(&[a, b, d.ends[0] + out, d.ends[1] + out]).expand(9.0);
                    assert!(!footprint.intersects(strip), "{label} meets the strip");
                }
                let visual_w = bounds.w;
                let across = dims
                    .iter()
                    .find(|d| (d.value - visual_w).abs() < 0.01)
                    .expect("one stringer measures the visual width");
                assert!(
                    across.offset.y > 0.99,
                    "{w}×{h_} at {rotation}° width is below"
                );
                let tall = dims.iter().find(|d| !std::ptr::eq(*d, across)).unwrap();
                assert!(
                    tall.offset.x > 0.99,
                    "{w}×{h_} at {rotation}° height is right"
                );
            }
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
        let colors = h.app.doc().view.recent_colors.as_ref().unwrap();
        assert_eq!(colors.last().copied(), Some([4, 5, 6]));
        assert_ne!(colors[0], [4, 5, 6]);
        h.app.board_undo();
        assert_eq!(
            h.app
                .doc()
                .view
                .recent_colors
                .as_ref()
                .unwrap()
                .last()
                .copied(),
            Some([4, 5, 6])
        );
        apply(&mut h, vec![id], vec![Property::FillRgb([4, 5, 6])]);
        let colors = h.app.doc().view.recent_colors.as_ref().unwrap();
        assert_eq!(colors[0], [4, 5, 6]);
        assert_eq!(colors.iter().filter(|c| **c == [4, 5, 6]).count(), 1);
    }

    #[test]
    fn shape_property_color_scrub_commits_one_group_and_one_recent_color() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(0.0, 0.0, 180.0, 120.0), 0.0);
        h.app.sync_shape_properties();
        h.app.shape_properties.panel = Some(Panel::Fill);
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let scrubbed = [[200, 40, 40], [180, 90, 40], [120, 120, 120]];
        for rgb in scrubbed.into_iter().chain([[160, 60, 200]]) {
            h.app.preview_shape_property(Property::FillRgb(rgb));
        }
        h.app.preview_shape_property(Property::FillAlpha(128));
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
        h.app.apply_shape_preview(&h.ctx, true);
        assert_eq!(
            scene::fill_of(h.app.doc().scene.node(id).unwrap()),
            Some(Rgba([160, 60, 200, 128]))
        );
        let colors = h.app.doc().view.recent_colors.clone().unwrap();
        assert!(colors.contains(&[160, 60, 200]));
        assert!(scrubbed.iter().all(|rgb| !colors.contains(rgb)));
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    }

    #[test]
    fn an_agent_picture_offers_photo_filters_picked_or_not() {
        let (mut h, generator) =
            super::super::board_flow::tests::generator_with_output("agent_picture_filters");
        for picked in [false, true] {
            if picked {
                h.app.pick_agent_result(generator, 0);
            }
            let nodes = vec![h.app.doc().scene.node(generator).unwrap().clone()];
            let items = live_property_strip_items(&h.app, &nodes);
            assert!(
                items.contains(&StripItem::Panel(Panel::Filter)),
                "picked {picked}: {items:?}"
            );
            assert!(
                items.contains(&StripItem::Panel(Panel::Agent)),
                "the Agent squircle stays: {items:?}"
            );
        }
    }

    fn frame_node(h: &mut Harness, rect: WorldRect) -> NodeId {
        let node = h.app.doc_mut().scene.build_node(
            rect,
            NodeKind::Frame(scene::FrameNode {
                title: "Slide".into(),
                order: 0,
                fill: Rgba([20, 30, 40, 255]),
                fill_authored: false,
                assignments: Default::default(),
                stroke: scene::Stroke::none(),
                corner: scene::Corner::Square,
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
                stroke: Default::default(),
                agent: None,
            }),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    /// Screen rect of a strip squircle, as the last frame laid it out.
    fn strip_button(h: &Harness, panel: Panel) -> Rect {
        let items = &h.app.shape_properties.last_chrome.as_ref().unwrap().items;
        let index = items
            .iter()
            .position(|item| *item == StripItem::Panel(panel))
            .unwrap_or_else(|| panel_missing(panel, items));
        h.app.shape_properties.chrome_hits[index]
    }

    fn panel_missing(panel: Panel, items: &[StripItem]) -> usize {
        panic!("{panel:?} is not on the strip: {items:?}")
    }

    /// One frame at an explicit clock, so selection fades run at 60 Hz.
    fn timed_frame(h: &mut Harness, time: &mut f64, events: Vec<egui::Event>) -> egui::FullOutput {
        *time += 1.0 / 60.0;
        h.ctx.clone().run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ctx| h.app.update_app(ctx),
        )
    }

    fn primary(p: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// Open the selected node's Text editor with a real click on its squircle.
    fn open_text_editor(h: &mut Harness, time: &mut f64) -> Rect {
        for _ in 0..3 {
            timed_frame(h, time, vec![]);
        }
        let button = strip_button(h, Panel::Text);
        let p = button.center();
        timed_frame(h, time, vec![egui::Event::PointerMoved(p)]);
        timed_frame(h, time, vec![primary(p, true)]);
        timed_frame(h, time, vec![primary(p, false)]);
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Text));
        button
    }

    /// User, 26 September 2026: the blue outline around selected text stays
    /// while the person works in the strip and its Text editor. Fill still
    /// fades it, so an authored fill previews without the tint.
    #[test]
    fn text_selection_outline_stays_while_navigating_the_text_editor() {
        for dark in [false, true] {
            let mut h = board();
            h.app.dark_mode = dark;
            let id = text_node(&mut h, WorldRect::new(-120.0, -30.0, 240.0, 60.0));
            let mut time = h.ctx.input(|i| i.time);
            let theme = h.app.palette();
            let tint = theme
                .select
                .gamma_multiply(atlas_shell::tokens::current().board_preview.select_opacity * 0.16);
            let silhouettes = |out: &egui::FullOutput| {
                out.shapes
                    .iter()
                    .filter(|s| matches!(&s.shape, egui::Shape::Path(p) if p.fill == tint))
                    .count()
            };
            for _ in 0..3 {
                timed_frame(&mut h, &mut time, vec![]);
            }
            let rest = silhouettes(&timed_frame(&mut h, &mut time, vec![]));
            assert!(rest > 0, "selected text shows its outline");
            let button = open_text_editor(&mut h, &mut time);
            let editor = *h.app.shape_properties.chrome_hits.last().unwrap();
            for hover in [button.center(), editor.center(), button.center()] {
                for _ in 0..12 {
                    timed_frame(&mut h, &mut time, vec![egui::Event::PointerMoved(hover)]);
                }
                let out = timed_frame(&mut h, &mut time, vec![egui::Event::PointerMoved(hover)]);
                assert_eq!(silhouettes(&out), rest, "dark={dark} hover={hover:?}");
                assert_eq!(h.app.shape_properties.panel, Some(Panel::Text));
            }
            assert!(h.app.board_sel.contains(&id));
            h.app.shape_properties.panel = Some(Panel::Fill);
            for _ in 0..12 {
                timed_frame(&mut h, &mut time, vec![]);
            }
            assert_eq!(
                silhouettes(&timed_frame(&mut h, &mut time, vec![])),
                0,
                "Fill still fades the selection"
            );
        }
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

    #[test]
    fn default_portal_corner_panel_displays_resolved_radius() {
        let mut h = board();
        let id = portal_node(&mut h, WorldRect::new(0.0, 0.0, 320.0, 240.0));
        let node = h.app.doc().scene.node(id).unwrap();
        let (chamfer, percent, amount) = h.app.corner_for_editor(node).parameters();
        assert!(!chamfer && !percent);
        assert!((amount - slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET).abs() < 1e-4);
    }

    #[test]
    fn default_portal_corner_switches_keep_resolved_radius() {
        let mut h = board();
        let id = portal_node(&mut h, WorldRect::new(0.0, 0.0, 320.0, 240.0));
        let expected = slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET;

        apply(&mut h, vec![id], vec![Property::CornerTreatment(true)]);
        let node = h.app.doc().scene.node(id).unwrap();
        assert!(matches!(
            scene::corner_of(node),
            Some(Corner::Chamfer { .. })
        ));
        assert!((scene::resolved_corner_effective(node, None).1 - expected).abs() < 1e-4);

        apply(&mut h, vec![id], vec![Property::CornerMode(true)]);
        let node = h.app.doc().scene.node(id).unwrap();
        assert!(matches!(
            scene::corner_of(node),
            Some(Corner::ChamferPercent { .. })
        ));
        assert!((scene::resolved_corner_effective(node, None).1 - expected).abs() < 1e-4);
    }

    #[test]
    fn a_property_editor_clipped_by_the_canvas_top_opens_below_the_selection() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
        h.frame();
        let canvas = h.app.canvas_rect;
        h.app.tab_mut().cam.offset.y = -60.0 - (canvas.top() + 70.0 - canvas.center().y);
        h.app.shape_properties.panel = Some(Panel::Fill);
        for _ in 0..3 {
            h.frame();
        }
        let selection = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let editor = h
            .ctx
            .memory(|m| m.area_rect(Id::new("shape_property_editor")))
            .expect("the Fill editor is open");
        assert!(canvas.contains_rect(editor), "{editor:?} in {canvas:?}");
        assert!(
            !editor.intersects(selection),
            "{editor:?} over {selection:?}"
        );
        assert!(editor.top() > selection.bottom());
    }

    /// Painted text and where it sits, so a list's scroll can be measured.
    fn painted_at(out: &egui::FullOutput, label: &str) -> Option<Pos2> {
        fn walk(shape: &egui::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos),
                egui::Shape::Vec(v) => v.iter().find_map(|s| walk(s, label)),
                _ => None,
            }
        }
        out.shapes.iter().find_map(|c| walk(&c.shape, label))
    }

    fn wheel(h: &mut Harness, at: Pos2) -> egui::FullOutput {
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerMoved(at));
            i.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -80.0),
                modifiers: egui::Modifiers::NONE,
            });
        });
        for _ in 0..8 {
            h.frame();
        }
        h.frame_output(|_| {})
    }

    #[test]
    fn the_wheel_over_an_open_typeface_list_scrolls_it_and_leaves_the_camera() {
        let mut h = board();
        text_node(&mut h, WorldRect::new(-100.0, 40.0, 200.0, 60.0));
        h.frame();
        h.app.shape_properties.panel = Some(Panel::Text);
        h.app.shape_properties.text_family_open = true;
        h.frame();
        h.frame();
        let open = h.frame_output(|_| {});
        let row = painted_at(&open, "Serif").expect("the typeface list is open");
        let z = h.app.tab().cam.z;
        let after = wheel(&mut h, row);
        assert_eq!(
            h.app.tab().cam.z,
            z,
            "the board did not zoom under the list"
        );
        let moved = painted_at(&after, "Serif").expect("the list is still open");
        assert!(
            moved.y < row.y - 1.0,
            "the list scrolled: {row:?} -> {moved:?}"
        );

        let empty = Pos2::new(1300.0, 700.0);
        wheel(&mut h, empty);
        assert_ne!(h.app.tab().cam.z, z, "the empty board still zooms");
    }

    /// A text box takes the shared Stroke editor like other geometry. The
    /// border is authored (one undo) and painted around the box.
    #[test]
    fn text_box_takes_the_shared_stroke_editor_and_paints_its_border() {
        let mut h = board();
        let id = text_node(&mut h, WorldRect::new(-120.0, -30.0, 240.0, 60.0));
        let node = h.app.doc().scene.node(id).unwrap().clone();
        assert!(
            live_property_strip_items(&h.app, &[node]).contains(&StripItem::Panel(Panel::Stroke))
        );
        let before = h.app.tab().journal.undo_depth();
        apply(
            &mut h,
            vec![id],
            vec![
                Property::StrokeRgb([200, 30, 30]),
                Property::StrokeAlpha(255),
                Property::StrokeWidth(3.0),
            ],
        );
        assert_eq!(h.app.tab().journal.undo_depth(), before + 1);
        let stroke =
            scene::stroke_of(h.app.doc().scene.node(id).unwrap()).expect("a text box has a stroke");
        assert_eq!(stroke.color, Rgba([200, 30, 30, 255]));
        assert_eq!(stroke.width, 3.0);
        h.app.board_sel.clear();
        h.frame();
        let screen = h
            .app
            .board_xf()
            .rect_w2s(WorldRect::new(-120.0, -30.0, 240.0, 60.0));
        let red = egui::epaint::ColorMode::Solid(egui::Color32::from_rgb(200, 30, 30));
        let border = |out: &egui::FullOutput| {
            out.shapes.iter().any(|s| match &s.shape {
                egui::Shape::Path(p) => {
                    p.closed
                        && p.stroke.color == red
                        && p.points.iter().all(|q| screen.expand(1.0).contains(*q))
                        && p.points.iter().any(|q| (q.x - screen.left()).abs() < 1.0)
                }
                _ => false,
            })
        };
        assert!(
            border(&h.frame_output(|_| {})),
            "the board paints the border"
        );
        h.app.board_undo();
        assert!(scene::stroke_of(h.app.doc().scene.node(id).unwrap()).is_none_or(|s| s.is_none()));
        assert!(!border(&h.frame_output(|_| {})));
    }

    fn pointer(h: &mut Harness, p: Pos2, pressed: Option<bool>) {
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerMoved(p));
            if let Some(pressed) = pressed {
                i.events.push(egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        });
    }

    fn key(h: &mut Harness, key: egui::Key) {
        h.frame_with(|i| {
            i.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }

    /// The typeface list scrolls under the wheel instead of zooming the board.
    #[test]
    fn wheel_over_the_typeface_list_does_not_zoom_the_board() {
        let mut h = board();
        text_node(&mut h, WorldRect::new(-120.0, -30.0, 240.0, 60.0));
        let mut time = h.ctx.input(|i| i.time);
        open_text_editor(&mut h, &mut time);
        timed_frame(&mut h, &mut time, vec![]);
        let editor = *h.app.shape_properties.chrome_hits.last().unwrap();
        let z = h.app.board_xf().z;
        let family = editor.min + Vec2::new(24.0, chrome::TEXT_ROW_HEIGHT * 0.5) * z;
        timed_frame(&mut h, &mut time, vec![egui::Event::PointerMoved(family)]);
        timed_frame(&mut h, &mut time, vec![primary(family, true)]);
        timed_frame(&mut h, &mut time, vec![primary(family, false)]);
        assert!(
            h.app.shape_properties.text_family_open,
            "typeface list open"
        );
        timed_frame(&mut h, &mut time, vec![]);
        let strip_len = h
            .app
            .shape_properties
            .last_chrome
            .as_ref()
            .unwrap()
            .items
            .len();
        let list = h.app.shape_properties.chrome_hits[strip_len];
        assert!(list.top() > editor.top() && list.height() > 100.0 * z);
        let before = h.app.tab().cam.z;
        let over = list.center();
        for _ in 0..6 {
            timed_frame(
                &mut h,
                &mut time,
                vec![
                    egui::Event::PointerMoved(over),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, -80.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(h.app.tab().cam.z, before, "the board must not zoom");
        assert!(h.app.shape_properties.text_family_open, "list stays open");
    }

    fn corner_grip(h: &Harness, id: NodeId) -> Pos2 {
        let xf = h.app.board_xf();
        let node = h.app.doc().scene.node(id).unwrap();
        h.app
            .fillet_grip_at(node, &xf)
            .expect("visible corner grip")
    }

    fn corner_amount(h: &Harness, id: NodeId) -> f32 {
        let node = h.app.doc().scene.node(id).unwrap();
        h.app
            .node_resolved_corner(node)
            .effective(node.rect.w, node.rect.h)
            .1
    }

    fn polygon(h: &mut Harness, rect: WorldRect, corner: Corner) -> NodeId {
        let node = h.app.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(scene::ShapeNode {
                shape: ShapeKind::RegularPolygon,
                fill: Some(Rgba([10, 20, 30, 255])),
                stroke: scene::Stroke::default(),
                corner,
                sides: 6,
                phase_deg: 0.0,
                flip: false,
                path: None,
                text: None,
            }),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    fn polyline(h: &mut Harness, rect: WorldRect) -> NodeId {
        let path = scene::PathData {
            start: [0.0, 0.0],
            segs: vec![
                scene::PathSeg::Line { to: [1.0, 0.0] },
                scene::PathSeg::Line { to: [1.0, 1.0] },
            ],
            closed: false,
            ..Default::default()
        };
        let node = h.app.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(scene::ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: scene::Stroke::default(),
                corner: Corner::Square,
                sides: scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(std::sync::Arc::new(path)),
                text: None,
            }),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    #[test]
    fn corner_grip_rides_the_top_edge_at_the_fillet_tangent_point() {
        for angle in [0.0, 30.0] {
            let mut h = board();
            let rect = WorldRect::new(-90.0, -60.0, 180.0, 120.0);
            let id = rectangle(&mut h, rect, angle);
            h.frame();
            let xf = h.app.board_xf();
            let at = |local: [f32; 2]| {
                let [x, y] = rect.rotate_point(local, angle);
                xf.w2s(Pos2::new(x, y))
            };
            let inset = super::super::board_handles::FILLET_GRIP_MIN_INSET_WORLD;
            assert!(
                corner_grip(&h, id).distance(at([-90.0 + inset, -60.0])) < 0.01,
                "square corner: grip inset along the top edge ({angle} deg)"
            );
            h.app.patch_nodes(&[id], |n| {
                scene::set_corner(n, Corner::Rounded { radius: 24.0 })
            });
            assert!(
                corner_grip(&h, id).distance(at([-66.0, -60.0])) < 0.01,
                "the grip sits where the fillet meets the top edge ({angle} deg)"
            );
            h.app.patch_nodes(&[id], |n| {
                scene::set_corner(n, Corner::Chamfer { cut: 30.0 })
            });
            assert!(
                corner_grip(&h, id).distance(at([-60.0, -60.0])) < 0.01,
                "a chamfer's grip sits at the cut ({angle} deg)"
            );
        }
    }

    #[test]
    fn corner_grip_tracks_the_pointer_from_the_first_pixel() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-90.0, -60.0, 180.0, 120.0), 0.0);
        h.app.patch_nodes(&[id], |n| {
            scene::set_corner(n, Corner::Rounded { radius: 24.0 })
        });
        h.frame();
        let g0 = corner_grip(&h, id);
        pointer(&mut h, g0, None);
        pointer(&mut h, g0, Some(true));
        let g1 = g0 + Vec2::new(1.0, 0.0);
        pointer(&mut h, g1, None);
        assert!(
            corner_grip(&h, id).distance(g1) < 0.01,
            "1 px of travel moves the grip 1 px: {:?} vs {g1:?}",
            corner_grip(&h, id)
        );
        assert!((corner_amount(&h, id) - 25.0).abs() < 0.01);
        pointer(&mut h, g0 + Vec2::new(10.0, 7.0), None);
        assert!(
            corner_grip(&h, id).distance(g0 + Vec2::new(10.0, 0.0)) < 0.01,
            "off the edge, the grip stays on the edge under the pointer"
        );
        assert!((corner_amount(&h, id) - 34.0).abs() < 0.01);
        let g3 = g0 + Vec2::new(-20.0, 0.0);
        pointer(&mut h, g3, None);
        assert!(
            corner_grip(&h, id).distance(g3) < 0.01,
            "below the inset the held grip is still under the pointer"
        );
        assert!(
            (corner_amount(&h, id) - 4.0).abs() < 0.01,
            "toward the corner decreases the radius"
        );
        pointer(&mut h, g3, Some(false));
        assert!((corner_amount(&h, id) - 4.0).abs() < 0.01);
        assert_eq!(h.app.board_sel, [id].into_iter().collect());
        h.app.board_undo();
        assert!(
            (corner_amount(&h, id) - 24.0).abs() < 0.01,
            "the drag is one undo step"
        );
    }

    #[test]
    fn corner_grip_grab_from_square_moves_one_unit_per_pixel_without_a_jump() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-90.0, -60.0, 180.0, 120.0), 0.0);
        h.frame();
        let g0 = corner_grip(&h, id);
        pointer(&mut h, g0, None);
        pointer(&mut h, g0, Some(true));
        assert!(
            corner_amount(&h, id).abs() < 1e-4,
            "the press leaves the corner square"
        );
        let g1 = g0 + Vec2::new(1.0, 0.0);
        pointer(&mut h, g1, None);
        assert!(
            (corner_amount(&h, id) - 1.0).abs() < 0.01,
            "the first pixel changes the radius by one unit, got {}",
            corner_amount(&h, id)
        );
        assert!(
            corner_grip(&h, id).distance(g1) < 0.01,
            "the held grip is under the pointer even below the inset"
        );
        let g15 = g0 + Vec2::new(15.0, 0.0);
        pointer(&mut h, g15, None);
        assert!((corner_amount(&h, id) - 15.0).abs() < 0.01);
        assert!(corner_grip(&h, id).distance(g15) < 0.01);
        pointer(&mut h, g15, Some(false));
        assert!((corner_amount(&h, id) - 15.0).abs() < 0.01);
        let inset = super::super::board_handles::FILLET_GRIP_MIN_INSET_WORLD;
        let xf = h.app.board_xf();
        assert!(
            corner_grip(&h, id).distance(xf.w2s(Pos2::new(-90.0 + 15.0_f32.max(inset), -60.0)))
                < 0.01,
            "released, the grip rests at max(radius, inset)"
        );
        h.app.board_undo();
        assert!(corner_amount(&h, id).abs() < 1e-4, "one undo step");
    }

    #[test]
    fn corner_grip_grab_from_25_moves_one_unit_and_stays_under_the_pointer() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-90.0, -60.0, 180.0, 120.0), 0.0);
        h.app.patch_nodes(&[id], |n| {
            scene::set_corner(n, Corner::Rounded { radius: 25.0 })
        });
        h.frame();
        let g0 = corner_grip(&h, id);
        pointer(&mut h, g0, None);
        pointer(&mut h, g0, Some(true));
        assert!((corner_amount(&h, id) - 25.0).abs() < 1e-4);
        let g1 = g0 + Vec2::new(1.0, 0.0);
        pointer(&mut h, g1, None);
        assert!(
            (corner_amount(&h, id) - 26.0).abs() < 0.01,
            "got {}",
            corner_amount(&h, id)
        );
        assert!(
            corner_grip(&h, id).distance(g1) < 0.01,
            "at or above the inset the grip is exactly under the pointer"
        );
        // Past the click threshold so the release commits.
        let g6 = g0 + Vec2::new(6.0, 0.0);
        pointer(&mut h, g6, None);
        assert!((corner_amount(&h, id) - 31.0).abs() < 0.01);
        pointer(&mut h, g6, Some(false));
        assert!(
            corner_grip(&h, id).distance(g6) < 0.01,
            "above the inset the release does not move the grip"
        );
    }

    #[test]
    fn corner_grip_dragged_past_the_corner_clamps_to_square_then_rests_at_the_inset() {
        let mut h = board();
        let rect = WorldRect::new(-90.0, -60.0, 180.0, 120.0);
        let id = rectangle(&mut h, rect, 0.0);
        h.app.patch_nodes(&[id], |n| {
            scene::set_corner(n, Corner::Rounded { radius: 25.0 })
        });
        h.frame();
        let g0 = corner_grip(&h, id);
        pointer(&mut h, g0, None);
        pointer(&mut h, g0, Some(true));
        let past = g0 + Vec2::new(-60.0, 5.0);
        pointer(&mut h, past, None);
        assert!(
            corner_amount(&h, id).abs() < 1e-4,
            "past the corner the radius clamps to 0, got {}",
            corner_amount(&h, id)
        );
        let inset = super::super::board_handles::FILLET_GRIP_MIN_INSET_WORLD;
        let xf = h.app.board_xf();
        assert!(
            corner_grip(&h, id).distance(xf.w2s(Pos2::new(rect.x, rect.y))) < 0.01,
            "held past the corner, the grip clamps to the edge start"
        );
        pointer(&mut h, past, Some(false));
        assert!(corner_amount(&h, id).abs() < 1e-4);
        assert!(
            corner_grip(&h, id).distance(xf.w2s(Pos2::new(rect.x + inset, rect.y))) < 0.01,
            "released, a square corner's grip rests at the inset"
        );
        h.app.board_undo();
        assert!((corner_amount(&h, id) - 25.0).abs() < 1e-4);
    }

    #[test]
    fn corner_grip_click_types_a_radius_enter_commits_esc_cancels() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-90.0, -60.0, 180.0, 120.0), 0.0);
        h.app.patch_nodes(&[id], |n| {
            scene::set_corner(n, Corner::Rounded { radius: 24.0 })
        });
        h.frame();
        let click = |h: &mut Harness| {
            let g = corner_grip(h, id);
            pointer(h, g, None);
            pointer(h, g, Some(true));
            pointer(h, g, Some(false));
        };
        click(&mut h);
        assert!(
            (corner_amount(&h, id) - 24.0).abs() < 1e-4,
            "a click does not change the radius"
        );
        assert_eq!(h.app.board_sel, [id].into_iter().collect());
        h.frame_with(|i| i.events.push(egui::Event::Text("40".into())));
        key(&mut h, egui::Key::Enter);
        assert!(
            (corner_amount(&h, id) - 40.0).abs() < 1e-4,
            "Enter commits the typed radius, got {}",
            corner_amount(&h, id)
        );
        assert_eq!(h.app.board_sel, [id].into_iter().collect());

        h.frame();
        click(&mut h);
        h.frame_with(|i| i.events.push(egui::Event::Text("7".into())));
        key(&mut h, egui::Key::Escape);
        h.frame();
        assert!(
            (corner_amount(&h, id) - 40.0).abs() < 1e-4,
            "Esc leaves the radius alone"
        );
        assert_eq!(
            h.app.board_sel,
            [id].into_iter().collect(),
            "Esc cancels the entry, not the selection"
        );

        h.app.board_undo();
        assert!((corner_amount(&h, id) - 24.0).abs() < 1e-4);
        h.app.board_undo();
        assert!(
            corner_amount(&h, id).abs() < 1e-4,
            "the typed radius added exactly one undo step"
        );
    }

    #[test]
    fn corner_grip_typed_radius_clamps_to_the_host() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-90.0, -60.0, 180.0, 120.0), 0.0);
        h.frame();
        let g = corner_grip(&h, id);
        pointer(&mut h, g, None);
        pointer(&mut h, g, Some(true));
        pointer(&mut h, g, Some(false));
        h.frame_with(|i| i.events.push(egui::Event::Text("500".into())));
        key(&mut h, egui::Key::Enter);
        assert!((corner_amount(&h, id) - 60.0).abs() < 1e-4);
    }

    fn three_corner_hosts(h: &mut Harness) -> [NodeId; 3] {
        let big = rectangle(h, WorldRect::new(-200.0, -60.0, 180.0, 120.0), 0.0);
        let small = rectangle(h, WorldRect::new(40.0, -20.0, 40.0, 30.0), 0.0);
        let hex = polygon(
            h,
            WorldRect::new(120.0, -50.0, 100.0, 100.0),
            Corner::Square,
        );
        h.frame();
        assert_eq!(h.app.board_sel.len(), 3);
        [big, small, hex]
    }

    #[test]
    fn corner_grip_shows_on_every_selected_host_and_one_drag_sets_them_all() {
        let mut h = board();
        let [big, small, hex] = three_corner_hosts(&mut h);
        let xf = h.app.board_xf();
        for id in [big, small, hex] {
            let node = h.app.doc().scene.node(id).unwrap();
            assert!(
                h.app.fillet_grip_at(node, &xf).is_some(),
                "every selected host shows its grip"
            );
        }
        let g0 = corner_grip(&h, big);
        pointer(&mut h, g0, None);
        pointer(&mut h, g0, Some(true));
        let g10 = g0 + Vec2::new(10.0, 0.0);
        pointer(&mut h, g10, None);
        for id in [big, small, hex] {
            assert!(
                (corner_amount(&h, id) - 10.0).abs() < 0.01,
                "the drag is live on every host, got {}",
                corner_amount(&h, id)
            );
        }
        let g30 = g0 + Vec2::new(30.0, 0.0);
        pointer(&mut h, g30, None);
        pointer(&mut h, g30, Some(false));
        assert!((corner_amount(&h, big) - 30.0).abs() < 0.01);
        assert!(
            (corner_amount(&h, small) - 15.0).abs() < 0.01,
            "a smaller host clamps to half its short side, got {}",
            corner_amount(&h, small)
        );
        assert!((corner_amount(&h, hex) - 30.0).abs() < 0.01);
        assert_eq!(
            h.app.board_sel,
            [big, small, hex].into_iter().collect(),
            "the selection is kept"
        );
        h.app.board_undo();
        for id in [big, small, hex] {
            assert!(
                corner_amount(&h, id).abs() < 1e-4,
                "one undo step restores every host"
            );
        }
    }

    #[test]
    fn corner_grip_multi_esc_restores_all_and_typed_amount_applies_to_all() {
        let mut h = board();
        let [big, small, hex] = three_corner_hosts(&mut h);
        let g0 = corner_grip(&h, small);
        pointer(&mut h, g0, None);
        pointer(&mut h, g0, Some(true));
        pointer(&mut h, g0 + Vec2::new(5.0, 0.0), None);
        assert!((corner_amount(&h, big) - 5.0).abs() < 0.01);
        key(&mut h, egui::Key::Escape);
        pointer(&mut h, g0 + Vec2::new(5.0, 0.0), Some(false));
        for id in [big, small, hex] {
            assert!(
                corner_amount(&h, id).abs() < 1e-4,
                "Esc restores every host"
            );
        }
        assert_eq!(h.app.board_sel.len(), 3);

        h.frame();
        assert_eq!(
            h.app.board_sel.len(),
            3,
            "releasing after Esc keeps the selection"
        );
        let g = corner_grip(&h, small);
        pointer(&mut h, g, None);
        pointer(&mut h, g, Some(true));
        pointer(&mut h, g, Some(false));
        assert_eq!(
            h.app.board_sel.len(),
            3,
            "a second quick click on the grip keeps the selection"
        );
        h.frame_with(|i| i.events.push(egui::Event::Text("20".into())));
        key(&mut h, egui::Key::Enter);
        assert!((corner_amount(&h, big) - 20.0).abs() < 1e-4);
        assert!((corner_amount(&h, small) - 15.0).abs() < 1e-4);
        assert!((corner_amount(&h, hex) - 20.0).abs() < 1e-4);
        h.app.board_undo();
        for id in [big, small, hex] {
            assert!(
                corner_amount(&h, id).abs() < 1e-4,
                "the typed amount is one undo step"
            );
        }
    }

    #[test]
    fn polygon_corner_grip_rides_a_side_and_drags_along_it() {
        let mut h = board();
        let id = polygon(
            &mut h,
            WorldRect::new(-50.0, -50.0, 100.0, 100.0),
            Corner::Rounded { radius: 40.0 },
        );
        h.frame();
        let xf = h.app.board_xf();
        let v0 = Pos2::new(0.0, -50.0);
        let dir = Vec2::new(43.30127, 25.0).normalized();
        // Tangent distance per unit radius at a 120 degree vertex.
        let k = 30f32.to_radians().tan();
        let expected = xf.w2s(v0 + dir * 40.0 * k);
        let g0 = corner_grip(&h, id);
        assert!(g0.distance(expected) < 0.01, "{g0:?} vs {expected:?}");
        pointer(&mut h, g0, None);
        pointer(&mut h, g0, Some(true));
        let g1 = g0 + dir;
        pointer(&mut h, g1, None);
        assert!(corner_grip(&h, id).distance(g1) < 0.01);
        assert!((corner_amount(&h, id) - (40.0 * k + 1.0) / k).abs() < 0.01);
        let g2 = g0 - dir * 10.0;
        pointer(&mut h, g2, None);
        assert!(corner_grip(&h, id).distance(g2) < 0.01);
        pointer(&mut h, g2, Some(false));
        assert!((corner_amount(&h, id) - (40.0 * k - 10.0) / k).abs() < 0.01);
    }

    #[test]
    fn corner_grip_rides_the_top_edge_of_frames_portals_and_media() {
        let rect = WorldRect::new(-160.0, -120.0, 320.0, 240.0);
        let inset = super::super::board_handles::FILLET_GRIP_MIN_INSET_WORLD;
        for kind in ["frame", "portal", "image"] {
            let mut h = board();
            let id = match kind {
                "frame" => frame_node(&mut h, rect),
                "portal" => portal_node(&mut h, rect),
                _ => image_node(&mut h, rect),
            };
            h.frame();
            let along = corner_amount(&h, id).max(inset);
            let xf = h.app.board_xf();
            let expected = xf.w2s(Pos2::new(rect.x + along, rect.y));
            assert!(
                corner_grip(&h, id).distance(expected) < 0.01,
                "{kind}: {:?} vs {expected:?}",
                corner_grip(&h, id)
            );
        }
    }

    #[test]
    fn polyline_corners_slider_reads_the_committed_radius_and_chamfer_is_selectable() {
        let mut h = board();
        let id = polyline(&mut h, WorldRect::new(-60.0, -60.0, 120.0, 120.0));
        apply(&mut h, vec![id], vec![Property::CornerAmount(12.0)]);
        let node = h.app.doc().scene.node(id).unwrap();
        assert_eq!(
            h.app.corner_for_editor(node).parameters(),
            (false, false, 12.0),
            "the slider shows the stored radius"
        );
        apply(&mut h, vec![id], vec![Property::CornerTreatment(true)]);
        let node = h.app.doc().scene.node(id).unwrap().clone();
        assert_eq!(scene::corner_of(&node), Some(Corner::Chamfer { cut: 12.0 }));
        assert_eq!(
            h.app.corner_for_editor(&node).parameters(),
            (true, false, 12.0),
            "Chamfer stays selected"
        );
        let NodeKind::Shape(shape) = &node.kind else {
            panic!("shape");
        };
        let bez = super::super::board_path::shape_path_world_bez(
            &node,
            shape,
            shape.path.as_ref().unwrap(),
        );
        assert!(
            bez.elements().iter().all(|e| !matches!(
                e,
                vector_ink::kurbo::PathEl::CurveTo(..) | vector_ink::kurbo::PathEl::QuadTo(..)
            )),
            "the board draws a chamfered polyline with straight cuts"
        );
        assert_eq!(
            bez.elements().len(),
            4,
            "move, line to the cut, the cut, the far end"
        );
    }

    /// An open U: (0,0) → (1,0) → (1,1) → (0,1) in `rect`, corner radius 20.
    fn u_polyline(h: &mut Harness, rect: WorldRect) -> NodeId {
        let path = scene::PathData {
            start: [0.0, 0.0],
            segs: vec![
                scene::PathSeg::Line { to: [1.0, 0.0] },
                scene::PathSeg::Line { to: [1.0, 1.0] },
                scene::PathSeg::Line { to: [0.0, 1.0] },
            ],
            closed: false,
            ..Default::default()
        };
        let node = h.app.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(scene::ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: scene::Stroke::default(),
                corner: Corner::Rounded { radius: 20.0 },
                sides: scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(std::sync::Arc::new(path)),
                text: None,
            }),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    fn vertex_grips(h: &Harness, id: NodeId) -> Vec<(Option<usize>, Pos2)> {
        let xf = h.app.board_xf();
        let node = h.app.doc().scene.node(id).unwrap();
        h.app.corner_grips(node, &xf)
    }

    fn vertex_amount(h: &Harness, id: NodeId, vertex: usize) -> f32 {
        let node = h.app.doc().scene.node(id).unwrap();
        let NodeKind::Shape(shape) = &node.kind else {
            panic!("shape");
        };
        shape
            .path
            .as_ref()
            .unwrap()
            .vertex_corner_amount(vertex, corner_amount(h, id))
    }

    #[test]
    fn polyline_shows_a_grip_per_corner_and_each_drags_its_own_vertex() {
        let mut h = board();
        let id = u_polyline(&mut h, WorldRect::new(-60.0, -60.0, 120.0, 120.0));
        h.frame();
        let xf = h.app.board_xf();
        let grips = vertex_grips(&h, id);
        let at: Vec<_> = grips.iter().map(|(v, _)| *v).collect();
        assert_eq!(at, vec![Some(1), Some(2)], "one grip per turning vertex");
        assert!(
            grips[0].1.distance(xf.w2s(Pos2::new(40.0, -60.0))) < 0.01,
            "vertex 1's grip rides its incoming segment at the tangent point: {:?}",
            grips[0].1
        );
        let g2 = grips[1].1;
        assert!(
            g2.distance(xf.w2s(Pos2::new(60.0, 40.0))) < 0.01,
            "vertex 2's grip rides its incoming segment: {g2:?}"
        );
        pointer(&mut h, g2, None);
        pointer(&mut h, g2, Some(true));
        let g = g2 + Vec2::new(0.0, -10.0);
        pointer(&mut h, g, None);
        assert!(
            (vertex_amount(&h, id, 2) - 30.0).abs() < 0.01,
            "the dragged corner follows the pointer, got {}",
            vertex_amount(&h, id, 2)
        );
        assert!(
            (vertex_amount(&h, id, 1) - 20.0).abs() < 0.01,
            "the other corner is untouched"
        );
        assert!(vertex_grips(&h, id)[1].1.distance(g) < 0.01);
        pointer(&mut h, g, Some(false));
        assert!((vertex_amount(&h, id, 2) - 30.0).abs() < 0.01);
        assert!(
            (corner_amount(&h, id) - 20.0).abs() < 0.01,
            "the shape corner stays"
        );
        assert_eq!(h.app.board_sel, [id].into_iter().collect());
        h.app.board_undo();
        assert!(
            (vertex_amount(&h, id, 2) - 20.0).abs() < 1e-4,
            "the drag is one undo step"
        );
    }

    #[test]
    fn polyline_vertex_grip_click_types_that_corner_and_the_panel_sets_all() {
        let mut h = board();
        let id = u_polyline(&mut h, WorldRect::new(-60.0, -60.0, 120.0, 120.0));
        h.frame();
        let g1 = vertex_grips(&h, id)[0].1;
        pointer(&mut h, g1, None);
        pointer(&mut h, g1, Some(true));
        pointer(&mut h, g1, Some(false));
        h.frame_with(|i| i.events.push(egui::Event::Text("5".into())));
        key(&mut h, egui::Key::Enter);
        assert!(
            (vertex_amount(&h, id, 1) - 5.0).abs() < 1e-4,
            "the typed amount sets the clicked corner, got {}",
            vertex_amount(&h, id, 1)
        );
        assert!((vertex_amount(&h, id, 2) - 20.0).abs() < 1e-4);
        assert!((corner_amount(&h, id) - 20.0).abs() < 1e-4);

        apply(&mut h, vec![id], vec![Property::CornerAmount(12.0)]);
        for v in [1, 2] {
            assert!(
                (vertex_amount(&h, id, v) - 12.0).abs() < 1e-4,
                "the Corners value sets every corner"
            );
        }

        let other = rectangle(&mut h, WorldRect::new(100.0, -60.0, 80.0, 80.0), 0.0);
        h.frame();
        assert_eq!(h.app.board_sel.len(), 2);
        let at: Vec<_> = vertex_grips(&h, id).iter().map(|(v, _)| *v).collect();
        assert_eq!(
            at,
            vec![None],
            "in a multi-selection the polyline shows its shared grip"
        );
        let _ = other;
    }

    fn strip_kinds(h: &Harness) -> Vec<&'static str> {
        let chrome = h.app.shape_properties.last_chrome.as_ref();
        item_kinds(&chrome.expect("the strip painted").items)
    }

    /// User (28 September 2026, ed9): "just ad more strict filtering of what
    /// menue items are apropriat for vetice selection. for instance
    /// bumpercars makes no sence for individal verticies." With vertices
    /// picked the strip keeps Stroke (width, color, opacity) and Corners
    /// where a picked vertex turns a corner, under Select and Direct Select.
    #[test]
    fn picked_vertices_strip_offers_only_vertex_controls() {
        let mut h = board();
        h.app.settings.optional_bumper_cars = true;
        let id = u_polyline(&mut h, WorldRect::new(-60.0, -60.0, 120.0, 120.0));
        for _ in 0..3 {
            h.frame();
        }
        let whole = strip_kinds(&h);
        assert!(
            whole.contains(&"bumper") && whole.contains(&"corners"),
            "the whole curve: {whole:?}"
        );
        let xf = h.app.board_xf();
        click_at(&mut h, xf.w2s(Pos2::new(60.0, -60.0)));
        h.frame();
        assert_eq!(h.app.shape_property_points(), vec![1]);
        assert_eq!(strip_kinds(&h), ["stroke", "corners"], "a picked corner");

        click_at(&mut h, xf.w2s(Pos2::new(-60.0, -60.0)));
        h.frame();
        assert_eq!(h.app.shape_property_points(), vec![0]);
        assert_eq!(strip_kinds(&h), ["stroke"], "an end point turns no corner");

        h.app.set_board_tool(BoardTool::DirectSelect);
        h.app.direct_set_target(Some(id));
        h.app.direct.anchors = [2].into_iter().collect();
        h.frame();
        h.frame();
        assert_eq!(strip_kinds(&h), ["stroke", "corners"], "Direct Select");
    }

    /// Center of the Corners amount handle painted in `out`.
    fn corner_handle(out: &egui::FullOutput, accent: egui::Color32, z: f32) -> Option<Pos2> {
        fn walk(shape: &egui::Shape, accent: egui::Color32, size: Vec2) -> Option<Pos2> {
            match shape {
                egui::Shape::Rect(r)
                    if r.fill == accent && (r.rect.size() - size).length() < 0.5 =>
                {
                    Some(r.rect.center())
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| walk(s, accent, size)),
                _ => None,
            }
        }
        let size = Vec2::new(20.0, 11.0) * z;
        out.shapes.iter().find_map(|c| walk(&c.shape, accent, size))
    }

    /// User (28 September 2026, ep2): "fillet edits all". With corners
    /// picked the Corners slider starts at the first picked corner and
    /// writes only the picked corners, one undo step per accepted edit.
    #[test]
    fn picked_corners_take_the_corners_amount_alone() {
        let mut h = board();
        let id = u_polyline(&mut h, WorldRect::new(-60.0, -60.0, 120.0, 120.0));
        h.frame();
        assert!(h.app.dispatch(
            &h.ctx,
            CommandId("board.shape.fillet"),
            Some(
                serde_json::to_string(&board_transform::FilletRequest {
                    ids: vec![id],
                    radius: 8.0,
                    vertex: Some(2),
                })
                .unwrap()
            ),
        ));
        assert!((vertex_amount(&h, id, 2) - 8.0).abs() < 1e-4);
        let xf = h.app.board_xf();
        click_at(&mut h, xf.w2s(Pos2::new(60.0, 60.0)));
        h.frame();
        assert_eq!(h.app.shape_property_points(), vec![2]);

        let button = strip_button(&h, Panel::Corners);
        click_at(&mut h, button.center());
        h.frame();
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Corners));
        assert_eq!(h.app.shape_property_points(), vec![2], "picks survive");
        let editor = *h.app.shape_properties.chrome_hits.last().unwrap();
        let z = h.app.board_xf().z;
        let track = chrome::corner_layout(editor, false, z).track;
        let rail_w = track.width() - 24.0 * z;
        let rail_x = |amount: f32| track.center().x - rail_w * 0.5 + rail_w * amount / 60.0;
        let out = h.frame_output(|_| {});
        let handle = corner_handle(&out, h.app.palette().accent, z).expect("amount handle");
        assert!(
            (handle.x - rail_x(8.0)).abs() < 1.0,
            "the slider starts at the picked corner's 8, not the shared 20: {handle:?}"
        );

        let depth = h.app.tab().journal.undo_depth();
        let to = Pos2::new(rail_x(30.0), handle.y);
        pointer(&mut h, handle, None);
        pointer(&mut h, handle, Some(true));
        pointer(&mut h, handle + (to - handle) * 0.5, None);
        pointer(&mut h, to, None);
        pointer(&mut h, to, Some(false));
        let button = strip_button(&h, Panel::Corners);
        click_at(&mut h, button.center());
        h.frame();
        assert_eq!(h.app.shape_properties.panel, None, "closing accepts");
        assert!(
            (vertex_amount(&h, id, 2) - 30.0).abs() < 0.6,
            "the picked corner takes the amount, got {}",
            vertex_amount(&h, id, 2)
        );
        assert!(
            (vertex_amount(&h, id, 1) - 20.0).abs() < 1e-4,
            "the other corner keeps the shared 20, got {}",
            vertex_amount(&h, id, 1)
        );
        assert!((corner_amount(&h, id) - 20.0).abs() < 1e-4);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
        h.app.board_undo();
        assert!((vertex_amount(&h, id, 2) - 8.0).abs() < 1e-4);
        assert!((vertex_amount(&h, id, 1) - 20.0).abs() < 1e-4);
    }

    fn polygon_vertices_world(h: &Harness, id: NodeId) -> Vec<Pos2> {
        let node = h.app.doc().scene.node(id).unwrap();
        let NodeKind::Shape(s) = &node.kind else {
            panic!("shape");
        };
        scene::regular_polygon_vertices(node.rect, s.sides, s.phase_deg)
            .into_iter()
            .map(|p| {
                let [x, y] = node.rect.rotate_point(p, node.rotation_deg);
                Pos2::new(x, y)
            })
            .collect()
    }

    fn sides_of(h: &Harness, id: NodeId) -> (u8, f32) {
        match &h.app.doc().scene.node(id).unwrap().kind {
            NodeKind::Shape(s) => (s.sides, s.phase_deg),
            _ => panic!("shape"),
        }
    }

    /// Rest the pointer on `p`. egui reports the board hovered only from the
    /// second frame the pointer sits over it.
    fn hover(h: &mut Harness, p: Pos2) {
        pointer(h, p, None);
        pointer(h, p, None);
    }

    /// Hover `vertex` and return where its + (or −) shows.
    fn sides_glyph(h: &mut Harness, id: NodeId, vertex: usize, add: bool) -> Pos2 {
        let v = h.app.board_xf().w2s(polygon_vertices_world(h, id)[vertex]);
        hover(h, v);
        let xf = h.app.board_xf();
        h.app
            .polygon_sides_glyphs(&xf)
            .into_iter()
            .find(|g| g.id == id && g.vertex == vertex && g.add == add)
            .map(|g| g.center)
            .expect("the hovered vertex shows its glyph")
    }

    fn click_at(h: &mut Harness, p: Pos2) {
        pointer(h, p, None);
        pointer(h, p, Some(true));
        pointer(h, p, Some(false));
    }

    #[test]
    fn polygon_vertex_hover_shows_plus_outside_and_minus_inside_only_there() {
        let mut h = board();
        let id = polygon(
            &mut h,
            WorldRect::new(-50.0, -50.0, 100.0, 100.0),
            Corner::Square,
        );
        h.frame();
        let xf = h.app.board_xf();
        assert!(
            h.app.polygon_sides_glyphs(&xf).is_empty(),
            "nothing before a hover"
        );
        let center = xf.w2s(Pos2::ZERO);
        let v2 = xf.w2s(polygon_vertices_world(&h, id)[2]);
        hover(&mut h, v2);
        let glyphs = h.app.polygon_sides_glyphs(&xf);
        assert_eq!(glyphs.len(), 2, "{glyphs:?}");
        assert!(
            glyphs.iter().all(|g| g.id == id && g.vertex == 2),
            "only the hovered vertex shows its glyphs: {glyphs:?}"
        );
        let plus = glyphs.iter().find(|g| g.add).unwrap();
        let minus = glyphs.iter().find(|g| !g.add).unwrap();
        let out = (v2 - center).normalized();
        assert!(
            (plus.center - v2).normalized().dot(out) > 0.999,
            "+ sits outside, on the vertex's bisector"
        );
        assert!(
            (minus.center - v2).normalized().dot(out) < -0.999,
            "− sits inside, on the vertex's bisector"
        );
        let offset = plus.center.distance(v2);

        h.app.tab_mut().cam.z *= 2.0;
        let xf = h.app.board_xf();
        let v2 = xf.w2s(polygon_vertices_world(&h, id)[2]);
        hover(&mut h, v2);
        let glyphs = h.app.polygon_sides_glyphs(&xf);
        let plus2 = glyphs.iter().find(|g| g.add).expect("+ at 2x");
        assert!(
            (plus2.center.distance(v2) - 2.0 * offset).abs() < 0.01
                && (plus2.radius - 2.0 * plus.radius).abs() < 0.01,
            "the glyphs scale with the canvas (P0.9)"
        );

        hover(&mut h, v2 + Vec2::new(400.0, 400.0));
        let xf = h.app.board_xf();
        assert!(
            h.app.polygon_sides_glyphs(&xf).is_empty(),
            "gone off the vertex"
        );

        h.app.tab_mut().cam.z = 0.1;
        let xf = h.app.board_xf();
        let v = xf.w2s(polygon_vertices_world(&h, id)[2]);
        hover(&mut h, v);
        assert!(
            h.app.polygon_sides_glyphs(&xf).is_empty(),
            "too small to read, the glyphs drop (LOD)"
        );
    }

    #[test]
    fn polygon_plus_keeps_the_vertex_and_minus_keeps_one_there() {
        let mut h = board();
        let rect = WorldRect::new(-80.0, -50.0, 160.0, 100.0);
        let id = polygon(&mut h, rect, Corner::Square);
        h.frame();
        let before = polygon_vertices_world(&h, id);
        let plus = sides_glyph(&mut h, id, 2, true);
        click_at(&mut h, plus);
        assert_eq!(sides_of(&h, id).0, 7, "+ adds a side");
        let after = polygon_vertices_world(&h, id);
        assert!(
            after[2].distance(before[2]) < 1e-3,
            "vertex 2 stays put: {:?} vs {:?}",
            after[2],
            before[2]
        );
        assert_eq!(h.app.board_sel, [id].into_iter().collect());
        h.app.board_undo();
        assert_eq!(sides_of(&h, id), (6, 0.0), "+ is one undo step");

        for _ in 0..30 {
            h.frame();
        }
        let minus = sides_glyph(&mut h, id, 4, false);
        click_at(&mut h, minus);
        assert_eq!(sides_of(&h, id).0, 5, "− removes a side");
        let after = polygon_vertices_world(&h, id);
        assert!(
            after[0].distance(before[4]) < 1e-3,
            "a vertex stays exactly where vertex 4 was: {:?} vs {:?}",
            after[0],
            before[4]
        );
        h.app.board_undo();
        assert_eq!(sides_of(&h, id), (6, 0.0), "− is one undo step");
    }

    #[test]
    fn polygon_sides_stop_at_3_and_12_and_a_quick_second_click_is_not_a_double_click() {
        let mut h = board();
        let id = polygon(
            &mut h,
            WorldRect::new(-50.0, -50.0, 100.0, 100.0),
            Corner::Square,
        );
        let set_sides = |h: &mut Harness, n: u8| {
            h.app.patch_nodes(&[id], |node| {
                if let NodeKind::Shape(s) = &mut node.kind {
                    s.sides = n;
                }
            });
            h.frame();
        };
        let shown = |h: &mut Harness| {
            let v = h.app.board_xf().w2s(polygon_vertices_world(h, id)[1]);
            pointer(h, v, None);
            let xf = h.app.board_xf();
            let mut adds: Vec<bool> = h
                .app
                .polygon_sides_glyphs(&xf)
                .iter()
                .map(|g| g.add)
                .collect();
            adds.sort();
            adds
        };
        set_sides(&mut h, 12);
        assert_eq!(shown(&mut h), vec![false], "12 sides: only −");
        set_sides(&mut h, 3);
        assert_eq!(shown(&mut h), vec![true], "3 sides: only +");

        set_sides(&mut h, 6);
        for _ in 0..30 {
            h.frame();
        }
        let minus = sides_glyph(&mut h, id, 2, false);
        click_at(&mut h, minus);
        assert_eq!(sides_of(&h, id).0, 5);
        pointer(&mut h, minus, Some(true));
        pointer(&mut h, minus, Some(false));
        assert_eq!(
            sides_of(&h, id).0,
            4,
            "the − under the pointer again removes another side"
        );
        assert_eq!(
            h.app.board_sel,
            [id].into_iter().collect(),
            "a quick second click keeps the selection"
        );
        assert!(
            !h.app.text_compose_active(),
            "a quick second click does not open text editing"
        );
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
                StripItem::Panel(Panel::Pages) => "pages",
                StripItem::Panel(Panel::AtlasFormat) => "format",
                StripItem::Panel(Panel::Text) => "text",
                StripItem::Panel(Panel::Agent) => "agent",
                StripItem::Panel(Panel::Bumper) => "bumper",
                StripItem::Panel(Panel::ModelDisplay) => "display",
                StripItem::ModelMeasure => "measure",
                StripItem::ModelScreenshot => "screenshot",
                StripItem::Frame(FrameAction::Prev) => "prev",
                StripItem::Frame(FrameAction::Next) => "next",
                StripItem::Frame(FrameAction::Present) => "present",
                StripItem::Frame(FrameAction::Deck) => "deck",
                StripItem::Agent(true) => "unbundle",
                StripItem::Agent(false) => "bundle",
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
        let atlas = {
            let node = h.app.doc_mut().scene.build_node(
                rect,
                NodeKind::Portal(scene::PortalNode::unbound_file_atlas("Folder")),
            );
            let id = h.app.add_nodes(vec![node])[0];
            h.app.board_sel.insert(id);
            id
        };
        let shape = rectangle(&mut h, rect, 0.0);
        let node = |id| h.app.doc().scene.node(id).unwrap().clone();
        assert_eq!(
            item_kinds(&property_strip_items(&[node(frame)])),
            ["fill", "stroke", "corners", "prev", "next", "present", "deck"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(image)])),
            ["stroke", "corners", "filter"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(text)])),
            ["fill", "stroke"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(portal)])),
            ["fill", "stroke", "corners"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(atlas)])),
            ["fill", "stroke", "corners", "format"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(shape)])),
            ["fill", "stroke", "corners"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(frame), node(shape)])),
            ["fill", "stroke", "corners"]
        );
        assert_eq!(
            item_kinds(&property_strip_items(&[node(image), node(shape)])),
            ["stroke", "corners"]
        );
    }

    #[test]
    fn bumper_squircle_needs_the_preference_and_a_compatible_selection() {
        let mut h = board();
        let rect = WorldRect::new(0.0, 0.0, 120.0, 80.0);
        let shape = rectangle(&mut h, rect, 0.0);
        let image = image_node(&mut h, rect);
        let node = |h: &Harness, id| h.app.doc().scene.node(id).unwrap().clone();
        let has = |h: &Harness, nodes: &[Node]| {
            item_kinds(&live_property_strip_items(&h.app, nodes)).contains(&"bumper")
        };
        assert!(!has(&h, &[node(&h, shape)]), "off by default");
        h.app.settings.optional_bumper_cars = true;
        assert!(has(&h, &[node(&h, shape)]));
        assert!(!has(&h, &[node(&h, image)]));
        assert!(!has(&h, &[node(&h, shape), node(&h, image)]));

        apply(&mut h, vec![shape], vec![Property::BumperOn(true)]);
        let b = node(&h, shape).bumper.expect("turned on");
        assert_eq!(b, slate_doc::bumper::Bumper::default());
        apply(&mut h, vec![shape], vec![Property::BumperFriction(1.0)]);
        assert!(node(&h, shape).bumper.unwrap().is_anchor());
        h.app.board_undo();
        assert_eq!(node(&h, shape).bumper, Some(b));
        apply(&mut h, vec![shape], vec![Property::BumperOn(false)]);
        assert_eq!(node(&h, shape).bumper, None);
    }

    #[test]
    fn text_documents_skip_photo_filters() {
        let mut h = board();
        let rect = WorldRect::new(0.0, 0.0, 240.0, 180.0);
        let item = h.app.doc_mut().add_item(
            std::path::PathBuf::from("rows.csv"),
            "rows.csv",
            1,
            0,
            "csv",
        );
        let node = h
            .app
            .doc_mut()
            .scene
            .build_node(rect, NodeKind::Image(scene::ImageNode::new(item)));
        let id = h.app.add_nodes(vec![node])[0];
        let node = h.app.doc().scene.node(id).unwrap().clone();
        let kinds = item_kinds(&live_property_strip_items(&h.app, &[node]));
        assert!(
            !kinds.contains(&"filter"),
            "text cards keep stroke and corners, not photo filters: {kinds:?}"
        );
        assert!(kinds.contains(&"stroke"));
        assert!(kinds.contains(&"corners"));
    }

    fn model_node(h: &mut Harness, name: &str, rect: WorldRect) -> NodeId {
        let path = h.base.join(name);
        let _ = std::fs::write(&path, b" ");
        let item = h.app.doc_mut().add_item(path, name, 1, 0, name);
        let img = scene::ImageNode::new(item);
        let node = h.app.doc_mut().scene.build_node(rect, NodeKind::Image(img));
        h.app.add_nodes(vec![node])[0]
    }

    #[test]
    fn model_viewports_offer_display_measure_screenshot_then_filters() {
        let mut h = board();
        let rect = WorldRect::new(0.0, 0.0, 240.0, 180.0);
        let tower = model_node(&mut h, "tower.3dm", rect);
        let blend = model_node(&mut h, "scene.blend", rect);
        let node = |h: &Harness, id| h.app.doc().scene.node(id).unwrap().clone();
        assert_eq!(
            item_kinds(&live_property_strip_items(&h.app, &[node(&h, tower)])),
            [
                "stroke",
                "corners",
                "display",
                "measure",
                "screenshot",
                "filter",
                "agent"
            ]
        );
        // A recognized format with no reader has no viewport to drive.
        let kinds = item_kinds(&live_property_strip_items(&h.app, &[node(&h, blend)]));
        assert!(!kinds.contains(&"display") && !kinds.contains(&"measure"));
        // Two models share no single viewport.
        let two = model_node(&mut h, "tower-b.3dm", rect);
        let kinds = item_kinds(&live_property_strip_items(
            &h.app,
            &[node(&h, tower), node(&h, two)],
        ));
        assert!(!kinds.contains(&"display") && !kinds.contains(&"measure"));

        // A frozen viewport takes a display pass as one undo step.
        let display = |h: &Harness| h.app.model_node_info(tower).unwrap().cam.display;
        assert!(h.app.dispatch(
            &h.ctx,
            CommandId("board.model_display"),
            Some(format!("{}:depth", tower.0)),
        ));
        assert_eq!(display(&h), scene::ModelDisplay::Depth);
        assert!(
            !h.app.dispatch(
                &h.ctx,
                CommandId("board.model_display"),
                Some(format!("{}:depth", tower.0)),
            ),
            "re-picking the shown pass is not an edit"
        );
        h.app.board_undo();
        assert_eq!(display(&h), scene::ModelDisplay::Shaded);
        // From the palette, the bare command cycles the selected model.
        h.app.board_sel = [tower].into_iter().collect();
        assert!(h
            .app
            .dispatch(&h.ctx, CommandId("board.model_display"), None));
        assert_eq!(display(&h), scene::ModelDisplay::Arctic);
    }

    #[test]
    fn model_viewport_filter_commits_and_undoes() {
        let mut h = board();
        let rect = WorldRect::new(0.0, 0.0, 240.0, 180.0);
        let tower = model_node(&mut h, "tower.3dm", rect);
        h.app.board_sel = [tower].into_iter().collect();
        h.app.sync_shape_properties();
        let before = scene::adjust_of(h.app.doc().scene.node(tower).unwrap()).unwrap();
        h.app
            .preview_shape_property(Property::ImageAdjust(PhotoFilter::Juno.at(0.75)));
        h.app.apply_shape_preview(&h.ctx, true);
        let after = scene::adjust_of(h.app.doc().scene.node(tower).unwrap()).unwrap();
        assert_ne!(after, before);
        h.app.board_undo();
        assert_eq!(
            scene::adjust_of(h.app.doc().scene.node(tower).unwrap()).unwrap(),
            before
        );
    }

    #[test]
    fn view_drop_restores_camera_in_one_undo_step() {
        let mut h = board();
        let rect = WorldRect::new(0.0, 0.0, 240.0, 180.0);
        let tower = model_node(&mut h, "tower.3dm", rect);
        let before = h.app.model_node_info(tower).unwrap().cam;
        let mut restored = before;
        restored.yaw = before.yaw + 0.5;
        restored.display = scene::ModelDisplay::Arctic;
        h.app.apply_view_drop(
            tower,
            model_preview::view_meta::ViewMetaParsed {
                camera: restored,
                model_name: "tower.3dm".into(),
                model_path: "tower.3dm".into(),
                model_hash: String::new(),
                model_size: 0,
                node_id: tower.0,
            },
        );
        assert_eq!(h.app.model_node_info(tower).unwrap().cam, restored);
        h.app.board_undo();
        assert_eq!(h.app.model_node_info(tower).unwrap().cam, before);
    }

    fn pdf_node(h: &mut Harness, rect: WorldRect) -> NodeId {
        let path = h.base.join("deck.pdf");
        std::fs::write(&path, b"pdf").unwrap();
        let item = h.app.doc_mut().add_item(path, "deck.pdf", 1, 0, "pdf");
        let node = h
            .app
            .doc_mut()
            .scene
            .build_node(rect, NodeKind::Image(scene::ImageNode::new(item)));
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel.insert(id);
        id
    }

    #[test]
    fn live_strip_shows_pages_for_a_single_pdf() {
        let mut h = board();
        let rect = WorldRect::new(0.0, 0.0, 120.0, 80.0);
        let pdf = pdf_node(&mut h, rect);
        let image = image_node(&mut h, rect);
        let node = |id| h.app.doc().scene.node(id).unwrap().clone();
        assert_eq!(
            item_kinds(&live_property_strip_items(&h.app, &[node(pdf)])),
            ["stroke", "corners", "filter", "pages"]
        );
        // A selected picture offers the Agent squircle (portal-agent-link D01,
        // DYNAMIC_PANELS.md). A PDF is pages, not that media.
        assert_eq!(
            item_kinds(&live_property_strip_items(&h.app, &[node(image)])),
            ["stroke", "corners", "filter", "agent"]
        );
        assert_eq!(
            item_kinds(&live_property_strip_items(
                &h.app,
                &[node(pdf), node(image)]
            )),
            ["stroke", "corners", "filter"]
        );
    }

    #[test]
    fn file_atlas_fill_picker_authors_an_opaque_window() {
        let mut h = board();
        let id = {
            let node = h.app.doc_mut().scene.build_node(
                WorldRect::new(0.0, 0.0, 200.0, 120.0),
                NodeKind::Portal(scene::PortalNode::unbound_file_atlas("Folder")),
            );
            h.app.add_nodes(vec![node])[0]
        };
        apply(&mut h, vec![id], vec![Property::FillRgb([40, 90, 140])]);
        let NodeKind::Portal(portal) = &h.app.doc().scene.node(id).unwrap().kind else {
            panic!("expected a File Atlas portal");
        };
        assert_eq!(portal.fill, Rgba([40, 90, 140, 255]));
        assert!(!portal.fill_follows_theme());
        apply(
            &mut h,
            vec![id],
            vec![
                Property::StrokeRgb([200, 40, 40]),
                Property::StrokeWidth(3.0),
            ],
        );
        let NodeKind::Portal(portal) = &h.app.doc().scene.node(id).unwrap().kind else {
            panic!("expected a File Atlas portal");
        };
        assert_eq!(portal.stroke.color.0[..3], [200, 40, 40]);
        assert_eq!(portal.stroke.width, 3.0);
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
    fn hovering_a_photo_filter_does_not_count_as_selecting_it() {
        let hover_click = chrome::FilterEdit {
            hovered: Some(1),
            clicked: Some(1),
            amount: None,
        };
        match photo_filter_gesture(Some(0), 1.0, Some(0), hover_click) {
            FilterStep::Commit(adjust) => {
                assert_eq!(
                    PhotoFilter::recognize(&adjust),
                    Some((PhotoFilter::Mono, 1.0))
                );
            }
            FilterStep::Peek(_) | FilterStep::Rest => {
                panic!("click must record the hovered filter")
            }
        }
        let clear = chrome::FilterEdit {
            hovered: Some(0),
            clicked: Some(0),
            amount: None,
        };
        match photo_filter_gesture(Some(1), 1.0, Some(1), clear) {
            FilterStep::Commit(adjust) => assert!(adjust.is_identity()),
            FilterStep::Peek(_) | FilterStep::Rest => panic!("None clears the filter"),
        }
        match photo_filter_gesture(Some(1), 1.0, Some(1), hover_click) {
            FilterStep::Commit(adjust) => assert!(adjust.is_identity()),
            FilterStep::Peek(_) | FilterStep::Rest => panic!("a second click clears the filter"),
        }
        let scrub = chrome::FilterEdit {
            hovered: None,
            clicked: None,
            amount: Some(0.35),
        };
        match photo_filter_gesture(Some(0), 1.0, Some(3), scrub) {
            FilterStep::Commit(adjust) => {
                let (kind, amount) = PhotoFilter::recognize(&adjust).unwrap();
                assert_eq!(kind, PhotoFilter::Clarendon);
                assert!((amount - 0.35).abs() < 0.02);
            }
            FilterStep::Peek(_) | FilterStep::Rest => {
                panic!("the slider must keep the filter that was just hovered")
            }
        }
        match photo_filter_gesture(Some(2), 0.8, Some(0), scrub) {
            FilterStep::Commit(adjust) => {
                let (kind, amount) = PhotoFilter::recognize(&adjust).unwrap();
                assert_eq!(kind, PhotoFilter::Invert);
                assert!((amount - 0.35).abs() < 0.02);
            }
            FilterStep::Peek(_) | FilterStep::Rest => panic!("a selected filter owns the slider"),
        }
        match photo_filter_gesture(Some(0), 1.0, Some(0), scrub) {
            FilterStep::Rest => {}
            FilterStep::Commit(_) | FilterStep::Peek(_) => {
                panic!("None has no intensity to scrub")
            }
        }
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

    fn empty_canvas_point(h: &Harness, id: NodeId) -> Pos2 {
        let xf = h.app.board_xf();
        let r = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
        let canvas = h.app.canvas_rect;
        let p = Pos2::new(
            (r.max.x + 90.0).min(canvas.max.x - 24.0),
            (r.max.y + 70.0).min(canvas.max.y - 24.0),
        );
        assert!(canvas.contains(p));
        assert!(!r.expand(20.0).contains(p));
        p
    }

    #[test]
    fn right_drag_pan_keeps_the_property_editor_open() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
        h.frame();
        h.frame();
        h.app.shape_properties.panel = Some(Panel::Fill);
        let p = empty_canvas_point(&h, id);
        let before = h.app.tab().cam.offset;
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
        assert!(h.app.board_sel.contains(&id));
        let end = p + Vec2::new(80.0, -36.0);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(end)));
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        h.frame();
        assert_eq!(
            h.app.shape_properties.panel,
            Some(Panel::Fill),
            "right-drag pan must leave the editor open"
        );
        assert!(
            h.app.board_sel.contains(&id),
            "right-drag pan must keep the node selected"
        );
        assert!(
            (h.app.tab().cam.offset - before).length() > 10.0,
            "the gesture must actually pan the board"
        );
    }

    #[test]
    fn secondary_click_without_a_drag_still_collapses_the_editor() {
        let mut h = board();
        let id = rectangle(&mut h, WorldRect::new(-100.0, -60.0, 200.0, 120.0), 0.0);
        h.frame();
        h.frame();
        h.app.shape_properties.panel = Some(Panel::Fill);
        let p = empty_canvas_point(&h, id);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Fill));
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
        assert!(h.app.shape_properties.panel.is_none());
        assert!(h.app.board_sel.is_empty());
    }
}
