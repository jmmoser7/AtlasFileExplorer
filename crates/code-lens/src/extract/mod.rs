use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{CodeGraph, EdgeKind, LensEdge, LensError, LensNode, NodeId, NodeKind};

pub mod cargo;
pub mod modules;
pub mod rust_src;

use cargo::{
    discover_packages, normalize_crate_name, package_name_index, workspace_dependency_names,
};
use modules::walk_package_src;
use rust_src::{
    extract_impl_trait_edges, extract_items, extract_use_edges, parse_file, rollup_loc, TraitIndex,
};

pub fn analyze_workspace(root: &Path) -> Result<CodeGraph, LensError> {
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(LensError::NotACodeRoot(root.to_path_buf()));
    }

    let packages = discover_packages(root)?;
    let member_names: HashSet<String> = packages
        .iter()
        .map(|p| normalize_crate_name(&p.name))
        .collect();

    let mut builder = GraphBuilder::new();

    let ws_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "workspace".to_string());

    let workspace_id = builder.push_node(None, NodeKind::Workspace, ws_name, PathBuf::from("."), 0);

    let mut package_ids: Vec<NodeId> = Vec::new();
    let mut package_id_by_name: HashMap<String, NodeId> = HashMap::new();

    for pkg in &packages {
        let pkg_id = builder.push_node(
            Some(workspace_id),
            NodeKind::Package { is_app: pkg.is_app },
            pkg.name.clone(),
            pkg.rel_path.clone(),
            0,
        );
        package_ids.push(pkg_id);
        package_id_by_name.insert(normalize_crate_name(&pkg.name), pkg_id);
    }

    for (pkg, &from_id) in packages.iter().zip(package_ids.iter()) {
        let pkg_root = package_root(root, &pkg.rel_path);
        let dep_names = workspace_dependency_names(&pkg_root, &member_names)?;
        for dep_name in dep_names {
            let dep_norm = normalize_crate_name(&dep_name);
            if let Some(&to_id) = package_id_by_name.get(&dep_norm) {
                if from_id != to_id {
                    builder.add_edge(from_id, to_id, EdgeKind::PackageDep);
                }
            }
        }
    }

    let mut all_maps: HashMap<NodeId, modules::ModuleMaps> = HashMap::new();
    for (pkg, &pkg_id) in packages.iter().zip(package_ids.iter()) {
        let pkg_root = package_root(root, &pkg.rel_path);
        let maps = walk_package_src(&mut builder, pkg_id, &pkg.rel_path, &pkg_root.join("src"))?;
        all_maps.insert(pkg_id, maps);
    }

    let mut package_for_item: HashMap<NodeId, NodeId> = HashMap::new();
    let mut parsed_files: Vec<(NodeId, NodeId, PathBuf, syn::File)> = Vec::new();

    for &pkg_id in &package_ids {
        let maps = all_maps.get(&pkg_id).expect("maps for package");
        let mut file_paths: Vec<_> = maps.file_by_path.keys().cloned().collect();
        file_paths.sort();

        for file_rel in file_paths {
            let file_id = maps.file_by_path[&file_rel];
            let source = match fs::read_to_string(root.join(&file_rel)) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if let Some(ast) = parse_file(&source) {
                for item_id in extract_items(&mut builder, file_id, &file_rel, &source, &ast) {
                    package_for_item.insert(item_id, pkg_id);
                }
                parsed_files.push((file_id, pkg_id, file_rel, ast));
            } else {
                builder.set_loc(file_id, rust_src::count_nonempty_lines(&source));
            }
        }
    }

    let trait_index = TraitIndex::build_from_graph(builder.nodes(), &package_for_item);
    let package_by_name: HashMap<String, NodeId> = package_name_index(&packages)
        .into_iter()
        .filter_map(|(name, rel)| {
            packages
                .iter()
                .zip(package_ids.iter())
                .find(|(p, _)| p.rel_path == rel)
                .map(|(_, &id)| (name, id))
        })
        .collect();

    parsed_files.sort_by(|a, b| a.2.cmp(&b.2));

    for (file_id, pkg_id, file_rel, ast) in parsed_files {
        let nodes = builder.nodes().to_vec();
        let maps = all_maps.get(&pkg_id).expect("maps for package");
        let imports = extract_use_edges(
            &mut builder,
            file_id,
            &file_rel,
            &ast,
            pkg_id,
            maps,
            &package_by_name,
            &nodes,
        );
        let nodes = builder.nodes().to_vec();
        extract_impl_trait_edges(
            &mut builder,
            file_id,
            &ast,
            pkg_id,
            &imports,
            &trait_index,
            &package_by_name,
            &nodes,
            &package_for_item,
        );
    }

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    builder.finish(workspace_id, generated_at)
}

