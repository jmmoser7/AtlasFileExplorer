use super::cam::FolderCam;
use super::collapse::{grip_positions, DirGrip};
use super::input::MapHover;
use super::labels::{
    label_detail_active, paint_dir_labels, paint_file_labels, paint_file_labels_detail,
    paint_portal_labels,
};
use crate::canvas_scale;
use crate::canvas_text;
use crate::theme::Palette;
use atlas_core::pack_sheet::{PackedSheet, SheetTile};
use atlas_core::tree::{self, FilePlace, Orient, Tree};
use atlas_core::types::{Family, FileEntry};
use eframe::egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, TextureId, Vec2,
};
use std::collections::HashSet;
use std::path::Path;
use vector_ink::rails::LaneSpacing;

/// File Atlas default LOD buckets (percent × 0.01 → zoom).
pub const LOD_MID: usize = 6;
pub const LOD_FULL: usize = 20;
pub const LOD_DETAIL: usize = 600;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeaderStyle {
    Bezier,
    Orthogonal,
}

#[derive(Clone, Copy)]
pub struct MapStyle {
    pub orient: Orient,
    pub leader: LeaderStyle,
    pub structure_only: bool,
    pub any_filter: bool,
    pub filter_hide: bool,
}

impl Default for MapStyle {
    fn default() -> Self {
        Self {
            orient: Orient::H,
            leader: LeaderStyle::Orthogonal,
            structure_only: false,
            any_filter: false,
            filter_hide: false,
        }
    }
}

pub trait MapMedia {
    fn texture(&mut self, file: u32) -> Option<(TextureId, Vec2)>;
    fn avg_color(&self, file: u32) -> Option<[u8; 3]>;
    fn request_preview(&mut self, file: u32);
    fn request_color(&mut self, file: u32);
    fn folder_heat(&self, dir: usize) -> Option<f32>;
    fn is_staged(&self, rel: &str) -> bool;
}

pub struct PaintArgs<'a> {
    pub canvas: Rect,
    pub cam: FolderCam,
    pub palette: Palette,
    pub style: MapStyle,
    pub hover: MapHover,
    pub selection: &'a HashSet<u32>,
    pub entries: &'a [FileEntry],
    pub file_match: &'a [bool],
    pub root: &'a Path,
    pub lod: u8,
}

pub fn lod_for(z: f32, lod_mid: usize, lod_full: usize, lod_detail: usize) -> u8 {
    let mid = lod_mid.min(lod_full.saturating_sub(1)).max(1) as f32 / 100.0;
    let full = lod_full.max(lod_mid + 1) as f32 / 100.0;
    let detail = lod_detail.max(lod_full + 1) as f32 / 100.0;
    if z < mid {
        0
    } else if z < full {
        1
    } else if z < detail {
        2
    } else {
        3
    }
}

pub fn paint_tree<M: MapMedia>(
    painter: &Painter,
    tree: &Tree,
    args: &mut PaintArgs<'_>,
    media: &mut M,
) {
    let view = Rect::from_min_max(args.cam.s2w(args.canvas.min), args.cam.s2w(args.canvas.max));
    if !tree.dirs.is_empty() {
        paint_branch(painter, tree, 0, view, args, media);
    }
}

/// Flush image tiles: no card fill, no fillet, no default border.
pub fn paint_sheet<M: MapMedia>(
    painter: &Painter,
    sheet: &PackedSheet,
    args: &mut PaintArgs<'_>,
    media: &mut M,
) {
    let view = Rect::from_min_max(args.cam.s2w(args.canvas.min), args.cam.s2w(args.canvas.max));
    for (_i, world, tile) in sheet.visible_cells(view) {
        paint_sheet_tile(painter, args, media, tile, world);
    }
}

