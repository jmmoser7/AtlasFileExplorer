use std::collections::{HashMap, HashSet};

use crate::model::{CodeGraph, EdgeKind, LensNode, NodeId, NodeKind};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectf {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone)]
pub struct PlacedNode {
    pub id: crate::model::NodeId,
    pub rect: Rectf,     // world-space; containers enclose children
    pub collapsed: bool, // true when drawn as a chip (children hidden)
    pub depth: u8,       // 0 = workspace, 1 = package, ...
}

#[derive(Debug, Clone)]
pub struct LensWire {
    pub from: crate::model::NodeId,
    pub to: crate::model::NodeId,
    pub kind: crate::model::EdgeKind,
    pub weight: u32,
    pub from_pt: (f32, f32), // attachment on `from` rect edge
    pub to_pt: (f32, f32),
}

#[derive(Debug, Clone)]
pub struct LensLayout {
    pub placed: Vec<PlacedNode>, // paint order: parents before children
    pub wires: Vec<LensWire>,
    pub bounds: Rectf,
}

impl Default for LensLayout {
    fn default() -> Self {
        Self {
            placed: Vec::new(),
            wires: Vec::new(),
            bounds: Rectf {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
        }
    }
}

// Top-level package columns.
const COLUMN_GAP: f32 = 48.0;
const ROW_GAP: f32 = 32.0;

// Container interior.
const PADDING: f32 = 12.0;
const HEADER_H: f32 = 28.0;
const INNER_GAP: f32 = 8.0;

// Chip width from LOC.
const BASE_W: f32 = 60.0;
const LOC_K: f32 = 12.0;
const MIN_W: f32 = 48.0;
const MAX_W: f32 = 200.0;

// Chip heights by kind (containers slightly taller than items).
const CHIP_ITEM: f32 = 22.0;
const CHIP_FILE: f32 = 26.0;
const CHIP_MODULE: f32 = 30.0;
const CHIP_PACKAGE: f32 = 32.0;
const CHIP_WORKSPACE: f32 = 34.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum EdgeSide {
    Left,
    Right,
}

struct PlaceNodeCtx<'a> {
    graph: &'a CodeGraph,
    expanded: &'a HashSet<NodeId>,
    sizes: &'a HashMap<NodeId, (f32, f32)>,
    placed: &'a mut Vec<PlacedNode>,
}

/// `expanded`: nodes whose children are shown. A node is visible when every
/// ancestor is in `expanded`. Edges roll up to the deepest visible ancestor
/// on each side; (from,to,kind) duplicates merge summing weight; self-loops
/// after rollup are dropped.
pub fn layout_graph(graph: &CodeGraph, expanded: &HashSet<NodeId>) -> LensLayout {
    if graph.is_empty() {
        return LensLayout::default();
    }

    let mut sizes: HashMap<NodeId, (f32, f32)> = HashMap::new();
    for id in 0..graph.nodes.len() as NodeId {
        if is_visible(graph, id, expanded) {
            compute_size(graph, id, expanded, &mut sizes);
        }
    }

    let mut placed = Vec::new();
    if skip_workspace_box(graph) {
        place_top_level(graph, expanded, &sizes, &mut placed);
    } else {
        let (w, h) = sizes
            .get(&graph.root)
            .copied()
            .unwrap_or_else(|| chip_size(graph.node(graph.root)));
        let mut ctx = PlaceNodeCtx {
            graph,
            expanded,
            sizes: &sizes,
            placed: &mut placed,
        };
        place_node(&mut ctx, graph.root, 0.0, 0.0, w, h);
    }

    placed.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.id.cmp(&b.id)));

    let wires = build_wires(graph, expanded, &placed);
    let bounds = union_bounds(&placed);

    LensLayout {
        placed,
        wires,
        bounds,
    }
}

fn skip_workspace_box(graph: &CodeGraph) -> bool {
    if graph.nodes.len() == 1 {
        return false;
    }
    matches!(graph.node(graph.root).kind, NodeKind::Workspace)
}

