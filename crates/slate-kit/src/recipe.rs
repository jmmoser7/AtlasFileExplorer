//! Recipes — what a completed gesture produces.
//!
//! A recipe is the *data* half of a tool. It may only name node kinds and style
//! properties the document model already has, because a kit cannot teach
//! `slate-artifact` anything (Art. IV: a style property lands in both
//! interpreters or neither).
//!
//! Two kinds ship: [`Recipe::Shape`] for nodes the recipe can build itself, and
//! [`Recipe::Portal`] for a preset source and query over an existing portal
//! kind. Stamps — a saved group of nodes placed by the gesture — are the next
//! kind and are deliberately absent rather than stubbed.

use serde::{Deserialize, Serialize};
use slate_doc::scene::{
    Corner, FontChoice, FrameNode, NodeKind, PortalClass, PortalKind, PortalNode, RepoPortalQuery,
    Rgba, ShapeKind, ShapeNode, SourceUri, Stroke, TextAlign, TextNode, WorldRect,
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
            "repo_lens" => Some(PortalKind::RepoLens),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A recipe that places a portal with a preset source and query.
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
    pub query: RepoPortalQuery,
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
                let mut node = match kind {
                    PortalKind::RepoLens => PortalNode::unbound_repo_lens(
                        p.title.clone().unwrap_or_else(|| "Repository Lens".into()),
                    ),
                };
                node.class = PortalClass::Generated;
                node.query = p.query.clone();
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
            NodeTarget::Frame => NodeKind::Frame(FrameNode {
                title: s
                    .frame
                    .title
                    .replace("{n}", &(ctx.next_frame_order + 1).to_string()),
                order: ctx.next_frame_order,
                fill: s.frame.fill.resolve(ctx.accent),
                assignments: std::collections::BTreeMap::new(),
            }),
            NodeTarget::Rect | NodeTarget::Ellipse => NodeKind::Shape(ShapeNode {
                shape: if s.node == NodeTarget::Rect {
                    ShapeKind::Rect
                } else {
                    ShapeKind::Ellipse
                },
                fill,
                stroke,
                corner: s.corner,
                flip: false,
                path: None,
            }),
            NodeTarget::Text => NodeKind::Text(TextNode {
                text: s.text.text.clone(),
                family: s.text.family,
                size: s.text.size,
                color: s.text.color.resolve(ctx.accent),
                align: s.text.align,
                fill: s.text.fill.map(|c| c.resolve(ctx.accent)),
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
mod tests {
    use super::*;

    fn ctx() -> BuildCtx {
        BuildCtx {
            accent: Rgba([10, 120, 240, 255]),
            next_frame_order: 3,
        }
    }

    fn r() -> WorldRect {
        WorldRect::new(0.0, 0.0, 100.0, 50.0)
    }

    #[test]
    fn a_rect_recipe_builds_the_shape_the_board_would_have_hardcoded() {
        let toml = r#"
            kind = "shape"
            node = "rect"
            fill = "accent@60"
            stroke = { width = 2.0, color = "accent" }
        "#;
        let recipe: Recipe = toml::from_str(toml).unwrap();
        let specs = recipe.instantiate(r(), &ctx());
        assert_eq!(specs.len(), 1);
        let NodeKind::Shape(shape) = &specs[0].kind else {
            panic!("expected a shape, got {:?}", specs[0].kind);
        };
        assert_eq!(shape.shape, ShapeKind::Rect);
        assert_eq!(shape.fill, Some(Rgba([10, 120, 240, 60])));
        assert_eq!(shape.stroke.width, 2.0);
        assert_eq!(shape.stroke.color, Rgba([10, 120, 240, 255]));
        assert_eq!(specs[0].rect, r());
    }

    #[test]
    fn a_frame_recipe_claims_the_next_slide_order_and_numbers_its_title() {
        let recipe: Recipe = toml::from_str(
            r##"
            kind = "shape"
            node = "frame"
        "##,
        )
        .unwrap();
        let specs = recipe.instantiate(r(), &ctx());
        let NodeKind::Frame(f) = &specs[0].kind else {
            panic!("expected a frame");
        };
        assert_eq!(f.order, 3);
        assert_eq!(f.title, "Slide 4");
        assert_eq!(f.fill, Rgba::WHITE);
    }

    #[test]
    fn a_text_recipe_with_a_fill_is_a_sticky_note() {
        let recipe: Recipe = toml::from_str(
            r##"
            kind = "shape"
            node = "text"
            text = { text = "note", size = 18.0, fill = "#ffe066" }
        "##,
        )
        .unwrap();
        let specs = recipe.instantiate(r(), &ctx());
        let NodeKind::Text(t) = &specs[0].kind else {
            panic!("expected text");
        };
        assert_eq!(t.text, "note");
        assert_eq!(t.size, 18.0);
        assert_eq!(t.fill, Some(Rgba([0xff, 0xe0, 0x66, 255])));
    }

    #[test]
    fn a_path_recipe_styles_but_does_not_build() {
        let recipe: Recipe = toml::from_str(
            r##"
            kind = "shape"
            node = "path"
            create_style = "pinned"
            stroke = { width = 2.0, color = "#e8443a", cap = "round", join = "round" }
        "##,
        )
        .unwrap();
        assert!(recipe.instantiate(r(), &ctx()).is_empty());
        let stroke = recipe.pinned_stroke(ctx().accent).expect("pinned stroke");
        assert_eq!(stroke.color, Rgba([0xe8, 0x44, 0x3a, 255]));
        assert_eq!(stroke.cap, slate_doc::scene::StrokeCap::Round);
    }

    #[test]
    fn an_inheriting_recipe_pins_nothing() {
        let recipe: Recipe = toml::from_str(
            r##"
            kind = "shape"
            node = "path"
            stroke = { color = "#e8443a" }
        "##,
        )
        .unwrap();
        assert_eq!(recipe.pinned_stroke(ctx().accent), None);
    }

    #[test]
    fn a_portal_recipe_presets_source_and_query() {
        let recipe: Recipe = toml::from_str(
            r#"
            kind = "portal"
            portal = "repo_lens"
            title = "This repo"
            source = "."
            query = { max_commits = 500, axis = "chronological" }
            default_size = [960.0, 540.0]
        "#,
        )
        .unwrap();
        let specs = recipe.instantiate(r(), &ctx());
        let NodeKind::Portal(p) = &specs[0].kind else {
            panic!("expected a portal");
        };
        assert_eq!(p.class, PortalClass::Generated);
        assert_eq!(p.kind, PortalKind::RepoLens);
        assert_eq!(p.title, "This repo");
        assert_eq!(p.source.as_ref().unwrap().locator, ".");
        assert_eq!(p.query.max_commits, 500);
        assert_eq!(p.query.axis, slate_doc::scene::RepoTimeAxis::Chronological);
    }

    #[test]
    fn an_unknown_portal_kind_builds_nothing_rather_than_guessing() {
        let recipe = Recipe::Portal(PortalRecipe {
            portal: PortalKindRef("figma_frame".into()),
            title: None,
            source: None,
            query: RepoPortalQuery::default(),
            default_size: None,
            fill: None,
        });
        assert!(recipe.instantiate(r(), &ctx()).is_empty());
        assert!(!recipe.accepts(Grammar::DragRect));
    }

    #[test]
    fn the_routing_rule_rejects_grammars_that_cannot_feed_the_recipe() {
        let rect: Recipe = toml::from_str("kind = \"shape\"\nnode = \"rect\"").unwrap();
        assert!(rect.accepts(Grammar::DragRect));
        // Select and Sample create nothing, so no recipe can ride them.
        assert!(!rect.accepts(Grammar::Select));
        assert!(!rect.accepts(Grammar::Sample));
        assert!(!rect.accepts(Grammar::Sweep));
        // A freehand stroke cannot produce a rectangle.
        assert!(!rect.accepts(Grammar::Freehand));

        let pen: Recipe = toml::from_str("kind = \"shape\"\nnode = \"path\"").unwrap();
        assert!(pen.accepts(Grammar::Freehand));
        assert!(pen.accepts(Grammar::MultiPoint));
        assert!(!pen.accepts(Grammar::DragRect));
    }

    #[test]
    fn a_click_to_place_default_lets_a_rect_recipe_ride_place_point() {
        let sticky: Recipe = toml::from_str(
            r#"
            kind = "shape"
            node = "text"
            default_size = [180.0, 140.0]
        "#,
        )
        .unwrap();
        assert!(sticky.accepts(Grammar::PlacePoint));
        assert_eq!(sticky.default_size(), Some([180.0, 140.0]));
    }

    #[test]
    fn unknown_recipe_fields_are_rejected_rather_than_dropped() {
        assert!(toml::from_str::<Recipe>(
            r##"
            kind = "shape"
            node = "rect"
            fillColor = "#ff0000"
        "##
        )
        .is_err());
    }
}
