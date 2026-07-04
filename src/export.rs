//! Copy-only export engine. Sources are never touched. A JSON manifest is
//! written into the destination recording every source->dest mapping, the
//! tags, and any renames — the organization scheme as a documented artifact.
//! Undo-by-manifest deletes exactly the copies we created, nothing else.

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct ExportItem {
    pub source: PathBuf,
    pub rel: String,
    pub dest_rel: String,
    pub new_name: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Serialize)]
struct ManifestItem {
    source: String,
    dest: String,
    tags: Vec<String>,
    renamed_from: Option<String>,
}

#[derive(Serialize)]
struct Manifest {
    tool: &'static str,
    exported_at: i64,
    source_root: String,
    dest_root: String,
    items: Vec<ManifestItem>,
}

pub enum ExportMsg {
    Progress {
        done: usize,
        total: usize,
        current: String,
    },
    Done {
        manifest_path: String,
        copied: Vec<String>,
        created_dirs: Vec<String>,
        errors: Vec<String>,
    },
}

pub fn start_export(
    source_root: PathBuf,
    dest_root: PathBuf,
    items: Vec<ExportItem>,
) -> Receiver<ExportMsg> {
    let (tx, rx) = unbounded::<ExportMsg>();
    std::thread::spawn(move || run_export(source_root, dest_root, items, tx));
    rx
}

fn run_export(
    source_root: PathBuf,
    dest_root: PathBuf,
    items: Vec<ExportItem>,
    tx: Sender<ExportMsg>,
) {
    let total = items.len();
    let mut copied: Vec<String> = Vec::new();
    let mut created_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    let mut manifest_items: Vec<ManifestItem> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let _ = tx.send(ExportMsg::Progress {
            done: i,
            total,
            current: item.rel.clone(),
        });

        let dir = dest_root.join(&item.dest_rel);
        if !dir.exists() {
            if std::fs::create_dir_all(&dir).is_ok() {
                // Record every directory level we may have created.
                let mut d = dir.clone();
                while d.starts_with(&dest_root) && d != dest_root {
                    created_dirs.insert(d.clone());
                    if !d.pop() {
                        break;
                    }
                }
            } else {
                errors.push(format!("could not create folder {}", dir.display()));
                continue;
            }
        }

        let name = item.new_name.clone().unwrap_or_else(|| {
            item.source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| item.rel.clone())
        });
        let dest = unique_dest(&dir, &name);

        match std::fs::copy(&item.source, &dest) {
            Ok(_) => {
                copied.push(dest.to_string_lossy().into_owned());
                manifest_items.push(ManifestItem {
                    source: item.source.to_string_lossy().into_owned(),
                    dest: dest.to_string_lossy().into_owned(),
                    tags: item.tags.clone(),
                    renamed_from: item.new_name.as_ref().map(|_| {
                        item.source
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    }),
                });
            }
            Err(e) => errors.push(format!("{}: {}", item.rel, e)),
        }
    }

    let manifest = Manifest {
        tool: "native-file-atlas",
        exported_at: crate::scanner::now_unix(),
        source_root: source_root.to_string_lossy().into_owned(),
        dest_root: dest_root.to_string_lossy().into_owned(),
        items: manifest_items,
    };
    let stamp = crate::scanner::now_unix();
    let manifest_path = dest_root.join(format!("file-atlas-manifest-{stamp}.json"));
    if let Ok(json) = serde_json::to_string_pretty(&manifest) {
        let _ = std::fs::write(&manifest_path, json);
    }

    let _ = tx.send(ExportMsg::Done {
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        copied,
        created_dirs: created_dirs
            .into_iter()
            .rev() // deepest first, so undo can rmdir in order
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        errors,
    });
}

fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for i in 2.. {
        let c = dir.join(format!("{stem} ({i}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    unreachable!()
}

/// Undo an export: delete exactly the files we copied and any now-empty
/// directories we created, plus the manifest itself. Sources are untouched.
pub fn undo_export(manifest_path: &str, copied: &[String], created_dirs: &[String]) -> usize {
    let mut removed = 0;
    for f in copied {
        if std::fs::remove_file(f).is_ok() {
            removed += 1;
        }
    }
    let _ = std::fs::remove_file(manifest_path);
    for d in created_dirs {
        // Only removes if empty — never deletes user content.
        let _ = std::fs::remove_dir(d);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_copies_writes_manifest_and_undoes() {
        let base = std::env::temp_dir().join(format!("nfa_export_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        let dst = base.join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("photo.jpg"), b"jpegdata").unwrap();
        std::fs::write(src.join("model.3dm"), b"rhinodata").unwrap();

        let items = vec![
            ExportItem {
                source: src.join("photo.jpg"),
                rel: "photo.jpg".into(),
                dest_rel: r"Renders\Final".into(),
                new_name: Some("hero-shot.jpg".into()),
                tags: vec!["hero".into()],
            },
            ExportItem {
                source: src.join("model.3dm"),
                rel: "model.3dm".into(),
                dest_rel: "Models".into(),
                new_name: None,
                tags: vec![],
            },
        ];

        let rx = start_export(src.clone(), dst.clone(), items);
        let done = loop {
            match rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap() {
                ExportMsg::Progress { .. } => continue,
                done @ ExportMsg::Done { .. } => break done,
            }
        };
        let ExportMsg::Done {
            manifest_path,
            copied,
            created_dirs,
            errors,
        } = done
        else {
            unreachable!()
        };

        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(copied.len(), 2);
        assert!(dst.join(r"Renders\Final\hero-shot.jpg").exists());
        assert!(dst.join(r"Models\model.3dm").exists());
        assert!(std::path::Path::new(&manifest_path).exists());
        // Sources untouched.
        assert!(src.join("photo.jpg").exists());
        assert!(src.join("model.3dm").exists());
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["items"].as_array().unwrap().len(), 2);
        assert_eq!(manifest["items"][0]["tags"][0], "hero");

        // Undo removes copies + empty dirs but never the sources.
        let removed = undo_export(&manifest_path, &copied, &created_dirs);
        assert_eq!(removed, 2);
        assert!(!dst.join(r"Renders\Final\hero-shot.jpg").exists());
        assert!(!dst.join("Models").exists());
        assert!(src.join("photo.jpg").exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn collision_gets_numbered_suffix() {
        let base = std::env::temp_dir().join(format!("nfa_dup_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("f.txt"), b"x").unwrap();
        let p = unique_dest(&base, "f.txt");
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "f (2).txt");
        let _ = std::fs::remove_dir_all(&base);
    }
}