fn is_visible(graph: &CodeGraph, id: NodeId, expanded: &HashSet<NodeId>) -> bool {
    let mut current = id;
    loop {
        let Some(parent) = graph.node(current).parent else {
            return true;
        };
        if !expanded.contains(&parent) {
            return false;
        }
        current = parent;
    }
}

fn is_collapsed(graph: &CodeGraph, id: NodeId, expanded: &HashSet<NodeId>) -> bool {
    let node = graph.node(id);
    !expanded.contains(&id) || node.children.is_empty()
}

fn node_depth(graph: &CodeGraph, id: NodeId) -> u8 {
    let mut depth = 0u8;
    let mut current = id;
    while let Some(parent) = graph.node(current).parent {
        depth = depth.saturating_add(1);
        current = parent;
    }
    depth
}

fn kind_rank(kind: NodeKind) -> u8 {
    match kind {
        NodeKind::Module => 0,
        NodeKind::File => 1,
        NodeKind::Item { .. } => 2,
        NodeKind::Package { .. } => 3,
        NodeKind::Workspace => 4,
    }
}

fn chip_height(kind: NodeKind) -> f32 {
    match kind {
        NodeKind::Item { .. } => CHIP_ITEM,
        NodeKind::File => CHIP_FILE,
        NodeKind::Module => CHIP_MODULE,
        NodeKind::Package { .. } => CHIP_PACKAGE,
        NodeKind::Workspace => CHIP_WORKSPACE,
    }
}

fn chip_width(loc: u32) -> f32 {
    let log = (loc.max(2) as f32).log2();
    (BASE_W + LOC_K * log).clamp(MIN_W, MAX_W)
}

fn chip_size(node: &LensNode) -> (f32, f32) {
    (chip_width(node.loc), chip_height(node.kind))
}

fn sorted_visible_children(
    graph: &CodeGraph,
    parent: NodeId,
    expanded: &HashSet<NodeId>,
) -> Vec<NodeId> {
    let mut children: Vec<NodeId> = graph
        .node(parent)
        .children
        .iter()
        .copied()
        .filter(|&id| is_visible(graph, id, expanded))
        .collect();
    children.sort_by(|&a, &b| {
        let na = graph.node(a);
        let nb = graph.node(b);
        kind_rank(na.kind)
            .cmp(&kind_rank(nb.kind))
            .then_with(|| na.name.cmp(&nb.name))
            .then_with(|| a.cmp(&b))
    });
    children
}

fn compute_size(
    graph: &CodeGraph,
    id: NodeId,
    expanded: &HashSet<NodeId>,
    cache: &mut HashMap<NodeId, (f32, f32)>,
) -> (f32, f32) {
    if let Some(&size) = cache.get(&id) {
        return size;
    }

    let node = graph.node(id);
    if is_collapsed(graph, id, expanded) {
        let size = chip_size(node);
        cache.insert(id, size);
        return size;
    }

    let children = sorted_visible_children(graph, id, expanded);
    if children.is_empty() {
        let size = chip_size(node);
        cache.insert(id, size);
        return size;
    }

    for &child in &children {
        compute_size(graph, child, expanded, cache);
    }

    let n = children.len();
    let cols = (n as f32).sqrt().ceil() as usize;
    let cols = cols.max(1);
    let rows = n.div_ceil(cols);

    let mut row_widths = vec![0.0_f32; rows];
    let mut row_heights = vec![0.0_f32; rows];

    for (i, &child) in children.iter().enumerate() {
        let (cw, ch) = cache[&child];
        let row = i / cols;
        let col = i % cols;
        if col > 0 {
            row_widths[row] += INNER_GAP;
        }
        row_widths[row] += cw;
        row_heights[row] = row_heights[row].max(ch);
    }

    let content_w = row_widths.iter().copied().fold(0.0_f32, f32::max);
    let content_h = row_heights
        .iter()
        .enumerate()
        .map(|(r, &h)| h + if r > 0 { INNER_GAP } else { 0.0 })
        .sum::<f32>();

    let size = (
        content_w + 2.0 * PADDING,
        HEADER_H + PADDING + content_h + PADDING,
    );
    cache.insert(id, size);
    size
}

