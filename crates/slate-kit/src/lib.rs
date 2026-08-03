//! Declarative tool kits for the Slate board.
//!
//! A board tool is two things: a **gesture grammar** (how the pointer is read)
//! and a **result recipe** (what the commit produces). The grammars are code
//! and there are nine of them; the recipes are data. Splitting them is what
//! makes a tool something a person can author without a compiler, and what
//! keeps the core from growing a variant per tool anyone ever wanted
//! (Art. III, Art. VII.3).
//!
//! ```
//! use slate_kit::{Kit, Registry, Scope};
//!
//! let kit = Kit::from_toml(
//!     r##"
//!     format_version = 1
//!     id = "mine"
//!     name = "My tools"
//!
//!     [[tool]]
//!     id = "redline"
//!     name = "Redline pen"
//!     grammar = "freehand"
//!     sticky = "sticky"
//!
//!     [tool.recipe]
//!     kind = "shape"
//!     node = "path"
//!     create_style = "pinned"
//!     stroke = { width = 2.0, color = "#e8443a", cap = "round" }
//!     "##,
//! )
//! .unwrap();
//!
//! let reg = Registry::build(&[(Scope::User, kit)]);
//! assert_eq!(reg.get("redline").unwrap().def.name, "Redline pen");
//! ```
//!
//! This crate is pure: no egui, no renderer, no app state (Art. I.1). It
//! depends on `slate-doc` because a recipe's whole job is to name node kinds
//! and style properties the document model already has — it can never invent
//! one, which is what keeps the board painter and the HTML artifact writer in
//! agreement (Art. IV).

pub mod color;
pub mod grammar;
pub mod kit;
pub mod recipe;
pub mod resolve;
pub mod style;

pub use color::{ColorRef, KitColor};
pub use grammar::{Grammar, GrammarRef};
pub use kit::{BarDef, Kit, KitError, FORMAT_VERSION};
pub use recipe::{
    BuildCtx, CreateStyle, FrameSpec, NodeSpec, NodeTarget, PortalRecipe, Recipe, ShapeRecipe,
    TextSpec,
};
pub use resolve::{Finding, Health, Registry, ResolvedBar, ResolvedTool, Scope, Severity};
pub use style::{Cap, Join, Profile, StrokeSpec};
pub use tool::{IconRef, SnapDefaults, Sticky, ToolDef};

pub mod tool;

/// File extension for kit files.
pub const KIT_EXT: &str = "slatekit";

/// The kit compiled into the build, holding the board's own tool defaults.
///
/// It loads through exactly the same path as a user's kit — no privileged
/// parser, no fields only the built-in may use. If this file cannot express a
/// shipped tool, that is the format's bug and not the tool's exemption.
pub const BUILTIN_KIT_TOML: &str = include_str!("../builtin/core.slatekit");

/// Parse the built-in kit.
///
/// # Panics
/// If the compiled-in kit is malformed, which a unit test in this crate and
/// `cargo xtask kits` both prevent from reaching a build.
#[must_use]
pub fn builtin_kit() -> Kit {
    Kit::from_toml(BUILTIN_KIT_TOML).expect("the built-in kit is validated by `cargo xtask kits`")
}

/// Read every `.slatekit` file directly inside `dir`.
///
/// A missing directory is not an error — most installs have never had one. A
/// file that fails to parse costs that file and is reported, so one bad kit
/// cannot stop the others from loading.
#[must_use]
pub fn load_dir(dir: &std::path::Path) -> (Vec<Kit>, Vec<Finding>) {
    let mut kits = Vec::new();
    let mut findings = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (kits, findings);
    };
    // Sort by file name so load order — and therefore tie-breaks within a
    // scope — does not depend on the filesystem.
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(KIT_EXT))
        .collect();
    paths.sort();

    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        match std::fs::read_to_string(&path) {
            Err(e) => findings.push(Finding {
                severity: Severity::Error,
                kit: name,
                subject: None,
                message: format!("could not be read: {e}"),
            }),
            Ok(text) => match Kit::from_toml(&text) {
                Ok(kit) => kits.push(kit),
                Err(e) => findings.push(Finding {
                    severity: Severity::Error,
                    kit: name,
                    subject: None,
                    message: e.to_string(),
                }),
            },
        }
    }
    (kits, findings)
}

