//! One invertible rewrite of linked paths.
//!
//! Collecting pasted and generated images into the workbook folder changes
//! locators and nothing else. Undo puts the old locators back; the copied
//! files may remain on disk.

use std::path::{Path, PathBuf};

use crate::ids::ItemId;
use crate::scene::{self, NodeKind};
use crate::SlateDoc;

const ASSETS_DIR: &str = "assets";

/// One item whose locator changes. `before` is the path the item holds when
/// the command is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatorChange {
    pub item: ItemId,
    pub before: PathBuf,
    pub after: PathBuf,
}

/// Journaled relocation of workbook-owned image locators (Article VI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteLocators {
    pub changes: Vec<LocatorChange>,
}

impl RewriteLocators {
    pub fn inverted(&self) -> Self {
        Self {
            changes: self
                .changes
                .iter()
                .map(|change| LocatorChange {
                    item: change.item,
                    before: change.after.clone(),
                    after: change.before.clone(),
                })
                .collect(),
        }
    }

    /// All or nothing. A missing item or a locator that is no longer
    /// `before` leaves the document untouched.
    pub fn apply(&self, doc: &mut SlateDoc) -> bool {
        if self.changes.is_empty() {
            return false;
        }
        for change in &self.changes {
            let Some(item) = doc.item(change.item) else {
                return false;
            };
            if !same_locator(&item.path, &change.before) {
                return false;
            }
        }
        for change in &self.changes {
            let before = doc
                .item(change.item)
                .map(|item| item.path.clone())
                .unwrap_or_else(|| change.before.clone());
            if !doc.set_item_locator(change.item, change.after.clone()) {
                return false;
            }
            rewrite_output_sources(&mut doc.scene, &before, &change.after);
            rewrite_output_sources(&mut doc.scene, &change.before, &change.after);
        }
        true
    }
}

/// On save, paths that already live under this workbook's `assets/` are
/// stored relative to the workbook file. Other links stay as they are.
pub fn persist_asset_locators(doc: &mut SlateDoc, workbook: &Path) {
    let items: Vec<(ItemId, PathBuf)> = doc
        .items
        .iter()
        .filter_map(|item| {
            let resolved = scene::resolve_source(Some(workbook), &item.path.to_string_lossy());
            if !under_assets(workbook, &resolved) && !locator_is_asset(&item.path) {
                return None;
            }
            let locator = PathBuf::from(scene::source_locator(Some(workbook), &resolved));
            (locator != item.path).then_some((item.id, locator))
        })
        .collect();
    for (id, locator) in items {
        if let Some(before) = doc.item(id).map(|item| item.path.clone()) {
            let _ = doc.set_item_locator(id, locator.clone());
            rewrite_output_sources(&mut doc.scene, &before, &locator);
        }
    }
}

fn rewrite_output_sources(scene: &mut scene::Scene, before: &Path, after: &Path) {
    for node in &mut scene.nodes {
        let agent = match &mut node.kind {
            NodeKind::Image(image) => image.agent.as_mut(),
            NodeKind::Portal(portal) => portal.agent.as_mut(),
            NodeKind::Text(text) => text.agent.as_mut(),
            NodeKind::Frame(_)
            | NodeKind::Shape(_)
            | NodeKind::Connector(_)
            | NodeKind::DockStrip(_) => None,
        };
        if let Some(agent) = agent {
            if let Some(seed) = agent.seed.as_mut() {
                if same_locator(Path::new(&seed.source), before) {
                    seed.source = after.to_string_lossy().replace('\\', "/");
                }
            }
        }
    }
}

fn under_assets(workbook: &Path, path: &Path) -> bool {
    let Some(dir) = workbook.parent() else {
        return false;
    };
    path.strip_prefix(dir.join(ASSETS_DIR)).is_ok()
}

fn locator_is_asset(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('\\', "/");
    text == ASSETS_DIR || text.starts_with(&format!("{ASSETS_DIR}/"))
}

fn same_locator(a: &Path, b: &Path) -> bool {
    let left = a.to_string_lossy().replace('\\', "/");
    let right = b.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{ImageNode, NodeKind, WorldRect};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn a_locator_rewrite_is_invertible() {
        let mut doc = SlateDoc::new("assets");
        let before = PathBuf::from("/data/pasted/paste-1.png");
        let after = PathBuf::from("assets/pasted/paste-1.png");
        let id = doc.add_item(before.clone(), "paste-1.png", 4, 0, "k");
        let rect = WorldRect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        doc.scene
            .build_node(rect, NodeKind::Image(ImageNode::new(id)));
        let cmd = RewriteLocators {
            changes: vec![LocatorChange {
                item: id,
                before: before.clone(),
                after: after.clone(),
            }],
        };
        assert!(cmd.apply(&mut doc));
        assert_eq!(doc.item(id).unwrap().path, after);
        assert!(!cmd.apply(&mut doc), "a second apply does not match");
        assert!(cmd.inverted().apply(&mut doc));
        assert_eq!(doc.item(id).unwrap().path, before);
    }

    #[test]
    fn moving_the_workbook_folder_resolves_relative_asset_locators() {
        let from = temp_dir("wb-from");
        let workbook = from.join("Board.slate");
        fs::create_dir_all(from.join("assets").join("pasted")).unwrap();
        fs::write(
            from.join("assets").join("pasted").join("paste-1.png"),
            b"new",
        )
        .unwrap();
        fs::write(from.join("assets").join("paste-old.png"), b"old").unwrap();
        let mut doc = SlateDoc::new("Board");
        doc.add_item(
            PathBuf::from("assets/pasted/paste-1.png"),
            "paste-1.png",
            3,
            0,
            "a",
        );
        doc.add_item(
            PathBuf::from("assets/paste-old.png"),
            "paste-old.png",
            3,
            0,
            "b",
        );
        doc.save_to(&workbook).unwrap();

        let to = temp_dir("wb-to");
        copy_tree(&from, &to).unwrap();
        let loaded = SlateDoc::load_from(&to.join("Board.slate")).unwrap();
        for (locator, bytes) in [
            ("assets/pasted/paste-1.png", &b"new"[..]),
            ("assets/paste-old.png", &b"old"[..]),
        ] {
            let resolved = scene::resolve_source(Some(&to.join("Board.slate")), locator);
            assert_eq!(fs::read(resolved).unwrap(), bytes, "{locator}");
            assert!(loaded
                .items
                .iter()
                .any(|item| item.path.as_path() == Path::new(locator)));
        }
        let _ = fs::remove_dir_all(from);
        let _ = fs::remove_dir_all(to);
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let dest = to.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_tree(&entry.path(), &dest)?;
            } else {
                fs::copy(entry.path(), dest)?;
            }
        }
        Ok(())
    }
}
