//! Conservative name resolution. Ambiguity and unsupported dispatch produce
//! unresolved counts, never a guessed nearest or globally same-named symbol.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::model::{CodeGraph, NodeId};

use super::namespaces::{Address, Namespaces};
use super::{ExtractionCounts, Facts, PathRef, SourceInfo, SourceRelation};

pub(super) fn resolve(
    graph: &CodeGraph,
    source: &[SourceInfo],
    facts: &Facts,
    counts: &mut ExtractionCounts,
) -> Vec<SourceRelation> {
    let namespaces = Namespaces::build(facts);
    counts.unresolved_modules = namespaces.unresolved;
    let mut imports = HashMap::<(NodeId, String), Vec<usize>>::new();
    for (index, import) in facts.imports.iter().enumerate().filter(|(_, i)| !i.glob) {
        imports
            .entry((import.scope, import.alias.clone()))
            .or_default()
            .push(index);
    }
    let owners = facts.files.iter().map(|f| (f.node, f.package)).collect();
    let packages = facts
        .files
        .iter()
        .map(|f| (f.package_name.replace('-', "_"), f.package))
        .collect();
    let mut resolver = Resolver {
        graph,
        source,
        facts,
        namespaces,
        imports,
        owners,
        packages,
        impl_types: HashMap::new(),
        methods: HashMap::new(),
    };
    for implementation in &facts.impls {
        if let Some(path) = &implementation.self_type {
            if let Some(node) = resolver.one(implementation.scope, path, 0) {
                if ["struct", "enum"].contains(&source[node as usize].kind) {
                    resolver.impl_types.insert(implementation.node, node);
                }
            }
        }
    }
    for implementation in &facts.impls {
        if implementation.trait_type.is_some() {
            continue;
        }
        if let Some(ty) = resolver.impl_types.get(&implementation.node) {
            for child in &graph.node(implementation.node).children {
                if source[*child as usize].kind == "method" {
                    resolver
                        .methods
                        .entry((*ty, graph.node(*child).name.clone()))
                        .or_default()
                        .push(*child);
                }
            }
        }
    }
    let mut edges = BTreeMap::new();
    for (from, to) in &resolver.namespaces.module_edges {
        add(&mut edges, *from, *to, "module");
    }
    counts.observed_imports = facts.imports.len();
    for import in &facts.imports {
        if let Some(target) = resolver.one(import.scope, &import.path, 0) {
            add(&mut edges, import.scope, target, "import");
        } else {
            counts.unresolved_imports += 1;
        }
    }
    for implementation in &facts.impls {
        if let Some(path) = &implementation.trait_type {
            match resolver.one(implementation.scope, path, 0) {
                Some(target) if source[target as usize].kind == "trait" => {
                    add(&mut edges, implementation.node, target, "implements")
                }
                _ => counts.unresolved_impls += 1,
            }
        }
    }
    counts.observed_calls = facts.calls.len();
    for call in &facts.calls {
        let target = if let Some(path) = &call.path {
            if path.parts.len() == 1 && resolver.shadowed(call.scope, &path.parts[0]) {
                None
            } else {
                resolver
                    .one(call.scope, path, 0)
                    .filter(|id| ["function", "method"].contains(&source[*id as usize].kind))
            }
        } else if let Some(method) = &call.self_method {
            resolver.self_method(call.scope, method)
        } else {
            None
        };
        if let Some(target) = target {
            add(&mut edges, call.caller, target, "call");
        } else {
            counts.unresolved_calls += 1;
        }
    }
    edges
        .into_iter()
        .map(|((from, to, kind), weight)| SourceRelation {
            from,
            to,
            kind,
            weight,
        })
        .collect()
}

fn add(
    edges: &mut BTreeMap<(NodeId, NodeId, &'static str), u32>,
    from: NodeId,
    to: NodeId,
    kind: &'static str,
) {
    if from != to || kind == "call" {
        *edges.entry((from, to, kind)).or_default() += 1;
    }
}

struct Resolver<'a> {
    graph: &'a CodeGraph,
    source: &'a [SourceInfo],
    facts: &'a Facts,
    namespaces: Namespaces,
    impl_types: HashMap<NodeId, NodeId>,
    methods: HashMap<(NodeId, String), Vec<NodeId>>,
    imports: HashMap<(NodeId, String), Vec<usize>>,
    owners: HashMap<NodeId, NodeId>,
    packages: HashMap<String, NodeId>,
}