fn paint_sheet_tile<M: MapMedia>(
    painter: &Painter,
    args: &mut PaintArgs<'_>,
    media: &mut M,
    tile: SheetTile,
    world: Rect,
) {
    let p = args.palette;
    let f = tile.cover();
    let Some(e) = args.entries.get(f as usize) else {
        return;
    };
    let sr = args.cam.w2s_rect(world);
    if sr.width() < 0.5 || sr.height() < 0.5 {
        return;
    }
    let selected = args.selection.contains(&f);
    let hovered = match tile {
        SheetTile::File(id) => args.hover.file == Some(id),
        SheetTile::Stack { dir, .. } => args.hover.dir == Some(dir),
    };
    let fam_color = e.family.color();

    if args.lod == 0 {
        media.request_color(f);
        let c = media
            .avg_color(f)
            .map(|[r, g, b]| Color32::from_rgb(r, g, b))
            .unwrap_or(fam_color.gamma_multiply(0.5));
        painter.rect_filled(sr, CornerRadius::ZERO, c);
    } else {
        media.request_preview(f);
        let mut drew = false;
        if let Some((tex, size)) = media.texture(f) {
            let uv = cover_uv(size, sr.size());
            painter.image(tex, sr, uv, Color32::WHITE);
            drew = true;
        }
        if !drew {
            media.request_color(f);
            let c = media
                .avg_color(f)
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(fam_color.gamma_multiply(0.28));
            painter.rect_filled(sr, CornerRadius::ZERO, c);
        }
        if hovered && !selected {
            painter.rect_filled(
                sr,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(255, 255, 255, 28),
            );
        }
    }
    if selected {
        painter.rect_stroke(
            sr,
            CornerRadius::ZERO,
            Stroke::new(canvas_scale::px(2.0, args.cam.z), p.select),
            StrokeKind::Inside,
        );
    }
}

fn hide(args: &PaintArgs<'_>) -> bool {
    args.style.filter_hide && args.style.any_filter
}

fn matched(args: &PaintArgs<'_>, i: usize) -> bool {
    args.file_match.get(i).copied().unwrap_or(true)
}

/// Leader fan along the folder card's edge. No floor: the folder map packs
/// as tight as its room.
const LEADER_EXITS: LaneSpacing = LaneSpacing {
    preferred: 4.0,
    min: 0.0,
};
/// Leader turn depth steps out from the card.
const LEADER_RAILS: LaneSpacing = LaneSpacing {
    preferred: 8.0,
    min: 0.0,
};

