//! Drawing commands share the path, brush and eraser result owners with gestures.
use super::*;
use crate::app::{board_color, board_path};
use eframe::egui::Pos2;
use slate_doc::scene::{NodeKind, PathData, SceneCmd, WorldRect};
use vector_ink::kurbo::{BezPath, Point};

const MAX_TOTAL_POINTS: usize = 4096;

pub(super) fn recognizes(command: &str) -> bool {
    matches!(
        command,
        "board.paths.draw" | "board.eraser.stroke" | "board.path.join"
    )
}

pub(super) fn commands(
    app: &SlateApp,
    command: &str,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    super::prepare::validate_keys(args, super::schema::arguments(command)?)?;
    match command {
        "board.paths.draw" => draw(app, ids, args),
        "board.eraser.stroke" => erase(app, ids, args),
        "board.path.join" => join(app, ids),
        _ => Err("Unsupported drawing command".into()),
    }
}

fn join(app: &SlateApp, ids: &[NodeId]) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if !(2..=64).contains(&ids.len()) {
        return Err("Join needs 2–64 explicit open path targets".into());
    }
    let mut unique = std::collections::HashSet::with_capacity(ids.len());
    for id in ids {
        if !unique.insert(*id) {
            return Err("Join targets must be unique".into());
        }
        let node = app.doc().scene.node(*id).ok_or("Join target is missing")?;
        if node.locked {
            return Err(format!("Object {} is locked", id.0));
        }
        if node.hidden {
            return Err(format!("Object {} is hidden", id.0));
        }
        let NodeKind::Shape(shape) = &node.kind else {
            return Err("Join accepts open path shapes".into());
        };
        if !matches!(
            shape.shape,
            slate_doc::scene::ShapeKind::Line
                | slate_doc::scene::ShapeKind::Rect
                | slate_doc::scene::ShapeKind::Ellipse
                | slate_doc::scene::ShapeKind::RegularPolygon
        ) && !(shape.shape == slate_doc::scene::ShapeKind::Path
            && shape.path.as_ref().is_some_and(|path| !path.is_empty()))
        {
            return Err("Join accepts open paths and closed shapes".into());
        }
    }
    let has_closed = ids.iter().any(|id| app.doc().scene.node(*id).is_some_and(|node| {
        matches!(&node.kind, NodeKind::Shape(shape) if matches!(shape.shape, slate_doc::scene::ShapeKind::Rect | slate_doc::scene::ShapeKind::Ellipse | slate_doc::scene::ShapeKind::RegularPolygon) || shape.shape == slate_doc::scene::ShapeKind::Path && shape.path.as_ref().is_some_and(|path| path.closed))
    }));
    let (cmds, _) = if has_closed {
        super::super::super::board_join::prepare_join_regions(app, ids)
    } else {
        let (cmds, _) = super::super::super::board_direct::join::prepare_join_nodes(
            app,
            ids,
            app.board_snap_threshold_pub() as f64,
        )
        .ok_or("The selected paths cannot be joined")?;
        if cmds.len() != ids.len() + 1 {
            return Err("Join could not prepare every selected path".into());
        }
        Some((cmds, Vec::new()))
    }
    .ok_or("The selected paths cannot be joined")?;
    if cmds.is_empty() {
        return Err("The selected paths cannot be joined".into());
    }
    Ok((cmds, ids.to_vec()))
}

