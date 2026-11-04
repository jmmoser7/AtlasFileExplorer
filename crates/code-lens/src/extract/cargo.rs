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

fn read_manifest(root: &Path) -> Result<toml::Value, LensError> {
    let content = fs::read_to_string(root.join("Cargo.toml"))?;
    toml::from_str(&content)
        .map_err(|e| LensError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
}

pub fn discover_packages(root: &Path) -> Result<Vec<PackageMeta>, LensError> {
    let manifest = read_manifest(root)?;
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
    let manifest = read_manifest(pkg_root)?;
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
    let manifest = read_manifest(pkg_root)?;
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

/// Rust extern binding to actual package name, across all declared target kinds.
/// Conflicting target-specific aliases are omitted instead of guessed.
pub(crate) fn dependency_bindings(
    root: &Path,
    pkg_root: &Path,
) -> Result<HashMap<String, String>, LensError> {
    bindings_from_manifests(&read_manifest(root)?, &read_manifest(pkg_root)?)
}

fn bindings_from_manifests(
    workspace: &toml::Value,
    package: &toml::Value,
) -> Result<HashMap<String, String>, LensError> {
    let inherited = workspace
        .get("workspace")
        .and_then(|w| w.get("dependencies"));
    let mut bindings = HashMap::new();
    let mut ambiguous = HashSet::new();
    let targets = package
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|table| table.values());
    for group in std::iter::once(package).chain(targets) {
        for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
            let Some(table) = group.get(key).and_then(toml::Value::as_table) else {
                continue;
            };
            for (binding, value) in table {
                let definition =
                    if value.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                        inherited
                            .and_then(|table| table.get(binding))
                            .ok_or_else(|| {
                                LensError::Io(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    format!("missing workspace dependency binding `{binding}`"),
                                ))
                            })?
                    } else {
                        value
                    };
                let binding = normalize_crate_name(binding);
                let actual = normalize_crate_name(&dependency_name(binding.as_str(), definition));
                if bindings
                    .get(&binding)
                    .is_some_and(|previous| previous != &actual)
                {
                    bindings.remove(&binding);
                    ambiguous.insert(binding);
                } else if !ambiguous.contains(&binding) {
                    bindings.insert(binding, actual);
                }
            }
        }
    }
    Ok(bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extern_bindings_preserve_workspace_renames_and_target_declarations() {
        let workspace: toml::Value = toml::from_str(
            "[workspace.dependencies]\nshared-alias = { package = 'actual-package', version = '1' }",
        ).unwrap();
        let package: toml::Value = toml::from_str(
            "[dependencies]\nshared-alias = { workspace = true }\nplain-name = '1'\n\
             [dev-dependencies]\ntest-alias = { package = 'test-package', version = '1' }\n\
             [build-dependencies]\nbuild-alias = { package = 'build-package', version = '1' }\n\
             [target.'cfg(windows)'.dependencies]\nwin-alias = { package = 'win-package', version = '1' }",
        ).unwrap();
        let bindings = bindings_from_manifests(&workspace, &package).unwrap();
        assert_eq!(
            bindings.get("shared_alias").map(String::as_str),
            Some("actual_package")
        );
        assert_eq!(
            bindings.get("plain_name").map(String::as_str),
            Some("plain_name")
        );
        assert_eq!(
            bindings.get("test_alias").map(String::as_str),
            Some("test_package")
        );
        assert_eq!(
            bindings.get("build_alias").map(String::as_str),
            Some("build_package")
        );
        assert_eq!(
            bindings.get("win_alias").map(String::as_str),
            Some("win_package")
        );
        assert!(!bindings.contains_key("actual_package"));
    }

    #[test]
    fn ambiguous_bindings_stay_ambiguous_and_missing_inheritance_fails() {
        let workspace: toml::Value = toml::from_str("[workspace]").unwrap();
        let package: toml::Value = toml::from_str(
            "[dependencies]\nrenamed = { package = 'first', version = '1' }\n\
             [target.'cfg(windows)'.dependencies]\nrenamed = { package = 'second', version = '1' }\n\
             [target.'cfg(unix)'.dependencies]\nrenamed = { package = 'first', version = '1' }",
        ).unwrap();
        assert!(!bindings_from_manifests(&workspace, &package)
            .unwrap()
            .contains_key("renamed"));
        let missing = toml::from_str("[dependencies]\nmissing = { workspace = true }").unwrap();
        assert!(bindings_from_manifests(&workspace, &missing).is_err());
    }
}