fn paint_branch<M: MapMedia>(
    painter: &Painter,
    tree: &Tree,
    di: usize,
    view: Rect,
    args: &mut PaintArgs<'_>,
    media: &mut M,
) {
    let p = args.palette;
    let d = &tree.dirs[di];
    if !args.style.structure_only && hide(args) && di != 0 && d.desc_matches == 0 {
        return;
    }
    if !d.bounds.expand(40.0).intersects(view) {
        return;
    }
    let z = args.cam.z;
    let dimming = args.style.any_filter;
    let lod = args.lod;

    if !d.collapsed {
        let v = args.style.orient == Orient::V;
        let px = if v { d.x + d.w } else { d.x + d.w / 2.0 };
        let py = if v { d.y } else { d.y + d.h / 2.0 };
        let stroke_w = if lod == 0 { 1.6 } else { (1.3 * z).max(1.0) };
        let port = Pos2::new(px, py);
        let mut targets: Vec<Pos2> = Vec::new();
        for &c in d.child_dirs.iter() {
            let cd = &tree.dirs[c as usize];
            if !args.style.structure_only && hide(args) && cd.desc_matches == 0 {
                continue;
            }
            targets.push(if v {
                Pos2::new(cd.x, cd.y)
            } else {
                Pos2::new(cd.x + cd.w / 2.0, cd.y - cd.h / 2.0)
            });
        }
        if let Some(gb) = d.grid_bounds {
            targets.push(if v {
                Pos2::new(gb.min.x, gb.center().y)
            } else {
                Pos2::new(gb.center().x, gb.min.y)
            });
        }

        let mut routes: Vec<Option<(f32, f32)>> = vec![None; targets.len()];
        let (p_b, p_d) = if v { (py, px) } else { (px, py) };
        let breadth = |t: &Pos2| if v { t.y } else { t.x };
        let depth_of = |t: &Pos2| if v { t.x } else { t.y };
        let offsets: Vec<f32> = targets.iter().map(|t| breadth(t) - p_b).collect();
        let rails = vector_ink::rails::nested_rails(&offsets, 0.5);
        let exit_limit = ((if v { d.h } else { d.w }) / 2.0 - 8.0).max(2.0);
        for sign in [-1.0f32, 1.0f32] {
            let side = || {
                rails
                    .iter()
                    .enumerate()
                    .filter_map(move |(i, r)| r.filter(|r| r.side == sign).map(|r| (i, r)))
            };
            let min_td = side()
                .map(|(i, _)| depth_of(&targets[i]))
                .fold(f32::INFINITY, f32::min);
            for (i, rail) in side() {
                let exit_gap = LEADER_EXITS.fit(rail.count, exit_limit);
                let avail = (min_td - p_d - 16.0 - 14.0).max(0.0);
                let rail_gap = LEADER_RAILS.fit(rail.count - 1, avail);
                let exit = p_b + rail.exit(exit_gap);
                let rail = (p_d + 16.0 + rail.rank as f32 * rail_gap)
                    .min(depth_of(&targets[i]) - 12.0)
                    .max(p_d + 4.0);
                routes[i] = Some((exit, rail));
            }
        }

        for (i, tgt) in targets.iter().enumerate() {
            let edge_extent = Rect::from_two_pos(port, *tgt);
            if !edge_extent.expand(60.0).intersects(view) {
                continue;
            }
            route_edge(painter, args, port, *tgt, routes[i], v, stroke_w);
        }

        if let Some(gb) = d.grid_bounds {
            if gb.expand(40.0).intersects(view) && lod > 0 {
                let sr = args.cam.w2s_rect(gb);
                let dash = canvas_scale::px(7.0, z);
                let gap = canvas_scale::px(6.0, z);
                let pts = [
                    sr.min,
                    Pos2::new(sr.max.x, sr.min.y),
                    sr.max,
                    Pos2::new(sr.min.x, sr.max.y),
                    sr.min,
                ];
                for w in pts.windows(2) {
                    painter.add(eframe::egui::Shape::dashed_line(
                        w,
                        Stroke::new(1.0_f32, p.border_strong),
                        dash,
                        gap,
                    ));
                }
            }
        }

        for &f in &d.files {
            let fp = &tree.file_pos[f as usize];
            if fp.place == FilePlace::Hidden {
                continue;
            }
            let fr = fp.rect();
            if !fr.intersects(view) {
                continue;
            }
            let parent_heat = media.folder_heat(di);
            paint_file_card(painter, args, media, f, fr, dimming, parent_heat);
        }
    }

    paint_dir_node(painter, tree, di, args, media);

    if !d.collapsed {
        for &c in &d.child_dirs {
            if !args.style.structure_only && hide(args) && tree.dirs[c as usize].desc_matches == 0 {
                continue;
            }
            paint_branch(painter, tree, c as usize, view, args, media);
        }
    }
}

fn route_edge(
    painter: &Painter,
    args: &PaintArgs<'_>,
    port: Pos2,
    tgt: Pos2,
    route: Option<(f32, f32)>,
    v: bool,
    stroke_w: f32,
) {
    let stroke = Stroke::new(stroke_w, args.palette.line);
    let Some((exit, rail)) = route else {
        painter.line_segment([args.cam.w2s(port), args.cam.w2s(tgt)], stroke);
        return;
    };
    let start = if v {
        Pos2::new(port.x, exit)
    } else {
        Pos2::new(exit, port.y)
    };
    let (m1, m2) = if v {
        (Pos2::new(rail, exit), Pos2::new(rail, tgt.y))
    } else {
        (Pos2::new(exit, rail), Pos2::new(tgt.x, rail))
    };
    if args.style.leader == LeaderStyle::Orthogonal {
        let pts = [
            args.cam.w2s(start),
            args.cam.w2s(m1),
            args.cam.w2s(m2),
            args.cam.w2s(tgt),
        ];
        rounded_route(painter, &pts, canvas_scale::px(9.0, args.cam.z), stroke);
        return;
    }
    painter.add(eframe::egui::Shape::CubicBezier(
        eframe::egui::epaint::CubicBezierShape::from_points_stroke(
            [
                args.cam.w2s(start),
                args.cam.w2s(m1),
                args.cam.w2s(m2),
                args.cam.w2s(tgt),
            ],
            false,
            Color32::TRANSPARENT,
            stroke,
        ),
    ));
}

