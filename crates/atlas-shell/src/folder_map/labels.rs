//! Folder-map card labels. Every size, inset, and wrap width is a fraction of
//! the card's world rect, so each label is shaped once and the camera only
//! scales it (P0.9). Zoom decides which lines show; it never moves a break
//! or an ellipsis.

use super::cam::FolderCam;
use super::paint::PaintArgs;
use crate::canvas_text::{self, WorldSpec};
use crate::theme::Palette;
use crate::widgets::group_digits;
use atlas_core::tree::{self, DirNode};
use atlas_core::types::{date_string, human_size, FileEntry};
use eframe::egui::{Align, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Vec2};
use std::path::Path;

/// Line pitch of stacked one-line labels, in font sizes.
const LINE_GAP: f32 = 1.15;

/// Close-up cards trade the one-line summary for wrapped name, path, and
/// details.
pub(super) fn label_detail_active(lod: u8, sr: Rect, canvas: Rect) -> bool {
    lod >= 3 || (lod >= 2 && card_fills_majority(sr, canvas))
}

fn card_fills_majority(sr: Rect, canvas: Rect) -> bool {
    let ch = canvas.height().max(1.0);
    let cw = canvas.width().max(1.0);
    let h_frac = sr.height() / ch;
    let area_frac = (sr.width() / cw) * (sr.height() / ch);
    h_frac >= 0.55 || area_frac >= 0.40
}

/// One line at world `at`, cut with `…` past `max_w` world units.
#[allow(clippy::too_many_arguments)]
fn line(
    painter: &Painter,
    cam: FolderCam,
    at: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    max_w: f32,
    color: Color32,
) {
    if text.is_empty() || max_w <= 0.0 {
        return;
    }
    let layout =
        canvas_text::world_layout_spec(painter.ctx(), text, &WorldSpec::label(font, max_w));
    canvas_text::world_text(painter.ctx(), &layout, cam.z).paint_anchored(
        painter,
        cam.w2s(at),
        anchor,
        color,
    );
}

/// Text wrapped at `wrap` world units from world `at`; returns its world height.
fn block(
    painter: &Painter,
    cam: FolderCam,
    at: Pos2,
    text: &str,
    font: FontId,
    wrap: f32,
    color: Color32,
) -> f32 {
    let layout = canvas_text::world_layout(painter.ctx(), text, font, wrap, Align::LEFT);
    canvas_text::world_text(painter.ctx(), &layout, cam.z).paint(painter, cam.w2s(at), color);
    layout.height
}

/// Name, summary, and path of a folder card whose world rect is `world`.
pub(super) fn paint_dir_labels(painter: &Painter, d: &DirNode, world: Rect, args: &PaintArgs<'_>) {
    let cam = args.cam;
    let z = cam.z;
    let sr = cam.w2s_rect(world);
    if label_detail_active(args.lod, sr, args.canvas) {
        paint_dir_labels_detail(painter, d, world, args);
        return;
    }
    let p = args.palette;
    let (pad_l, pad_r, pad_y) = (34.0, 8.0, 3.0);
    let max_w = world.width() - pad_l - pad_r;
    let max_h = world.height() - pad_y * 2.0;
    if max_w * z < 6.0 || max_h * z < 4.0 {
        return;
    }
    let name_px = max_h * 0.36;
    if name_px * z < 3.5 {
        return;
    }
    let sub_px = max_h * 0.24;
    let lod = args.lod;
    let close = max_h * z >= 28.0 || sr.width() >= 260.0;
    let show_meta = lod >= 1 && sub_px * z >= 3.0 && max_h >= name_px * 1.7;
    let show_path = lod >= 2 && close && !d.rel.is_empty() && max_h >= name_px * 2.6;
    let show_match = args.style.any_filter && d.desc_matches > 0 && d.collapsed && lod >= 2;
    let meta = show_meta.then(|| {
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
        m
    });
    let mut lines: [(&str, f32, Color32); 3] = [(d.name.as_str(), name_px, p.ink); 3];
    let mut count = 1;
    if let Some(m) = &meta {
        lines[count] = (m.as_str(), sub_px, p.sub);
        count += 1;
    }
    if show_path {
        lines[count] = (d.rel.as_str(), sub_px * 0.92, p.sub.gamma_multiply(0.85));
        count += 1;
    }
    let lines = &lines[..count];
    let lp = painter.with_clip_rect(sr);
    let block_h: f32 = lines.iter().map(|(_, px, _)| px * LINE_GAP).sum();
    let x = world.min.x + pad_l;
    let mut y = world.min.y + pad_y + (max_h - block_h).max(0.0) * 0.5 + lines[0].1 * 0.5;
    for (text, px, color) in lines {
        let font = FontId::proportional(*px);
        line(
            &lp,
            cam,
            Pos2::new(x, y),
            Align2::LEFT_CENTER,
            text,
            font,
            max_w,
            *color,
        );
        y += px * LINE_GAP;
    }
    if show_match {
        let label = format!("{} match", group_digits(d.desc_matches as u64));
        let at = Pos2::new(world.max.x - pad_r, world.min.y + pad_y + sub_px * 0.55);
        let font = FontId::proportional(sub_px);
        line(
            &lp,
            cam,
            at,
            Align2::RIGHT_CENTER,
            &label,
            font,
            max_w,
            p.accent,
        );
    }
}

