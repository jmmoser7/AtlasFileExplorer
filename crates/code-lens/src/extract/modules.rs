use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{LensNode, NodeId, NodeKind};

use super::GraphBuilder;

#[derive(Debug, Default)]
pub struct ModuleMaps {
    pub file_by_path: std::collections::HashMap<PathBuf, NodeId>,
    pub module_by_path: std::collections::HashMap<PathBuf, NodeId>,
}

/// Legacy callers retain direct src children. Observatory callers also receive
/// explicit physical source-root directories and integration-test files.
pub(crate) fn walk_package_sources(
    builder: &mut GraphBuilder,
    package_id: NodeId,
    pkg_rel: &Path,
    pkg_root: &Path,
    include_tests: bool,
) -> Result<ModuleMaps, std::io::Error> {
    if !include_tests {
        return walk_package_src(builder, package_id, pkg_rel, &pkg_root.join("src"));
    }
    let mut maps = ModuleMaps::default();
    for name in ["src", "tests"] {
        let source_root = pkg_root.join(name);
        if !source_root.is_dir() {
            continue;
        }
        let root_path = pkg_rel.join(name);
        let root_id = builder.push_node(
            Some(package_id),
            NodeKind::Module,
            name.into(),
            root_path.clone(),
            0,
        );
        let walked = walk_package_src(builder, root_id, pkg_rel, &source_root)?;
        maps.module_by_path.insert(root_path, root_id);
        maps.module_by_path.extend(walked.module_by_path);
        maps.file_by_path.extend(walked.file_by_path);
    }
    Ok(maps)
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
            let rel_module = source_rel_path(pkg_rel, src_root, &path);
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
            let rel_file = source_rel_path(pkg_rel, src_root, &path);
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

fn source_rel_path(pkg_rel: &Path, source_root: &Path, path: &Path) -> PathBuf {
    let under_source = path.strip_prefix(source_root).unwrap_or(path);
    let mut rel = pkg_rel.to_path_buf();
    if let Some(name) = source_root.file_name() {
        rel.push(name);
    }
    rel.push(under_source);
    rel
}

pub fn parent_module_for_file(maps: &ModuleMaps, file_rel: &Path) -> Option<NodeId> {
    let parent = file_rel.parent()?;
    if parent.ends_with("src") {
        return None;
    }
    maps.module_by_path.get(parent).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_source_paths_preserve_the_root_directory() {
        let package = Path::new("crates/example");
        for root in ["src", "tests"] {
            let source = Path::new("checkout/example").join(root);
            assert_eq!(
                source_rel_path(package, &source, &source.join("nested/case.rs")),
                package.join(root).join("nested/case.rs")
            );
        }
    }

    #[test]
    fn test_files_are_opt_in_and_keep_their_physical_directory() {
        let temp = std::env::temp_dir();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = temp.join(format!(
            "code-lens-source-roots-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("tests/nested")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn source() {}\n").unwrap();
        fs::write(root.join("tests/nested/case.rs"), "#[test] fn case() {}\n").unwrap();

        let mut legacy = GraphBuilder::new();
        let package = legacy.push_node(
            None,
            NodeKind::Package { is_app: false },
            "test".into(),
            "pkg".into(),
            0,
        );
        let maps =
            walk_package_sources(&mut legacy, package, Path::new("pkg"), &root, false).unwrap();
        assert_eq!(maps.file_by_path.len(), 1);
        assert!(maps.module_by_path.is_empty());
        let source = maps.file_by_path[Path::new("pkg/src/lib.rs")];
        assert_eq!(legacy.nodes()[source as usize].parent, Some(package));

        let mut full = GraphBuilder::new();
        let package = full.push_node(
            None,
            NodeKind::Package { is_app: false },
            "test".into(),
            "pkg".into(),
            0,
        );
        let maps = walk_package_sources(&mut full, package, Path::new("pkg"), &root, true).unwrap();
        assert_eq!(maps.file_by_path.len(), 2);
        assert!(maps.module_by_path.contains_key(Path::new("pkg/src")));
        assert!(maps.module_by_path.contains_key(Path::new("pkg/tests")));
        let case = maps.file_by_path[Path::new("pkg/tests/nested/case.rs")];
        let nested = maps.module_by_path[Path::new("pkg/tests/nested")];
        assert_eq!(full.nodes()[case as usize].parent, Some(nested));

        assert_eq!(root.parent(), Some(temp.as_path()));
        assert!(root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("code-lens-source-roots-"));
        fs::remove_dir_all(root).unwrap();
    }
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