fn paint_dir_node<M: MapMedia>(
    painter: &Painter,
    tree: &Tree,
    di: usize,
    args: &mut PaintArgs<'_>,
    media: &mut M,
) {
    let p = args.palette;
    let d = &tree.dirs[di];
    let z = args.cam.z;
    let sr = args.cam.w2s_rect(d.rect());
    let hovered = args.hover.dir == Some(di as u32);
    let heat = media.folder_heat(di);
    let lod = args.lod;

    if lod == 0 {
        painter.rect_filled(
            sr,
            CornerRadius::ZERO,
            if tree.shows_portal(di) {
                p.portal.gamma_multiply(0.85)
            } else {
                p.accent.gamma_multiply(0.75)
            },
        );
        if let Some(h) = heat {
            painter.rect_stroke(
                sr,
                CornerRadius::ZERO,
                Stroke::new(1.5_f32, folder_heat_color(h)),
                StrokeKind::Inside,
            );
        }
        return;
    }

    if tree.shows_portal(di) {
        paint_group_preview(painter, tree, di, sr, args, media);
        return;
    }

    let cr = 10.0 * z;
    let fill = if hovered { p.card_hover } else { p.card };
    painter.rect_filled(sr, cr, fill);
    let stroke_w = if hovered { 1.6_f32 } else { 1.25_f32 };
    let stroke_c = match heat {
        Some(h) => folder_heat_color(h),
        None if hovered => p.border_strong,
        None => p.border,
    };
    painter.rect_stroke(sr, cr, Stroke::new(stroke_w, stroke_c), StrokeKind::Inside);

    let ring_c = args.cam.w2s(Pos2::new(d.x + 20.0, d.y));
    let ring_r = 6.5 * z;
    if ring_r > 1.5 {
        painter.circle_stroke(ring_c, ring_r, Stroke::new((1.8 * z).max(1.0), p.accent));
        if !d.collapsed {
            painter.circle_filled(ring_c, 2.4 * z, p.accent);
        }
    }

    if d.collapsed && (hovered || lod >= 2) {
        let (inc, full) = grip_positions(sr, z, args.style.orient);
        let grip_r = canvas_scale::px(4.5, z);
        let inc_hover = hovered && args.hover.grip == Some(DirGrip::Incremental);
        let full_hover = hovered && args.hover.grip == Some(DirGrip::Full);
        painter.circle_filled(
            inc,
            grip_r + if inc_hover { 2.0 } else { 0.0 },
            if inc_hover { p.accent } else { p.border_strong },
        );
        painter.circle_stroke(
            full,
            grip_r + if full_hover { 2.0 } else { 0.0 },
            Stroke::new(
                1.5_f32,
                if full_hover {
                    p.portal
                } else {
                    p.border_strong
                },
            ),
        );
        painter.circle_stroke(
            full,
            grip_r * 0.55,
            Stroke::new(
                1.2_f32,
                if full_hover {
                    p.portal
                } else {
                    p.border_strong
                },
            ),
        );
    }

    paint_dir_labels(painter, d, d.rect(), args);
}

