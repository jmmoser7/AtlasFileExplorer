//! Trim arrangement serialization into authored scene paths and clips.

use super::super::board_path::bounds_of_world_points;
use super::{Polygon, Pos2, WorldRect};
use slate_doc::scene::{PathContour, PathData, PathFillRule, PathSeg};

pub(crate) fn pieces_to_path_data(pieces: &[Polygon]) -> Option<(WorldRect, PathData)> {
    if pieces.is_empty() {
        return None;
    }
    let mut pts = Vec::new();
    for piece in pieces {
        for ring in piece {
            for p in ring {
                pts.push(Pos2::new(p[0], p[1]));
            }
        }
    }
    if pts.is_empty() {
        return None;
    }
    let rect = bounds_of_world_points(&pts);
    let mut extra = Vec::new();
    let mut start = [0.0, 0.0];
    let mut segs = Vec::new();
    let mut first = true;
    for piece in pieces {
        for ring in piece {
            let (s, sg) = ring_to_contour(ring, rect);
            if first {
                start = s;
                segs = sg;
                first = false;
            } else {
                extra.push(PathContour {
                    start: s,
                    segs: sg,
                    closed: true,
                });
            }
        }
    }
    let fill_rule = if extra.is_empty() {
        PathFillRule::NonZero
    } else {
        PathFillRule::EvenOdd
    };
    Some((
        rect,
        PathData {
            start,
            segs,
            closed: true,
            extra,
            fill_rule,
            tips: Vec::new(),
            field_colors: Vec::new(),
            erase: Vec::new(),
            corner_amounts: Vec::new(),
        },
    ))
}

pub(super) fn polygon_to_path_data(poly: &Polygon) -> (WorldRect, PathData) {
    let mut pts = Vec::new();
    for ring in poly {
        for p in ring {
            pts.push(Pos2::new(p[0], p[1]));
        }
    }
    let rect = bounds_of_world_points(&pts);
    let mut extra = Vec::new();
    let mut fill_rule = PathFillRule::NonZero;
    let first = ring_to_contour(&poly[0], rect);
    if poly.len() > 1 {
        fill_rule = PathFillRule::EvenOdd;
        for ring in poly.iter().skip(1) {
            extra.push(ring_to_extra(ring, rect));
        }
    }
    (
        rect,
        PathData {
            start: first.0,
            segs: first.1,
            closed: true,
            extra,
            fill_rule,
            tips: Vec::new(),
            field_colors: Vec::new(),
            erase: Vec::new(),
            corner_amounts: Vec::new(),
        },
    )
}

pub(super) fn polygons_to_clip(pieces: &[Polygon], host: WorldRect) -> PathData {
    let mut extra = Vec::new();
    let mut fill_rule = PathFillRule::EvenOdd;
    let mut start = [0.0, 0.0];
    let mut segs = Vec::new();
    let mut first = true;
    for piece in pieces {
        for (i, ring) in piece.iter().enumerate() {
            let (s, sg) = ring_to_contour(ring, host);
            if first {
                start = s;
                segs = sg;
                first = false;
            } else {
                extra.push(PathContour {
                    start: s,
                    segs: sg,
                    closed: true,
                });
            }
            let _ = i;
        }
    }
    if pieces.len() == 1 && pieces[0].len() == 1 {
        fill_rule = PathFillRule::NonZero;
    }
    PathData {
        start,
        segs,
        closed: true,
        extra,
        fill_rule,
        tips: Vec::new(),
        field_colors: Vec::new(),
        erase: Vec::new(),
        corner_amounts: Vec::new(),
    }
}

fn ring_to_contour(ring: &[[f32; 2]], rect: WorldRect) -> ([f32; 2], Vec<PathSeg>) {
    if ring.is_empty() {
        return ([0.0, 0.0], Vec::new());
    }
    let start = norm_pt(ring[0], rect);
    let segs = ring
        .iter()
        .skip(1)
        .map(|p| PathSeg::Line {
            to: norm_pt(*p, rect),
        })
        .collect();
    (start, segs)
}

fn ring_to_extra(ring: &[[f32; 2]], rect: WorldRect) -> PathContour {
    let (start, segs) = ring_to_contour(ring, rect);
    PathContour {
        start,
        segs,
        closed: true,
    }
}

fn norm_pt(p: [f32; 2], rect: WorldRect) -> [f32; 2] {
    [
        if rect.w.abs() < f32::EPSILON {
            0.0
        } else {
            (p[0] - rect.x) / rect.w
        },
        if rect.h.abs() < f32::EPSILON {
            0.0
        } else {
            (p[1] - rect.y) / rect.h
        },
    ]
}
