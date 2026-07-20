//! Shared Cover Flow home screen (File Atlas + Slate).
//!
//! Orthogonal to workbook/folder tabs: the shelf opens items; a single New
//! button starts a fresh workspace. Both apps drive the same [`HomeScreen`]
//! — covers, textures, motion, and painting are identical by construction.
//! Geometry and motion are tunable via `[home]` in `ui-tokens.toml`.

use crate::recent::{cover_cache_path, RecentEntry, RecentList};
use crate::theme::Palette;
use crate::tokens::{self, HomeTokens};
use eframe::egui::epaint::Vertex;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Mesh, Pos2, Rect, Sense, Stroke, TextureId,
    Ui, Vec2,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// The one home surface both apps embed. Owns cover textures and shelf focus;
/// apps only translate the returned action into their own open/new flows.
pub struct HomeScreen {
    id_salt: &'static str,
    kind: HomeShelfKind,
    focus: usize,
    textures: HashMap<PathBuf, egui::TextureHandle>,
}

/// What the user asked the home shelf to do.
pub enum HomeScreenAction {
    /// Open this recent folder / workbook.
    Open(PathBuf),
    /// Start a new workspace (folder pick / blank workbook).
    New,
}

impl HomeScreen {
    pub fn new(id_salt: &'static str, kind: HomeShelfKind) -> Self {
        Self {
            id_salt,
            kind,
            focus: 0,
            textures: HashMap::new(),
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        palette: &Palette,
        recents: &RecentList,
    ) -> Option<HomeScreenAction> {
        self.ensure_textures(ui.ctx(), recents);
        let mut covers = covers_or_placeholders(&recents.entries, self.kind, 20);
        for c in &mut covers {
            if let Some(tex) = self.textures.get(&c.path) {
                c.texture = Some(tex.id());
            }
        }
        let result = cover_flow_home(
            ui,
            palette,
            HomeModel {
                id_salt: self.id_salt,
                new_label: "New",
                covers: &covers,
                focus: self.focus,
            },
        );
        self.focus = result.focus;
        match result.action {
            Some(HomeAction::New) => Some(HomeScreenAction::New),
            Some(HomeAction::Open(i)) => {
                let c = covers.get(i)?;
                Some(HomeScreenAction::Open(c.path.clone()))
            }
            None => None,
        }
    }

    /// Upload baked cover PNGs as textures; keep pumping frames while covers
    /// are still baking on background threads.
    fn ensure_textures(&mut self, ctx: &egui::Context, recents: &RecentList) {
        for e in &recents.entries {
            if self.textures.contains_key(&e.path) {
                continue;
            }
            let cover = e.cover.clone().unwrap_or_else(|| cover_cache_path(&e.path));
            if !cover.is_file() {
                continue;
            }
            let Ok(img) = image::open(&cover) else {
                continue;
            };
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            let tex = ctx.load_texture(
                format!("home-cover-{}", e.path.to_string_lossy()),
                color,
                egui::TextureOptions::LINEAR,
            );
            self.textures.insert(e.path.clone(), tex);
        }
        let missing = recents
            .entries
            .iter()
            .any(|e| !self.textures.contains_key(&e.path));
        if missing {
            ctx.request_repaint_after(std::time::Duration::from_millis(400));
        }
    }
}

/// One cover in the flow — data from the app, paint from the shell.
#[derive(Clone)]
pub struct HomeCover {
    pub path: PathBuf,
    pub title: String,
    /// High-res cover when ready; `None` draws a styled placeholder card.
    pub texture: Option<TextureId>,
    /// Template / empty-shelf placeholders (synthetic paths — apps decide how to open).
    pub placeholder: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HomeAction {
    /// Open the focused (or clicked) recent entry.
    Open(usize),
    /// Start a new workspace (folder pick / blank workbook).
    New,
}

pub struct HomeModel<'a> {
    /// Salts interaction ids (Atlas vs Slate).
    pub id_salt: &'a str,
    /// Label for the sole bottom CTA (e.g. "New").
    pub new_label: &'a str,
    pub covers: &'a [HomeCover],
    /// Index of the focused cover in the flow.
    pub focus: usize,
}

/// Result of interacting with the home surface this frame.
pub struct HomeResult {
    pub action: Option<HomeAction>,
    pub focus: usize,
}

// ── Cover Flow motion & layout ───────────────────────────────────────────────

/// Resolved Cover Flow geometry and motion for one frame — derived from the
/// live `[home]` tokens plus the current cover size, so the ui-tuner can
/// adjust everything without touching this module.
#[derive(Clone, Copy, Debug)]
struct CoverFlowTuning {
    /// Asymptotic spacing between packed side covers (px at z = 0).
    side_step: f32,
    /// Extra separation pushed outward around the focused cover (px).
    center_bulge: f32,
    /// Sigmoid width of the center gap (album units).
    bulge_width: f32,
    /// Perspective focal length (px).
    focal: f32,
    /// Saturating Z push-back for side covers (px).
    depth_max: f32,
    /// Sigmoid width of the depth falloff (album units).
    depth_width: f32,
    /// Saturating yaw for side covers (radians).
    angle_max: f32,
    /// Sigmoid width of the yaw transition (album units).
    angle_width: f32,
    /// Corner fillet radius (px in card-local space).
    bevel: f32,
    /// Ambient-occlusion halo reach (px) and strength (0..1).
    ao_size: f32,
    ao_strength: f32,
    /// Drag dead-zone in album units before gain ramps up.
    drag_dead_zone: f32,
    /// Minimum drag gain near rest (the "sticky" feel).
    drag_gain_min: f32,
    /// Maximum drag gain once past the dead-zone.
    drag_gain_max: f32,
    /// Scroll delta (px) that equals one album step.
    wheel_px_per_album: f32,
    /// Velocity damping during free inertia (1/s).
    friction: f32,
    /// Spring stiffness toward the target detent (1/s²).
    spring_stiffness: f32,
    /// Spring damping (1/s) — near critical for a clean ease-in.
    spring_damping: f32,
    /// Below this |velocity|, inertia hands over to the detent spring.
    snap_velocity: f32,
    /// Release velocity clamp (album units / s).
    max_velocity: f32,
}

impl CoverFlowTuning {
    fn from_tokens(t: &HomeTokens, cover: f32) -> Self {
        Self {
            side_step: cover * t.side_step_frac,
            center_bulge: cover * t.center_bulge_frac,
            bulge_width: t.bulge_width,
            focal: t.focal,
            depth_max: t.depth_max,
            depth_width: t.depth_width,
            angle_max: t.angle_max_deg.to_radians(),
            angle_width: t.angle_width,
            bevel: cover * t.corner_bevel_frac,
            ao_size: t.ao_size,
            ao_strength: t.ao_strength,
            drag_dead_zone: 0.06,
            drag_gain_min: 0.35,
            drag_gain_max: 1.0,
            wheel_px_per_album: t.wheel_px_per_album,
            friction: t.friction,
            spring_stiffness: t.spring_stiffness,
            spring_damping: t.spring_damping,
            snap_velocity: t.snap_velocity,
            max_velocity: 12.0,
        }
    }

