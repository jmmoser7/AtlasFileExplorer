use std::collections::{HashMap, HashSet};
use std::path::Path;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Item, ItemImpl, UseTree};

use crate::model::{EdgeKind, ItemKind, NodeId, NodeKind};

use super::cargo::normalize_crate_name;
use super::modules::{resolve_in_container, ModuleMaps};
use super::GraphBuilder;

#[derive(Debug, Default)]
pub struct TraitIndex {
    by_name: HashMap<String, Vec<NodeId>>,
    by_package: HashMap<NodeId, HashMap<String, NodeId>>,
}

impl TraitIndex {
    fn record(&mut self, package_id: NodeId, trait_name: &str, item_id: NodeId) {
        self.by_name
            .entry(trait_name.to_string())
            .or_default()
            .push(item_id);

        let pkg_map = self.by_package.entry(package_id).or_default();
        match pkg_map.get(trait_name) {
            Some(existing) if *existing != item_id => {
                pkg_map.remove(trait_name);
            }
            None => {
                pkg_map.insert(trait_name.to_string(), item_id);
            }
            _ => {}
        }
    }

    pub fn build_from_graph(
        nodes: &[crate::model::LensNode],
        package_for_item: &HashMap<NodeId, NodeId>,
    ) -> Self {
        let mut index = Self::default();
        for node in nodes {
            if let NodeKind::Item {
                item: ItemKind::Trait,
            } = node.kind
            {
                if let Some(&pkg_id) = package_for_item.get(&node.id) {
                    index.record(pkg_id, &node.name, node.id);
                }
            }
        }
        index
    }

    pub fn resolve_trait(
        &self,
        trait_name: &str,
        file_package_id: NodeId,
        file_imports: &HashMap<String, Vec<String>>,
        package_by_name: &HashMap<String, NodeId>,
        nodes: &[crate::model::LensNode],
        package_for_item: &HashMap<NodeId, NodeId>,
    ) -> Option<NodeId> {
        if let Some(path) = file_imports.get(trait_name) {
            if let Some(id) =
                resolve_trait_via_import(path, package_by_name, nodes, package_for_item)
            {
                return Some(id);
            }
        }

        if let Some(pkg_map) = self.by_package.get(&file_package_id) {
            if let Some(&id) = pkg_map.get(trait_name) {
                return Some(id);
            }
        }

        let matches = self.by_name.get(trait_name)?;
        if matches.len() == 1 {
            return Some(matches[0]);
        }
        None
    }
}

fn resolve_trait_via_import(
    path: &[String],
    package_by_name: &HashMap<String, NodeId>,
    nodes: &[crate::model::LensNode],
    package_for_item: &HashMap<NodeId, NodeId>,
) -> Option<NodeId> {
    if path.is_empty() {
        return None;
    }
    let first = normalize_crate_name(&path[0]);
    let trait_name = path.last()?;

    if let Some(&pkg_node_id) = package_by_name.get(&first) {
        for node in nodes {
            if let NodeKind::Item {
                item: ItemKind::Trait,
            } = node.kind
            {
                if node.name == *trait_name && package_for_item.get(&node.id) == Some(&pkg_node_id)
                {
                    return Some(node.id);
                }
            }
        }
    }
    None
}

pub type ImportMap = HashMap<String, Vec<String>>;

/// Shared resolution context for `use`-edge extraction.
pub(crate) struct UseEdgeCtx<'a> {
    pub file_package_id: NodeId,
    pub maps: &'a ModuleMaps,
    pub package_by_name: &'a HashMap<String, NodeId>,
    pub nodes: &'a [crate::model::LensNode],
}

/// Shared resolution context for `impl Trait` edge extraction.
pub(crate) struct ImplTraitEdgeCtx<'a> {
    pub file_package_id: NodeId,
    pub file_imports: &'a ImportMap,
    pub trait_index: &'a TraitIndex,
    pub package_by_name: &'a HashMap<String, NodeId>,
    pub nodes: &'a [crate::model::LensNode],
    pub package_for_item: &'a HashMap<NodeId, NodeId>,
}

pub(crate) fn extract_items(
    builder: &mut GraphBuilder,
    file_id: NodeId,
    file_rel: &Path,
    source: &str,
    ast: &syn::File,
) -> Vec<NodeId> {
    let file_loc = count_nonempty_lines(source);
    builder.set_loc(file_id, file_loc);

    let mut item_ids = Vec::new();
    for item in &ast.items {
        if let Some((kind, name, loc)) = item_metadata(item) {
            let item_id = builder.push_node(
                Some(file_id),
                NodeKind::Item { item: kind },
                name,
                file_rel.to_path_buf(),
                loc,
            );
            item_ids.push(item_id);
        }
    }
    item_ids
}

pub(crate) fn extract_use_edges(
    builder: &mut GraphBuilder,
    file_id: NodeId,
    file_rel: &Path,
    ast: &syn::File,
    ctx: &UseEdgeCtx<'_>,
) -> ImportMap {
    let mut imports = ImportMap::new();
    let mut seen_paths: HashSet<Vec<String>> = HashSet::new();

    for item in &ast.items {
        if let Item::Use(item_use) = item {
            let mut prefix = Vec::new();
            for path in collect_use_paths(&item_use.tree, &mut prefix) {
                if path.is_empty() {
                    continue;
                }
                record_import(&mut imports, &path);
                if seen_paths.insert(path.clone()) {
                    if let Some(target) = resolve_use_path(&path, file_rel, ctx) {
                        if target != file_id {
                            builder.add_edge(file_id, target, EdgeKind::Use);
                        }
                    }
                }
            }
        }
    }

    imports
}

