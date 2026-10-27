//! Recipes — what a completed gesture produces.
//! A recipe is the *data* half of a tool. It may only name node kinds and style
//! properties the document model already has, because a kit cannot teach
//! `slate-artifact` anything (Art. IV: a style property lands in both
//! interpreters or neither).
//!
//! Two kinds ship: [`Recipe::Shape`] for nodes the recipe can build itself, and
//! [`Recipe::Portal`] for a preset source over an existing portal kind.
//! Stamps — a saved group of nodes placed by the gesture — are the next
//! kind and are deliberately absent rather than stubbed.

use serde::{Deserialize, Serialize};
use slate_doc::scene::{
    Corner, FontChoice, FrameNode, NodeKind, PortalClass, PortalKind, PortalNode, Rgba, ShapeKind,
    ShapeNode, SourceUri, Stroke, TextAlign, TextNode, WorldRect,
};

use crate::color::{ColorRef, KitColor};
use crate::grammar::Grammar;
use crate::style::StrokeSpec;

/// A node the recipe wants built, before the scene assigns it an id.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSpec {
    pub rect: WorldRect,
    pub kind: NodeKind,
}

/// Runtime values a recipe defers to the app: theme-dependent color and the
/// scene's own counters. Keeping these out of the file is what lets one kit
/// look right in both themes.
#[derive(Debug, Clone, Copy)]
pub struct BuildCtx {
    /// The active palette's accent, for `"accent"` color references.
    pub accent: Rgba,
    /// `Scene::next_frame_order()` — the slide sequence position to claim.
    pub next_frame_order: u32,
}

/// Which document node a shape recipe produces.
///
/// `Path` is style-only: path geometry comes from the gesture (`Freehand`,
/// `MultiPoint`, `TwoPoint`), so the recipe contributes the stroke and nothing
/// else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeTarget {
    Frame,
    Rect,
    Ellipse,
    RegularPolygon,
    Text,
    Path,
}

impl NodeTarget {
    /// Whether the recipe can build this node from a rect alone.
    #[must_use]
    pub const fn builds_from_rect(self) -> bool {
        !matches!(self, NodeTarget::Path)
    }
}

/// Where a new node's style comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreateStyle {
    /// Adopt the last style the user edited (today's `board_last_style`).
    #[default]
    Inherit,
    /// Always use the recipe's own style. A redline pen is always redline red.
    Pinned,
}

/// Text defaults for `node = "text"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextSpec {
    /// Initial content. Empty means the tool opens an inline editor.
    pub text: String,
    pub size: f32,
    pub color: ColorRef,
    pub align: TextAlign,
    pub family: FontChoice,
    /// Background fill. A sticky note is a text node with one.
    pub fill: Option<ColorRef>,
}

impl Default for TextSpec {
    fn default() -> Self {
        TextSpec {
            text: String::new(),
            size: 24.0,
            color: ColorRef::Fixed(KitColor(Rgba::BLACK)),
            align: TextAlign::default(),
            family: FontChoice::default(),
            fill: None,
        }
    }
}

/// Frame defaults for `node = "frame"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FrameSpec {
    /// Title template. `{n}` becomes the 1-based slide number — a named
    /// substitution, not an expression (Art. VII.7).
    pub title: String,
    pub fill: ColorRef,
}

impl Default for FrameSpec {
    fn default() -> Self {
        FrameSpec {
            title: "Slide {n}".into(),
            fill: ColorRef::Fixed(KitColor(Rgba::WHITE)),
        }
    }
}

/// A recipe that produces one node of an existing kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeRecipe {
    pub node: NodeTarget,
    #[serde(default)]
    pub stroke: Option<StrokeSpec>,
    #[serde(default)]
    pub fill: Option<ColorRef>,
    #[serde(default)]
    pub corner: Corner,
    #[serde(default)]
    pub create_style: CreateStyle,
    /// Size used when the gesture is a click rather than a drag, in world
    /// units. `DragRect` tools need it; others ignore it.
    #[serde(default)]
    pub default_size: Option<[f32; 2]>,
    #[serde(default)]
    pub text: TextSpec,
    #[serde(default)]
    pub frame: FrameSpec,
}