    #[cfg(test)]
    fn for_cover(cover: f32) -> Self {
        Self::from_tokens(&HomeTokens::default(), cover)
    }

    /// Pixels of pointer travel per album unit at the center of the rack
    /// (derivative of the layout curve at offset 0).
    fn px_per_album_at_center(&self) -> f32 {
        self.side_step + self.center_bulge / self.bulge_width
    }
}

/// Horizontal rack position for a cover at `offset` album units from focus.
/// Sigmoidal: wide through the center, tightly packed toward the sides.
fn rack_x(offset: f32, t: &CoverFlowTuning) -> f32 {
    t.side_step * offset + t.center_bulge * (offset / t.bulge_width).tanh()
}

/// Yaw angle: 0 at focus, saturating to ±angle_max on the sides.
fn rack_angle(offset: f32, t: &CoverFlowTuning) -> f32 {
    -t.angle_max * (offset / t.angle_width).tanh()
}

/// Depth push-back: 0 at focus, saturating to depth_max on the sides.
fn rack_z(offset: f32, t: &CoverFlowTuning) -> f32 {
    t.depth_max * (offset.abs() / t.depth_width).tanh()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InteractionPhase {
    Idle,
    Dragging,
    Inertia,
    Snapping,
}

#[derive(Clone, Debug)]
struct CoverFlowState {
    /// Continuous position in album units (unwrapped while moving).
    position: f32,
    velocity: f32,
    /// Detent the spring is easing toward (same unwrapped space as `position`).
    target: f32,
    wheel_accum: f32,
    phase: InteractionPhase,
    /// Press that only stopped momentum — must not open on release.
    stop_gesture: bool,
    /// Count when state was last synced (reset on change).
    cover_count: usize,
}

impl Default for CoverFlowState {
    fn default() -> Self {
        Self {
            position: 0.0,
            velocity: 0.0,
            target: 0.0,
            wheel_accum: 0.0,
            phase: InteractionPhase::Idle,
            stop_gesture: false,
            cover_count: 0,
        }
    }
}

/// Euclidean modulo for signed integers.
fn mod_index(i: i32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let n = count as i32;
    (((i % n) + n) % n) as usize
}

/// Shortest signed circular distance from `position` to `index` in album units.
fn circular_offset(position: f32, index: usize, count: usize) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let n = count as f32;
    let mut d = index as f32 - position;
    d -= (d / n).round() * n;
    d
}

/// Slot-based visual offset for cyclic rack painting (may repeat indices when count is small).
fn slot_visual_offset(position: f32, slot: i32, count: usize) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let n = count as f32;
    let base = position.floor();
    let frac = position - base;
    let mut off = slot as f32 - frac;
    // Keep offsets near center for shortest wrap representation.
    off -= (off / n).round() * n;
    off
}