fn paint_dir_labels_detail(painter: &Painter, d: &DirNode, world: Rect, args: &PaintArgs<'_>) {
    let p = args.palette;
    let cam = args.cam;
    let pad_l = world.width() * 0.06;
    let pad_r = world.width() * 0.04;
    let pad_y = world.height() * 0.08;
    let max_w = world.width() - pad_l - pad_r;
    let max_h = world.height() - pad_y * 2.0;
    if max_w * cam.z < 40.0 || max_h * cam.z < 24.0 {
        return;
    }
    let name_px = max_h * 0.11;
    let body_px = max_h * 0.048;
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
    let lp = painter.with_clip_rect(cam.w2s_rect(world));
    let x = world.min.x + pad_l;
    let mut y = world.min.y + pad_y;
    let gap = body_px * 0.35;
    let name_font = FontId::proportional(name_px);
    y += block(&lp, cam, Pos2::new(x, y), &d.name, name_font, max_w, p.ink) + gap;
    let remain = (world.max.y - pad_y - y).max(0.0);
    if remain < body_px * 1.2 || abs.is_empty() {
        return;
    }
    let path_budget = (remain * 0.55).max(body_px * 2.0);
    let path_font = FontId::proportional(body_px * 0.95);
    let path_color = p.sub.gamma_multiply(0.9);
    let path_h = block(
        &lp,
        cam,
        Pos2::new(x, y),
        &abs,
        path_font,
        max_w,
        path_color,
    );
    y += path_h.min(path_budget) + gap;
    if world.max.y - pad_y - y >= body_px {
        let body_font = FontId::proportional(body_px);
        block(&lp, cam, Pos2::new(x, y), &details, body_font, max_w, p.sub);
    }
}

/// Name, item count, and path under a group-preview mosaic; `footer` is world.
pub(super) fn paint_portal_labels(
    painter: &Painter,
    d: &DirNode,
    footer: Rect,
    args: &PaintArgs<'_>,
) {
    let p = args.palette;
    let cam = args.cam;
    let z = cam.z;
    let max_w = footer.width();
    let max_h = footer.height();
    if max_w * z < 6.0 || max_h * z < 4.0 {
        return;
    }
    let name_px = max_h * 0.42;
    if name_px * z < 3.5 {
        return;
    }
    let sub_px = max_h * 0.30;
    let lod = args.lod;
    let show_meta = lod >= 1 && sub_px * z >= 3.0 && max_h >= name_px * 1.6;
    let show_path = lod >= 2 && max_h * z >= 36.0 && !d.rel.is_empty();
    let lp = painter.with_clip_rect(cam.w2s_rect(footer));
    let x = footer.min.x + 2.0;
    let mut y = footer.min.y + name_px * 0.55;
    let name_font = FontId::proportional(name_px);
    let at = Pos2::new(x, y);
    line(
        &lp,
        cam,
        at,
        Align2::LEFT_CENTER,
        &d.name,
        name_font,
        max_w,
        p.ink,
    );
    y += name_px * 1.15;
    let sub_font = FontId::proportional(sub_px);
    if show_meta {
        let meta = format!(
            "{} items · {}",
            group_digits((d.child_dirs.len() + d.files.len()) as u64),
            human_size(d.desc_bytes)
        );
        let at = Pos2::new(x, y);
        line(
            &lp,
            cam,
            at,
            Align2::LEFT_CENTER,
            &meta,
            sub_font.clone(),
            max_w * 0.72,
            p.sub,
        );
        let at = Pos2::new(footer.max.x - 2.0, y);
        line(
            &lp,
            cam,
            at,
            Align2::RIGHT_CENTER,
            "Enter ↵",
            sub_font.clone(),
            max_w,
            p.accent,
        );
    }
    if show_path {
        y += sub_px * 1.1;
        let path_font = FontId::proportional(sub_px * 0.9);
        let at = Pos2::new(x, y);
        line(
            &lp,
            cam,
            at,
            Align2::LEFT_CENTER,
            &d.rel,
            path_font,
            max_w,
            p.sub,
        );
    }
}