/// Build the registry the board should use: the built-in kit, then the user's
/// kit folder, then any workbook-scoped kits.
#[must_use]
pub fn registry_for(
    user_dir: Option<&std::path::Path>,
    workbook: &[Kit],
) -> (Registry, Vec<Finding>) {
    let mut scoped: Vec<(Scope, Kit)> = vec![(Scope::Builtin, builtin_kit())];
    let mut findings = Vec::new();
    if let Some(dir) = user_dir {
        let (kits, mut f) = load_dir(dir);
        findings.append(&mut f);
        scoped.extend(kits.into_iter().map(|k| (Scope::User, k)));
    }
    scoped.extend(workbook.iter().cloned().map(|k| (Scope::Workbook, k)));
    let reg = Registry::build(&scoped);
    findings.extend(reg.findings().iter().cloned());
    (reg, findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::{NodeKind, Rgba, ShapeKind, WorldRect};

    fn ctx() -> BuildCtx {
        BuildCtx {
            accent: Rgba([10, 120, 240, 255]),
            next_frame_order: 0,
        }
    }

    #[test]
    fn the_builtin_kit_parses_and_resolves_without_a_single_error() {
        let reg = Registry::build(&[(Scope::Builtin, builtin_kit())]);
        assert!(
            reg.errors().is_empty(),
            "built-in kit has errors: {:#?}",
            reg.errors()
        );
        assert!(reg.active().count() >= 4);
    }

    #[test]
    fn the_builtin_kit_carries_the_tools_finish_draw_used_to_hardcode() {
        let reg = Registry::build(&[(Scope::Builtin, builtin_kit())]);
        for id in ["frame", "rect", "ellipse", "portal-repo-lens"] {
            assert!(reg.get(id).is_some(), "built-in kit is missing `{id}`");
        }
    }

    #[test]
    fn the_builtin_rect_recipe_reproduces_the_previous_hardcoded_shape() {
        let reg = Registry::build(&[(Scope::Builtin, builtin_kit())]);
        let rect = reg.get("rect").unwrap();
        let specs = rect
            .def
            .recipe
            .instantiate(WorldRect::new(0.0, 0.0, 10.0, 10.0), &ctx());
        let NodeKind::Shape(s) = &specs[0].kind else {
            panic!("expected a shape");
        };
        assert_eq!(s.shape, ShapeKind::Rect);
        // The old constants: accent at alpha 60 filled, 2px solid accent
        // stroke, square corners.
        assert_eq!(s.fill, Some(Rgba([10, 120, 240, 60])));
        assert_eq!(s.stroke.width, 2.0);
        assert_eq!(s.stroke.color, Rgba([10, 120, 240, 255]));
        assert_eq!(s.corner, slate_doc::scene::Corner::Square);
        assert!(s.path.is_none());
        assert!(!s.flip);
    }

    #[test]
    fn the_builtin_kit_round_trips_through_the_writer() {
        let kit = builtin_kit();
        let back = Kit::from_toml(&kit.to_toml().unwrap()).unwrap();
        assert_eq!(kit, back);
    }

    #[test]
    fn the_builtin_kit_declares_no_accelerators() {
        // Bare letters are the app's to arbitrate in `commands.rs`; a kit that
        // claimed them would fight the command table.
        for t in &builtin_kit().tools {
            assert_eq!(t.key, None, "`{}` claims an accelerator", t.id);
        }
    }

    #[test]
    fn a_missing_user_kit_folder_is_not_an_error() {
        let (kits, findings) = load_dir(std::path::Path::new("/definitely/not/here"));
        assert!(kits.is_empty());
        assert!(findings.is_empty());
    }

    #[test]
    fn a_user_kit_folder_loads_alongside_the_builtin_and_can_shadow_it() {
        let dir = std::env::temp_dir().join(format!("slate-kit-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mine.slatekit"),
            r#"
            format_version = 1
            id = "mine"
            name = "Mine"

            [[tool]]
            id = "rect"
            name = "Rounded rectangle"
            grammar = "drag_rect"
            recipe = { kind = "shape", node = "rect", corner = { rounded = { radius = 8.0 } } }
            "#,
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        std::fs::write(dir.join("broken.slatekit"), "format_version = ").unwrap();

        let (reg, findings) = registry_for(Some(&dir), &[]);

        // The user's rect wins; the built-in one is shadowed, not lost.
        let rect = reg.get("rect").unwrap();
        assert_eq!(rect.scope, Scope::User);
        assert_eq!(rect.def.name, "Rounded rectangle");
        assert!(reg.all().iter().any(|t| t.scope == Scope::Builtin
            && t.def.id == "rect"
            && matches!(t.health, Health::Shadowed { .. })));

        // The built-in tools the user did not override are still there.
        assert_eq!(reg.get("ellipse").unwrap().scope, Scope::Builtin);

        // The unparseable file is reported by name and costs only itself.
        let broken: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.kit == "broken.slatekit")
            .collect();
        assert_eq!(broken.len(), 1, "{findings:#?}");
        assert_eq!(broken[0].severity, Severity::Error);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
