//! Journaled curve joins and their carried vertex attributes.
use super::*;

impl SlateApp {
    // ---------- Join (Ctrl+J) ----------

    /// Selection-driven join per the spec: two selected endpoints of the
    /// A-tool target merge-or-bridge; one selected open Path closes; two+
    /// selected open Paths join at nearest endpoints keeping the first
    /// node's style (Remove+Add group). Returns whether anything ran.
    pub(crate) fn cmd_join(&mut self) -> bool {
        // Case 1: A tool, both endpoints of the open target selected.
        if self.board_tool == super::super::board::BoardTool::DirectSelect {
            if let Some(id) = self.direct.node {
                if let Some((anchors, closed)) = self.direct_anchors_of(id) {
                    let last = anchors.len().saturating_sub(1);
                    let endpoints: HashSet<usize> = [0usize, last].into_iter().collect();
                    if !closed && anchors.len() >= 2 && self.direct.anchors == endpoints {
                        let radius = (self.board_snap_threshold_pub()) as f64;
                        return self.join_close_node(id, &anchors, radius);
                    }
                }
            }
        }
        // Joinable shapes in selection (z-order). Closed operands switch
        // the whole set to region union (`P2.RhinoJoin.region`).
        let mut opens = Vec::new();
        let mut closeds = Vec::new();
        let mut joinable = Vec::new();
        for n in &self.doc().scene.nodes {
            if !self.board_sel.contains(&n.id) || n.hidden || n.locked {
                continue;
            }
            let NodeKind::Shape(s) = &n.kind else {
                continue;
            };
            let closed = match s.shape {
                ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::RegularPolygon => true,
                ShapeKind::Path => s.path.as_ref().is_some_and(|p| p.closed),
                ShapeKind::Line => false,
            };
            let open = match s.shape {
                ShapeKind::Line => true,
                ShapeKind::Path => s.path.as_ref().is_some_and(|p| !p.closed && !p.is_empty()),
                _ => false,
            };
            if !closed && !open {
                continue;
            }
            joinable.push(n.id);
            if closed {
                closeds.push(n.id);
            } else {
                opens.push(n.id);
            }
        }
        if !closeds.is_empty() {
            return self.join_regions(&joinable);
        }
        match opens.len() {
            0 => false,
            // Case 2: close the single open path (merge within 24 world
            // units, else bridge with a straight closing segment).
            1 => {
                let id = opens[0];
                let Some((anchors, _)) = self.direct_anchors_of(id) else {
                    return false;
                };
                self.join_close_node(id, &anchors, 24.0)
            }
            // Case 3: object-level join — fold nearest endpoint pairs,
            // first node's style wins, per-vertex style follows its vertex,
            // one Remove+Add group.
            _ => self.join_nodes(&opens),
        }
    }

    fn join_close_node(&mut self, id: NodeId, anchors: &[Anchor], radius: f64) -> bool {
        let Some((joined, closed)) = join_endpoints(anchors, None, radius) else {
            return false;
        };
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        // The open path's anchors are its vertices, in order; a merge drops
        // the last into the first.
        let sources: Vec<f32> = (0..joined.len()).map(|i| i as f32).collect();
        let bez = bezpath_from_anchors(&joined, closed);
        self.write_back_world_bez(id, &bez, closed, Some(&sources));
        if joined.len() < anchors.len() {
            if let NodeKind::Shape(original) = &before.kind {
                let mut styles = vertex_style::vertex_styles(
                    original.path.as_deref(),
                    &original.stroke,
                    anchors.len(),
                );
                styles[0] = vertex_style::average_vertex_style(styles[0], *styles.last().unwrap());
                styles.truncate(joined.len());
                if let Some(NodeKind::Shape(shape)) =
                    self.doc_mut().scene.node_mut(id).map(|n| &mut n.kind)
                {
                    if let Some(path) = &mut shape.path {
                        let path = std::sync::Arc::make_mut(path);
                        let styles: Vec<_> = (0..=path.segs.len())
                            .map(|i| styles[i % styles.len()])
                            .collect();
                        vertex_style::apply_vertex_styles(path, &mut shape.stroke, &styles);
                    }
                }
            }
        }
        if closed {
            if let Some(NodeKind::Shape(s)) = self.doc_mut().scene.node_mut(id).map(|n| &mut n.kind)
            {
                s.stroke.keep_ends([false; 2]);
            }
        }
        if let Some(after) = self.doc().scene.node(id).cloned() {
            if after != before {
                self.tab_mut().journal.record(vec![SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                }]);
                self.tab_mut().dirty = true;
                self.note_scene_change();
            }
        }
        self.direct.anchors.clear();
        true
    }

    fn join_nodes(&mut self, ids: &[NodeId]) -> bool {
        let Some((cmds, new_id)) =
            prepare_join_nodes(self, ids, self.board_snap_threshold_pub() as f64)
        else {
            return false;
        };
        if !self.commit_scene(cmds) {
            return false;
        }
        self.board_sel.clear();
        self.board_sel.insert(new_id);
        if self.board_tool == super::super::board::BoardTool::DirectSelect {
            self.direct.node = Some(new_id);
            self.direct.anchors.clear();
        }
        true
    }
}