fn paint_group_preview<M: MapMedia>(
    painter: &Painter,
    tree: &Tree,
    di: usize,
    sr: Rect,
    args: &mut PaintArgs<'_>,
    media: &mut M,
) {
    let p = args.palette;
    let d = &tree.dirs[di];
    let z = args.cam.z;
    let hovered = args.hover.dir == Some(di as u32);
    let heat = media.folder_heat(di);
    let cr = 12.0 * z;
    let fill = if hovered { p.card_hover } else { p.card };
    painter.rect_filled(sr, cr, fill);
    let stroke_c = match heat {
        Some(h) => folder_heat_color(h),
        None => p.portal,
    };
    painter.rect_stroke(
        sr,
        cr,
        Stroke::new(if hovered { 1.8_f32 } else { 1.4_f32 }, stroke_c),
        StrokeKind::Inside,
    );

    if label_detail_active(args.lod, sr, args.canvas) {
        paint_dir_labels(painter, d, d.rect(), args);
        return;
    }

    let pad = 9.0 * z;
    let mos_h = sr.height() - 62.0 * z;
    let mos = Rect::from_min_size(
        sr.min + Vec2::splat(pad),
        Vec2::new(sr.width() - pad * 2.0, mos_h.max(0.0)),
    );
    if mos.height() > 2.0 && !args.style.structure_only {
        let mp = painter.with_clip_rect(mos);
        mp.rect_filled(mos, CornerRadius::ZERO, p.thumb_bg);
        let gp = 3.0 * z;
        let cw = (mos.width() - gp * 2.0) / 3.0;
        let ch = (mos.height() - gp * 2.0) / 3.0;
        for i in 0..9usize {
            let sample = d.portal_samples.get(i).copied();
            let cell = Rect::from_min_size(
                mos.min + Vec2::new((i % 3) as f32 * (cw + gp), (i / 3) as f32 * (ch + gp)),
                Vec2::new(cw, ch),
            );
            match sample {
                Some(f) => {
                    if args.lod >= 1 {
                        media.request_preview(f);
                    }
                    if let Some((tex, size)) = media.texture(f) {
                        let uv = cover_uv(size, cell.size());
                        mp.image(tex, cell, uv, Color32::WHITE);
                    } else {
                        let e = &args.entries[f as usize];
                        let c = media
                            .avg_color(f)
                            .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                            .unwrap_or(e.family.color().gamma_multiply(0.16));
                        mp.rect_filled(cell, CornerRadius::ZERO, c);
                    }
                }
                None => {
                    mp.rect_filled(cell, CornerRadius::ZERO, p.thumb_bg.gamma_multiply(1.4));
                }
            }
        }
    }

    let world = d.rect();
    let mosaic_bottom = world.min.y + 9.0 + (world.height() - 62.0).max(0.0);
    let footer = Rect::from_min_max(
        Pos2::new(
            world.min.x + 9.0,
            (world.max.y - 48.0).max(mosaic_bottom + 2.0),
        ),
        Pos2::new(world.max.x - 9.0, world.max.y - 6.0),
    );
    if footer.height() * z >= 4.0 && args.lod >= 1 {
        paint_portal_labels(painter, d, footer, args);
    }
}