/// Visible slot indices far-to-near, skipping duplicate logical indices when list is short.
fn visible_slots(position: f32, count: usize, max_slot: i32) -> Vec<(i32, usize)> {
    if count == 0 {
        return Vec::new();
    }
    let base = position.floor() as i32;
    let mut slots: Vec<(i32, usize)> = Vec::new();
    for slot in -max_slot..=max_slot {
        let logical = mod_index(base + slot, count);
        slots.push((slot, logical));
    }
    // When the list is shorter than the slot span, the same index can appear twice at
    // different slots — keep the slot closest to center for each logical index.
    let mut best: std::collections::BTreeMap<usize, (i32, f32)> = std::collections::BTreeMap::new();
    for &(slot, logical) in &slots {
        let off = slot_visual_offset(position, slot, count).abs();
        match best.get(&logical) {
            Some((_, prev_off)) if *prev_off <= off => {}
            _ => {
                best.insert(logical, (slot, off));
            }
        }
    }
    let mut out: Vec<(i32, usize)> = best
        .into_iter()
        .map(|(logical, (slot, _))| (slot, logical))
        .collect();
    out.sort_by(|a, b| {
        slot_visual_offset(position, b.0, count)
            .abs()
            .partial_cmp(&slot_visual_offset(position, a.0, count).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Sticky detents: low gain right at rest, full gain once the gesture commits.
fn drag_gain(offset_from_detent: f32, tuning: &CoverFlowTuning) -> f32 {
    let a = offset_from_detent.abs();
    if a <= tuning.drag_dead_zone {
        let t = a / tuning.drag_dead_zone;
        tuning.drag_gain_min + (t * t) * (tuning.drag_gain_max - tuning.drag_gain_min)
    } else {
        tuning.drag_gain_max
    }
}

/// Integrate free inertia and the detent spring. Returns true while moving.
///
/// `position` and `target` live in the same unwrapped album space; both are
/// re-anchored into `[0, count)` together when motion settles so wrap-around
/// never causes a visible jump mid-flight.
fn integrate_motion(
    position: &mut f32,
    velocity: &mut f32,
    target: &mut f32,
    phase: &mut InteractionPhase,
    count: usize,
    dt: f32,
    tuning: &CoverFlowTuning,
) -> bool {
    if count == 0 {
        *position = 0.0;
        *velocity = 0.0;
        *target = 0.0;
        *phase = InteractionPhase::Idle;
        return false;
    }

    match *phase {
        InteractionPhase::Inertia => {
            *velocity *= (-tuning.friction * dt).exp();
            *position += *velocity * dt;
            if velocity.abs() < tuning.snap_velocity {
                *phase = InteractionPhase::Snapping;
                *target = position.round();
            }
            true
        }
        InteractionPhase::Snapping => {
            let dist = *position - *target;
            let accel = -tuning.spring_stiffness * dist - tuning.spring_damping * *velocity;
            *velocity += accel * dt;
            *position += *velocity * dt;

            let dist = *position - *target;
            if velocity.abs() < 0.02 && dist.abs() < 0.001 {
                *position = *target;
                *velocity = 0.0;
                *phase = InteractionPhase::Idle;
                // Re-anchor both into [0, n) with the same shift.
                let mut anchored = *position;
                normalize_position(&mut anchored, count);
                let shift = anchored - *position;
                *position += shift;
                *target += shift;
                false
            } else {
                true
            }
        }
        InteractionPhase::Idle | InteractionPhase::Dragging => false,
    }
}

fn normalize_position(position: &mut f32, count: usize) {
    if count == 0 {
        *position = 0.0;
        return;
    }
    let n = count as f32;
    *position -= (*position / n).floor() * n;
}

/// Draw the Cover Flow home into `ui`'s full available rect.
pub fn cover_flow_home(ui: &mut Ui, palette: &Palette, model: HomeModel<'_>) -> HomeResult {
    let rect = ui.available_rect_before_wrap();
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let count = model.covers.len();
    let mut action = None;

    // Background: subtle mesh gradient (soft color blobs over the theme bg).
    paint_mesh_gradient(&painter, rect, palette);

    // Square covers (album-art aspect), sized and centered by the live tokens.
    let home_tokens = tokens::current().home;
    let flow_center = Pos2::new(
        rect.center().x,
        rect.min.y + rect.height() * home_tokens.center_y_frac,
    );
    let cover = (rect.height() * home_tokens.cover_frac)
        .clamp(home_tokens.cover_min, home_tokens.cover_max);
    let (cover_w, cover_h) = (cover, cover);
    let tuning = CoverFlowTuning::from_tokens(&home_tokens, cover);

    let state_id = Id::new(("home_flow_state", model.id_salt));
    let mut flow = ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_insert_with(state_id, CoverFlowState::default)
            .clone()
    });

    if flow.cover_count != count {
        flow.position = if count == 0 {
            0.0
        } else {
            model.focus.min(count - 1) as f32
        };
        flow.velocity = 0.0;
        flow.target = flow.position;
        flow.wheel_accum = 0.0;
        flow.phase = InteractionPhase::Idle;
        flow.stop_gesture = false;
        flow.cover_count = count;
    } else if count > 0 {
        // Gentle external focus sync when app changes focus without interaction.
        let app_focus = model.focus.min(count - 1);
        if flow.phase == InteractionPhase::Idle
            && flow.velocity.abs() < 0.01
            && mod_index(flow.position.round() as i32, count) != app_focus
        {
            flow.position = app_focus as f32;
            flow.target = flow.position;
        }
    }

    let dt = ui.input(|i| i.stable_dt).clamp(1.0 / 240.0, 1.0 / 20.0);
    let pointer_down = resp.is_pointer_button_down_on();
    let pointer_pressed =
        resp.hovered() && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));

    // Fresh press during motion stops immediately (iPod behavior) and the
    // release must not open a cover.
    if pointer_pressed
        && matches!(
            flow.phase,
            InteractionPhase::Inertia | InteractionPhase::Snapping
        )
    {
        flow.velocity = 0.0;
        flow.target = flow.position.round();
        flow.phase = InteractionPhase::Snapping;
        flow.stop_gesture = true;
    }

    if count > 0 {
        // Wheel / trackpad: accumulate px, then step the target one detent at
        // a time so every advance is spring-animated (never a teleport).
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.x + i.smooth_scroll_delta.y);
            if scroll.abs() > 0.01 && flow.phase != InteractionPhase::Dragging {
                flow.wheel_accum += -scroll / tuning.wheel_px_per_album;
                flow.wheel_accum = flow.wheel_accum.clamp(-3.0, 3.0);
                let step = flow.wheel_accum.trunc();
                if step.abs() >= 1.0 {
                    flow.wheel_accum -= step;
                    let base = if flow.phase == InteractionPhase::Snapping {
                        flow.target
                    } else {
                        flow.position.round()
                    };
                    flow.target = base + step.clamp(-2.0, 2.0);
                    flow.phase = InteractionPhase::Snapping;
                    flow.stop_gesture = false;
                }
            }
        }

        // Drag: egui's drag_delta() is already per-frame. Sticky detents via
        // the dead-zone gain, then 1:1 with the rack's center spacing.
        if resp.drag_started() {
            flow.velocity = 0.0;
            flow.phase = InteractionPhase::Dragging;
            flow.stop_gesture = false;
        }
        if resp.dragged() && flow.phase == InteractionPhase::Dragging {
            let dx = resp.drag_delta().x;
            if dx != 0.0 {
                let detent = flow.position - flow.position.round();
                let gain = drag_gain(detent, &tuning) / tuning.px_per_album_at_center();
                flow.position -= dx * gain;
                let vel_sample = (-dx * gain / dt).clamp(-tuning.max_velocity, tuning.max_velocity);
                flow.velocity = flow.velocity * 0.6 + vel_sample * 0.4;
            }
        }
        if resp.drag_stopped() && flow.phase == InteractionPhase::Dragging {
            flow.velocity = flow
                .velocity
                .clamp(-tuning.max_velocity, tuning.max_velocity);
            flow.phase = InteractionPhase::Inertia;
        }

        // Keyboard: animated single-detent steps (repeat presses queue up).
        let arrow = ui.input(|i| {
            i.key_pressed(egui::Key::ArrowRight) as i32 - i.key_pressed(egui::Key::ArrowLeft) as i32
        });
        if arrow != 0 {
            let base = if flow.phase == InteractionPhase::Snapping {
                flow.target
            } else {
                flow.position.round()
            };
            flow.target = base + arrow as f32;
            flow.phase = InteractionPhase::Snapping;
            flow.stop_gesture = false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let focus = mod_index(flow.position.round() as i32, count);
            action = Some(HomeAction::Open(focus));
        }

        // Inertia / detent spring when the pointer isn't holding the rack.
        if flow.phase != InteractionPhase::Dragging && !pointer_down {
            let moving = integrate_motion(
                &mut flow.position,
                &mut flow.velocity,
                &mut flow.target,
                &mut flow.phase,
                count,
                dt,
                &tuning,
            );
            if moving {
                ui.ctx().request_repaint();
            }
        }
    }

    let focus = if count == 0 {
        0
    } else {
        mod_index(flow.position.round() as i32, count)
    };

    if count > 0 {
        // Full bleed: paint enough slots that the rack runs off both screen
        // edges (projected side spacing shrinks with depth).
        let projected_step =
            tuning.side_step * tuning.focal / (tuning.focal + tuning.depth_max).max(1.0);
        let max_slot =
            (((rect.width() * 0.5 + cover_w) / projected_step.max(8.0)).ceil() as i32).clamp(3, 64);
        let slots = visible_slots(flow.position, count, max_slot);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mut hit: Option<(i32, usize)> = None;

        for &(slot, logical) in &slots {
            let off = slot_visual_offset(flow.position, slot, count);
            let quad = project_cover(flow_center, cover_w, cover_h, off, &tuning);
            let cover = &model.covers[logical];
            paint_cover(
                &painter,
                palette,
                flow_center,
                cover_w,
                cover_h,
                off,
                &tuning,
                cover.texture,
                cover.placeholder,
            );
            if let Some(p) = pointer {
                if point_in_quad(p, &quad) {
                    hit = Some((slot, logical));
                }
            }
        }

        // Subtle label under the focused projected cover.
        if let Some(cover) = model.covers.get(focus) {
            let focused_off = circular_offset(flow.position, focus, count);
            let quad = project_cover(flow_center, cover_w, cover_h, focused_off, &tuning);
            let bottom = quad.bl.y.max(quad.br.y);
            painter.text(
                Pos2::new(rect.center().x, bottom + 14.0),
                Align2::CENTER_TOP,
                &cover.title,
                FontId::proportional(13.0),
                palette.sub,
            );
        }

        // Click / tap — suppressed when the press only stopped momentum.
        if resp.clicked() && !flow.stop_gesture {
            if let Some((_slot, logical)) = hit {
                if logical == focus {
                    action = Some(HomeAction::Open(logical));
                } else {
                    let off = slot_visual_offset(flow.position, _slot, count);
                    flow.target = (flow.position + off).round();
                    flow.phase = InteractionPhase::Snapping;
                    ui.ctx().request_repaint();
                }
            }
        }
    }

    if flow.stop_gesture && (!pointer_down || resp.clicked()) {
        flow.stop_gesture = false;
    }

    ui.ctx().data_mut(|d| d.insert_temp(state_id, flow));

    // Sole CTA — New. Opening is the album flow.
    if !model.new_label.is_empty() {
        let cta_y = rect.max.y - 52.0;
        if pill_button(
            ui,
            &painter,
            Pos2::new(rect.center().x, cta_y),
            model.new_label,
            palette.accent,
            Color32::WHITE,
        ) {
            action = Some(HomeAction::New);
        }
    }

    HomeResult { action, focus }
}

