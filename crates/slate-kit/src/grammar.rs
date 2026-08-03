//! Gesture grammars — the closed set a kit tool may borrow.
//!
//! A grammar is *code*: the state machine that turns pointer input into a
//! committed result. There are nine, they cover every board tool that exists,
//! and a kit may reference one but never define one (Art. VII.3, VII.4). This
//! is the boundary that keeps a tool definition data rather than a program.

use serde::{Deserialize, Serialize};

/// One of the nine gesture grammars the core implements.
///
/// Adding a member is core work under Article III — it must name its real
/// recurring use, and it lands with an interaction contract in
/// `docs/keymap/contracts/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grammar {
    /// Pick, marquee, handles, grips.
    Select,
    /// Anchor / handle editing on paths.
    DirectSelect,
    /// Press-drag-release a bounding box; a click places a default size.
    /// Frame, Rect, Ellipse, and the Repository Lens portal all use this.
    DragRect,
    /// `P2.RhinoDraft`: click-move-click or press-drag-release, with a
    /// direction lock and typed magnitude. The architect's grammar.
    TwoPoint,
    /// Repeated clicks; Enter or double-click finishes. Polyline, arc, bezier.
    MultiPoint,
    /// A sampled stroke, fitted or variable-width. Pen, brush.
    Freehand,
    /// A single click places a thing. Text, sticky, stamps.
    PlacePoint,
    /// Continuous hit-test along a drag. Eraser.
    Sweep,
    /// Read a property from whatever is under the cursor. Eyedropper.
    Sample,
}

impl Grammar {
    /// Every grammar, in declaration order.
    pub const ALL: [Grammar; 9] = [
        Grammar::Select,
        Grammar::DirectSelect,
        Grammar::DragRect,
        Grammar::TwoPoint,
        Grammar::MultiPoint,
        Grammar::Freehand,
        Grammar::PlacePoint,
        Grammar::Sweep,
        Grammar::Sample,
    ];

    /// The identifier used in kit files.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Grammar::Select => "select",
            Grammar::DirectSelect => "direct_select",
            Grammar::DragRect => "drag_rect",
            Grammar::TwoPoint => "two_point",
            Grammar::MultiPoint => "multi_point",
            Grammar::Freehand => "freehand",
            Grammar::PlacePoint => "place_point",
            Grammar::Sweep => "sweep",
            Grammar::Sample => "sample",
        }
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Grammar> {
        Grammar::ALL.into_iter().find(|g| g.id() == id)
    }

    /// Whether a completed gesture of this grammar produces new nodes. The
    /// others drive selection, sampling, or deletion, so a kit tool built on
    /// them has nothing to stamp and is rejected by validation.
    #[must_use]
    pub const fn creates_nodes(self) -> bool {
        matches!(
            self,
            Grammar::DragRect
                | Grammar::TwoPoint
                | Grammar::MultiPoint
                | Grammar::Freehand
                | Grammar::PlacePoint
        )
    }

    /// Whether the gesture hands the recipe a meaningful rectangle. Grammars
    /// that do not must fall back to the recipe's own default size.
    #[must_use]
    pub const fn supplies_rect(self) -> bool {
        matches!(self, Grammar::DragRect)
    }
}

/// A grammar named in a kit file, kept as text until resolved.
///
/// Unknown names must not fail the whole file: a kit written against a newer
/// build marks that one tool unsupported and keeps the rest (the tolerance
/// `ViewKind::Unknown` gives view kinds, applied per tool).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GrammarRef(pub String);

impl GrammarRef {
    #[must_use]
    pub fn resolve(&self) -> Option<Grammar> {
        Grammar::from_id(&self.0)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<Grammar> for GrammarRef {
    fn from(g: Grammar) -> Self {
        GrammarRef(g.id().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_grammar_round_trips_through_its_id() {
        for g in Grammar::ALL {
            assert_eq!(Grammar::from_id(g.id()), Some(g), "{}", g.id());
        }
    }

    #[test]
    fn ids_are_unique_and_snake_case() {
        let mut seen = std::collections::HashSet::new();
        for g in Grammar::ALL {
            assert!(seen.insert(g.id()), "duplicate id {}", g.id());
            assert!(
                g.id().chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{} is not snake_case",
                g.id()
            );
        }
    }

    #[test]
    fn serde_uses_the_same_spelling_as_the_id() {
        for g in Grammar::ALL {
            let json = serde_json::to_string(&g).unwrap();
            assert_eq!(json, format!("\"{}\"", g.id()));
        }
    }

    #[test]
    fn unknown_grammar_resolves_to_none_without_erroring() {
        let r = GrammarRef("constraint_solve".into());
        assert_eq!(r.resolve(), None);
        assert_eq!(r.as_str(), "constraint_solve");
    }

    #[test]
    fn only_creation_grammars_can_back_a_kit_tool() {
        assert!(Grammar::DragRect.creates_nodes());
        assert!(Grammar::PlacePoint.creates_nodes());
        assert!(!Grammar::Select.creates_nodes());
        assert!(!Grammar::Sample.creates_nodes());
        assert!(!Grammar::Sweep.creates_nodes());
    }
}