fn draw(
    app: &SlateApp,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if !ids.is_empty() {
        return Err("Drawing creates new paths and takes no existing targets".into());
    }
    let strokes = args["strokes"]
        .as_array()
        .filter(|v| (1..=64).contains(&v.len()))
        .ok_or("Supply 1–64 strokes")?;
    let mut scratch = app.doc().scene.node_preparation();
    let mut cmds = Vec::with_capacity(strokes.len());
    let mut created = Vec::with_capacity(strokes.len());
    let mut total_points = 0;
    for stroke in strokes {
        let mode = stroke["tool"].as_str().ok_or("Each stroke needs a tool")?;
        let points = points(&stroke["points"], 4096, if mode == "brush" { 1 } else { 2 })?;
        total_points += points.len();
        if total_points > MAX_TOTAL_POINTS {
            return Err("A draw request may contain at most 4096 total path points".into());
        }
        if mode == "line" && points.len() != 2 {
            return Err("A line needs exactly two endpoints".into());
        }
        let closed = stroke
            .get("closed")
            .map(|v| v.as_bool().ok_or("closed must be a boolean"))
            .transpose()?
            .unwrap_or(false);
        if !matches!(mode, "pen" | "line" | "polyline" | "bezier" | "brush") {
            return Err("Tool must be pen, brush, polyline, bezier, or line".into());
        }
        let width = optional_number(stroke, "width")?.unwrap_or_else(|| {
            if mode == "brush" {
                app.brush_stroke().width
            } else {
                board_path::path_tool(mode).map_or(1.0, |tool| app.stroke_for_tool(tool).width)
            }
        });
        if !(0.01..=10_000.0).contains(&width) {
            return Err("Stroke width must be between 0.01 and 10000".into());
        }
        let opacity = optional_number(stroke, "opacity")?.unwrap_or_else(|| {
            board_path::path_tool(mode).map_or(1.0, |tool| app.opacity_for_tool(tool))
        });
        if !(0.0..=1.0).contains(&opacity) {
            return Err("Opacity must be between 0 and 1".into());
        }
        let color = stroke
            .get("color")
            .map(rgba)
            .transpose()?
            .unwrap_or_else(|| {
                if mode == "brush" {
                    app.brush_stroke().color
                } else {
                    board_path::path_tool(mode)
                        .map_or(app.board_colors.fg, |tool| app.stroke_for_tool(tool).color)
                }
            });
        let node = if mode == "brush" {
            if closed {
                return Err("Brush paths are open strokes".into());
            }
            let mut style = app.brush_stroke();
            style.width = width;
            style.color = color;
            style.softness = optional_number(stroke, "softness")?.unwrap_or(style.softness);
            if !(0.0..=1.0).contains(&style.softness) {
                return Err("Brush softness must be between 0 and 1".into());
            }
            let (rect, path, tips) = if points.len() == 1 {
                let (rect, path) = board_path::points_to_path_data(&points, false);
                (rect, path, vec![slate_doc::scene::StrokeSpan::of(&style)])
            } else {
                let tip = slate_doc::scene::StrokeSpan::of(&style);
                let zoom = app.tab().cam.z.max(f32::EPSILON);
                let mut stroke = board_path::FreehandTips::new(points[0], tip);
                for point in &points[1..] {
                    stroke.push(*point, tip, zoom);
                }
                let (bez, tips) = board_path::fit_freehand(&stroke, zoom)
                    .ok_or("Brush samples could not be fitted")?;
                let (rect, path) = board_path::bezpath_to_path_data(&bez, false);
                (rect, path, tips)
            };
            let mut node = board_path::brush_path_node(&mut scratch, rect, path, style, tips)
                .ok_or("Brush path needs at least one segment")?;
            node.opacity = opacity;
            node
        } else {
            if stroke.get("softness").is_some() {
                return Err("Softness is available for brush strokes only".into());
            }
            let tool = board_path::path_tool(mode).ok_or("Unsupported drawing tool")?;
            let mut style = app.stroke_for_tool(tool);
            style.width = width;
            style.color = color;
            let (rect, path, tips) = if mode == "pen" {
                let tip = slate_doc::vertex_style::PlacedTip {
                    width,
                    color,
                    opacity,
                };
                let zoom = app.tab().cam.z.max(f32::EPSILON);
                let mut stroke = board_path::FreehandTips::new(points[0], tip);
                for point in &points[1..] {
                    stroke.push(*point, tip, zoom);
                }
                let (bez, tips) = board_path::fit_freehand(&stroke, zoom)
                    .ok_or("Pen samples could not be fitted")?;
                let (rect, path) = board_path::bezpath_to_path_data(&bez, false);
                (rect, path, tips)
            } else if mode == "bezier" {
                let (rect, path) = bezier_path(&points, closed)?;
                (rect, path, Vec::new())
            } else {
                let (rect, path) = board_path::points_to_path_data(&points, closed);
                (rect, path, Vec::new())
            };
            let fill = if closed {
                app.fill_for_new_shape()
            } else {
                None
            };
            board_path::drawn_path_node(
                &mut scratch,
                rect,
                path,
                closed,
                style,
                slate_doc::vertex_style::PlacedTip {
                    width,
                    color,
                    opacity,
                },
                fill,
                if tips.is_empty() {
                    board_path::DrawnTips::None
                } else {
                    board_path::DrawnTips::Vertices(&tips)
                },
            )
        };
        if !valid_rect(node.rect) {
            return Err("Path geometry is outside the board bounds".into());
        }
        created.push(node.id);
        cmds.push(SceneCmd::Add {
            index: app.doc().scene.nodes.len() + cmds.len(),
            node,
        });
    }
    Ok((cmds, created))
}

fn bezier_path(points: &[Pos2], closed: bool) -> Result<(WorldRect, PathData), String> {
    if points.len() < 4 || !(points.len() - 1).is_multiple_of(3) {
        return Err("Bezier points must be cubic control points in 3n+1 order".into());
    }
    let mut bez = BezPath::new();
    bez.move_to(Point::new(points[0].x as f64, points[0].y as f64));
    for curve in points[1..].chunks_exact(3) {
        bez.curve_to(
            Point::new(curve[0].x as f64, curve[0].y as f64),
            Point::new(curve[1].x as f64, curve[1].y as f64),
            Point::new(curve[2].x as f64, curve[2].y as f64),
        );
    }
    if closed {
        bez.close_path();
    }
    Ok(board_path::bezpath_to_path_data(&bez, closed))
}

