//! The kit audit: every `.slatekit` file in the tree parses and resolves.
//!
//! A tool kit is data the program reads at startup, which makes a malformed one
//! a runtime surprise rather than a build failure — exactly the class of bug
//! this repository already refuses to accept for interaction contracts. The
//! audit closes that gap for the built-in kit and for any example kit committed
//! alongside the docs, using the same parser and resolver the app uses so the
//! validator cannot drift from the runtime.
//!
//! `cargo xtask kits [dir]` also points at an arbitrary folder, which is how a
//! person debugs a kit they just wrote.

use std::path::{Path, PathBuf};

use slate_kit::{Health, Kit, Registry, Scope, Severity};

use crate::MetricsError;

/// Where the compiled-in kit lives, relative to the workspace root.
pub const BUILTIN_KIT: &str = "crates/slate-kit/builtin/core.slatekit";

/// One kit file that parsed.
#[derive(Debug, Clone)]
pub struct KitFile {
    pub path: PathBuf,
    pub id: String,
    pub name: String,
    pub tools: usize,
    pub bars: usize,
}

/// Something wrong with a kit, tied to the file it came from.
#[derive(Debug, Clone)]
pub struct Finding {
    pub path: PathBuf,
    pub subject: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Audit {
    pub files: Vec<KitFile>,
    pub findings: Vec<Finding>,
}

impl Audit {
    pub fn tools(&self) -> usize {
        self.files.iter().map(|f| f.tools).sum()
    }
}

/// Audit every kit file under `root`, which is the workspace root for the
/// committed check and any folder for the ad-hoc one.
pub fn audit(root: &Path) -> Result<Audit, MetricsError> {
    let mut paths = Vec::new();
    push_kit_files(root, &mut paths)?;
    paths.sort();

    let mut audit = Audit::default();
    for path in paths {
        let text = std::fs::read_to_string(&path).map_err(|e| MetricsError::io(&path, e))?;
        let kit = match Kit::from_toml(&text) {
            Ok(kit) => kit,
            Err(e) => {
                audit.findings.push(Finding {
                    path: path.clone(),
                    subject: None,
                    message: e.to_string(),
                });
                continue;
            }
        };

        // Resolve the kit alone: a file has to stand up on its own, without
        // borrowing health from whatever else happens to be installed.
        let reg = Registry::build(&[(Scope::User, kit.clone())]);
        for tool in reg.all() {
            let trouble = match &tool.health {
                Health::Ok => None,
                Health::UnsupportedGrammar(g) => {
                    Some(format!("grammar `{g}` is not one this build implements"))
                }
                Health::UnroutableRecipe => {
                    Some("this grammar cannot produce what the recipe asks for".to_string())
                }
                Health::DuplicateId => Some("defined twice in this kit".to_string()),
                // Impossible with a single kit, and not a defect anyway.
                Health::Shadowed { .. } => None,
            };
            if let Some(message) = trouble {
                audit.findings.push(Finding {
                    path: path.clone(),
                    subject: Some(tool.def.id.clone()),
                    message,
                });
            }
        }
        for f in reg
            .findings()
            .iter()
            .filter(|f| f.severity == Severity::Error)
        {
            // Errors already reported per tool above are skipped, so a single
            // mistake is not counted twice.
            if f.subject
                .as_ref()
                .is_some_and(|s| reg.all().iter().any(|t| &t.def.id == s))
            {
                continue;
            }
            audit.findings.push(Finding {
                path: path.clone(),
                subject: f.subject.clone(),
                message: f.message.clone(),
            });
        }

        audit.files.push(KitFile {
            path,
            id: kit.id.clone(),
            name: kit.name.clone(),
            tools: kit.tools.len(),
            bars: kit.bars.len(),
        });
    }

    // The compiled-in kit is not optional: if it went missing the app would
    // start with no tool results at all.
    let builtin = root.join(BUILTIN_KIT);
    if builtin.is_file() && !audit.files.iter().any(|f| f.path == builtin) {
        audit.findings.push(Finding {
            path: builtin,
            subject: None,
            message: "the built-in kit did not parse".into(),
        });
    }

    Ok(audit)
}

fn push_kit_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), MetricsError> {
    if !dir.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| MetricsError::io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| MetricsError::io(dir, e))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            push_kit_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == slate_kit::KIT_EXT) {
            out.push(path);
        }
    }
    Ok(())
}

pub fn render(audit: &Audit) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} kit file(s), {} tool(s)\n",
        audit.files.len(),
        audit.tools()
    ));
    for f in &audit.files {
        out.push_str(&format!(
            "  {} — `{}` ({} tool(s), {} bar(s))\n",
            f.path.display(),
            f.id,
            f.tools,
            f.bars
        ));
    }
    for f in &audit.findings {
        match &f.subject {
            Some(s) => out.push_str(&format!("  ! {} [{s}]: {}\n", f.path.display(), f.message)),
            None => out.push_str(&format!("  ! {}: {}\n", f.path.display(), f.message)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "xtask-kits-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_folder_with_no_kits_audits_clean_and_says_so() {
        let d = temp_dir("empty");
        let a = audit(&d).unwrap();
        assert!(a.files.is_empty());
        assert!(a.findings.is_empty());
        assert!(render(&a).starts_with("0 kit file(s)"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_healthy_kit_is_listed_with_its_counts() {
        let d = temp_dir("healthy");
        std::fs::write(
            d.join("ok.slatekit"),
            "format_version = 1\nid = \"k\"\nname = \"K\"\n\
             [[tool]]\nid = \"rect\"\nname = \"Rect\"\ngrammar = \"drag_rect\"\n\
             recipe = { kind = \"shape\", node = \"rect\" }\n",
        )
        .unwrap();
        let a = audit(&d).unwrap();
        assert!(a.findings.is_empty(), "{}", render(&a));
        assert_eq!(a.files.len(), 1);
        assert_eq!(a.tools(), 1);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_unparseable_kit_is_reported_with_its_path() {
        let d = temp_dir("broken");
        std::fs::write(d.join("bad.slatekit"), "format_version = ").unwrap();
        let a = audit(&d).unwrap();
        assert_eq!(a.findings.len(), 1);
        assert!(a.findings[0].path.ends_with("bad.slatekit"));
        assert!(render(&a).contains("bad.slatekit"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_unroutable_tool_names_the_tool_not_just_the_file() {
        let d = temp_dir("unroutable");
        std::fs::write(
            d.join("x.slatekit"),
            "format_version = 1\nid = \"k\"\nname = \"K\"\n\
             [[tool]]\nid = \"bad\"\nname = \"Bad\"\ngrammar = \"sample\"\n\
             recipe = { kind = \"shape\", node = \"rect\" }\n",
        )
        .unwrap();
        let a = audit(&d).unwrap();
        assert_eq!(a.findings.len(), 1, "{}", render(&a));
        assert_eq!(a.findings[0].subject.as_deref(), Some("bad"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn files_that_are_not_kits_are_ignored() {
        let d = temp_dir("mixed");
        std::fs::write(d.join("notes.toml"), "nonsense = ").unwrap();
        std::fs::write(d.join("readme.md"), "hi").unwrap();
        let a = audit(&d).unwrap();
        assert!(a.files.is_empty());
        assert!(a.findings.is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
