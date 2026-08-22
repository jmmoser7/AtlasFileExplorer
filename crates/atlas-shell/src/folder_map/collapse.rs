use atlas_core::tree::Tree;
use eframe::egui::{Pos2, Rect, Vec2};
use std::collections::HashMap;

/// Incremental = one level; Full = subtree (stopping at group-preview folders).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DirGrip {
    Incremental,
    Full,
}

pub enum ToggleOutcome {
    FlyTo(Rect),
    KeepNode { cam_delta: Vec2 },
}

/// Screen positions of the (incremental, full) expand grips for a collapsed dir.
pub fn grip_positions(sr: Rect, z: f32, orient: atlas_core::tree::Orient) -> (Pos2, Pos2) {
    let z = z.max(0.4);
    match orient {
        atlas_core::tree::Orient::V => (
            Pos2::new(sr.max.x + 10.0 * z, sr.center().y - 11.0 * z),
            Pos2::new(sr.max.x + 10.0 * z, sr.center().y + 11.0 * z),
        ),
        atlas_core::tree::Orient::H => (
            Pos2::new(sr.center().x - 11.0 * z, sr.max.y + 10.0 * z),
            Pos2::new(sr.center().x + 11.0 * z, sr.max.y + 10.0 * z),
        ),
    }
}

pub fn toggle_dir(
    tree: &mut Tree,
    di: u32,
    grip: DirGrip,
    hide_unmatched: bool,
    file_match: &[bool],
    structure_only: bool,
    orient: atlas_core::tree::Orient,
    cam_z: f32,
) -> Option<ToggleOutcome> {
    let di = di as usize;
    if di >= tree.dirs.len() {
        return None;
    }
    let was_portal = tree.shows_portal(di);
    let before = Pos2::new(tree.dirs[di].x, tree.dirs[di].y);
    let threshold = tree.cfg.normalized().portal_threshold;
    match grip {
        DirGrip::Incremental => {
            let expanding = tree.dirs[di].collapsed;
            tree.dirs[di].collapsed = !tree.dirs[di].collapsed;
            if expanding {
                let children = tree.dirs[di].child_dirs.clone();
                for c in children {
                    tree.dirs[c as usize].collapsed = true;
                }
            }
        }
        DirGrip::Full => {
            let fully_expanded = tree.dirs[di].child_dirs.iter().all(|&c| {
                let cd = &tree.dirs[c as usize];
                !cd.collapsed || cd.child_dirs.len() + cd.files.len() > threshold
            });
            let collapse = !tree.dirs[di].collapsed && fully_expanded;
            set_subtree_collapsed(tree, di, collapse);
            tree.dirs[di].collapsed = collapse;
        }
    }
    tree.layout_filtered(orient, hide_unmatched, file_match, structure_only);

    if was_portal && !tree.dirs[di].collapsed && grip == DirGrip::Incremental {
        let b = tree.dirs[di].grid_bounds.unwrap_or(tree.dirs[di].bounds);
        let own = tree.dirs[di].rect();
        Some(ToggleOutcome::FlyTo(b.union(own)))
    } else {
        let after = Pos2::new(tree.dirs[di].x, tree.dirs[di].y);
        Some(ToggleOutcome::KeepNode {
            cam_delta: (before - after) * cam_z,
        })
    }
}

pub fn record_collapse(tree: &Tree, dest: &mut HashMap<String, bool>) {
    for d in &tree.dirs {
        match dest.get_mut(&d.rel) {
            Some(c) => *c = d.collapsed,
            None => {
                dest.insert(d.rel.clone(), d.collapsed);
            }
        }
    }
}

pub fn apply_collapse(tree: &mut Tree, recorded: &HashMap<String, bool>) {
    for d in &mut tree.dirs {
        if let Some(&c) = recorded.get(&d.rel) {
            d.collapsed = c;
        }
    }
}

fn set_subtree_collapsed(t: &mut Tree, di: usize, collapsed: bool) {
    let threshold = t.cfg.normalized().portal_threshold;
    let children = t.dirs[di].child_dirs.clone();
    for c in children {
        let c = c as usize;
        if !collapsed && t.dirs[c].child_dirs.len() + t.dirs[c].files.len() > threshold {
            t.dirs[c].collapsed = true;
            continue;
        }
        set_subtree_collapsed(t, c, collapsed);
    }
    if di != 0 {
        t.dirs[di].collapsed = collapsed;
    }
}
