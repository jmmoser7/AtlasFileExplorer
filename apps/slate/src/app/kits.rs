//! Tool kits in the app: where they are read from and how the board asks for a
//! recipe.
//!
//! The model, parsing, and resolution all live in `slate-kit` (pure, testable
//! without a window). This module is only the app-side seam: the folder to scan,
//! the registry the board consults, and the runtime values a recipe defers to.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use slate_kit::{BuildCtx, Finding, Kit, Recipe, Registry, Severity, FORMAT_VERSION};

use super::board::BoardTool;

const COPIES_KIT_ID: &str = "user-copies";
const COPIES_FILE: &str = "copies.slatekit";

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
        self.recipe_for_id(tool.kit_id()?)
    }

    /// Recipe for a kit tool id (built-in or a catalog duplicate).
    pub fn recipe_for_id(&self, id: &str) -> Option<&Recipe> {
        self.registry.get(id).map(|t| &t.def.recipe)
    }

    pub fn tool_name(&self, id: &str) -> Option<&str> {
        self.registry.get(id).map(|t| t.def.name.as_str())
    }

    pub fn is_derived(&self, id: &str) -> bool {
        let kit = kit_id_for_catalog(id).unwrap_or(id);
        self.registry
            .get(kit)
            .is_some_and(|t| t.def.derived_from.is_some())
    }

    /// Walk `derived_from` to the built-in seed this copy started from.
    pub fn root_kit_id(&self, kit_id: &str) -> String {
        let mut cur = kit_id.to_string();
        for _ in 0..32 {
            match self
                .registry
                .get(&cur)
                .and_then(|t| t.def.derived_from.clone())
            {
                Some(parent) => cur = parent,
                None => break,
            }
        }
        cur
    }

    /// Board tool a catalog card or kit id arms. Copies resolve to their seed.
    pub fn board_tool_for(&self, id: &str) -> Option<BoardTool> {
        let kit = kit_id_for_catalog(id).unwrap_or(id);
        if self.registry.get(kit).is_none() && kit_id_for_catalog(id).is_none() {
            return None;
        }
        match self.root_kit_id(kit).as_str() {
            "portal-repo-lens" => Some(BoardTool::RepoLens),
            "portal-status-board" => Some(BoardTool::StatusBoard),
            "portal-agent" => Some(BoardTool::AgentPortal),
            "portal-web" => Some(BoardTool::WebPortal),
            "portal-file-atlas" => Some(BoardTool::AtlasPortal),
            "rect" => Some(BoardTool::RectShape),
            "ellipse" => Some(BoardTool::Ellipse),
            "frame" => Some(BoardTool::Frame),
            _ => None,
        }
    }

    /// User-authored copies that belong on a catalog group (same seed family).
    pub fn derived_in_group(&self, group: &str) -> Vec<DerivedCatalogItem> {
        self.registry
            .active()
            .filter(|t| t.def.derived_from.is_some())
            .filter_map(|t| {
                let root = self.root_kit_id(&t.def.id);
                let item_group = group_for_kit(&root)?;
                if item_group != group {
                    return None;
                }
                Some(DerivedCatalogItem {
                    id: intern_id(&t.def.id),
                    name: t.def.name.clone(),
                    description: t.def.doc.clone().unwrap_or_else(|| {
                        format!(
                            "Custom tool derived from {}.",
                            t.def.derived_from.as_deref().unwrap_or(&root)
                        )
                    }),
                    group: item_group,
                })
            })
            .collect()
    }

    /// Clone selected Advanced-catalog cards into the user kit folder.
    pub fn duplicate_catalog_ids(&mut self, ids: &[String]) -> usize {
        self.duplicate_catalog_ids_in(&user_kit_dir(), ids)
    }

    pub fn duplicate_catalog_ids_in(&mut self, dir: &Path, ids: &[String]) -> usize {
        let mut copies = load_copies_kit(dir);
        let mut n = 0;
        for id in ids {
            let Some(seed) = self.seed_def(id).cloned() else {
                continue;
            };
            let new_id = unique_copy_id(&self.registry, &copies, &seed.id);
            let new_name = unique_copy_name(&self.registry, &copies, &seed.name);
            let mut def = seed.clone();
            def.derived_from = Some(seed.id);
            def.id = new_id;
            def.name = new_name;
            def.key = None;
            if def.doc.as_deref().unwrap_or("").is_empty() {
                def.doc = Some(format!("Copy of {}.", seed.name));
            }
            copies.tools.push(def);
            n += 1;
        }
        if n == 0 {
            return 0;
        }
        if write_copies_kit(dir, &copies).is_err() {
            return 0;
        }
        *self = KitState::load_from(Some(dir), &[]);
        n
    }

    fn seed_def(&self, catalog_or_kit: &str) -> Option<&slate_kit::ToolDef> {
        let kit = kit_id_for_catalog(catalog_or_kit).unwrap_or(catalog_or_kit);
        self.registry.get(kit).map(|t| &t.def)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
    }
}

