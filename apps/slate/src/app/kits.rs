//! Tool kits in the app: where they are read from and how the board asks for a
//! recipe.
//!
//! The model, parsing, and resolution all live in `slate-kit` (pure, testable
//! without a window). This module is only the app-side seam: the folder to scan,
//! the registry the board consults, and the runtime values a recipe defers to.

use std::path::PathBuf;

use slate_kit::{BuildCtx, Finding, Kit, Recipe, Registry, Severity};

use super::board::BoardTool;

/// Directory scanned for user tool kits, beside the theme folder: every
/// `.slatekit` file in it is loaded, and its tools shadow the built-ins they
/// share an id with.
///
/// It is under `data_dir()` and not the install directory on purpose — the
/// program's own folder is not writable on a normal install, and a tool the
/// user authored has to survive an upgrade.
pub fn user_kit_dir() -> PathBuf {
    atlas_core::index::data_dir().join("tools")
}

/// The board's tool registry: the built-in kit plus the user's, resolved.
#[derive(Debug, Clone)]
pub struct KitState {
    pub registry: Registry,
    /// Everything worth telling the user about the kits that loaded. Surfaced
    /// in Advanced rather than as a modal — a bad kit must never block startup.
    pub findings: Vec<Finding>,
}

impl KitState {
    /// Load the built-in kit and every kit in the user's folder.
    pub fn load() -> KitState {
        Self::load_from(Some(&user_kit_dir()), &[])
    }

    /// Load with an explicit user folder, for tests and for a future
    /// workbook-scoped set.
    pub fn load_from(user_dir: Option<&std::path::Path>, workbook: &[Kit]) -> KitState {
        let (registry, findings) = slate_kit::registry_for(user_dir, workbook);
        KitState { registry, findings }
    }

    /// Just the built-in kit — the registry a headless test starts from, so a
    /// stray file in the developer's own kit folder cannot change a test result.
    pub fn builtin_only() -> KitState {
        Self::load_from(None, &[])
    }

    /// The result recipe for a shipped board tool, if its result is data yet.
    pub fn recipe_for(&self, tool: BoardTool) -> Option<&Recipe> {
        let id = tool.kit_id()?;
        self.registry.get(id).map(|t| &t.def.recipe)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
    }
}

impl Default for KitState {
    fn default() -> Self {
        KitState::builtin_only()
    }
}

/// The runtime values a recipe defers to the app.
pub fn build_ctx(accent: slate_doc::scene::Rgba, next_frame_order: u32) -> BuildCtx {
    BuildCtx {
        accent,
        next_frame_order,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_kit::Grammar;

    #[test]
    fn all_lists_every_board_tool_exactly_once() {
        // The match below is exhaustive with no catch-all: adding a `BoardTool`
        // variant stops compiling here until it is listed in `ALL` too, which is
        // what keeps the tests in this module from silently skipping a tool.
        fn tag(t: BoardTool) -> u8 {
            match t {
                BoardTool::Select => 0,
                BoardTool::Pan => 1,
                BoardTool::Frame => 2,
                BoardTool::RectShape => 3,
                BoardTool::Ellipse => 4,
                BoardTool::Line => 5,
                BoardTool::Arc => 6,
                BoardTool::Polyline => 7,
                BoardTool::BezierSpan => 8,
                BoardTool::Pen => 9,
                BoardTool::Text => 10,
                BoardTool::Brush => 11,
                BoardTool::Eraser => 12,
                BoardTool::Eyedropper => 13,
                BoardTool::Sticky => 14,
                BoardTool::DirectSelect => 15,
                BoardTool::RepoLens => 16,
                BoardTool::AgentPortal => 17,
                BoardTool::WebPortal => 18,
            }
        }
        let mut tags: Vec<u8> = BoardTool::ALL.into_iter().map(tag).collect();
        tags.sort_unstable();
        assert_eq!(tags, (0..19).collect::<Vec<u8>>());
    }

    #[test]
    fn every_shipped_tool_names_a_grammar_the_core_implements() {
        for tool in BoardTool::ALL {
            assert!(
                Grammar::ALL.contains(&tool.grammar()),
                "{tool:?} names an unknown grammar"
            );
        }
    }

    #[test]
    fn the_drag_rect_family_is_exactly_the_tools_that_start_a_draw_gesture() {
        let by_grammar: Vec<BoardTool> = BoardTool::ALL
            .into_iter()
            .filter(|t| t.grammar() == Grammar::DragRect)
            .collect();
        assert_eq!(
            by_grammar,
            vec![
                BoardTool::Frame,
                BoardTool::RectShape,
                BoardTool::Ellipse,
                BoardTool::RepoLens,
                BoardTool::AgentPortal,
                BoardTool::WebPortal,
            ],
            "begin_gesture's Draw arm and grammar() must agree"
        );
    }

    #[test]
    fn a_recipe_is_available_for_every_tool_that_claims_a_kit_entry() {
        let kits = KitState::builtin_only();
        for tool in BoardTool::ALL {
            match tool.kit_id() {
                Some(id) => assert!(
                    kits.recipe_for(tool).is_some(),
                    "{tool:?} claims kit entry `{id}` but no recipe resolved"
                ),
                None => assert!(kits.recipe_for(tool).is_none()),
            }
        }
    }

    #[test]
    fn a_tool_with_a_recipe_is_always_a_grammar_that_recipe_accepts() {
        let kits = KitState::builtin_only();
        for tool in BoardTool::ALL {
            if let Some(recipe) = kits.recipe_for(tool) {
                assert!(
                    recipe.accepts(tool.grammar()),
                    "{tool:?}'s grammar cannot drive its recipe"
                );
            }
        }
    }

    #[test]
    fn the_builtin_registry_loads_without_errors() {
        let kits = KitState::builtin_only();
        assert_eq!(
            kits.errors().count(),
            0,
            "{:#?}",
            kits.errors().collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_user_kit_folder_sits_next_to_the_theme_folder() {
        assert_eq!(
            user_kit_dir().parent(),
            atlas_shell::theme::user_theme_dir().parent()
        );
    }
}