fn place_children_grid(ctx: &mut PlaceNodeCtx<'_>, parent: NodeId, origin_x: f32, origin_y: f32) {
    let children = sorted_visible_children(ctx.graph, parent, ctx.expanded);
    if children.is_empty() {
        return;
    }

    let n = children.len();
    let cols = (n as f32).sqrt().ceil() as usize;
    let cols = cols.max(1);
    let rows = n.div_ceil(cols);

    let mut row_widths = vec![0.0_f32; rows];
    let mut row_heights = vec![0.0_f32; rows];

    for (i, &child) in children.iter().enumerate() {
        let (cw, ch) = ctx.sizes[&child];
        let row = i / cols;
        let col = i % cols;
        if col > 0 {
            row_widths[row] += INNER_GAP;
        }
        row_widths[row] += cw;
        row_heights[row] = row_heights[row].max(ch);
    }

    let mut y = origin_y;
    for (row, &row_h) in row_heights.iter().enumerate() {
        let mut x = origin_x;
        let row_start = row * cols;
        let row_end = (row_start + cols).min(n);
        for (col, &child) in children[row_start..row_end].iter().enumerate() {
            if col > 0 {
                x += INNER_GAP;
            }
            let (cw, ch) = ctx.sizes[&child];
            place_node(ctx, child, x, y, cw, ch);
            x += cw;
        }
        y += row_h;
        if row + 1 < rows {
            y += INNER_GAP;
        }
    }
}

fn place_node(ctx: &mut PlaceNodeCtx<'_>, id: NodeId, x: f32, y: f32, w: f32, h: f32) {
    let collapsed = is_collapsed(ctx.graph, id, ctx.expanded);
    ctx.placed.push(PlacedNode {
        id,
        rect: Rectf { x, y, w, h },
        collapsed,
        depth: node_depth(ctx.graph, id),
    });

    if !collapsed {
        let content_x = x + PADDING;
        let content_y = y + HEADER_H + PADDING;
        place_children_grid(ctx, id, content_x, content_y);
    }
}

fn package_layer(graph: &CodeGraph, id: NodeId, memo: &mut HashMap<NodeId, u32>) -> u32 {
    if let Some(&layer) = memo.get(&id) {
        return layer;
    }
    memo.insert(id, 0);

    let mut max_dep: Option<u32> = None;
    for edge in &graph.edges {
        if edge.from == id && edge.kind == EdgeKind::PackageDep {
            let dep_layer = package_layer(graph, edge.to, memo);
            max_dep = Some(max_dep.map_or(dep_layer, |m| m.max(dep_layer)));
        }
    }
    let layer = max_dep.map(|m| m.saturating_add(1)).unwrap_or(0);
    memo.insert(id, layer);
    layer
}

fn top_level_ids(graph: &CodeGraph, expanded: &HashSet<NodeId>) -> Vec<NodeId> {
    let root = graph.root;
    let mut ids: Vec<NodeId> = if skip_workspace_box(graph) {
        graph
            .node(root)
            .children
            .iter()
            .copied()
            .filter(|&id| is_visible(graph, id, expanded))
            .collect()
    } else if is_visible(graph, root, expanded) {
        vec![root]
    } else {
        Vec::new()
    };

    let mut layers = HashMap::new();
    for &id in &ids {
        if matches!(graph.node(id).kind, NodeKind::Package { .. }) {
            package_layer(graph, id, &mut layers);
        } else {
            layers.insert(id, 0);
        }
    }

    ids.sort_by(|&a, &b| {
        let la = layers[&a];
        let lb = layers[&b];
        la.cmp(&lb)
            .then_with(|| {
                let pa = matches!(graph.node(a).kind, NodeKind::Package { is_app: true });
                let pb = matches!(graph.node(b).kind, NodeKind::Package { is_app: true });
                pa.cmp(&pb)
            })
            .then_with(|| kind_rank(graph.node(a).kind).cmp(&kind_rank(graph.node(b).kind)))
            .then_with(|| graph.node(a).name.cmp(&graph.node(b).name))
            .then_with(|| a.cmp(&b))
    });
    ids
}

