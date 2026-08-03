use crate::model::{CommitIx, Oid, RefKind, RepoGraph, RepoQuery, TimeAxis};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RepoLayout {
    pub placed: Vec<PlacedCommit>,
    pub ribbons: Vec<Ribbon>,
    pub joins: Vec<Join>,
    pub labels: Vec<RefLabel>,
    pub elided: Vec<Elision>,
    pub bounds: Rectf,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacedCommit {
    pub ix: CommitIx,
    pub oid: Oid,
    pub x: f32,
    pub y: f32,
    pub lane: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ribbon {
    pub lane: usize,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Join {
    pub from: Oid,
    pub to: Oid,
    pub kind: JoinKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JoinKind {
    Branch,
    Merge,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RefLabel {
    pub name: String,
    pub target: Oid,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Elision {
    pub count: u32,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rectf {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

const LANE_PITCH: f32 = 24.0;
const COLUMN_PITCH: f32 = 18.0;

pub fn layout_graph(graph: &RepoGraph, query: &RepoQuery, frame: Size) -> RepoLayout {
    let lanes = assign_lanes(graph, query);
    let by_oid = graph.commit_index();
    let min_time = graph.commits.first().map(|c| c.time).unwrap_or(0);
    let max_time = graph.commits.last().map(|c| c.time).unwrap_or(min_time);
    let time_span = (max_time - min_time).max(1) as f32;

    let mut placed = Vec::with_capacity(graph.commits.len());
    for (ix, commit) in graph.commits.iter().enumerate() {
        let lane = *lanes.get(commit.oid.as_str()).unwrap_or(&0);
        let x = match query.axis {
            TimeAxis::Topological => ix as f32 * COLUMN_PITCH,
            TimeAxis::Chronological => ((commit.time - min_time) as f32 / time_span) * frame.w,
        };
        placed.push(PlacedCommit {
            ix,
            oid: commit.oid.clone(),
            x,
            y: lane as f32 * LANE_PITCH,
            lane,
        });
    }

    let mut points_by_lane: BTreeMap<usize, Vec<Point>> = BTreeMap::new();
    for p in &placed {
        points_by_lane
            .entry(p.lane)
            .or_default()
            .push(Point { x: p.x, y: p.y });
    }
    let ribbons = points_by_lane
        .into_iter()
        .map(|(lane, points)| Ribbon { lane, points })
        .collect();

    let mut joins = Vec::new();
    for commit in &graph.commits {
        let Some(&child_lane) = lanes.get(commit.oid.as_str()) else {
            continue;
        };
        for (parent_ix, parent) in commit.parents.iter().enumerate() {
            if !by_oid.contains_key(parent.as_str()) {
                continue;
            }
            let parent_lane = *lanes.get(parent.as_str()).unwrap_or(&child_lane);
            joins.push(Join {
                from: parent.clone(),
                to: commit.oid.clone(),
                kind: if parent_ix == 0 && parent_lane == child_lane {
                    JoinKind::Branch
                } else {
                    JoinKind::Merge
                },
            });
        }
    }

    let coords: HashMap<&str, (f32, f32)> = placed
        .iter()
        .map(|p| (p.oid.as_str(), (p.x, p.y)))
        .collect();
    let labels = graph
        .refs
        .iter()
        .filter(|r| !matches!(r.kind, RefKind::Head))
        .filter_map(|r| {
            let (x, y) = *coords.get(r.target.as_str())?;
            Some(RefLabel {
                name: r.name.clone(),
                target: r.target.clone(),
                x,
                y,
            })
        })
        .collect();

    let max_x = placed.iter().map(|p| p.x).fold(0.0, f32::max);
    let max_y = placed.iter().map(|p| p.y).fold(0.0, f32::max);
    RepoLayout {
        placed,
        ribbons,
        joins,
        labels,
        elided: Vec::new(),
        bounds: Rectf {
            x: 0.0,
            y: 0.0,
            w: max_x.max(frame.w),
            h: max_y.max(frame.h),
        },
        fingerprint: graph.fingerprint(),
    }
}

fn assign_lanes<'a>(graph: &'a RepoGraph, query: &RepoQuery) -> HashMap<&'a str, usize> {
    let mut lanes = HashMap::new();
    let mut next_lane = 1usize;

    if let Some(trunk) = query.trunk.as_deref() {
        if let Some(root) = graph.refs.iter().find(|r| r.name == trunk) {
            mark_first_parent_chain(graph, &root.target, 0, &mut lanes);
        }
    }

    for commit in &graph.commits {
        if lanes.contains_key(commit.oid.as_str()) {
            continue;
        }
        let inherited = commit
            .parents
            .first()
            .and_then(|p| lanes.get(p.as_str()))
            .copied();
        let lane = inherited.unwrap_or_else(|| {
            let lane = next_lane;
            next_lane += 1;
            lane
        });
        lanes.insert(commit.oid.as_str(), lane);
    }
    lanes
}

fn mark_first_parent_chain<'a>(
    graph: &'a RepoGraph,
    oid: &'a str,
    lane: usize,
    lanes: &mut HashMap<&'a str, usize>,
) {
    let by_oid = graph.commit_index();
    let mut cur = oid;
    while lanes.insert(cur, lane).is_none() {
        let Some(ix) = by_oid.get(cur).copied() else {
            break;
        };
        let Some(parent) = graph.commits[ix].parents.first() else {
            break;
        };
        cur = parent;
    }
}