fn paint_file_card<M: MapMedia>(
    painter: &Painter,
    args: &mut PaintArgs<'_>,
    media: &mut M,
    f: u32,
    world: Rect,
    dimming: bool,
    parent_heat: Option<f32>,
) {
    let p = args.palette;
    let i = f as usize;
    let Some(e) = args.entries.get(i) else {
        return;
    };
    let e = e.clone();
    let z = args.cam.z;
    let sr = args.cam.w2s_rect(world);
    let file_matched = matched(args, i);
    let alpha = if dimming && !file_matched { 0.15 } else { 1.0 };
    let fam_color = e.family.color();
    let selected = args.selection.contains(&f);
    let hovered = args.hover.file == Some(f);
    let lod = args.lod;

    if lod == 0 {
        media.request_color(f);
        let c = media
            .avg_color(f)
            .map(|[r, g, b]| Color32::from_rgb(r, g, b))
            .unwrap_or(fam_color.gamma_multiply(0.5));
        painter.rect_filled(sr, CornerRadius::ZERO, c.gamma_multiply(alpha));
        if selected {
            painter.rect_stroke(
                sr,
                CornerRadius::ZERO,
                Stroke::new(1.0_f32, p.select),
                StrokeKind::Inside,
            );
        } else if let Some(h) = parent_heat {
            painter.rect_stroke(
                sr,
                CornerRadius::ZERO,
                Stroke::new(1.2_f32, folder_heat_color(h).gamma_multiply(alpha)),
                StrokeKind::Inside,
            );
        }
        return;
    }

    let cr = 9.0 * z;
    let card_fill = if hovered || selected {
        p.card_hover
    } else {
        p.card
    };
    painter.rect_filled(sr, cr, card_fill.gamma_multiply(alpha));
    let border = if selected {
        Stroke::new(2.0_f32, p.select)
    } else if let Some(h) = parent_heat {
        Stroke::new(
            if hovered { 1.5_f32 } else { 1.25_f32 },
            folder_heat_color(h).gamma_multiply(alpha),
        )
    } else if file_matched && dimming {
        Stroke::new(1.2_f32, p.accent.gamma_multiply(0.65))
    } else if hovered {
        Stroke::new(1.4_f32, p.border_strong)
    } else {
        Stroke::new(1.0_f32, p.border.gamma_multiply(alpha))
    };
    painter.rect_stroke(sr, cr, border, StrokeKind::Inside);

    if lod >= 2 {
        let detail = label_detail_active(lod, sr, args.canvas);
        let pad = if detail { world.width() * 0.04 } else { 6.0 };
        let thumb_h = if detail {
            (world.height() * 0.42).min(tree::THUMB_H * 1.15)
        } else {
            tree::THUMB_H
        };
        let thumb_world = Rect::from_min_size(
            world.min + Vec2::splat(pad),
            Vec2::new((world.width() - pad * 2.0).max(0.0), thumb_h),
        );
        let thumb = args.cam.w2s_rect(thumb_world);
        let tp = painter.with_clip_rect(thumb);
        tp.rect_filled(thumb, CornerRadius::ZERO, p.thumb_bg.gamma_multiply(alpha));
        media.request_preview(f);
        let mut drew = false;
        if let Some((tex, size)) = media.texture(f) {
            let uv = cover_uv(size, thumb.size());
            tp.image(tex, thumb, uv, Color32::WHITE.gamma_multiply(alpha));
            drew = true;
            if e.family == Family::Video {
                let c = thumb.max - Vec2::splat(14.0 * z);
                let r = 9.0 * z;
                if r > 2.0 {
                    tp.circle_filled(
                        Pos2::new(c.x, c.y),
                        r,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 230),
                    );
                    canvas_text::text(
                        &tp,
                        Pos2::new(c.x + r * 0.08, c.y),
                        Align2::CENTER_CENTER,
                        "▶",
                        FontId::proportional(r),
                        p.ink,
                    );
                }
            }
        }
        if !drew {
            let c = media
                .avg_color(f)
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(fam_color.gamma_multiply(0.14));
            tp.rect_filled(thumb, CornerRadius::ZERO, c.gamma_multiply(alpha));
            let glyph_px = 14.0 * z;
            if glyph_px >= 6.0 {
                canvas_text::text(
                    &tp,
                    thumb.center(),
                    Align2::CENTER_CENTER,
                    format_args!(".{}", if e.ext.is_empty() { "?" } else { &e.ext }),
                    FontId::monospace(glyph_px),
                    if media.avg_color(f).is_some() {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 217)
                    } else {
                        fam_color
                    },
                );
            }
        }
        if detail {
            let region = Rect::from_min_max(
                Pos2::new(world.min.x + pad, thumb_world.max.y + pad * 0.6),
                Pos2::new(world.max.x - pad, world.max.y - pad),
            );
            paint_file_labels_detail(painter, &e, region, sr, args.cam, fam_color, alpha, &p);
        } else {
            paint_file_labels(painter, &e, world, args, fam_color, alpha);
        }
        if media.is_staged(&e.rel) {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(sr.min.x, sr.max.y - 2.0),
                    Vec2::new(sr.width(), 2.0),
                ),
                CornerRadius::ZERO,
                p.staged.gamma_multiply(alpha),
            );
        }
    } else {
        media.request_color(f);
        let c = media
            .avg_color(f)
            .map(|[r, g, b]| Color32::from_rgb(r, g, b))
            .unwrap_or(fam_color.gamma_multiply(0.28));
        let inner = sr.shrink(5.0 * z);
        painter.rect_filled(inner, 6.0 * z, c.gamma_multiply(alpha));
        if media.is_staged(&e.rel) {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(sr.min.x, sr.max.y - 2.0),
                    Vec2::new(sr.width(), 2.0),
                ),
                CornerRadius::ZERO,
                p.staged.gamma_multiply(alpha),
            );
        }
    }
}