fn package_root(root: &Path, rel: &Path) -> PathBuf {
    if rel.as_os_str() == "." {
        root.to_path_buf()
    } else {
        root.join(rel)
    }
}

pub(crate) struct GraphBuilder {
    nodes: Vec<LensNode>,
    edge_weights: HashMap<(NodeId, NodeId, EdgeKind), u32>,
}

impl GraphBuilder {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edge_weights: HashMap::new(),
        }
    }

    pub(crate) fn nodes(&self) -> &[LensNode] {
        &self.nodes
    }

    pub(crate) fn push_node(
        &mut self,
        parent: Option<NodeId>,
        kind: NodeKind,
        name: String,
        path: PathBuf,
        loc: u32,
    ) -> NodeId {
        let id = self.nodes.len() as NodeId;
        let node = LensNode {
            id,
            parent,
            kind,
            name,
            path,
            loc,
            children: Vec::new(),
        };
        if let Some(parent_id) = parent {
            self.nodes[parent_id as usize].children.push(id);
        }
        self.nodes.push(node);
        id
    }

    pub(crate) fn set_loc(&mut self, id: NodeId, loc: u32) {
        self.nodes[id as usize].loc = loc;
    }

    pub(crate) fn add_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
        *self.edge_weights.entry((from, to, kind)).or_insert(0) += 1;
    }

    fn finish(mut self, root: NodeId, generated_at: u64) -> Result<CodeGraph, LensError> {
        rollup_loc(&mut self.nodes);

        for id in 0..self.nodes.len() {
            let mut children = self.nodes[id].children.clone();
            children.sort_by(|&a, &b| {
                let na = &self.nodes[a as usize];
                let nb = &self.nodes[b as usize];
                kind_order(na.kind)
                    .cmp(&kind_order(nb.kind))
                    .then_with(|| na.name.cmp(&nb.name))
                    .then_with(|| na.path.cmp(&nb.path))
            });
            self.nodes[id].children = children;
        }

        let mut edges: Vec<LensEdge> = self
            .edge_weights
            .into_iter()
            .map(|((from, to, kind), weight)| LensEdge {
                from,
                to,
                kind,
                weight,
            })
            .collect();
        edges.sort_by(|a, b| {
            (a.from, a.to, edge_kind_key(a.kind)).cmp(&(b.from, b.to, edge_kind_key(b.kind)))
        });

        Ok(CodeGraph {
            root,
            nodes: self.nodes,
            edges,
            generated_at,
        })
    }
}

fn kind_order(kind: NodeKind) -> u8 {
    match kind {
        NodeKind::Workspace => 0,
        NodeKind::Package { .. } => 1,
        NodeKind::Module => 2,
        NodeKind::File => 3,
        NodeKind::Item { .. } => 4,
    }
}

fn edge_kind_key(kind: EdgeKind) -> u8 {
    match kind {
        EdgeKind::PackageDep => 0,
        EdgeKind::Use => 1,
        EdgeKind::ImplTrait => 2,
    }
}
