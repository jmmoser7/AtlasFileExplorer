//! Tool definitions — one grammar plus one recipe, plus how it presents.

use serde::{Deserialize, Serialize};

use crate::grammar::{Grammar, GrammarRef};
use crate::recipe::Recipe;

/// Whether the tool stays armed after a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sticky {
    /// Commit returns to Select. Correct for tools you reach for once.
    #[default]
    OneShot,
    /// Stays armed for repeat placement. Escape disarms.
    Sticky,
}

/// Snap defaults a tool wants on arm. These are preferences, not new
/// behaviours: the board already implements each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SnapDefaults {
    pub grid: bool,
    pub geometry: bool,
    pub angle: bool,
}

/// An icon named in a kit file. Kits reference glyphs the build ships; they
/// cannot supply image files, which would make a tool file an asset bundle and
/// a security surface.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IconRef(pub String);

impl IconRef {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A tool as a kit file declares it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDef {
    /// Stable identity within the kit. A user kit shadows a built-in tool by
    /// reusing its id, which is how "edit an existing tool" works without
    /// mutating the shipped file.
    pub id: String,
    /// Menu and command-palette label.
    pub name: String,
    pub grammar: GrammarRef,
    #[serde(default)]
    pub icon: IconRef,
    /// Optional accelerator. Leaving it unset is recommended: type-to-command
    /// already reaches any tool by name, and bare letters are a scarce shared
    /// resource that a kit cannot arbitrate.
    #[serde(default)]
    pub key: Option<String>,
    /// Extra names type-to-command should match, for people arriving from
    /// other software.
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub sticky: Sticky,
    #[serde(default)]
    pub snap: SnapDefaults,
    /// The tool this one started as, recorded when a contract was seeded by
    /// right-clicking an existing tool. Provenance, not inheritance: the
    /// definition is complete on its own.
    #[serde(default)]
    pub derived_from: Option<String>,
    /// One-line description for the command reference.
    #[serde(default)]
    pub doc: Option<String>,
    pub recipe: Recipe,
}

impl ToolDef {
    #[must_use]
    pub fn grammar(&self) -> Option<Grammar> {
        self.grammar.resolve()
    }

    /// Every string type-to-command should match against.
    #[must_use]
    pub fn search_terms(&self) -> Vec<&str> {
        let mut v = vec![self.name.as_str(), self.id.as_str()];
        v.extend(self.aliases.iter().map(String::as_str));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: &str = r#"
        id = "rect"
        name = "Rectangle"
        grammar = "drag_rect"
        recipe = { kind = "shape", node = "rect" }
    "#;

    #[test]
    fn a_minimal_tool_needs_only_id_name_grammar_and_recipe() {
        let t: ToolDef = toml::from_str(RECT).unwrap();
        assert_eq!(t.grammar(), Some(Grammar::DragRect));
        assert_eq!(t.sticky, Sticky::OneShot);
        assert_eq!(t.key, None);
        assert!(t.icon.is_empty());
        assert_eq!(t.snap, SnapDefaults::default());
    }

    #[test]
    fn search_terms_cover_name_id_and_aliases() {
        let t: ToolDef = toml::from_str(
            r#"
            id = "line-2pt"
            name = "Line"
            grammar = "two_point"
            aliases = ["polyline", "l"]
            recipe = { kind = "shape", node = "path" }
        "#,
        )
        .unwrap();
        assert_eq!(t.search_terms(), vec!["Line", "line-2pt", "polyline", "l"]);
    }

    #[test]
    fn an_unknown_grammar_leaves_the_tool_parseable_but_unresolved() {
        let t: ToolDef = toml::from_str(
            r#"
            id = "solver"
            name = "Constraint solver"
            grammar = "constraint_solve"
            recipe = { kind = "shape", node = "rect" }
        "#,
        )
        .unwrap();
        assert_eq!(t.grammar(), None);
        assert_eq!(t.grammar.as_str(), "constraint_solve");
    }

    #[test]
    fn a_misspelled_key_is_rejected_rather_than_ignored() {
        let bad = RECT.to_string() + "\nsticy = \"sticky\"\n";
        assert!(toml::from_str::<ToolDef>(&bad).is_err());
    }

    #[test]
    fn a_tool_round_trips_through_toml() {
        let t: ToolDef = toml::from_str(RECT).unwrap();
        let back: ToolDef = toml::from_str(&toml::to_string(&t).unwrap()).unwrap();
        assert_eq!(t, back);
    }
}