fn cover_uv(tex_size: Vec2, cell: Vec2) -> Rect {
    if tex_size.x <= 0.0 || tex_size.y <= 0.0 || cell.x <= 0.0 || cell.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let tex_aspect = tex_size.x / tex_size.y;
    let cell_aspect = cell.x / cell.y;
    if tex_aspect > cell_aspect {
        let frac = cell_aspect / tex_aspect;
        let x0 = (1.0 - frac) / 2.0;
        Rect::from_min_max(Pos2::new(x0, 0.0), Pos2::new(x0 + frac, 1.0))
    } else {
        let frac = tex_aspect / cell_aspect;
        let y0 = (1.0 - frac) / 2.0;
        Rect::from_min_max(Pos2::new(0.0, y0), Pos2::new(1.0, y0 + frac))
    }
}

fn rounded_route(painter: &Painter, pts: &[Pos2], radius: f32, stroke: Stroke) {
    if pts.len() < 2 {
        return;
    }
    let mut cursor = pts[0];
    for i in 1..pts.len() {
        let cur = pts[i];
        if i + 1 < pts.len() {
            let next = pts[i + 1];
            let in_v = cur - cursor;
            let out_v = next - cur;
            let in_len = in_v.length();
            let out_len = out_v.length();
            let r = radius.min(in_len * 0.5).min(out_len * 0.5);
            if r < 0.5 || in_len < 0.5 || out_len < 0.5 {
                if in_len >= 0.5 {
                    painter.line_segment([cursor, cur], stroke);
                }
                cursor = cur;
                continue;
            }
            let a = cur - in_v.normalized() * r;
            let b = cur + out_v.normalized() * r;
            painter.line_segment([cursor, a], stroke);
            painter.add(eframe::egui::Shape::CubicBezier(
                eframe::egui::epaint::CubicBezierShape::from_points_stroke(
                    [a, cur, cur, b],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ),
            ));
            cursor = b;
        } else {
            painter.line_segment([cursor, cur], stroke);
        }
    }
}

/// Turbo colormap used by File Atlas folder-heat strokes.
pub fn folder_heat_color(t: f32) -> Color32 {
    let x = f64::from(t.clamp(0.0, 1.0));
    let x2 = x * x;
    let x3 = x2 * x;
    let x4 = x2 * x2;
    let x5 = x4 * x;
    let r = (0.135_721_38 + 4.615_392_60 * x - 42.660_322_58 * x2 + 132.131_082_34 * x3
        - 152.942_393_96 * x4
        + 59.286_379_43 * x5)
        .clamp(0.0, 1.0);
    let g = (0.091_402_61 + 2.194_188_39 * x + 4.842_966_58 * x2 - 14.185_033_33 * x3
        + 4.277_298_57 * x4
        + 2.829_566_04 * x5)
        .clamp(0.0, 1.0);
    let b = (0.106_673_30 + 12.641_946_08 * x - 60.582_048_36 * x2 + 110.362_767_71 * x3
        - 89.903_109_12 * x4
        + 27.348_249_73 * x5)
        .clamp(0.0, 1.0);
    Color32::from_rgb(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}