/// Build covers from persisted recents (textures filled by the app).
pub fn covers_from_recents(entries: &[RecentEntry]) -> Vec<HomeCover> {
    entries
        .iter()
        .map(|e| HomeCover {
            path: e.path.clone(),
            title: e.title.clone(),
            texture: None,
            placeholder: false,
        })
        .collect()
}

/// Placeholder shelf when there are no recents yet (stand-in for future templates).
pub fn placeholder_covers(kind: HomeShelfKind, count: usize) -> Vec<HomeCover> {
    let (prefix, salt) = match kind {
        HomeShelfKind::Folders => ("Folder template", "atlas-ph"),
        HomeShelfKind::Workbooks => ("Workbook template", "slate-ph"),
    };
    (1..=count.max(1))
        .map(|i| HomeCover {
            path: PathBuf::from(format!("{salt}-{i}")),
            title: format!("{prefix} {i}"),
            texture: None,
            placeholder: true,
        })
        .collect()
}

/// Synthetic shelf paths (template placeholders) — not filesystem locations.
pub fn is_synthetic_cover_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.starts_with("slate-ph-") || s.starts_with("atlas-ph-")
}

/// Recents when present; otherwise template placeholders.
pub fn covers_or_placeholders(
    entries: &[RecentEntry],
    kind: HomeShelfKind,
    placeholder_count: usize,
) -> Vec<HomeCover> {
    if entries.is_empty() {
        placeholder_covers(kind, placeholder_count)
    } else {
        covers_from_recents(entries)
    }
}