pub(crate) fn extract_impl_trait_edges(
    builder: &mut GraphBuilder,
    file_id: NodeId,
    ast: &syn::File,
    ctx: &ImplTraitEdgeCtx<'_>,
) {
    for item in &ast.items {
        if let Item::Impl(item_impl) = item {
            if let Some(trait_name) = trait_name_from_impl(item_impl) {
                if let Some(trait_id) = ctx.trait_index.resolve_trait(
                    &trait_name,
                    ctx.file_package_id,
                    ctx.file_imports,
                    ctx.package_by_name,
                    ctx.nodes,
                    ctx.package_for_item,
                ) {
                    builder.add_edge(file_id, trait_id, EdgeKind::ImplTrait);
                }
            }
        }
    }
}

pub fn parse_file(source: &str) -> Option<syn::File> {
    syn::parse_file(source).ok()
}

fn item_metadata(item: &Item) -> Option<(ItemKind, String, u32)> {
    match item {
        Item::Struct(item) => Some((
            ItemKind::Struct,
            item.ident.to_string(),
            span_loc(item.span()),
        )),
        Item::Enum(item) => Some((
            ItemKind::Enum,
            item.ident.to_string(),
            span_loc(item.span()),
        )),
        Item::Trait(item) => Some((
            ItemKind::Trait,
            item.ident.to_string(),
            span_loc(item.span()),
        )),
        Item::Fn(item) => Some((
            ItemKind::Function,
            item.sig.ident.to_string(),
            span_loc(item.span()),
        )),
        Item::Impl(item) => Some((
            ItemKind::Impl,
            impl_display_name(item),
            span_loc(item.span()),
        )),
        Item::Type(item) => Some((
            ItemKind::TypeAlias,
            item.ident.to_string(),
            span_loc(item.span()),
        )),
        Item::Const(item) => Some((
            ItemKind::Const,
            item.ident.to_string(),
            span_loc(item.span()),
        )),
        Item::Static(item) => Some((
            ItemKind::Static,
            item.ident.to_string(),
            span_loc(item.span()),
        )),
        Item::Macro(item) => Some((ItemKind::Macro, "macro".to_string(), span_loc(item.span()))),
        Item::Mod(_) => None,
        _ => None,
    }
}

fn impl_display_name(item: &ItemImpl) -> String {
    if let Some((_, trait_path, _)) = &item.trait_ {
        let trait_name = path_last_ident(trait_path);
        let self_name = type_name(&item.self_ty);
        format!("impl {trait_name} for {self_name}")
    } else {
        format!("impl {}", type_name(&item.self_ty))
    }
}

fn trait_name_from_impl(item: &ItemImpl) -> Option<String> {
    item.trait_
        .as_ref()
        .map(|(_, path, _)| path_last_ident(path))
}

fn path_last_ident(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_else(|| "Trait".to_string())
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_else(|| "_".to_string()),
        _ => "_".to_string(),
    }
}

fn span_loc(span: Span) -> u32 {
    let start = span.start().line;
    let end = span.end().line;
    end.saturating_sub(start).saturating_add(1) as u32
}

pub fn count_nonempty_lines(source: &str) -> u32 {
    source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u32
}

fn record_import(imports: &mut ImportMap, path: &[String]) {
    if let Some(leaf) = path.last() {
        imports.insert(leaf.clone(), path.to_vec());
    }
}

fn collect_use_paths(tree: &UseTree, prefix: &mut Vec<String>) -> Vec<Vec<String>> {
    match tree {
        UseTree::Path(use_path) => {
            prefix.push(use_path.ident.to_string());
            let out = collect_use_paths(&use_path.tree, prefix);
            prefix.pop();
            out
        }
        UseTree::Name(name) => {
            prefix.push(name.ident.to_string());
            let out = vec![prefix.clone()];
            prefix.pop();
            out
        }
        UseTree::Rename(rename) => {
            prefix.push(rename.ident.to_string());
            let out = vec![prefix.clone()];
            prefix.pop();
            out
        }
        UseTree::Glob(_) => {
            if prefix.is_empty() {
                Vec::new()
            } else {
                vec![prefix.clone()]
            }
        }
        UseTree::Group(group) => {
            let mut out = Vec::new();
            for item in &group.items {
                out.extend(collect_use_paths(item, prefix));
            }
            out
        }
    }
}

fn resolve_use_path(segments: &[String], file_rel: &Path, ctx: &UseEdgeCtx<'_>) -> Option<NodeId> {
    if segments.is_empty() {
        return None;
    }

    let first_norm = normalize_crate_name(&segments[0]);
    let rest = &segments[1..];

    if matches!(first_norm.as_str(), "crate" | "self" | "super") {
        let start = match first_norm.as_str() {
            "crate" => ctx.file_package_id,
            "self" => {
                super::modules::module_for_file(ctx.maps, file_rel).unwrap_or(ctx.file_package_id)
            }
            "super" => super::modules::parent_module_for_file(ctx.maps, file_rel)
                .unwrap_or(ctx.file_package_id),
            _ => ctx.file_package_id,
        };
        if let Some(target) = resolve_in_container(ctx.nodes, start, rest) {
            return Some(target);
        }
        return Some(ctx.file_package_id);
    }

    if let Some(&pkg_id) = ctx.package_by_name.get(&first_norm) {
        if let Some(target) = resolve_in_container(ctx.nodes, pkg_id, rest) {
            return Some(target);
        }
        return Some(pkg_id);
    }

    None
}

pub fn rollup_loc(nodes: &mut [crate::model::LensNode]) {
    let len = nodes.len();
    for id in (0..len).rev() {
        let child_sum: u32 = nodes[id]
            .children
            .iter()
            .map(|&c| nodes[c as usize].loc)
            .sum();
        if !nodes[id].children.is_empty() {
            nodes[id].loc = child_sum;
        }
    }
}