fn place_top_level(
    graph: &CodeGraph,
    expanded: &HashSet<NodeId>,
    sizes: &HashMap<NodeId, (f32, f32)>,
    placed: &mut Vec<PlacedNode>,
) {
    let top = top_level_ids(graph, expanded);
    if top.is_empty() {
        return;
    }

    let mut layers = HashMap::new();
    for &id in &top {
        if matches!(graph.node(id).kind, NodeKind::Package { .. }) {
            package_layer(graph, id, &mut layers);
        } else {
            layers.insert(id, 0);
        }
    }

    let max_layer = top.iter().map(|&id| layers[&id]).max().unwrap_or(0);
    let mut columns: Vec<Vec<NodeId>> = vec![Vec::new(); max_layer as usize + 1];
    for id in top {
        columns[layers[&id] as usize].push(id);
    }

    let mut col_widths = vec![0.0_f32; columns.len()];
    for (col_idx, col) in columns.iter().enumerate() {
        for &id in col {
            let (w, _) = sizes[&id];
            col_widths[col_idx] = col_widths[col_idx].max(w);
        }
    }

    let mut x = 0.0_f32;
    let mut ctx = PlaceNodeCtx {
        graph,
        expanded,
        sizes,
        placed,
    };
    for (col_idx, col) in columns.iter().enumerate() {
        let mut y = 0.0_f32;
        for (row_idx, &id) in col.iter().enumerate() {
            let (w, h) = sizes[&id];
            let px = x + (col_widths[col_idx] - w) * 0.5;
            place_node(&mut ctx, id, px, y, w, h);
            y += h;
            if row_idx + 1 < col.len() {
                y += ROW_GAP;
            }
        }
        x += col_widths[col_idx];
        if col_idx + 1 < columns.len() {
            x += COLUMN_GAP;
        }
    }
}

fn deepest_visible_ancestor(
    graph: &CodeGraph,
    id: NodeId,
    expanded: &HashSet<NodeId>,
) -> Option<NodeId> {
    let mut current = id;
    loop {
        if is_visible(graph, current, expanded) {
            return Some(current);
        }
        current = graph.node(current).parent?;
    }
}