/// A user copy that should appear as an extra card in an Advanced catalog group.
pub struct DerivedCatalogItem {
    pub id: &'static str,
    pub name: String,
    pub description: String,
    pub group: &'static str,
}

fn kit_id_for_catalog(catalog_id: &str) -> Option<&'static str> {
    match catalog_id {
        "portal.repo" => Some("portal-repo-lens"),
        "portal.status" => Some("portal-status-board"),
        "portal.agent" => Some("portal-agent"),
        "portal.web" => Some("portal-web"),
        "portal.atlas" => Some("portal-file-atlas"),
        "shape.rect" => Some("rect"),
        "shape.ellipse" => Some("ellipse"),
        _ => None,
    }
}

fn group_for_kit(kit_id: &str) -> Option<&'static str> {
    match kit_id {
        "portal-repo-lens"
        | "portal-status-board"
        | "portal-agent"
        | "portal-web"
        | "portal-file-atlas" => Some("portals"),
        "rect" | "ellipse" => Some("shapes"),
        "frame" => Some("frame"),
        _ => None,
    }
}

pub(crate) fn intern_label(s: &str) -> &'static str {
    intern_id(s)
}

fn intern_id(s: &str) -> &'static str {
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut pool = POOL
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .expect("intern pool");
    if let Some(existing) = pool.get(s).copied() {
        return existing;
    }
    let leaked: &'static str = Box::leak(s.to_owned().into_boxed_str());
    pool.insert(leaked);
    leaked
}

fn copies_path(dir: &Path) -> PathBuf {
    dir.join(COPIES_FILE)
}

fn empty_copies_kit() -> Kit {
    Kit {
        format_version: FORMAT_VERSION,
        id: COPIES_KIT_ID.into(),
        name: "User copies".into(),
        author: None,
        doc: Some("Tools duplicated from the Advanced catalog.".into()),
        tools: Vec::new(),
        bars: Vec::new(),
    }
}

fn load_copies_kit(dir: &Path) -> Kit {
    let path = copies_path(dir);
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(kit) = Kit::from_toml(&text) {
            return kit;
        }
    }
    empty_copies_kit()
}

fn write_copies_kit(dir: &Path, kit: &Kit) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = kit
        .to_toml()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(copies_path(dir), text)
}

fn unique_copy_id(reg: &Registry, copies: &Kit, seed_id: &str) -> String {
    let taken = |id: &str| reg.get(id).is_some() || copies.tool(id).is_some();
    let stem = format!("{seed_id}-copy");
    if !taken(&stem) {
        return stem;
    }
    (2..)
        .map(|n| format!("{stem}-{n}"))
        .find(|id| !taken(id))
        .expect("copy id")
}

fn unique_copy_name(reg: &Registry, copies: &Kit, seed_name: &str) -> String {
    let taken = |name: &str| {
        reg.active().any(|t| t.def.name == name) || copies.tools.iter().any(|t| t.name == name)
    };
    let stem = format!("{seed_name} copy");
    if !taken(&stem) {
        return stem;
    }
    (2..)
        .map(|n| format!("{stem} {n}"))
        .find(|name| !taken(name))
        .expect("copy name")
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
                BoardTool::StatusBoard => 17,
                BoardTool::AgentPortal => 18,
                BoardTool::WebPortal => 19,
                BoardTool::AtlasPortal => 20,
                BoardTool::Trim => 21,
                BoardTool::Split => 22,
            }
        }
        let mut tags: Vec<u8> = BoardTool::ALL.into_iter().map(tag).collect();
        tags.sort_unstable();
        assert_eq!(tags, (0..23).collect::<Vec<u8>>());
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
                BoardTool::StatusBoard,
                BoardTool::AgentPortal,
                BoardTool::WebPortal,
                BoardTool::AtlasPortal,
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

    #[test]
    fn duplicating_a_catalog_card_writes_a_derived_kit_tool() {
        let dir = std::env::temp_dir().join(format!(
            "atlas-kit-dup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp kit dir");
        let mut kits = KitState::load_from(Some(&dir), &[]);
        assert_eq!(
            kits.duplicate_catalog_ids_in(&dir, &["portal.web".into()]),
            1
        );
        let copy = kits
            .registry
            .get("portal-web-copy")
            .expect("derived tool loaded");
        assert_eq!(copy.def.derived_from.as_deref(), Some("portal-web"));
        assert_eq!(copy.def.name, "Web portal copy");
        assert_eq!(
            kits.board_tool_for("portal-web-copy"),
            Some(BoardTool::WebPortal)
        );
        assert!(kits.is_derived("portal-web-copy"));
        assert!(!kits.is_derived("portal.web"));
        let extras = kits.derived_in_group("portals");
        assert!(extras.iter().any(|e| e.id == "portal-web-copy"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
