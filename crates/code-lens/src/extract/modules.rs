use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{LensNode, NodeId, NodeKind};

use super::GraphBuilder;

#[derive(Debug, Default)]
pub struct ModuleMaps {
    pub file_by_path: std::collections::HashMap<PathBuf, NodeId>,
    pub module_by_path: std::collections::HashMap<PathBuf, NodeId>,
}

pub(crate) fn walk_package_src(
    builder: &mut GraphBuilder,
    package_id: NodeId,
    pkg_rel: &Path,
    src_dir: &Path,
) -> Result<ModuleMaps, std::io::Error> {
    let mut maps = ModuleMaps::default();
    if !src_dir.is_dir() {
        return Ok(maps);
    }
    walk_dir(builder, package_id, pkg_rel, src_dir, src_dir, &mut maps)?;
    Ok(maps)
}

fn walk_dir(
    builder: &mut GraphBuilder,
    parent_id: NodeId,
    pkg_rel: &Path,
    src_root: &Path,
    dir: &Path,
    maps: &mut ModuleMaps,
) -> Result<(), std::io::Error> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();

        if path.is_dir() {
            let rel_module = module_rel_path(pkg_rel, src_root, &path);
            let module_id = builder.push_node(
                Some(parent_id),
                NodeKind::Module,
                file_name.clone(),
                rel_module.clone(),
                0,
            );
            maps.module_by_path.insert(rel_module, module_id);
            walk_dir(builder, module_id, pkg_rel, src_root, &path, maps)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let rel_file = file_rel_path(pkg_rel, src_root, &path);
            let file_id = builder.push_node(
                Some(parent_id),
                NodeKind::File,
                file_name,
                rel_file.clone(),
                0,
            );
            maps.file_by_path.insert(rel_file, file_id);
        }
    }

    Ok(())
}

fn module_rel_path(pkg_rel: &Path, src_root: &Path, module_dir: &Path) -> PathBuf {
    let under_src = module_dir.strip_prefix(src_root).unwrap_or(module_dir);
    let mut rel = pkg_rel.to_path_buf();
    rel.push("src");
    rel.push(under_src);
    rel
}

fn file_rel_path(pkg_rel: &Path, src_root: &Path, file: &Path) -> PathBuf {
    let under_src = file.strip_prefix(src_root).unwrap_or(file);
    let mut rel = pkg_rel.to_path_buf();
    rel.push("src");
    rel.push(under_src);
    rel
}

pub fn parent_module_for_file(maps: &ModuleMaps, file_rel: &Path) -> Option<NodeId> {
    let parent = file_rel.parent()?;
    if parent.ends_with("src") {
        return None;
    }
    maps.module_by_path.get(parent).copied()
}

pub fn module_for_file(maps: &ModuleMaps, file_rel: &Path) -> Option<NodeId> {
    parent_module_for_file(maps, file_rel)
}

pub fn resolve_in_container(
    nodes: &[LensNode],
    container_id: NodeId,
    segments: &[String],
) -> Option<NodeId> {
    if segments.is_empty() {
        return Some(container_id);
    }

    let seg = &segments[0];
    let children = &nodes[container_id as usize].children;

    for &child_id in children {
        let child = &nodes[child_id as usize];
        match child.kind {
            NodeKind::Module if child.name == *seg => {
                return resolve_in_container(nodes, child_id, &segments[1..]);
            }
            NodeKind::File if child.name == format!("{seg}.rs") => {
                if segments.len() == 1 {
                    return Some(child_id);
                }
                return None;
            }
            _ => {}
        }
    }

    None
}
