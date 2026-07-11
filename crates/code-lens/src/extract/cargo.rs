use crate::model::LensError;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
#[derive(Debug, Clone)]
pub struct PackageMeta {
    pub rel_path: PathBuf,
    pub name: String,
    pub is_app: bool,
}
pub fn discover_packages(root: &Path) -> Result<Vec<PackageMeta>, LensError> {
    let content = fs::read_to_string(root.join("Cargo.toml"))?;
    let manifest: toml::Value = toml::from_str(&content)
        .map_err(|e| LensError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    let member_paths = if let Some(members) = manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        let mut paths = Vec::new();
        for member in members {
            if let Some(pattern) = member.as_str() {
                paths.extend(expand_member_pattern(root, pattern)?);
            }
        }
        paths.sort();
        paths.dedup();
        paths
    } else {
        vec![PathBuf::from(".")]
    };
    let mut packages = Vec::new();
    for rel in member_paths {
        let pkg_root = if rel.as_os_str() == "." {
            root.to_path_buf()
        } else {
            root.join(&rel)
        };
        if pkg_root.join("Cargo.toml").is_file() {
            packages.push(parse_package_meta(root, &pkg_root)?);
        }
    }
    packages.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(packages)
}
pub fn workspace_dependency_names(
    pkg_root: &Path,
    member_names: &HashSet<String>,
) -> Result<Vec<String>, LensError> {
    let content = fs::read_to_string(pkg_root.join("Cargo.toml"))?;
    let manifest: toml::Value = toml::from_str(&content)
        .map_err(|e| LensError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    let mut deps = Vec::new();
    for table_key in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = manifest.get(table_key).and_then(|v| v.as_table()) {
            for (key, value) in table {
                let dep_name = dependency_name(key, value);
                if member_names.contains(&normalize_crate_name(&dep_name)) {
                    deps.push(dep_name);
                }
            }
        }
    }
    deps.sort();
    deps.dedup();
    Ok(deps)
}
pub fn package_name_index(packages: &[PackageMeta]) -> HashMap<String, PathBuf> {
    packages
        .iter()
        .map(|p| (normalize_crate_name(&p.name), p.rel_path.clone()))
        .collect()
}
pub fn normalize_crate_name(name: &str) -> String {
    name.replace('-', "_")
}
fn expand_member_pattern(root: &Path, pattern: &str) -> Result<Vec<PathBuf>, LensError> {
    if let Some(star_idx) = pattern.find('*') {
        let prefix = pattern[..star_idx].trim_end_matches('/');
        let base = if prefix.is_empty() {
            root.to_path_buf()
        } else {
            root.join(prefix)
        };
        if !base.is_dir() {
            return Ok(Vec::new());
        }
        let mut entries: Vec<_> = fs::read_dir(&base)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        Ok(entries
            .into_iter()
            .filter_map(|e| {
                let path = e.path();
                if path.join("Cargo.toml").is_file() {
                    Some(
                        path.strip_prefix(root)
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|_| path.file_name().unwrap().into()),
                    )
                } else {
                    None
                }
            })
            .collect())
    } else {
        Ok(vec![PathBuf::from(pattern)])
    }
}
fn parse_package_meta(root: &Path, pkg_root: &Path) -> Result<PackageMeta, LensError> {
    let rel_path = pkg_root
        .strip_prefix(root)
        .map(|p| {
            if p.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                p.to_path_buf()
            }
        })
        .unwrap_or_else(|_| PathBuf::from("."));
    let content = fs::read_to_string(pkg_root.join("Cargo.toml"))?;
    let manifest: toml::Value = toml::from_str(&content)
        .map_err(|e| LensError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    let name = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string();
    let is_app = rel_path
        .components()
        .any(|c| matches!(c, Component::Normal(s) if s == "apps"))
        || pkg_root.join("src/main.rs").is_file();
    Ok(PackageMeta {
        rel_path,
        name,
        is_app,
    })
}
fn dependency_name(key: &str, value: &toml::Value) -> String {
    if let Some(table) = value.as_table() {
        if let Some(rename) = table.get("package").and_then(|p| p.as_str()) {
            return rename.to_string();
        }
    }
    key.to_string()
}
