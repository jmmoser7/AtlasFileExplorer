//! Proven module routes from explicit Rust `mod` declarations. Physical
//! directories alone never establish a semantic namespace.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::model::NodeId;

use super::{Facts, SourceInfo};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Address {
    pub root: String,
    pub parts: Vec<String>,
}

pub(super) struct Namespaces {
    pub files: HashMap<NodeId, Vec<Address>>,
    pub modules: BTreeMap<Address, Vec<NodeId>>,
    pub libraries: HashMap<String, String>,
    pub module_edges: Vec<(NodeId, NodeId)>,
    pub unresolved: usize,
}

impl Namespaces {
    pub fn unique_route(&self, address: &Address, facts: &Facts) -> bool {
        for length in 0..=address.parts.len() {
            let prefix = Address {
                root: address.root.clone(),
                parts: address.parts[..length].to_vec(),
            };
            if self
                .modules
                .get(&prefix)
                .is_some_and(|nodes| nodes.len() != 1)
            {
                return false;
            }
        }
        let mut parent = address.clone();
        if let Some(name) = parent.parts.pop() {
            if let Some(scopes) = self.modules.get(&parent) {
                if scopes.iter().any(|scope| {
                    facts
                        .definitions
                        .get(&(*scope, name.clone()))
                        .is_some_and(|nodes| nodes.len() > 1)
                }) {
                    return false;
                }
            }
        }
        true
    }

    pub fn build(facts: &Facts) -> Self {
        let mut result = Self {
            files: HashMap::new(),
            modules: BTreeMap::new(),
            libraries: HashMap::new(),
            module_edges: Vec::new(),
            unresolved: 0,
        };
        let by_path: HashMap<_, _> = facts
            .files
            .iter()
            .map(|f| (f.path.clone(), f.node))
            .collect();
        let by_id: HashMap<_, _> = facts.files.iter().map(|f| (f.node, f)).collect();
        for file in &facts.files {
            let relative = file
                .path
                .strip_prefix(&file.package_path)
                .unwrap_or(&file.path);
            let text = super::relative(relative);
            let root = if text == "src/lib.rs" {
                let root = format!("lib:{}", file.package_name);
                result
                    .libraries
                    .insert(file.package_name.replace('-', "_"), root.clone());
                Some(root)
            } else if text == "src/main.rs"
                || (text.starts_with("src/bin/")
                    && (relative.parent() == Some(Path::new("src/bin"))
                        || text.ends_with("/main.rs")))
            {
                Some(format!("bin:{}:{}", file.package_name, text))
            } else if relative.parent() == Some(Path::new("tests")) {
                Some(format!("test:{}:{}", file.package_name, text))
            } else {
                None
            };
            if let Some(root) = root {
                result.add(
                    file.node,
                    Address {
                        root,
                        parts: Vec::new(),
                    },
                );
            }
        }
        let mut resolved = HashSet::new();
        for _ in 0..32 {
            let mut changed = false;
            for (index, declaration) in facts.modules.iter().enumerate() {
                let Some(addresses) = result.files.get(&declaration.file).cloned() else {
                    continue;
                };
                for mut address in addresses {
                    address.parts.extend(declaration.inline.iter().cloned());
                    address.parts.push(declaration.name.clone());
                    if address.parts.len() > 32 {
                        continue;
                    }
                    if declaration.external {
                        if declaration.uncertain_path {
                            continue;
                        }
                        let file = by_id[&declaration.file];
                        let candidates = targets(
                            &file.path,
                            &declaration.inline,
                            &declaration.name,
                            declaration.path.as_deref(),
                        );
                        let targets: Vec<_> = candidates
                            .into_iter()
                            .filter_map(|p| by_path.get(&p).copied())
                            .collect();
                        if targets.len() == 1 {
                            changed |= result.add(targets[0], address);
                            result.module_edges.push((declaration.node, targets[0]));
                            resolved.insert(index);
                        }
                    } else {
                        let nodes = result.modules.entry(address).or_default();
                        if !nodes.contains(&declaration.node) {
                            nodes.push(declaration.node);
                            changed = true;
                        }
                        resolved.insert(index);
                    }
                }
            }
            if !changed {
                break;
            }
        }
        result.module_edges.sort();
        result.module_edges.dedup();
        result.unresolved = facts.modules.len() - resolved.len();
        result
    }