/// A portal kind named in a kit file, kept as text until resolved so an
/// unknown kind degrades one tool rather than the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PortalKindRef(pub String);

impl PortalKindRef {
    #[must_use]
    pub fn resolve(&self) -> Option<PortalKind> {
        // `PortalKind` is closed in `slate-doc`: a kit presets an existing kind
        // and can never introduce one.
        match self.0.as_str() {
            "agent" => Some(PortalKind::Agent),
            "web" => Some(PortalKind::Web),
            "file_atlas" => Some(PortalKind::FileAtlas),
            "slate" => Some(PortalKind::Slate),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A recipe that places a portal with a preset source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalRecipe {
    pub portal: PortalKindRef,
    #[serde(default)]
    pub title: Option<String>,
    /// Relative-first locator (Art. IX.2). `None` places an unbound portal,
    /// which is the honest default for a shared kit — see `resolve.rs`.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub default_size: Option<[f32; 2]>,
    #[serde(default)]
    pub fill: Option<ColorRef>,
}

/// What a completed gesture produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recipe {
    Shape(ShapeRecipe),
    Portal(PortalRecipe),
}

impl Recipe {
    /// The click-to-place size, when the recipe names one.
    #[must_use]
    pub fn default_size(&self) -> Option<[f32; 2]> {
        match self {
            Recipe::Shape(s) => s.default_size,
            Recipe::Portal(p) => p.default_size,
        }
    }

    /// Whether a completed create should adopt `BoardLastStyle` (stroke, fill
    /// where the node can take one, opacity). Portals never inherit.
    #[must_use]
    pub fn inherits_create_style(&self) -> bool {
        match self {
            Recipe::Shape(s) => s.create_style == CreateStyle::Inherit,
            Recipe::Portal(_) => false,
        }
    }

    /// Closed drag-rect shapes use [`StyleMemoryKind::Closed`]; paths use open.
    pub fn inherits_closed_shape_style(&self) -> bool {
        match self {
            Recipe::Shape(s) => matches!(
                s.node,
                NodeTarget::Rect
                    | NodeTarget::Ellipse
                    | NodeTarget::RegularPolygon
                    | NodeTarget::Frame
            ),
            Recipe::Portal(_) => false,
        }
    }

    /// The stroke a curve-producing grammar should draw with, if this recipe
    /// pins one. `None` means "inherit whatever the board would have used".
    #[must_use]
    pub fn pinned_stroke(&self, accent: Rgba) -> Option<Stroke> {
        match self {
            Recipe::Shape(s) if s.create_style == CreateStyle::Pinned => {
                s.stroke.map(|st| st.resolve(accent))
            }
            _ => None,
        }
    }

    /// Build the node(s) for a completed gesture over `rect`.
    ///
    /// Returns empty when the recipe contributes style rather than geometry
    /// (a `path` target), which is not a failure — the grammar owns the
    /// geometry in that case.
    #[must_use]
    pub fn instantiate(&self, rect: WorldRect, ctx: &BuildCtx) -> Vec<NodeSpec> {
        match self {
            Recipe::Shape(s) => self.instantiate_shape(s, rect, ctx),
            Recipe::Portal(p) => {
                let Some(kind) = p.portal.resolve() else {
                    return Vec::new();
                };
                let mut node =
                    match kind {
                        PortalKind::Application => {
                            let Some(surface) = p.source.as_deref().and_then(
                                slate_doc::application_surface::ApplicationSurface::parse,
                            ) else {
                                return Vec::new();
                            };
                            surface.portal(p.title.as_deref().unwrap_or("Application window"))
                        }
                        PortalKind::Agent => PortalNode::unbound_agent(
                            p.title.clone().unwrap_or_else(|| "Agent portal".into()),
                            "",
                        ),
                        PortalKind::Web => PortalNode::unbound_web(
                            p.title.clone().unwrap_or_else(|| "Web portal".into()),
                        ),
                        PortalKind::FileAtlas => PortalNode::unbound_file_atlas(
                            p.title.clone().unwrap_or_else(|| "File Atlas".into()),
                        ),
                        PortalKind::Slate => PortalNode::unbound_slate(
                            p.title.clone().unwrap_or_else(|| "Board".into()),
                        ),
                    };
                if kind != PortalKind::Slate {
                    node.class = PortalClass::Host;
                }
                node.source = p.source.clone().map(|locator| SourceUri { locator });
                if let Some(f) = p.fill {
                    node.fill = f.resolve(ctx.accent);
                }
                vec![NodeSpec {
                    rect,
                    kind: NodeKind::Portal(node),
                }]
            }
        }
    }

    fn instantiate_shape(&self, s: &ShapeRecipe, rect: WorldRect, ctx: &BuildCtx) -> Vec<NodeSpec> {
        let stroke = s
            .stroke
            .map(|st| st.resolve(ctx.accent))
            .unwrap_or_default();
        let fill = s.fill.map(|c| c.resolve(ctx.accent));
        let kind = match s.node {
            NodeTarget::Frame => {
                let frame_fill = s.frame.fill.resolve(ctx.accent);
                NodeKind::Frame(FrameNode {
                    title: s
                        .frame
                        .title
                        .replace("{n}", &(ctx.next_frame_order + 1).to_string()),
                    order: ctx.next_frame_order,
                    fill: frame_fill,
                    fill_authored: frame_fill != Rgba::WHITE,
                    assignments: std::collections::BTreeMap::new(),
                    stroke,
                    corner: s.corner,
                })
            }
            NodeTarget::Rect | NodeTarget::Ellipse | NodeTarget::RegularPolygon => {
                let shape = match s.node {
                    NodeTarget::Rect => ShapeKind::Rect,
                    NodeTarget::Ellipse => ShapeKind::Ellipse,
                    NodeTarget::RegularPolygon => ShapeKind::RegularPolygon,
                    _ => unreachable!(),
                };
                NodeKind::Shape(ShapeNode {
                    shape,
                    fill,
                    stroke,
                    corner: s.corner,
                    sides: slate_doc::scene::default_regular_sides(),
                    phase_deg: 0.0,
                    flip: false,
                    path: None,
                    text: None,
                })
            }
            NodeTarget::Text => NodeKind::Text(TextNode {
                text: s.text.text.clone(),
                style: Default::default(),
                family: s.text.family.into(),
                size: s.text.size,
                color: s.text.color.resolve(ctx.accent),
                align: s.text.align,
                fill: s.text.fill.map(|c| c.resolve(ctx.accent)),
                stroke: Default::default(),
                agent: None,
            }),
            // The gesture owns path geometry; the recipe only styles it.
            NodeTarget::Path => return Vec::new(),
        };
        vec![NodeSpec { rect, kind }]
    }

    /// Whether this recipe can be driven by `grammar`.
    ///
    /// This is the machine-checkable half of the routing rule: a contract whose
    /// grammar cannot feed its recipe is not expressible, and the authoring
    /// interface must say so before Create rather than after.
    #[must_use]
    pub fn accepts(&self, grammar: Grammar) -> bool {
        if !grammar.creates_nodes() {
            return false;
        }
        match self {
            // A rect-built node needs a rect or a click-to-place default.
            Recipe::Shape(s) if s.node.builds_from_rect() => {
                grammar.supplies_rect()
                    || s.default_size.is_some()
                    || grammar == Grammar::PlacePoint
            }
            // A path recipe needs a grammar that draws paths.
            Recipe::Shape(_) => matches!(
                grammar,
                Grammar::Freehand | Grammar::MultiPoint | Grammar::TwoPoint
            ),
            Recipe::Portal(p) => {
                (grammar.supplies_rect() || p.default_size.is_some())
                    && p.portal.resolve().is_some()
            }
        }
    }
}

#[cfg(test)]
mod tests;
