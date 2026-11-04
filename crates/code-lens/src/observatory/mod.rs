//! Detailed, renderer-independent source graph for the developer observatory.
//!
//! The legacy Lens graph is unchanged. This view shares its physical workspace
//! discovery and Rust parser, then adds lexical symbols and strict relations.

mod namespaces;
mod relations;
mod single;
mod symbols;

pub use single::{analyze_source, SingleSource, SingleSymbol};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::extract::{prepare_workspace, rust_src, GraphBuilder};
use crate::model::{CodeGraph, LensError, NodeId, NodeKind};

#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub kind: &'static str,
    pub line: u32,
    #[serde(rename = "endLine")]
    pub end_line: u32,
    #[serde(rename = "selfLoc")]
    pub self_loc: u32,
    pub doc: String,
    pub conditional: bool,
    pub identity: String,
}

impl SourceInfo {
    fn physical(kind: &'static str, path: &Path, name: &str) -> Self {
        Self {
            kind,
            line: 0,
            end_line: 0,
            self_loc: 0,
            doc: String::new(),
            conditional: false,
            identity: format!("{kind}:{}:{name}", relative(path)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceRelation {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: &'static str,
    pub weight: u32,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionCounts {
    pub files: usize,
    pub parsed_files: usize,
    pub symbols: usize,
    pub observed_calls: usize,
    pub unresolved_calls: usize,
    pub observed_imports: usize,
    pub unresolved_imports: usize,
    pub unresolved_modules: usize,
    pub unresolved_impls: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceFailure {
    pub path: String,
    pub line: u32,
    pub message: String,
}

pub struct ObservatoryGraph {
    pub graph: CodeGraph,
    pub source: Vec<SourceInfo>,
    pub relations: Vec<SourceRelation>,
    pub counts: ExtractionCounts,
    pub failures: Vec<SourceFailure>,
}

#[derive(Clone, Debug)]
pub(crate) struct PathRef {
    pub parts: Vec<String>,
    pub absolute: bool,
}

#[derive(Clone)]
pub(crate) struct Import {
    pub scope: NodeId,
    pub path: PathRef,
    pub alias: String,
    pub glob: bool,
}

pub(crate) struct Call {
    pub caller: NodeId,
    pub scope: NodeId,
    pub path: Option<PathRef>,
    pub self_method: Option<String>,
}

pub(crate) struct Impl {
    pub node: NodeId,
    pub scope: NodeId,
    pub self_type: Option<PathRef>,
    pub trait_type: Option<PathRef>,
}

pub(crate) struct ModuleDecl {
    pub node: NodeId,
    pub file: NodeId,
    pub inline: Vec<String>,
    pub name: String,
    pub path: Option<String>,
    pub uncertain_path: bool,
    pub external: bool,
}

pub(crate) struct File {
    pub node: NodeId,
    pub package: NodeId,
    pub package_name: String,
    pub package_path: PathBuf,
    pub path: PathBuf,
}

#[derive(Default)]
pub(crate) struct Facts {
    pub parents: HashMap<NodeId, NodeId>,
    pub files: Vec<File>,
    pub file_for: HashMap<NodeId, NodeId>,
    pub definitions: HashMap<(NodeId, String), Vec<NodeId>>,
    pub bindings: HashMap<NodeId, HashSet<String>>,
    pub type_bindings: HashMap<NodeId, HashSet<String>>,
    pub extern_aliases: HashMap<NodeId, HashMap<String, String>>,
    pub imports: Vec<Import>,
    pub calls: Vec<Call>,
    pub impls: Vec<Impl>,
    pub modules: Vec<ModuleDecl>,
}

pub(crate) struct Builder {
    pub graph: GraphBuilder,
    pub source: Vec<SourceInfo>,
    pub facts: Facts,
    pub duplicates: HashMap<(NodeId, &'static str, String), usize>,
}

impl Builder {
    pub fn push(
        &mut self,
        parent: NodeId,
        kind: &'static str,
        name: String,
        path: &Path,
        line: u32,
        end_line: u32,
        doc: String,
        conditional: bool,
    ) -> NodeId {
        let count = self
            .duplicates
            .entry((parent, kind, name.clone()))
            .or_default();
        let identity = format!(
            "{}::{kind}:{name}:{}",
            self.source[parent as usize].identity, *count
        );
        *count += 1;
        let node_kind = if kind == "module" {
            NodeKind::Module
        } else {
            NodeKind::Item {
                item: symbols::item_kind(kind),
            }
        };
        let id = self
            .graph
            .push_node(Some(parent), node_kind, name.clone(), path.to_path_buf(), 0);
        self.source.push(SourceInfo {
            kind,
            line,
            end_line,
            self_loc: 0,
            doc,
            conditional,
            identity,
        });
        self.facts.parents.insert(id, parent);
        if let Some(file) = self.facts.file_for.get(&parent).copied() {
            self.facts.file_for.insert(id, file);
        }
        if kind != "impl" && kind != "macro" {
            self.facts
                .definitions
                .entry((parent, name))
                .or_default()
                .push(id);
        }
        id
    }
}

/// Analyze only the named packages, including every physical src/tests Rust
/// file. Unparseable files remain visible with explicit failure records.
pub fn analyze_observatory(
    root: &Path,
    package_names: &[String],
) -> Result<ObservatoryGraph, LensError> {
    let selection = package_names.iter().cloned().collect();
    let base = prepare_workspace(root, Some(&selection), true)?;
    let mut source = Vec::new();
    for node in base.builder.nodes() {
        let kind = match node.kind {
            NodeKind::Workspace => "workspace",
            NodeKind::Package { .. } => "crate",
            NodeKind::Module => "directory",
            NodeKind::File => "file",
            NodeKind::Item { .. } => "item",
        };
        source.push(SourceInfo::physical(kind, &node.path, &node.name));
    }
    let mut builder = Builder {
        graph: base.builder,
        source,
        facts: Facts::default(),
        duplicates: HashMap::new(),
    };
    for (package, id) in base.packages.iter().zip(&base.package_ids) {
        builder.facts.extern_aliases.insert(
            *id,
            crate::extract::cargo::dependency_bindings(root, &root.join(&package.rel_path))?,
        );
        let maps = &base.maps[id];
        let mut paths: Vec<_> = maps.file_by_path.iter().collect();
        paths.sort_by(|a, b| a.0.cmp(b.0));
        for (path, file) in paths {
            builder.facts.file_for.insert(*file, *file);
            builder.facts.files.push(File {
                node: *file,
                package: *id,
                package_name: package.name.clone(),
                package_path: package.rel_path.clone(),
                path: path.clone(),
            });
        }
    }
    let mut failures = Vec::new();
    let mut counts = ExtractionCounts {
        files: builder.facts.files.len(),
        ..Default::default()
    };
    let files: Vec<_> = builder
        .facts
        .files
        .iter()
        .map(|f| (f.node, f.path.clone()))
        .collect();
    for (file, path) in files {
        let text = std::fs::read_to_string(root.join(&path))?;
        let lines = text.lines().count() as u32;
        builder.graph.set_loc(file, lines);
        builder.source[file as usize].line = u32::from(lines > 0);
        builder.source[file as usize].end_line = lines;
        match rust_src::parse_file_with_error(&text) {
            Ok(ast) => {
                counts.parsed_files += 1;
                symbols::extract(&mut builder, file, &path, &ast);
            }
            Err(error) => failures.push(SourceFailure {
                path: relative(&path),
                line: error.span().start().line as u32,
                message: error.to_string(),
            }),
        }
    }
    counts.symbols = builder
        .source
        .iter()
        .filter(|n| !["workspace", "crate", "directory", "file"].contains(&n.kind))
        .count();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut graph = builder.graph.finish_without_rollup(base.root, now)?;
    finish_weights(&mut graph, &mut builder.source);
    let relations = relations::resolve(&graph, &builder.source, &builder.facts, &mut counts);
    Ok(ObservatoryGraph {
        graph,
        source: builder.source,
        relations,
        counts,
        failures,
    })
}

fn finish_weights(graph: &mut CodeGraph, source: &mut [SourceInfo]) {
    for index in (0..graph.nodes.len()).rev() {
        let info = &source[index];
        if ["workspace", "crate", "directory"].contains(&info.kind) {
            graph.nodes[index].loc = graph.nodes[index]
                .children
                .iter()
                .map(|id| graph.nodes[*id as usize].loc)
                .sum();
        } else if info.kind != "file" {
            graph.nodes[index].loc = info
                .end_line
                .saturating_sub(info.line)
                .saturating_add(u32::from(info.line > 0));
        }
        let start = source[index].line;
        let end = source[index].end_line;
        if start == 0 {
            continue;
        }
        let mut intervals: Vec<_> = graph.nodes[index]
            .children
            .iter()
            .filter_map(|id| {
                let child = &source[*id as usize];
                let a = child.line.max(start);
                let b = child.end_line.min(end);
                (a > 0 && a <= b).then_some((a, b))
            })
            .collect();
        intervals.sort();
        let mut covered = 0;
        let mut tail = start - 1;
        for (a, b) in intervals {
            if b > tail {
                covered += b - a.max(tail + 1) + 1;
                tail = b;
            }
        }
        source[index].self_loc = graph.nodes[index].loc.saturating_sub(covered);
    }
}

pub fn stable_id(identity: &str) -> String {
    let hash = identity.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    format!("n{hash:016x}")
}

pub(crate) fn relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests;