    fn add(&mut self, file: NodeId, address: Address) -> bool {
        let addresses = self.files.entry(file).or_default();
        if addresses.contains(&address) || addresses.len() >= 32 {
            return false;
        }
        addresses.push(address.clone());
        let nodes = self.modules.entry(address).or_default();
        if !nodes.contains(&file) {
            nodes.push(file);
        }
        true
    }

    pub fn at(
        &self,
        scope: NodeId,
        facts: &Facts,
        source: &[SourceInfo],
        names: impl Fn(NodeId) -> String,
    ) -> Vec<Address> {
        let Some(file) = facts.file_for.get(&scope) else {
            return Vec::new();
        };
        let mut inline = Vec::new();
        let mut cursor = scope;
        while cursor != *file {
            if source[cursor as usize].kind == "module" {
                inline.push(names(cursor));
            }
            let Some(parent) = facts.parents.get(&cursor) else {
                break;
            };
            cursor = *parent;
        }
        inline.reverse();
        self.files
            .get(file)
            .into_iter()
            .flatten()
            .cloned()
            .map(|mut a| {
                a.parts.extend(inline.iter().cloned());
                a
            })
            .collect()
    }

    pub fn lookup(&self, address: &Address, facts: &Facts) -> Vec<NodeId> {
        for length in 0..=address.parts.len() {
            let prefix = Address {
                root: address.root.clone(),
                parts: address.parts[..length].to_vec(),
            };
            if self
                .modules
                .get(&prefix)
                .is_some_and(|nodes| nodes.len() != 1)
            {
                return Vec::new();
            }
        }
        if let Some(nodes) = self.modules.get(address) {
            return nodes.clone();
        }
        let mut parent = address.clone();
        let Some(name) = parent.parts.pop() else {
            return Vec::new();
        };
        let result: Vec<_> = self
            .modules
            .get(&parent)
            .into_iter()
            .flatten()
            .flat_map(|scope| {
                facts
                    .definitions
                    .get(&(*scope, name.clone()))
                    .into_iter()
                    .flatten()
                    .copied()
            })
            .collect();
        if result.len() == 1 {
            result
        } else {
            Vec::new()
        }
    }
}

fn targets(file: &Path, inline: &[String], name: &str, explicit: Option<&str>) -> Vec<PathBuf> {
    let Some(parent) = file.parent() else {
        return Vec::new();
    };
    if let Some(explicit) = explicit {
        // Inline #[path] rules have target-specific directory subtleties; skip
        // them rather than guessing. Top-level literal paths are unambiguous.
        if !inline.is_empty() || Path::new(explicit).is_absolute() {
            return Vec::new();
        }
        return normalized(&parent.join(explicit)).into_iter().collect();
    }
    let mut directory = parent.to_path_buf();
    if !["lib.rs", "main.rs", "mod.rs"]
        .iter()
        .any(|name| file.file_name().is_some_and(|f| f == *name))
    {
        let Some(stem) = file.file_stem() else {
            return Vec::new();
        };
        directory.push(stem);
    }
    for part in inline {
        directory.push(part);
    }
    vec![
        directory.join(format!("{name}.rs")),
        directory.join(name).join("mod.rs"),
    ]
}

fn normalized(path: &Path) -> Option<PathBuf> {
    let mut output = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(p) => output.push(p),
            Component::CurDir => {}
            Component::ParentDir => {
                if !output.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(output)
}