fn erase(
    app: &SlateApp,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if ids.is_empty() || ids.len() > 64 {
        return Err("Eraser needs 1–64 explicit targets".into());
    }
    let points = points(&args["points"], 4096, 2)?;
    let span = board_color::targeted_eraser_span(
        app,
        optional_number(args, "radius")?,
        optional_number(args, "softness")?,
    )?;
    let radius = span.width * 0.5;
    let sample_count = board_color::eraser_sweep_sample_count(&points, radius);
    if sample_count > board_color::MAX_ERASER_SWEEP_SAMPLES {
        return Err("Eraser path exceeds the per-request sample budget".into());
    }
    let mut unique = std::collections::HashSet::with_capacity(ids.len());
    let mut targets = Vec::with_capacity(ids.len());
    let mut sweep_work = 0usize;
    for id in ids {
        if !unique.insert(*id) {
            return Err("Eraser targets must be unique".into());
        }
        let before = app
            .doc()
            .scene
            .node(*id)
            .ok_or("Eraser target is missing")?;
        if before.locked {
            return Err(format!("Object {} is locked", id.0));
        }
        if before.hidden {
            return Err(format!("Object {} is hidden", id.0));
        }
        let NodeKind::Shape(shape) = &before.kind else {
            return Err("Eraser targets must be path or line shapes".into());
        };
        if !matches!(
            shape.shape,
            slate_doc::scene::ShapeKind::Path | slate_doc::scene::ShapeKind::Line
        ) {
            return Err("Eraser targets must be path or line shapes".into());
        }
        let query = (!shape.stroke.paints_as_stamp())
            .then(|| board_color::EraserSweepQuery::prepare(before, radius, app.tab().cam.z))
            .flatten();
        if let Some(query) = &query {
            sweep_work = board_color::next_eraser_sweep_work(
                sweep_work,
                sample_count,
                query.segment_count(),
            )
            .ok_or("Eraser sweep exceeds the bounded geometry-work budget; split the sweep")?;
        }
        targets.push((before.clone(), query));
    }

    let mut patches = Vec::new();
    let mut removes = Vec::new();
    for (id, (before, query)) in ids.iter().zip(targets) {
        let NodeKind::Shape(shape) = &before.kind else {
            return Err("Eraser targets must be path or line shapes".into());
        };
        let hit = if shape.stroke.paints_as_stamp() {
            points
                .windows(2)
                .any(|segment| board_color::spot_touches(&before, segment[0], segment[1], radius))
        } else {
            query.is_some_and(|query| {
                points
                    .windows(2)
                    .any(|segment| query.hits(segment[0], segment[1]))
            })
        };
        if !hit {
            continue;
        }
        if shape.stroke.paints_as_stamp() {
            let after = board_color::with_erase_mark(&before, &points, span);
            patches.push(SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(after),
            });
        } else {
            let index = app
                .doc()
                .scene
                .index_of(*id)
                .ok_or("Eraser target is missing")?;
            removes.push((index, before.clone()));
        }
    }
    removes.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
    let mut cmds = patches;
    cmds.extend(
        removes
            .into_iter()
            .map(|(index, node)| SceneCmd::Remove { index, node }),
    );
    if cmds.is_empty() {
        return Err("The eraser path did not touch any target ink".into());
    }
    Ok((cmds, ids.to_vec()))
}

fn points(value: &Value, max: usize, min: usize) -> Result<Vec<Pos2>, String> {
    let values = value
        .as_array()
        .filter(|v| (min..=max).contains(&v.len()))
        .ok_or("Supply a bounded path with at least two points")?;
    values
        .iter()
        .map(|point| {
            let pair = point
                .as_array()
                .filter(|v| v.len() == 2)
                .ok_or("Each point must be [x,y]")?;
            let x = pair[0]
                .as_f64()
                .ok_or("Point coordinates must be finite numbers")? as f32;
            let y = pair[1]
                .as_f64()
                .ok_or("Point coordinates must be finite numbers")? as f32;
            if !x.is_finite() || !y.is_finite() || x.abs() > 1.0e9 || y.abs() > 1.0e9 {
                return Err("Point coordinates must be finite and inside board bounds".into());
            }
            Ok(Pos2::new(x, y))
        })
        .collect()
}

fn optional_number(value: &Value, key: &str) -> Result<Option<f32>, String> {
    value
        .get(key)
        .map(|v| {
            v.as_f64()
                .filter(|n| n.is_finite())
                .map(|n| n as f32)
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("{key} must be a finite number"))
        })
        .transpose()
}

fn rgba(value: &Value) -> Result<slate_doc::scene::Rgba, String> {
    let channels = value
        .as_array()
        .filter(|v| v.len() == 4)
        .ok_or("Color must be [r,g,b,a]")?;
    let mut rgba = [0; 4];
    for (i, channel) in channels.iter().enumerate() {
        rgba[i] = channel
            .as_u64()
            .filter(|v| *v <= 255)
            .ok_or("Color channels must be integers from 0 to 255")? as u8;
    }
    Ok(slate_doc::scene::Rgba(rgba))
}

fn valid_rect(rect: WorldRect) -> bool {
    [rect.x, rect.y, rect.w, rect.h]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1.0e9)
        && rect.w > 0.0
        && rect.h > 0.0
}

#[cfg(test)]
#[path = "drawing_tests.rs"]
mod tests;
