//! The `.slatekit` file: a set of tool definitions and optional bar layouts.

use serde::{Deserialize, Serialize};

use crate::tool::ToolDef;

/// Current kit format version. Bumped only for changes a previous build cannot
/// read; additive fields default instead.
pub const FORMAT_VERSION: u32 = 1;

/// A toolbar layout: an ordered list of tool ids, grouped.
///
/// A bar names ids and nothing else. Kits do not describe pixels — the dock
/// paints them, identically in both apps (Art. X).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarDef {
    pub id: String,
    pub name: String,
    /// Tool ids, in dock order. An id may live in more than one bar.
    #[serde(default)]
    pub items: Vec<String>,
}

/// A parsed kit file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kit {
    pub format_version: u32,
    /// Stable kit identity, used to qualify tool ids across kits.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub doc: Option<String>,
    #[serde(default, rename = "tool")]
    pub tools: Vec<ToolDef>,
    #[serde(default, rename = "bar")]
    pub bars: Vec<BarDef>,
}

impl Kit {
    /// Parse a kit from TOML text.
    pub fn from_toml(text: &str) -> Result<Kit, KitError> {
        let kit: Kit = toml::from_str(text).map_err(|e| KitError::Syntax(e.to_string()))?;
        if kit.format_version > FORMAT_VERSION {
            return Err(KitError::TooNew {
                found: kit.format_version,
                supported: FORMAT_VERSION,
            });
        }
        Ok(kit)
    }

    pub fn to_toml(&self) -> Result<String, KitError> {
        toml::to_string_pretty(self).map_err(|e| KitError::Syntax(e.to_string()))
    }

    #[must_use]
    pub fn tool(&self, id: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.id == id)
    }
}

/// Why a kit file could not be read at all. Anything narrower than this is a
/// per-tool [`crate::resolve::Finding`], so one bad tool never costs the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitError {
    Syntax(String),
    TooNew { found: u32, supported: u32 },
}

impl std::fmt::Display for KitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KitError::Syntax(m) => write!(f, "{m}"),
            KitError::TooNew { found, supported } => write!(
                f,
                "kit format version {found} is newer than this build supports ({supported})"
            ),
        }
    }
}

impl std::error::Error for KitError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::Grammar;

    const SAMPLE: &str = r##"
        format_version = 1
        id = "arch"
        name = "Architect"
        author = "someone"

        [[tool]]
        id = "redline"
        name = "Redline pen"
        grammar = "freehand"
        sticky = "sticky"
        recipe = { kind = "shape", node = "path", create_style = "pinned", stroke = { width = 2.0, color = "#e8443a", cap = "round" } }

        [[tool]]
        id = "north-arrow"
        name = "North arrow"
        grammar = "drag_rect"
        recipe = { kind = "shape", node = "ellipse", default_size = [64.0, 64.0] }

        [[bar]]
        id = "annotate"
        name = "Annotate"
        items = ["redline", "north-arrow"]
    "##;

    #[test]
    fn a_kit_parses_its_tools_and_bars() {
        let kit = Kit::from_toml(SAMPLE).unwrap();
        assert_eq!(kit.id, "arch");
        assert_eq!(kit.tools.len(), 2);
        assert_eq!(kit.bars.len(), 1);
        assert_eq!(kit.bars[0].items, vec!["redline", "north-arrow"]);
        assert_eq!(
            kit.tool("redline").unwrap().grammar(),
            Some(Grammar::Freehand)
        );
    }

    #[test]
    fn a_kit_round_trips_through_toml() {
        let kit = Kit::from_toml(SAMPLE).unwrap();
        let back = Kit::from_toml(&kit.to_toml().unwrap()).unwrap();
        assert_eq!(kit, back);
    }

    #[test]
    fn a_kit_from_a_newer_build_is_refused_with_both_versions_named() {
        let err = Kit::from_toml("format_version = 99\nid = \"x\"\nname = \"X\"").unwrap_err();
        assert_eq!(
            err,
            KitError::TooNew {
                found: 99,
                supported: FORMAT_VERSION
            }
        );
        assert!(err.to_string().contains("99"));
    }

    #[test]
    fn an_empty_kit_is_legal() {
        let kit = Kit::from_toml("format_version = 1\nid = \"x\"\nname = \"X\"").unwrap();
        assert!(kit.tools.is_empty());
        assert!(kit.bars.is_empty());
    }

    #[test]
    fn a_stray_top_level_key_is_reported_rather_than_ignored() {
        let err =
            Kit::from_toml("format_version = 1\nid = \"x\"\nname = \"X\"\ntoolz = []").unwrap_err();
        assert!(matches!(err, KitError::Syntax(_)));
    }
}