#[derive(Clone, Copy)]
pub enum HomeShelfKind {
    Folders,
    Workbooks,
}

struct Quad {
    tl: Pos2,
    tr: Pos2,
    br: Pos2,
    bl: Pos2,
}

/// Project one card-local point through the rack transform (rotateY around
/// the card center, saturating depth push-back, perspective divide).
fn project_point(
    flow_center: Pos2,
    lx: f32,
    ly: f32,
    slot_offset: f32,
    tuning: &CoverFlowTuning,
) -> Pos2 {
    let cx = rack_x(slot_offset, tuning);
    // Positive Z is away from the viewer because projection divides by
    // `focal + z`: push the whole side card back before applying its yaw.
    let cz = rack_z(slot_offset, tuning);
    let angle = rack_angle(slot_offset, tuning);
    let (rx, ry, rz) = rotate_y(lx, ly, 0.0, angle);
    let depth = (tuning.focal + cz + rz).max(1.0);
    Pos2::new(
        flow_center.x + tuning.focal * (cx + rx) / depth,
        flow_center.y + tuning.focal * ry / depth,
    )
}

/// Local card corners → rotateY → perspective project to screen.
/// (Un-beveled quad — used for hit testing, labels, and geometry tests.)
fn project_cover(
    flow_center: Pos2,
    card_w: f32,
    card_h: f32,
    slot_offset: f32,
    tuning: &CoverFlowTuning,
) -> Quad {
    let hw = card_w * 0.5;
    let hh = card_h * 0.5;
    Quad {
        tl: project_point(flow_center, -hw, -hh, slot_offset, tuning),
        tr: project_point(flow_center, hw, -hh, slot_offset, tuning),
        br: project_point(flow_center, hw, hh, slot_offset, tuning),
        bl: project_point(flow_center, -hw, hh, slot_offset, tuning),
    }
}

fn rotate_y(x: f32, y: f32, z: f32, angle: f32) -> (f32, f32, f32) {
    let (s, c) = angle.sin_cos();
    (x * c + z * s, y, -x * s + z * c)
}

