use super::cam::FolderCam;
use super::collapse::{grip_positions, DirGrip};
use crate::canvas_scale;
use atlas_core::tree::{Hit, Orient, Tree};
use eframe::egui::Pos2;

#[derive(Clone, Copy, Debug, Default)]
pub struct MapHover {
    pub file: Option<u32>,
    pub dir: Option<u32>,
    pub grip: Option<DirGrip>,
}

/// File-first hover, then directory, then collapse grips — File Atlas order.
pub fn hover_at(
    tree: &Tree,
    cam: FolderCam,
    screen: Pos2,
    orient: Orient,
    hide_unmatched: bool,
    structure_only: bool,
) -> MapHover {
    let mut hover = MapHover::default();
    match tree.hit_test(cam.s2w(screen)) {
        Some(Hit::File(f)) => hover.file = Some(f),
        Some(Hit::Dir(d)) => {
            let visible = tree
                .dirs
                .get(d as usize)
                .map(|dir| structure_only || !hide_unmatched || d == 0 || dir.desc_matches > 0)
                .unwrap_or(false);
            if visible {
                hover.dir = Some(d);
                hover.grip = dir_grip_at(tree, cam, d, screen, orient);
            }
        }
        None => {
            if let Some((d, grip)) =
                grip_hit_test(tree, cam, screen, orient, hide_unmatched, structure_only)
            {
                hover.dir = Some(d);
                hover.grip = Some(grip);
            }
        }
    }
    hover
}

fn dir_grip_at(
    tree: &Tree,
    cam: FolderCam,
    di: u32,
    screen: Pos2,
    orient: Orient,
) -> Option<DirGrip> {
    let d = tree.dirs.get(di as usize)?;
    if !d.collapsed {
        return None;
    }
    let sr = cam.w2s_rect(d.rect());
    let r = canvas_scale::hit_px(9.0, cam.z);
    let (inc, full) = grip_positions(sr, cam.z, orient);
    let d_inc = screen.distance(inc);
    let d_full = screen.distance(full);
    if d_inc.min(d_full) > r {
        None
    } else if d_inc <= d_full {
        Some(DirGrip::Incremental)
    } else {
        Some(DirGrip::Full)
    }
}

fn grip_hit_test(
    tree: &Tree,
    cam: FolderCam,
    screen: Pos2,
    orient: Orient,
    hide_unmatched: bool,
    structure_only: bool,
) -> Option<(u32, DirGrip)> {
    let mut best: Option<(f32, u32, DirGrip)> = None;
    for (di, d) in tree.dirs.iter().enumerate() {
        if !d.collapsed {
            continue;
        }
        if !structure_only && hide_unmatched && di != 0 && d.desc_matches == 0 {
            continue;
        }
        if let Some(grip) = dir_grip_at(tree, cam, di as u32, screen, orient) {
            let sr = cam.w2s_rect(d.rect());
            let (inc, full) = grip_positions(sr, cam.z, orient);
            let dist = match grip {
                DirGrip::Incremental => screen.distance(inc),
                DirGrip::Full => screen.distance(full),
            };
            if best.is_none_or(|(bd, _, _)| dist < bd) {
                best = Some((dist, di as u32, grip));
            }
        }
    }
    best.map(|(_, di, grip)| (di, grip))
}