fn is_strict_ancestor(graph: &CodeGraph, ancestor: NodeId, node: NodeId) -> bool {
    let mut current = node;
    while let Some(parent) = graph.node(current).parent {
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

fn edge_kind_rank(kind: EdgeKind) -> u8 {
    match kind {
        EdgeKind::PackageDep => 0,
        EdgeKind::Use => 1,
        EdgeKind::ImplTrait => 2,
    }
}

fn build_wires(
    graph: &CodeGraph,
    expanded: &HashSet<NodeId>,
    placed: &[PlacedNode],
) -> Vec<LensWire> {
    let rects: HashMap<NodeId, Rectf> = placed.iter().map(|p| (p.id, p.rect)).collect();

    let mut rolled: HashMap<(NodeId, NodeId, EdgeKind), u32> = HashMap::new();
    for edge in &graph.edges {
        let Some(from) = deepest_visible_ancestor(graph, edge.from, expanded) else {
            continue;
        };
        let Some(to) = deepest_visible_ancestor(graph, edge.to, expanded) else {
            continue;
        };
        if from == to {
            continue;
        }
        if is_strict_ancestor(graph, from, to) || is_strict_ancestor(graph, to, from) {
            continue;
        }
        *rolled.entry((from, to, edge.kind)).or_insert(0) += edge.weight;
    }

    let mut wires: Vec<LensWire> = rolled
        .into_iter()
        .map(|((from, to, kind), weight)| LensWire {
            from,
            to,
            kind,
            weight,
            from_pt: (0.0, 0.0),
            to_pt: (0.0, 0.0),
        })
        .collect();

    wires.sort_by(|a, b| {
        a.from
            .cmp(&b.from)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| edge_kind_rank(a.kind).cmp(&edge_kind_rank(b.kind)))
    });

    spread_wire_endpoints(&mut wires, &rects);
    wires
}

fn wire_sides(from_rect: Rectf, to_rect: Rectf) -> (EdgeSide, EdgeSide) {
    let from_cx = from_rect.x + from_rect.w * 0.5;
    let to_cx = to_rect.x + to_rect.w * 0.5;
    if to_cx > from_cx {
        (EdgeSide::Right, EdgeSide::Left)
    } else if to_cx < from_cx {
        (EdgeSide::Left, EdgeSide::Left)
    } else {
        (EdgeSide::Right, EdgeSide::Right)
    }
}

fn edge_point(rect: Rectf, side: EdgeSide, t: f32) -> (f32, f32) {
    let t = t.clamp(0.0, 1.0);
    match side {
        EdgeSide::Left => (rect.x, rect.y + rect.h * t),
        EdgeSide::Right => (rect.x + rect.w, rect.y + rect.h * t),
    }
}

fn spread_wire_endpoints(wires: &mut [LensWire], rects: &HashMap<NodeId, Rectf>) {
    let mut from_groups: HashMap<(NodeId, EdgeSide), Vec<usize>> = HashMap::new();
    let mut to_groups: HashMap<(NodeId, EdgeSide), Vec<usize>> = HashMap::new();

    for (idx, wire) in wires.iter().enumerate() {
        let (from_side, to_side) = wire_sides(rects[&wire.from], rects[&wire.to]);
        from_groups
            .entry((wire.from, from_side))
            .or_default()
            .push(idx);
        to_groups.entry((wire.to, to_side)).or_default().push(idx);
    }

    let mut from_keys: Vec<(NodeId, EdgeSide)> = from_groups.keys().copied().collect();
    from_keys.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (node, side) in from_keys {
        let mut indices = from_groups.remove(&(node, side)).unwrap();
        indices.sort_by(|&a, &b| {
            wires[a]
                .to
                .cmp(&wires[b].to)
                .then_with(|| edge_kind_rank(wires[a].kind).cmp(&edge_kind_rank(wires[b].kind)))
                .then_with(|| a.cmp(&b))
        });
        let n = indices.len();
        for (i, idx) in indices.into_iter().enumerate() {
            let t = if n == 1 {
                0.5
            } else {
                (i as f32 + 1.0) / (n as f32 + 1.0)
            };
            wires[idx].from_pt = edge_point(rects[&node], side, t);
        }
    }

    let mut to_keys: Vec<(NodeId, EdgeSide)> = to_groups.keys().copied().collect();
    to_keys.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (node, side) in to_keys {
        let mut indices = to_groups.remove(&(node, side)).unwrap();
        indices.sort_by(|&a, &b| {
            wires[a]
                .from
                .cmp(&wires[b].from)
                .then_with(|| edge_kind_rank(wires[a].kind).cmp(&edge_kind_rank(wires[b].kind)))
                .then_with(|| a.cmp(&b))
        });
        let n = indices.len();
        for (i, idx) in indices.into_iter().enumerate() {
            let t = if n == 1 {
                0.5
            } else {
                (i as f32 + 1.0) / (n as f32 + 1.0)
            };
            wires[idx].to_pt = edge_point(rects[&node], side, t);
        }
    }
}

fn union_bounds(placed: &[PlacedNode]) -> Rectf {
    let mut iter = placed.iter();
    let Some(first) = iter.next() else {
        return Rectf {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    };

    let mut min_x = first.rect.x;
    let mut min_y = first.rect.y;
    let mut max_x = first.rect.x + first.rect.w;
    let mut max_y = first.rect.y + first.rect.h;

    for p in iter {
        min_x = min_x.min(p.rect.x);
        min_y = min_y.min(p.rect.y);
        max_x = max_x.max(p.rect.x + p.rect.w);
        max_y = max_y.max(p.rect.y + p.rect.h);
    }

    Rectf {
        x: min_x,
        y: min_y,
        w: (max_x - min_x).max(0.0),
        h: (max_y - min_y).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::*;
    use crate::model::{CodeGraph, ItemKind, LensEdge, LensNode};

    fn pkg(id: NodeId, name: &str, parent: NodeId, is_app: bool) -> LensNode {
        LensNode {
            id,
            parent: Some(parent),
            kind: NodeKind::Package { is_app },
            name: name.into(),
            path: PathBuf::from(name),
            loc: 100,
            children: vec![],
        }
    }

    fn rect_of(layout: &LensLayout, id: NodeId) -> Rectf {
        layout
            .placed
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.rect)
            .expect("node not placed")
    }

    fn assert_no_overlap(rects: &[Rectf]) {
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let a = &rects[i];
                let b = &rects[j];
                let overlap_x = a.x < b.x + b.w && b.x < a.x + a.w;
                let overlap_y = a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(
                    !(overlap_x && overlap_y),
                    "rects overlap: {:?} vs {:?}",
                    a,
                    b
                );
            }
        }
    }

    fn diamond_graph() -> CodeGraph {
        let nodes = vec![
            LensNode {
                id: 0,
                parent: None,
                kind: NodeKind::Workspace,
                name: "ws".into(),
                path: PathBuf::new(),
                loc: 0,
                children: vec![1, 2, 3, 4],
            },
            pkg(1, "a", 0, true),
            pkg(2, "b", 0, false),
            pkg(3, "c", 0, false),
            pkg(4, "d", 0, false),
        ];
        let edges = vec![
            LensEdge {
                from: 1,
                to: 2,
                kind: EdgeKind::PackageDep,
                weight: 1,
            },
            LensEdge {
                from: 1,
                to: 3,
                kind: EdgeKind::PackageDep,
                weight: 1,
            },
            LensEdge {
                from: 2,
                to: 4,
                kind: EdgeKind::PackageDep,
                weight: 1,
            },
            LensEdge {
                from: 3,
                to: 4,
                kind: EdgeKind::PackageDep,
                weight: 1,
            },
        ];
        CodeGraph {
            root: 0,
            nodes,
            edges,
            generated_at: 0,
        }
    }

    #[test]
    fn diamond_package_deps_layer_ordering() {
        let graph = diamond_graph();
        let expanded: HashSet<NodeId> = [0].into();

        let layout1 = layout_graph(&graph, &expanded);
        let layout2 = layout_graph(&graph, &expanded);

        assert_eq!(layout1.placed.len(), 4);
        assert_eq!(layout1.placed.len(), layout2.placed.len());
        for p in &layout1.placed {
            let q = layout2.placed.iter().find(|q| q.id == p.id).unwrap();
            assert_eq!(p.rect, q.rect);
        }

        let ra = rect_of(&layout1, 1);
        let rb = rect_of(&layout1, 2);
        let rc = rect_of(&layout1, 3);
        let rd = rect_of(&layout1, 4);

        assert!(rd.x < rb.x);
        assert!(rd.x < rc.x);
        assert!((rb.x - rc.x).abs() < 0.01);
        assert!(ra.x > rb.x);
        assert!(ra.x > rc.x);

        assert_no_overlap(&[ra, rb, rc, rd]);
        assert!(!layout1.placed.iter().any(|p| p.id == 0));
    }

    #[test]
    fn expand_collapse_containment() {
        let nodes = vec![
            LensNode {
                id: 0,
                parent: None,
                kind: NodeKind::Workspace,
                name: "ws".into(),
                path: PathBuf::new(),
                loc: 0,
                children: vec![1],
            },
            LensNode {
                id: 1,
                parent: Some(0),
                kind: NodeKind::Package { is_app: false },
                name: "pkg".into(),
                path: PathBuf::from("crates/pkg"),
                loc: 50,
                children: vec![2, 3],
            },
            LensNode {
                id: 2,
                parent: Some(1),
                kind: NodeKind::Module,
                name: "mod_a".into(),
                path: PathBuf::from("crates/pkg/src/a"),
                loc: 10,
                children: vec![],
            },
            LensNode {
                id: 3,
                parent: Some(1),
                kind: NodeKind::File,
                name: "lib.rs".into(),
                path: PathBuf::from("crates/pkg/src/lib.rs"),
                loc: 20,
                children: vec![],
            },
        ];
        let graph = CodeGraph {
            root: 0,
            nodes,
            edges: vec![],
            generated_at: 0,
        };

        let expanded: HashSet<NodeId> = [0, 1].into();
        let open = layout_graph(&graph, &expanded);
        let parent = rect_of(&open, 1);
        let m = rect_of(&open, 2);
        let f = rect_of(&open, 3);

        assert!(!open.placed.iter().find(|p| p.id == 1).unwrap().collapsed);
        assert!(m.x >= parent.x + PADDING);
        assert!(f.x >= parent.x + PADDING);
        assert!(m.y >= parent.y + HEADER_H + PADDING);
        assert!(f.y >= parent.y + HEADER_H + PADDING);
        assert!(m.x + m.w <= parent.x + parent.w - PADDING);
        assert!(f.x + f.w <= parent.x + parent.w - PADDING);
        assert!(m.y + m.h <= parent.y + parent.h - PADDING);
        assert!(f.y + f.h <= parent.y + parent.h - PADDING);

        let collapsed_expanded: HashSet<NodeId> = [0].into();
        let closed = layout_graph(&graph, &collapsed_expanded);
        assert!(closed.placed.iter().find(|p| p.id == 1).unwrap().collapsed);
        assert!(!closed.placed.iter().any(|p| p.id == 2 || p.id == 3));
    }

    #[test]
    fn edge_rollup_collapsed_packages() {
        let nodes = vec![
            LensNode {
                id: 0,
                parent: None,
                kind: NodeKind::Workspace,
                name: "ws".into(),
                path: PathBuf::new(),
                loc: 0,
                children: vec![1, 2],
            },
            LensNode {
                id: 1,
                parent: Some(0),
                kind: NodeKind::Package { is_app: false },
                name: "alpha".into(),
                path: PathBuf::from("crates/alpha"),
                loc: 10,
                children: vec![3],
            },
            LensNode {
                id: 2,
                parent: Some(0),
                kind: NodeKind::Package { is_app: false },
                name: "beta".into(),
                path: PathBuf::from("crates/beta"),
                loc: 10,
                children: vec![4],
            },
            LensNode {
                id: 3,
                parent: Some(1),
                kind: NodeKind::File,
                name: "a.rs".into(),
                path: PathBuf::from("crates/alpha/src/a.rs"),
                loc: 5,
                children: vec![5],
            },
            LensNode {
                id: 4,
                parent: Some(2),
                kind: NodeKind::File,
                name: "b.rs".into(),
                path: PathBuf::from("crates/beta/src/b.rs"),
                loc: 5,
                children: vec![6],
            },
            LensNode {
                id: 5,
                parent: Some(3),
                kind: NodeKind::Item {
                    item: ItemKind::Function,
                },
                name: "fa".into(),
                path: PathBuf::from("crates/alpha/src/a.rs"),
                loc: 2,
                children: vec![],
            },
            LensNode {
                id: 6,
                parent: Some(4),
                kind: NodeKind::Item {
                    item: ItemKind::Struct,
                },
                name: "Sb".into(),
                path: PathBuf::from("crates/beta/src/b.rs"),
                loc: 2,
                children: vec![],
            },
        ];
        let edges = vec![
            LensEdge {
                from: 5,
                to: 6,
                kind: EdgeKind::Use,
                weight: 2,
            },
            LensEdge {
                from: 5,
                to: 6,
                kind: EdgeKind::Use,
                weight: 3,
            },
        ];
        let graph = CodeGraph {
            root: 0,
            nodes,
            edges,
            generated_at: 0,
        };

        let expanded: HashSet<NodeId> = [0].into();
        let layout = layout_graph(&graph, &expanded);
        assert_eq!(layout.wires.len(), 1);
        let wire = &layout.wires[0];
        assert_eq!(wire.from, 1);
        assert_eq!(wire.to, 2);
        assert_eq!(wire.kind, EdgeKind::Use);
        assert_eq!(wire.weight, 5);
    }

    #[test]
    fn loc_width_monotonicity() {
        let nodes = vec![
            LensNode {
                id: 0,
                parent: None,
                kind: NodeKind::Workspace,
                name: "ws".into(),
                path: PathBuf::new(),
                loc: 0,
                children: vec![1, 2],
            },
            LensNode {
                id: 1,
                parent: Some(0),
                kind: NodeKind::Item {
                    item: ItemKind::Function,
                },
                name: "small".into(),
                path: PathBuf::from("small.rs"),
                loc: 10,
                children: vec![],
            },
            LensNode {
                id: 2,
                parent: Some(0),
                kind: NodeKind::Item {
                    item: ItemKind::Function,
                },
                name: "big".into(),
                path: PathBuf::from("big.rs"),
                loc: 10_000,
                children: vec![],
            },
        ];
        let graph = CodeGraph {
            root: 0,
            nodes,
            edges: vec![],
            generated_at: 0,
        };

        let expanded: HashSet<NodeId> = [0].into();
        let layout = layout_graph(&graph, &expanded);
        let small = rect_of(&layout, 1);
        let big = rect_of(&layout, 2);
        assert!(big.w > small.w);
        assert!((MIN_W..=MAX_W).contains(&small.w));
        assert!((MIN_W..=MAX_W).contains(&big.w));
    }

    #[test]
    fn package_dep_cycle_finishes() {
        let nodes = vec![
            LensNode {
                id: 0,
                parent: None,
                kind: NodeKind::Workspace,
                name: "ws".into(),
                path: PathBuf::new(),
                loc: 0,
                children: vec![1, 2],
            },
            pkg(1, "x", 0, false),
            pkg(2, "y", 0, false),
        ];
        let edges = vec![
            LensEdge {
                from: 1,
                to: 2,
                kind: EdgeKind::PackageDep,
                weight: 1,
            },
            LensEdge {
                from: 2,
                to: 1,
                kind: EdgeKind::PackageDep,
                weight: 1,
            },
        ];
        let graph = CodeGraph {
            root: 0,
            nodes,
            edges,
            generated_at: 0,
        };

        let expanded: HashSet<NodeId> = [0].into();
        let layout = layout_graph(&graph, &expanded);
        assert_eq!(layout.placed.len(), 2);
        assert!(layout.bounds.w.is_finite() && layout.bounds.h.is_finite());
        assert_no_overlap(&[rect_of(&layout, 1), rect_of(&layout, 2)]);
    }

    #[test]
    fn siblings_do_not_overlap_in_grid() {
        let nodes = vec![
            LensNode {
                id: 0,
                parent: None,
                kind: NodeKind::Workspace,
                name: "ws".into(),
                path: PathBuf::new(),
                loc: 0,
                children: vec![1],
            },
            LensNode {
                id: 1,
                parent: Some(0),
                kind: NodeKind::Package { is_app: false },
                name: "pkg".into(),
                path: PathBuf::from("crates/pkg"),
                loc: 100,
                children: (2..8).collect(),
            },
        ];
        let mut nodes = nodes;
        for id in 2..8 {
            nodes.push(LensNode {
                id,
                parent: Some(1),
                kind: NodeKind::File,
                name: format!("f{id}.rs"),
                path: PathBuf::from(format!("crates/pkg/src/f{id}.rs")),
                loc: 30 + id,
                children: vec![],
            });
        }
        let graph = CodeGraph {
            root: 0,
            nodes,
            edges: vec![],
            generated_at: 0,
        };

        let expanded: HashSet<NodeId> = [0, 1].into();
        let layout = layout_graph(&graph, &expanded);
        let sibs: Vec<Rectf> = layout
            .placed
            .iter()
            .filter(|p| (2..8).contains(&p.id))
            .map(|p| p.rect)
            .collect();
        assert_eq!(sibs.len(), 6);
        assert_no_overlap(&sibs);
    }
}