/// Card-local silhouette with filleted (rounded) corners, clockwise.
fn fillet_outline(hw: f32, hh: f32, radius: f32) -> Vec<(f32, f32)> {
    let r = radius.clamp(0.0, hw.min(hh) * 0.9);
    if r <= 0.05 {
        return vec![(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
    }
    // Quarter-circle arcs at each corner (y-down clockwise: TL → TR → BR → BL).
    const ARC_STEPS: usize = 5;
    let corners = [
        (-hw + r, -hh + r, 180.0_f32), // top-left: 180° → 270°
        (hw - r, -hh + r, 270.0),      // top-right: 270° → 360°
        (hw - r, hh - r, 0.0),         // bottom-right: 0° → 90°
        (-hw + r, hh - r, 90.0),       // bottom-left: 90° → 180°
    ];
    let mut out = Vec::with_capacity(4 * (ARC_STEPS + 1));
    for (cx, cy, start_deg) in corners {
        for step in 0..=ARC_STEPS {
            let theta = (start_deg + 90.0 * step as f32 / ARC_STEPS as f32).to_radians();
            out.push((cx + r * theta.cos(), cy + r * theta.sin()));
        }
    }
    out
}

/// Minimal cover: artwork (or a quiet card) on a beveled silhouette, seated
/// on a computed ambient-occlusion halo — no borders, headers, or drop glow.
#[allow(clippy::too_many_arguments)]
fn paint_cover(
    painter: &egui::Painter,
    palette: &Palette,
    flow_center: Pos2,
    card_w: f32,
    card_h: f32,
    slot_offset: f32,
    tuning: &CoverFlowTuning,
    texture: Option<TextureId>,
    placeholder: bool,
) {
    let hw = card_w * 0.5;
    let hh = card_h * 0.5;
    let outline_local = fillet_outline(hw, hh, tuning.bevel);
    let outline: Vec<Pos2> = outline_local
        .iter()
        .map(|&(lx, ly)| project_point(flow_center, lx, ly, slot_offset, tuning))
        .collect();

    paint_ambient_occlusion(painter, palette, &outline, tuning);

    if let Some(tex) = texture {
        // Triangle fan around the projected center, per-vertex UVs from the
        // card-local coordinates so the artwork follows the perspective.
        let mut mesh = Mesh::with_texture(tex);
        let center = project_point(flow_center, 0.0, 0.0, slot_offset, tuning);
        mesh.vertices.push(Vertex {
            pos: center,
            uv: egui::pos2(0.5, 0.5),
            color: Color32::WHITE,
        });
        for (&(lx, ly), &pos) in outline_local.iter().zip(outline.iter()) {
            mesh.vertices.push(Vertex {
                pos,
                uv: egui::pos2(lx / card_w + 0.5, ly / card_h + 0.5),
                color: Color32::WHITE,
            });
        }
        let n = outline.len() as u32;
        for i in 0..n {
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
        }
        painter.add(egui::Shape::mesh(mesh));
    } else {
        let fill = if placeholder {
            palette.card.gamma_multiply(0.75)
        } else {
            palette.card
        };
        painter.add(egui::Shape::convex_polygon(
            outline.clone(),
            fill,
            Stroke::NONE,
        ));
    }
}

/// Normalized sigmoid falloff over `t ∈ [0, 1]`: 1 at the card edge, 0 at the
/// halo rim, with a logistic (soft shoulder → steep middle → soft tail) curve
/// instead of a linear ramp.
fn ao_falloff(t: f32) -> f32 {
    const K: f32 = 7.0;
    let f = |x: f32| 1.0 / (1.0 + (K * (x - 0.5)).exp());
    let f0 = f(0.0);
    let f1 = f(1.0);
    ((f(t.clamp(0.0, 1.0)) - f1) / (f0 - f1)).clamp(0.0, 1.0)
}

/// Computed contact occlusion: the silhouette is extruded outward in screen
/// space (further along +Y so the card feels seated) through several rings
/// whose alpha follows a sigmoid falloff — occlusion that hugs the card's
/// actual projected shape rather than a rectangle drop shadow.
fn paint_ambient_occlusion(
    painter: &egui::Painter,
    palette: &Palette,
    outline: &[Pos2],
    tuning: &CoverFlowTuning,
) {
    if tuning.ao_strength <= 0.005 || tuning.ao_size <= 0.5 || outline.len() < 3 {
        return;
    }
    let n = outline.len();
    let centroid = outline.iter().fold(Vec2::ZERO, |acc, p| acc + p.to_vec2()) / n as f32;
    let dark_theme = palette.bg.r() < 40;
    // AO reads differently per theme: lighter bg shows more of the halo.
    let core_alpha = tuning.ao_strength * if dark_theme { 110.0 } else { 70.0 };

    // Concentric rings sample the sigmoid so the gradient stays smooth.
    const RINGS: usize = 5;
    let mut mesh = Mesh::default();
    for ring in 0..=RINGS {
        let t = ring as f32 / RINGS as f32;
        let alpha = (core_alpha * ao_falloff(t)) as u8;
        let color = Color32::from_black_alpha(alpha);
        for p in outline {
            let dir = (p.to_vec2() - centroid).normalized();
            // Ground-contact bias: the halo reaches further below the card.
            let reach = tuning.ao_size * (0.55 + dir.y.max(0.0) * 1.1) * t;
            mesh.vertices.push(Vertex {
                pos: *p + dir * reach,
                uv: egui::pos2(0.0, 0.0),
                color,
            });
        }
    }
    let n = n as u32;
    for ring in 0..RINGS as u32 {
        let a = ring * n;
        let b = (ring + 1) * n;
        for i in 0..n {
            let j = (i + 1) % n;
            mesh.add_triangle(a + i, a + j, b + j);
            mesh.add_triangle(a + i, b + j, b + i);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Subtle mesh gradient: a coarse vertex grid tinted by a few soft color
/// blobs over the theme background — calculated per-vertex, both themes.
fn paint_mesh_gradient(painter: &egui::Painter, rect: Rect, palette: &Palette) {
    let dark = palette.bg.r() < 40;
    // (position in unit space, tint, reach) — chosen to stay quiet.
    let blobs: [(f32, f32, [f32; 3], f32); 4] = if dark {
        [
            (0.18, 0.12, [0x17 as f32, 0x2b as f32, 0x2f as f32], 0.55),
            (0.85, 0.20, [0x1c as f32, 0x21 as f32, 0x35 as f32], 0.50),
            (0.50, 0.95, [0x20 as f32, 0x1a as f32, 0x24 as f32], 0.60),
            (0.05, 0.80, [0x10 as f32, 0x1e as f32, 0x1a as f32], 0.45),
        ]
    } else {
        [
            (0.18, 0.12, [0xe3 as f32, 0xef as f32, 0xf2 as f32], 0.55),
            (0.85, 0.20, [0xe9 as f32, 0xe7 as f32, 0xf5 as f32], 0.50),
            (0.50, 0.95, [0xf2 as f32, 0xec as f32, 0xe4 as f32], 0.60),
            (0.05, 0.80, [0xe6 as f32, 0xf0 as f32, 0xe9 as f32], 0.45),
        ]
    };
    let base = [
        palette.bg.r() as f32,
        palette.bg.g() as f32,
        palette.bg.b() as f32,
    ];

    const COLS: usize = 24;
    const ROWS: usize = 14;
    let mut mesh = Mesh::default();
    for row in 0..=ROWS {
        for col in 0..=COLS {
            let u = col as f32 / COLS as f32;
            let v = row as f32 / ROWS as f32;
            let mut rgb = base;
            for (bx, by, tint, reach) in &blobs {
                let dx = u - bx;
                let dy = v - by;
                let d2 = dx * dx + dy * dy;
                let w = (-d2 / (reach * reach * 0.5)).exp();
                for c in 0..3 {
                    rgb[c] += (tint[c] - base[c]) * w * 0.65;
                }
            }
            let color = Color32::from_rgb(
                rgb[0].clamp(0.0, 255.0) as u8,
                rgb[1].clamp(0.0, 255.0) as u8,
                rgb[2].clamp(0.0, 255.0) as u8,
            );
            mesh.vertices.push(Vertex {
                pos: Pos2::new(
                    rect.min.x + u * rect.width(),
                    rect.min.y + v * rect.height(),
                ),
                uv: egui::pos2(0.0, 0.0),
                color,
            });
        }
    }
    let stride = (COLS + 1) as u32;
    for row in 0..ROWS as u32 {
        for col in 0..COLS as u32 {
            let i = row * stride + col;
            mesh.add_triangle(i, i + 1, i + stride);
            mesh.add_triangle(i + 1, i + stride + 1, i + stride);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn point_in_quad(p: Pos2, q: &Quad) -> bool {
    point_in_tri(p, q.tl, q.tr, q.br) || point_in_tri(p, q.tl, q.br, q.bl)
}

fn point_in_tri(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let sign = |p1: Pos2, p2: Pos2, p3: Pos2| {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    };
    let b1 = sign(p, a, b) < 0.0;
    let b2 = sign(p, b, c) < 0.0;
    let b3 = sign(p, c, a) < 0.0;
    b1 == b2 && b2 == b3
}

fn pill_button(
    ui: &mut Ui,
    painter: &egui::Painter,
    center: Pos2,
    label: &str,
    fill: Color32,
    text: Color32,
) -> bool {
    let galley = painter.layout_no_wrap(label.to_owned(), FontId::proportional(14.0), text);
    let pad = Vec2::new(18.0, 8.0);
    let size = galley.size() + pad * 2.0;
    let rect = Rect::from_center_size(center, size);
    let id = ui.id().with(("home_pill", label));
    let resp = ui.interact(rect, id, Sense::click());
    let fill = if resp.hovered() {
        fill.gamma_multiply(1.08)
    } else {
        fill
    };
    painter.rect_filled(rect, CornerRadius::same(16), fill);
    painter.galley(rect.center() - galley.size() * 0.5, galley, text);
    resp.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_index_wraps_both_directions() {
        assert_eq!(mod_index(-1, 5), 4);
        assert_eq!(mod_index(-6, 5), 4);
        assert_eq!(mod_index(5, 5), 0);
        assert_eq!(mod_index(7, 5), 2);
        assert_eq!(mod_index(0, 0), 0);
    }

    #[test]
    fn circular_offset_shortest_path_across_seam() {
        assert!((circular_offset(0.0, 0, 5) - 0.0).abs() < 1e-5);
        assert!((circular_offset(0.2, 4, 5) - (-1.2)).abs() < 1e-4);
        assert!((circular_offset(4.8, 0, 5) - 0.2).abs() < 1e-4);
        assert!((circular_offset(0.0, 3, 5) - (-2.0)).abs() < 1e-4);
        assert!((circular_offset(0.0, 3, 5) - 3.0).abs() > 1.0); // not the long way
    }

    #[test]
    fn visible_slots_no_duplicate_when_short_list() {
        let slots = visible_slots(0.0, 1, 3);
        let indices: Vec<usize> = slots.iter().map(|(_, i)| *i).collect();
        assert_eq!(indices.len(), 1);
        assert_eq!(indices[0], 0);
        assert!(indices.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn sigmoid_packing_tightens_toward_sides() {
        let t = CoverFlowTuning::for_cover(200.0);
        let near_gap = rack_x(1.0, &t) - rack_x(0.0, &t);
        let far_gap = rack_x(3.0, &t) - rack_x(2.0, &t);
        assert!(
            far_gap < near_gap * 0.5,
            "side covers should pack much closer: near {near_gap}, far {far_gap}"
        );
        // Odd symmetry.
        assert!((rack_x(1.5, &t) + rack_x(-1.5, &t)).abs() < 1e-3);
    }

    #[test]
    fn perspective_right_card_outer_edge_receded() {
        let tuning = CoverFlowTuning::for_cover(200.0);
        let center = Pos2::new(400.0, 300.0);
        let front = project_cover(center, 200.0, 260.0, 0.0, &tuning);
        let q = project_cover(center, 200.0, 260.0, 1.0, &tuning);
        // Right-side card: outer (right) edge should be narrower / more foreshortened than inner (left).
        let right_h = (q.tr.y - q.br.y).abs();
        let left_h = (q.tl.y - q.bl.y).abs();
        let front_avg_h = ((front.tl.y - front.bl.y).abs() + (front.tr.y - front.br.y).abs()) * 0.5;
        let side_avg_h = (left_h + right_h) * 0.5;
        assert!(
            right_h < left_h,
            "outer edge height {right_h} should be less than inner {left_h}"
        );
        assert!(
            side_avg_h < front_avg_h,
            "side average height {side_avg_h} should be less than front {front_avg_h}"
        );
        assert!(q.tr.x > q.tl.x);
        assert!(q.br.x > q.bl.x);
    }

    #[test]
    fn perspective_left_card_mirrored() {
        let tuning = CoverFlowTuning::for_cover(200.0);
        let center = Pos2::new(400.0, 300.0);
        let front = project_cover(center, 200.0, 260.0, 0.0, &tuning);
        let q = project_cover(center, 200.0, 260.0, -1.0, &tuning);
        let left_h = (q.tl.y - q.bl.y).abs();
        let right_h = (q.tr.y - q.br.y).abs();
        let front_avg_h = ((front.tl.y - front.bl.y).abs() + (front.tr.y - front.br.y).abs()) * 0.5;
        let side_avg_h = (left_h + right_h) * 0.5;
        assert!(
            left_h < right_h,
            "outer left edge height {left_h} should be less than inner {right_h}"
        );
        assert!(
            side_avg_h < front_avg_h,
            "side average height {side_avg_h} should be less than front {front_avg_h}"
        );
    }

    #[test]
    fn snapping_animates_toward_target_and_settles_exactly() {
        let tuning = CoverFlowTuning::for_cover(200.0);
        let mut pos = 1.0_f32;
        let mut vel = 0.0_f32;
        let mut target = 2.0_f32; // e.g. one wheel step
        let mut phase = InteractionPhase::Snapping;

        // First frame must move but not teleport.
        integrate_motion(
            &mut pos,
            &mut vel,
            &mut target,
            &mut phase,
            5,
            1.0 / 60.0,
            &tuning,
        );
        assert!(pos > 1.0 && pos < 1.5, "should ease, not jump: pos={pos}");

        for _ in 0..800 {
            integrate_motion(
                &mut pos,
                &mut vel,
                &mut target,
                &mut phase,
                5,
                1.0 / 60.0,
                &tuning,
            );
        }
        assert!((pos - 2.0).abs() < 1e-4, "pos={pos}");
        assert!(vel.abs() < 1e-3);
        assert_eq!(phase, InteractionPhase::Idle);
    }

    #[test]
    fn inertia_hands_over_to_spring_and_settles_on_integer() {
        let tuning = CoverFlowTuning::for_cover(200.0);
        let mut pos = 0.3_f32;
        let mut vel = 5.0_f32;
        let mut target = 0.0_f32;
        let mut phase = InteractionPhase::Inertia;
        for _ in 0..2000 {
            integrate_motion(
                &mut pos,
                &mut vel,
                &mut target,
                &mut phase,
                7,
                1.0 / 60.0,
                &tuning,
            );
        }
        assert_eq!(phase, InteractionPhase::Idle);
        assert!((pos - pos.round()).abs() < 1e-4, "pos={pos}");
        assert!((0.0..7.0).contains(&pos), "re-anchored into range: {pos}");
    }

    #[test]
    fn integrate_motion_finite_under_large_dt() {
        let tuning = CoverFlowTuning::for_cover(200.0);
        let mut pos = 0.5_f32;
        let mut vel = 50.0_f32;
        let mut target = 0.0_f32;
        let mut phase = InteractionPhase::Inertia;
        integrate_motion(&mut pos, &mut vel, &mut target, &mut phase, 3, 0.5, &tuning);
        assert!(pos.is_finite());
        assert!(vel.is_finite());
    }
}
