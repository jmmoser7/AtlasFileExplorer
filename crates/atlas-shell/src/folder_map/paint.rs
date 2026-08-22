use super::cam::FolderCam;
use super::collapse::{grip_positions, DirGrip};
use super::input::MapHover;
use crate::canvas_scale;
use crate::theme::Palette;
use crate::widgets::group_digits;
use atlas_core::tree::{self, FilePlace, Orient, Tree};
use atlas_core::types::{date_string, human_size, Family, FileEntry};
use eframe::egui::{
    Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, TextureId, Vec2,
};
use std::collections::HashSet;
use std::path::Path;

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

fn hide(args: &PaintArgs<'_>) -> bool {
    args.style.filter_hide && args.style.any_filter
}

fn matched(args: &PaintArgs<'_>, i: usize) -> bool {
    args.file_match.get(i).copied().unwrap_or(true)
}

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
        let mut neg: Vec<(f32, usize)> = Vec::new();
        let mut pos: Vec<(f32, usize)> = Vec::new();
        for (i, tp) in targets.iter().enumerate() {
            let db = breadth(tp) - p_b;
            if db > 0.5 {
                pos.push((db, i));
            } else if db < -0.5 {
                neg.push((-db, i));
            }
        }
        let exit_limit = ((if v { d.h } else { d.w }) / 2.0 - 8.0).max(2.0);
        for (mut list, sign) in [(neg, -1.0f32), (pos, 1.0f32)] {
            if list.is_empty() {
                continue;
            }
            list.sort_by(|a, b| b.0.total_cmp(&a.0));
            let n = list.len() as f32;
            let exit_gap = 4.0f32.min(exit_limit / n);
            let min_td = list
                .iter()
                .map(|&(_, i)| depth_of(&targets[i]))
                .fold(f32::INFINITY, f32::min);
            let avail = (min_td - p_d - 16.0 - 14.0).max(0.0);
            let rail_gap = if n > 1.0 {
                8.0f32.min(avail / (n - 1.0))
            } else {
                8.0
            };
            for (r, &(_, i)) in list.iter().enumerate() {
                let exit = p_b + sign * (n - r as f32) * exit_gap;
                let rail = (p_d + 16.0 + r as f32 * rail_gap)
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

    paint_dir_labels(painter, d, sr, args);
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
        paint_dir_labels_detail(painter, d, sr, args);
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

    let footer = Rect::from_min_max(
        Pos2::new(
            sr.min.x + pad,
            (sr.max.y - 48.0 * z).max(mos.max.y + 2.0 * z),
        ),
        Pos2::new(sr.max.x - pad, sr.max.y - 6.0 * z),
    );
    if footer.height() >= 4.0 && args.lod >= 1 {
        paint_portal_labels(painter, d, footer, args);
    }
}

fn paint_dir_labels(painter: &Painter, d: &tree::DirNode, sr: Rect, args: &PaintArgs<'_>) {
    if label_detail_active(args.lod, sr, args.canvas) {
        paint_dir_labels_detail(painter, d, sr, args);
        return;
    }
    let p = args.palette;
    let z = args.cam.z;
    let pad_l = 34.0 * z;
    let pad_r = 8.0 * z;
    let pad_y = (3.0 * z).max(1.0);
    let max_w = sr.width() - pad_l - pad_r;
    let max_h = sr.height() - pad_y * 2.0;
    if max_w < 6.0 || max_h < 4.0 {
        return;
    }
    let name_px = (max_h * 0.36).max(0.0);
    if name_px < 3.5 {
        return;
    }
    let sub_px = (max_h * 0.24).max(0.0);
    let lod = args.lod;
    let close = max_h >= 28.0 || sr.width() >= 260.0;
    let show_meta = lod >= 1 && sub_px >= 3.0 && max_h >= name_px * 1.7;
    let show_path = lod >= 2 && close && !d.rel.is_empty() && max_h >= name_px * 2.6;
    let show_match = args.style.any_filter && d.desc_matches > 0 && d.collapsed && lod >= 2;
    let name_font = FontId::proportional(name_px);
    let sub_font = FontId::proportional(sub_px);
    let path_px = (sub_px * 0.92).max(3.0);
    let path_font = FontId::proportional(path_px);
    let name = ellipsize_to_width(painter, &d.name, &name_font, max_w);
    let meta = if show_meta {
        let mut m = if lod >= 2 {
            format!(
                "{} files · {}",
                group_digits(d.desc_files as u64),
                human_size(d.desc_bytes)
            )
        } else {
            format!(
                "{} · {}",
                group_digits(d.desc_files as u64),
                human_size(d.desc_bytes)
            )
        };
        if lod >= 2 && d.ctime > 0 {
            m.push_str(" · created ");
            m.push_str(&date_string(d.ctime));
        }
        if lod >= 2 && !d.owner.is_empty() {
            m.push_str(" · ");
            m.push_str(&d.owner);
        } else if lod == 1 && d.ctime > 0 && close {
            m.push_str(" · ");
            m.push_str(&date_string(d.ctime));
        }
        if d.collapsed {
            m.push_str("  ▸");
        }
        Some(ellipsize_to_width(painter, &m, &sub_font, max_w))
    } else {
        None
    };
    let path = if show_path {
        Some(ellipsize_to_width(painter, &d.rel, &path_font, max_w))
    } else {
        None
    };
    let match_badge = if show_match {
        Some(format!("{} match", group_digits(d.desc_matches as u64)))
    } else {
        None
    };
    let lp = painter.with_clip_rect(sr);
    let mut lines: Vec<(String, FontId, Color32)> = Vec::with_capacity(3);
    lines.push((name, name_font, p.ink));
    if let Some(m) = meta {
        lines.push((m, sub_font.clone(), p.sub));
    }
    if let Some(path) = path {
        lines.push((path, path_font, p.sub.gamma_multiply(0.85)));
    }
    let line_gap = 1.15;
    let block_h: f32 = lines.iter().map(|(_, f, _)| f.size * line_gap).sum();
    let mut y = sr.min.y + pad_y + (max_h - block_h).max(0.0) * 0.5 + lines[0].1.size * 0.5;
    let x = sr.min.x + pad_l;
    for (text, font, color) in &lines {
        if !text.is_empty() {
            lp.text(
                Pos2::new(x, y),
                Align2::LEFT_CENTER,
                text,
                font.clone(),
                *color,
            );
        }
        y += font.size * line_gap;
    }
    if let Some(label) = match_badge {
        lp.text(
            Pos2::new(sr.max.x - pad_r, sr.min.y + pad_y + sub_px * 0.55),
            Align2::RIGHT_CENTER,
            label,
            sub_font,
            p.accent,
        );
    }
}

fn paint_dir_labels_detail(painter: &Painter, d: &tree::DirNode, sr: Rect, args: &PaintArgs<'_>) {
    let p = args.palette;
    let pad_l = sr.width() * 0.06;
    let pad_r = sr.width() * 0.04;
    let pad_y = sr.height() * 0.08;
    let max_w = sr.width() - pad_l - pad_r;
    let max_h = sr.height() - pad_y * 2.0;
    if max_w < 40.0 || max_h < 24.0 {
        return;
    }
    let name_px = max_h * 0.11;
    let body_px = max_h * 0.048;
    let name_font = FontId::proportional(name_px);
    let body_font = FontId::proportional(body_px);
    let path_font = FontId::proportional(body_px * 0.95);
    let abs = dir_abs(args.root, d);
    let mut details = format!(
        "{} files · {}",
        group_digits(d.desc_files as u64),
        human_size(d.desc_bytes)
    );
    if d.ctime > 0 {
        details.push_str("\nCreated ");
        details.push_str(&date_string(d.ctime));
    }
    if !d.owner.is_empty() {
        details.push_str("\nOwner ");
        details.push_str(&d.owner);
    }
    if d.collapsed {
        details.push_str("\nCollapsed — expand to show contents");
    }
    if args.style.any_filter && d.desc_matches > 0 {
        details.push('\n');
        details.push_str(&group_digits(d.desc_matches as u64));
        details.push_str(if d.desc_matches == 1 {
            " filter match"
        } else {
            " filter matches"
        });
    }
    let lp = painter.with_clip_rect(sr);
    let x = sr.min.x + pad_l;
    let mut y = sr.min.y + pad_y;
    let gap = (body_px * 0.35).max(4.0);
    let name_galley = painter.layout(d.name.clone(), name_font, p.ink, max_w);
    let name_h = name_galley.size().y;
    lp.galley(Pos2::new(x, y), name_galley, p.ink);
    y += name_h + gap;
    let remain = (sr.max.y - pad_y - y).max(0.0);
    if remain < body_px * 1.2 || abs.is_empty() {
        return;
    }
    let path_budget = (remain * 0.55).max(body_px * 2.0);
    let path_galley = painter.layout(abs, path_font, p.sub.gamma_multiply(0.9), max_w);
    let path_h = path_galley.size().y.min(path_budget);
    lp.galley(Pos2::new(x, y), path_galley, p.sub);
    y += path_h + gap;
    if (sr.max.y - pad_y - y) >= body_px {
        let details_galley = painter.layout(details, body_font, p.sub, max_w);
        lp.galley(Pos2::new(x, y), details_galley, p.sub);
    }
}

fn paint_portal_labels(painter: &Painter, d: &tree::DirNode, footer: Rect, args: &PaintArgs<'_>) {
    let p = args.palette;
    let max_w = footer.width();
    let max_h = footer.height();
    if max_w < 6.0 || max_h < 4.0 {
        return;
    }
    let name_px = (max_h * 0.42).max(0.0);
    if name_px < 3.5 {
        return;
    }
    let sub_px = (max_h * 0.30).max(0.0);
    let lod = args.lod;
    let show_meta = lod >= 1 && sub_px >= 3.0 && max_h >= name_px * 1.6;
    let show_path = lod >= 2 && max_h >= 36.0 && !d.rel.is_empty();
    let name_font = FontId::proportional(name_px);
    let sub_font = FontId::proportional(sub_px);
    let path_font = FontId::proportional((sub_px * 0.9).max(3.0));
    let name = ellipsize_to_width(painter, &d.name, &name_font, max_w);
    let meta = if show_meta {
        let m = format!(
            "{} items · {}",
            group_digits((d.child_dirs.len() + d.files.len()) as u64),
            human_size(d.desc_bytes)
        );
        Some(ellipsize_to_width(painter, &m, &sub_font, max_w * 0.72))
    } else {
        None
    };
    let lp = painter.with_clip_rect(footer);
    let x = footer.min.x + 2.0;
    let mut y = footer.min.y + name_px * 0.55;
    lp.text(Pos2::new(x, y), Align2::LEFT_CENTER, name, name_font, p.ink);
    y += name_px * 1.15;
    if let Some(m) = meta {
        lp.text(
            Pos2::new(x, y),
            Align2::LEFT_CENTER,
            m,
            sub_font.clone(),
            p.sub,
        );
        lp.text(
            Pos2::new(footer.max.x - 2.0, y),
            Align2::RIGHT_CENTER,
            "Enter ↵",
            sub_font,
            p.accent,
        );
    }
    if show_path {
        y += sub_px * 1.1;
        let path = ellipsize_to_width(painter, &d.rel, &path_font, max_w);
        lp.text(Pos2::new(x, y), Align2::LEFT_CENTER, path, path_font, p.sub);
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
        let pad = if detail { sr.width() * 0.04 } else { 6.0 * z };
        let thumb_h = if detail {
            (sr.height() * 0.42).min(tree::THUMB_H * z * 1.15).max(24.0)
        } else {
            tree::THUMB_H * z
        };
        let thumb = Rect::from_min_size(
            sr.min + Vec2::splat(pad),
            Vec2::new((sr.width() - pad * 2.0).max(1.0), thumb_h),
        );
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
                    tp.text(
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
                tp.text(
                    thumb.center(),
                    Align2::CENTER_CENTER,
                    format!(".{}", if e.ext.is_empty() { "?" } else { &e.ext }),
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
            paint_file_labels_detail(painter, &e, sr, thumb, pad, fam_color, alpha, &p);
        } else {
            let name_px = sr.height() * (11.0 / tree::FILE_H);
            if name_px >= 6.0 {
                let text_max_w = (sr.width() - 20.0 * z).max(8.0);
                let tick_h = sr.height() * (11.0 / tree::FILE_H);
                painter.rect_filled(
                    Rect::from_min_size(
                        args.cam
                            .w2s(Pos2::new(world.min.x + 6.0, world.max.y - 25.0)),
                        Vec2::new(sr.width() * (3.0 / tree::FILE_W), tick_h),
                    ),
                    CornerRadius::ZERO,
                    fam_color.gamma_multiply(alpha),
                );
                let name_font = FontId::proportional(name_px);
                let name = ellipsize_to_width(painter, &e.name, &name_font, text_max_w);
                painter.text(
                    args.cam
                        .w2s(Pos2::new(world.min.x + 14.0, world.max.y - 19.0)),
                    Align2::LEFT_CENTER,
                    name,
                    name_font,
                    p.ink.gamma_multiply(alpha),
                );
                let mut meta = format!("{} · created {}", human_size(e.size), date_string(e.ctime));
                if !e.owner.is_empty() {
                    meta.push_str(" · ");
                    meta.push_str(&e.owner);
                }
                let meta_font = FontId::proportional((sr.height() * (9.5 / tree::FILE_H)).max(6.0));
                let meta = ellipsize_to_width(painter, &meta, &meta_font, text_max_w);
                painter.text(
                    args.cam
                        .w2s(Pos2::new(world.min.x + 14.0, world.max.y - 8.0)),
                    Align2::LEFT_CENTER,
                    meta,
                    meta_font,
                    p.sub.gamma_multiply(alpha),
                );
            }
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

#[allow(clippy::too_many_arguments)]
fn paint_file_labels_detail(
    painter: &Painter,
    e: &FileEntry,
    sr: Rect,
    thumb: Rect,
    pad: f32,
    fam_color: Color32,
    alpha: f32,
    p: &Palette,
) {
    let text_top = thumb.max.y + pad * 0.6;
    let region = Rect::from_min_max(
        Pos2::new(sr.min.x + pad, text_top),
        Pos2::new(sr.max.x - pad, sr.max.y - pad),
    );
    let max_w = region.width();
    let max_h = region.height();
    if max_w < 24.0 || max_h < 18.0 {
        return;
    }
    let name_px = max_h * 0.16;
    let body_px = max_h * 0.09;
    let name_font = FontId::proportional(name_px);
    let body_font = FontId::proportional(body_px);
    let path_font = FontId::proportional(body_px * 0.95);
    let abs = e.path.display().to_string();
    let mut details = format!("{} · {}", human_size(e.size), e.family.label());
    if !e.ext.is_empty() {
        details.push_str(" · .");
        details.push_str(&e.ext);
    }
    if e.ctime > 0 {
        details.push_str("\nCreated ");
        details.push_str(&date_string(e.ctime));
    }
    if e.mtime > 0 {
        details.push_str("\nModified ");
        details.push_str(&date_string(e.mtime));
    }
    if !e.owner.is_empty() {
        details.push_str("\nOwner ");
        details.push_str(&e.owner);
    }
    let lp = painter.with_clip_rect(region.intersect(sr));
    let x = region.min.x;
    let mut y = region.min.y;
    let gap = (body_px * 0.3).max(3.0);
    let tick_w = (name_px * 0.28).max(3.0);
    let tick_h = (name_px * 0.85).max(8.0);
    lp.rect_filled(
        Rect::from_min_size(
            Pos2::new(x, y + (name_px - tick_h) * 0.35),
            Vec2::new(tick_w, tick_h),
        ),
        CornerRadius::ZERO,
        fam_color.gamma_multiply(alpha),
    );
    let name_x = x + tick_w + gap;
    let name_w = (max_w - tick_w - gap).max(8.0);
    let name_galley = painter.layout(
        e.name.clone(),
        name_font,
        p.ink.gamma_multiply(alpha),
        name_w,
    );
    let name_h = name_galley.size().y;
    lp.galley(Pos2::new(name_x, y), name_galley, p.ink);
    y += name_h + gap;
    let remain = (region.max.y - y).max(0.0);
    if remain < body_px {
        return;
    }
    let path_galley = painter.layout(abs, path_font, p.sub.gamma_multiply(alpha * 0.9), max_w);
    let path_h = path_galley.size().y.min(remain * 0.55);
    lp.galley(Pos2::new(x, y), path_galley, p.sub);
    y += path_h + gap;
    if region.max.y - y >= body_px {
        let details_galley = painter.layout(details, body_font, p.sub.gamma_multiply(alpha), max_w);
        lp.galley(Pos2::new(x, y), details_galley, p.sub);
    }
}

fn label_detail_active(lod: u8, sr: Rect, canvas: Rect) -> bool {
    lod >= 3 || (lod >= 2 && card_fills_majority(sr, canvas))
}

fn card_fills_majority(sr: Rect, canvas: Rect) -> bool {
    let ch = canvas.height().max(1.0);
    let cw = canvas.width().max(1.0);
    let h_frac = sr.height() / ch;
    let area_frac = (sr.width() / cw) * (sr.height() / ch);
    h_frac >= 0.55 || area_frac >= 0.40
}

fn dir_abs(root: &Path, d: &tree::DirNode) -> String {
    if d.rel.is_empty() {
        root.display().to_string()
    } else {
        root.join(d.rel.replace('\\', std::path::MAIN_SEPARATOR_STR))
            .display()
            .to_string()
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

fn text_width(painter: &Painter, text: &str, font: &FontId) -> f32 {
    painter
        .layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE)
        .size()
        .x
}

fn ellipsize_to_width(painter: &Painter, text: &str, font: &FontId, max_w: f32) -> String {
    if max_w <= 1.0 || text.is_empty() {
        return String::new();
    }
    if text_width(painter, text, font) <= max_w {
        return text.to_string();
    }
    const ELLIPSIS: &str = "…";
    let ell_w = text_width(painter, ELLIPSIS, font);
    if ell_w >= max_w {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect();
        let w = text_width(painter, &format!("{candidate}{ELLIPSIS}"), font);
        if w <= max_w {
            lo = mid;
        } else {
            hi = mid.saturating_sub(1);
        }
    }
    if lo == 0 {
        return ELLIPSIS.to_string();
    }
    let cut: String = chars[..lo].iter().collect();
    format!("{cut}{ELLIPSIS}")
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