/// Build the same open-curve join result for the human command and agent adapter.
pub(crate) fn prepare_join_nodes(
    app: &SlateApp,
    ids: &[NodeId],
    radius: f64,
) -> Option<(Vec<SceneCmd>, NodeId)> {
    type Joined = (
        Vec<Anchor>,
        NodeId,
        Vec<VertexStyle>,
        [StrokeEnd; 2],
        [Option<[f32; 2]>; 2],
    );
    let mut acc: Option<Joined> = None;
    let mut base_cap = StrokeCap::default();
    let opacity = ids
        .iter()
        .filter_map(|id| app.doc().scene.node(*id))
        .map(|n| n.opacity)
        .fold(0.0_f32, f32::max);
    for id in ids {
        let Some((anchors, closed)) = app.direct_anchors_of(*id) else {
            continue;
        };
        if closed {
            continue;
        }
        let Some(NodeKind::Shape(s)) = app.doc().scene.node(*id).map(|n| &n.kind) else {
            continue;
        };
        let mut styles = vertex_style::vertex_styles(s.path.as_deref(), &s.stroke, anchors.len());
        let node_opacity = app.doc().scene.node(*id).map_or(1.0, |n| n.opacity);
        for style in &mut styles {
            style.tip.color.0[3] = (style.tip.color.0[3] as f32 * node_opacity
                / opacity.max(f32::EPSILON))
            .round()
            .clamp(0.0, 255.0) as u8;
        }
        let stroke = s.stroke;
        acc = Some(match acc {
            None => {
                base_cap = stroke.cap;
                let ends = [stroke.end(0), stroke.end(1)];
                (anchors, *id, styles, ends, stroke.arrow_sizes)
            }
            Some((first, style_id, first_styles, first_ends, first_sizes)) => {
                let Some((joined, _, trace)) =
                    join_endpoints_traced(&first, Some(&anchors), radius)
                else {
                    return None;
                };
                // The first node's base cap wins; a later node brings
                // only the conditions set on its own ends.
                let base = StrokeEnd {
                    cap: base_cap,
                    arrow: false,
                    narrow: false,
                };
                let second_end = |i: usize| {
                    let e = stroke.end(i);
                    StrokeEnd {
                        cap: if stroke.end_caps()[i] != stroke.cap {
                            e.cap
                        } else {
                            base.cap
                        },
                        ..e
                    }
                };
                let (first_last, second_last) = (
                    first.len().saturating_sub(1),
                    anchors.len().saturating_sub(1),
                );
                let free_end = |from: Option<&JoinSource>| match from.copied() {
                    Some(JoinSource::First(0)) => first_ends[0],
                    Some(JoinSource::First(i)) if i == first_last => first_ends[1],
                    Some(JoinSource::Second(0)) => second_end(0),
                    Some(JoinSource::Second(i)) if i == second_last => second_end(1),
                    _ => base,
                };
                let ends = [free_end(trace.first()), free_end(trace.last())];
                let free_size = |from: Option<&JoinSource>| match from.copied() {
                    Some(JoinSource::First(0)) => first_sizes[0],
                    Some(JoinSource::First(i)) if i == first_last => first_sizes[1],
                    Some(JoinSource::Second(0)) => stroke.arrow_sizes[0],
                    Some(JoinSource::Second(i)) if i == second_last => stroke.arrow_sizes[1],
                    _ => None,
                };
                let sizes = [free_size(trace.first()), free_size(trace.last())];
                let styles = vertex_style::joined_vertex_styles(&first_styles, &styles, &trace);
                (joined, style_id, styles, ends, sizes)
            }
        });
    }
    let Some((joined, style_id, styles, ends, sizes)) = acc else {
        return None;
    };
    let Some(style_node) = app.doc().scene.node(style_id).cloned() else {
        return None;
    };
    let bez = bezpath_from_anchors(&joined, false);
    let (rect, mut data) = board_path::bezpath_to_path_data(&bez, false);
    let rect = WorldRect::new(
        rect.x,
        rect.y,
        rect.w.max(MIN_DRAW * 0.1),
        rect.h.max(MIN_DRAW * 0.1),
    );
    let mut new_node = {
        let mut scene = app.doc().scene.node_preparation();
        let mut n = scene.build_node(rect, style_node.kind.clone());
        n.opacity = opacity;
        n.group = style_node.group;
        n
    };
    if let NodeKind::Shape(s) = &mut new_node.kind {
        vertex_style::apply_vertex_styles(&mut data, &mut s.stroke, &styles);
        s.stroke.arrow_sizes = sizes;
        for (i, end) in ends.into_iter().enumerate() {
            s.stroke.set_end(i, end);
        }
        s.shape = ShapeKind::Path;
        s.flip = false;
        s.path = Some(data.into());
    }
    // One group: Removes (descending index) + the Add.
    let mut removes: Vec<(usize, Node)> = ids
        .iter()
        .filter_map(|id| {
            Some((
                app.doc().scene.index_of(*id)?,
                app.doc().scene.node(*id)?.clone(),
            ))
        })
        .collect();
    removes.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
    let mut cmds: Vec<SceneCmd> = removes
        .into_iter()
        .map(|(index, node)| SceneCmd::Remove { index, node })
        .collect();
    let new_id = new_node.id;
    cmds.push(SceneCmd::Add {
        index: app.doc().scene.nodes.len().saturating_sub(ids.len()),
        node: new_node,
    });
    Some((cmds, new_id))
}