/// Family tick, name, and one meta line along the bottom of a file card.
pub(super) fn paint_file_labels(
    painter: &Painter,
    e: &FileEntry,
    world: Rect,
    args: &PaintArgs<'_>,
    fam_color: Color32,
    alpha: f32,
) {
    let cam = args.cam;
    let p = args.palette;
    let name_px = world.height() * (11.0 / tree::FILE_H);
    if name_px * cam.z < 6.0 {
        return;
    }
    let max_w = (world.width() - 20.0).max(8.0);
    let tick = Rect::from_min_size(
        Pos2::new(world.min.x + 6.0, world.max.y - 25.0),
        Vec2::new(world.width() * (3.0 / tree::FILE_W), name_px),
    );
    painter.rect_filled(
        cam.w2s_rect(tick),
        CornerRadius::ZERO,
        fam_color.gamma_multiply(alpha),
    );
    let name_font = FontId::proportional(name_px);
    let at = Pos2::new(world.min.x + 14.0, world.max.y - 19.0);
    let ink = p.ink.gamma_multiply(alpha);
    line(
        painter,
        cam,
        at,
        Align2::LEFT_CENTER,
        &e.name,
        name_font,
        max_w,
        ink,
    );
    let mut meta = format!("{} · created {}", human_size(e.size), date_string(e.ctime));
    if !e.owner.is_empty() {
        meta.push_str(" · ");
        meta.push_str(&e.owner);
    }
    let meta_font = FontId::proportional(world.height() * (9.5 / tree::FILE_H));
    let at = Pos2::new(world.min.x + 14.0, world.max.y - 8.0);
    let sub = p.sub.gamma_multiply(alpha);
    line(
        painter,
        cam,
        at,
        Align2::LEFT_CENTER,
        &meta,
        meta_font,
        max_w,
        sub,
    );
}

/// Wrapped name, path, and details in a close-up file card's text `region`
/// (world units, under the thumbnail). `clip` is the card on screen.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_file_labels_detail(
    painter: &Painter,
    e: &FileEntry,
    region: Rect,
    clip: Rect,
    cam: FolderCam,
    fam_color: Color32,
    alpha: f32,
    p: &Palette,
) {
    let max_w = region.width();
    let max_h = region.height();
    if max_w * cam.z < 24.0 || max_h * cam.z < 18.0 {
        return;
    }
    let name_px = max_h * 0.16;
    let body_px = max_h * 0.09;
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
    let lp = painter.with_clip_rect(cam.w2s_rect(region).intersect(clip));
    let x = region.min.x;
    let mut y = region.min.y;
    let gap = body_px * 0.3;
    let tick_w = name_px * 0.28;
    let tick_h = name_px * 0.85;
    let tick = Rect::from_min_size(
        Pos2::new(x, y + (name_px - tick_h) * 0.35),
        Vec2::new(tick_w, tick_h),
    );
    lp.rect_filled(
        cam.w2s_rect(tick),
        CornerRadius::ZERO,
        fam_color.gamma_multiply(alpha),
    );
    let name_at = Pos2::new(x + tick_w + gap, y);
    let name_w = (max_w - tick_w - gap).max(1.0);
    let name_font = FontId::proportional(name_px);
    let ink = p.ink.gamma_multiply(alpha);
    y += block(&lp, cam, name_at, &e.name, name_font, name_w, ink) + gap;
    let remain = (region.max.y - y).max(0.0);
    if remain < body_px {
        return;
    }
    let path_font = FontId::proportional(body_px * 0.95);
    let path_color = p.sub.gamma_multiply(alpha * 0.9);
    let path_h = block(
        &lp,
        cam,
        Pos2::new(x, y),
        &abs,
        path_font,
        max_w,
        path_color,
    );
    y += path_h.min(remain * 0.55) + gap;
    if region.max.y - y >= body_px {
        let body_font = FontId::proportional(body_px);
        let sub = p.sub.gamma_multiply(alpha);
        block(&lp, cam, Pos2::new(x, y), &details, body_font, max_w, sub);
    }
}

fn dir_abs(root: &Path, d: &DirNode) -> String {
    if d.rel.is_empty() {
        root.display().to_string()
    } else {
        root.join(d.rel.replace('\\', std::path::MAIN_SEPARATOR_STR))
            .display()
            .to_string()
    }
}