impl Resolver<'_> {
    fn one(&self, scope: NodeId, path: &PathRef, depth: usize) -> Option<NodeId> {
        let result: BTreeSet<_> = self.path(scope, path, depth).into_iter().collect();
        (result.len() == 1).then(|| *result.first().unwrap())
    }

    fn path(&self, scope: NodeId, path: &PathRef, depth: usize) -> Vec<NodeId> {
        if depth > 16 || path.parts.is_empty() {
            return Vec::new();
        }
        let first = &path.parts[0];
        if !path.absolute && self.type_shadowed(scope, first) {
            return Vec::new();
        }
        if first == "Self" {
            return if path.parts.len() == 2 {
                self.self_method(scope, &path.parts[1])
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            };
        }
        if ["crate", "self", "super"].contains(&first.as_str()) {
            let addresses = self.namespaces.at(scope, self.facts, self.source, |n| {
                self.graph.node(n).name.clone()
            });
            return addresses
                .into_iter()
                .flat_map(|mut address| {
                    let mut index = 1;
                    if first == "crate" {
                        address.parts.clear();
                    }
                    if first == "super" {
                        if address.parts.pop().is_none() {
                            return Vec::new();
                        }
                        while path.parts.get(index).is_some_and(|p| p == "super") {
                            if address.parts.pop().is_none() {
                                return Vec::new();
                            }
                            index += 1;
                        }
                    }
                    address.parts.extend(path.parts[index..].iter().cloned());
                    self.canonical(scope, address, depth + 1)
                })
                .collect();
        }
        if !path.absolute {
            let mut current = scope;
            loop {
                // Impl/trait members are associated items, not unqualified
                // lexical names in their method bodies.
                if !["impl", "trait"].contains(&self.source[current as usize].kind) {
                    if self.ambiguous_binding(current, first) {
                        return Vec::new();
                    }
                    if let Some(found) = self.facts.definitions.get(&(current, first.clone())) {
                        if found.len() != 1 {
                            return Vec::new();
                        }
                        return self.extend(found.clone(), &path.parts[1..], depth + 1);
                    }
                    if let Some(aliases) = self.imports.get(&(current, first.clone())) {
                        if aliases.len() != 1 {
                            return Vec::new();
                        }
                        return aliases
                            .iter()
                            .map(|i| &self.facts.imports[*i])
                            .flat_map(|alias| {
                                let mut imported = alias.path.clone();
                                imported.parts.extend(path.parts[1..].iter().cloned());
                                self.path(alias.scope, &imported, depth + 1)
                            })
                            .collect();
                    }
                }
                let globs: Vec<_> = self
                    .facts
                    .imports
                    .iter()
                    .filter(|i| i.scope == current && i.glob)
                    .collect();
                if !globs.is_empty() {
                    if globs.len() != 1
                        || !["file", "module"].contains(&self.source[current as usize].kind)
                    {
                        return Vec::new();
                    }
                    // Ancestor members are accessible to descendants. Other
                    // globs require visibility analysis we do not infer.
                    let known = globs.iter().all(|g| {
                        !g.path.parts.is_empty()
                            && (g.path.parts.iter().all(|p| p == "super")
                                || g.path.parts == ["crate"])
                            && self.one(g.scope, &g.path, depth + 1).is_some_and(|id| {
                                ["module", "file"].contains(&self.source[id as usize].kind)
                            })
                    });
                    if !known {
                        return Vec::new();
                    }
                    return globs
                        .into_iter()
                        .flat_map(|glob| {
                            let mut expanded = glob.path.clone();
                            expanded.parts.extend(path.parts.iter().cloned());
                            self.path(glob.scope, &expanded, depth + 1)
                        })
                        .collect();
                }
                if ["file", "module"].contains(&self.source[current as usize].kind) {
                    break;
                }
                let Some(parent) = self.facts.parents.get(&current) else {
                    break;
                };
                current = *parent;
            }
            // A relative module path must be proven by `mod` declarations.
            let mut relative = Vec::new();
            for mut address in self.namespaces.at(scope, self.facts, self.source, |n| {
                self.graph.node(n).name.clone()
            }) {
                address.parts.extend(path.parts.iter().cloned());
                relative.extend(self.canonical(scope, address, depth + 1));
            }
            if !relative.is_empty() {
                return relative;
            }
        }
        let Some(package_name) = self.external_package(scope, &first.replace('-', "_")) else {
            return Vec::new();
        };
        self.namespaces
            .libraries
            .get(&package_name)
            .map(|root| {
                self.canonical(
                    scope,
                    Address {
                        root: root.clone(),
                        parts: path.parts[1..].to_vec(),
                    },
                    depth + 1,
                )
            })
            .unwrap_or_default()
    }

    fn canonical(&self, scope: NodeId, address: Address, depth: usize) -> Vec<NodeId> {
        if !self.namespaces.unique_route(&address, self.facts) {
            return Vec::new();
        }
        for length in 1..=address.parts.len() {
            let parent = Address {
                root: address.root.clone(),
                parts: address.parts[..length - 1].to_vec(),
            };
            if self.namespaces.modules.get(&parent).is_some_and(|scopes| {
                scopes
                    .iter()
                    .any(|id| self.ambiguous_binding(*id, &address.parts[length - 1]))
            }) {
                return Vec::new();
            }
        }
        let direct = self.namespaces.lookup(&address, self.facts);
        if !direct.is_empty() {
            return direct;
        }
        // Resolve explicit re-export aliases, preserving their full path.
        let mut parent = address.clone();
        let Some(name) = parent.parts.pop() else {
            return Vec::new();
        };
        let containers = self
            .namespaces
            .modules
            .get(&parent)
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::new();
        for container in containers {
            if self
                .imports
                .get(&(container, name.clone()))
                .is_some_and(|items| items.len() != 1)
            {
                return Vec::new();
            }
            for index in self
                .imports
                .get(&(container, name.clone()))
                .into_iter()
                .flatten()
            {
                let import = &self.facts.imports[*index];
                out.extend(self.path(import.scope, &import.path, depth + 1));
            }
        }
        // `Type::associated_function` needs a unique inherent method.
        if out.is_empty() && depth <= 16 {
            let mut ty = address;
            let Some(method) = ty.parts.pop() else {
                return out;
            };
            let types = self.canonical(scope, ty, depth + 1);
            out.extend(self.extend(types, &[method], depth + 1));
        }
        out
    }

    fn extend(&self, nodes: Vec<NodeId>, remaining: &[String], depth: usize) -> Vec<NodeId> {
        if remaining.is_empty() {
            return nodes;
        }
        if depth > 16 {
            return Vec::new();
        }
        let mut output = Vec::new();
        for node in nodes {
            if ["module", "file"].contains(&self.source[node as usize].kind) {
                for mut address in self.namespaces.at(node, self.facts, self.source, |n| {
                    self.graph.node(n).name.clone()
                }) {
                    address.parts.extend(remaining.iter().cloned());
                    output.extend(self.canonical(node, address, depth + 1));
                }
            } else if remaining.len() == 1 {
                output.extend(
                    self.methods
                        .get(&(node, remaining[0].clone()))
                        .into_iter()
                        .flatten()
                        .copied(),
                );
            }
        }
        output
    }

    fn external_package(&self, scope: NodeId, name: &str) -> Option<String> {
        let Some(file) = self.facts.file_for.get(&scope) else {
            return None;
        };
        let Some(owner) = self.owners.get(file) else {
            return None;
        };
        if let Some(actual) = self
            .facts
            .extern_aliases
            .get(owner)
            .and_then(|aliases| aliases.get(name))
        {
            return Some(actual.clone());
        }
        if self.packages.get(name) != Some(owner) {
            return None;
        }
        let addresses = self.namespaces.at(scope, self.facts, self.source, |n| {
            self.graph.node(n).name.clone()
        });
        (!addresses.is_empty()
            && addresses
                .iter()
                .all(|a| a.root.starts_with("bin:") || a.root.starts_with("test:")))
        .then(|| name.to_string())
    }

    fn ambiguous_binding(&self, scope: NodeId, name: &str) -> bool {
        let key = (scope, name.to_owned());
        self.facts.definitions.get(&key).map_or(0, Vec::len)
            + self.imports.get(&key).map_or(0, Vec::len)
            > 1
    }

    fn self_method(&self, scope: NodeId, name: &str) -> Option<NodeId> {
        let mut cursor = scope;
        loop {
            if let Some(ty) = self.impl_types.get(&cursor) {
                let methods = self.methods.get(&(*ty, name.to_string()))?;
                let unique: HashSet<_> = methods.iter().copied().collect();
                return (unique.len() == 1).then(|| *unique.iter().next().unwrap());
            }
            cursor = *self.facts.parents.get(&cursor)?;
        }
    }

    fn shadowed(&self, scope: NodeId, name: &str) -> bool {
        let mut cursor = scope;
        loop {
            if self
                .facts
                .bindings
                .get(&cursor)
                .is_some_and(|b| b.contains(name))
            {
                return true;
            }
            if ["file", "module"].contains(&self.source[cursor as usize].kind) {
                return false;
            }
            let Some(parent) = self.facts.parents.get(&cursor) else {
                return false;
            };
            cursor = *parent;
        }
    }

    fn type_shadowed(&self, scope: NodeId, name: &str) -> bool {
        let mut cursor = scope;
        loop {
            if self
                .facts
                .type_bindings
                .get(&cursor)
                .is_some_and(|b| b.contains(name))
            {
                return true;
            }
            if ["file", "module"].contains(&self.source[cursor as usize].kind) {
                return false;
            }
            let Some(parent) = self.facts.parents.get(&cursor) else {
                return false;
            };
            cursor = *parent;
        }
    }
}
