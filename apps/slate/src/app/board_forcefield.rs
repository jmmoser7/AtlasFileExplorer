//! Forcefield snap-guide pulse — a short ribbon that grows from the impact
//! and fades. Tokens live in `atlas-shell` (`board_forcefield`) so the tuner
//! can reach them; this module is the Slate-side clock and paint.

use super::board::BoardXf;
use super::board_snap::{GuideAxis, SnapGuide};
use atlas_shell::tokens::BoardForcefieldTokens;
use eframe::egui::{Color32, Pos2, Rect};

#[derive(Clone, Copy, Debug)]
pub struct ForcefieldPulse {
    pub axis: GuideAxis,
    pub pos: f32,
    pub origin: f32,
    pub born: f64,
}

/// Live pulses plus the fields already announced for the current hold.
#[derive(Clone, Debug, Default)]
pub struct Forcefield {
    pulses: Vec<ForcefieldPulse>,
    held: Vec<(GuideAxis, i32)>,
}

fn quantize(v: f32) -> i32 {
    (v * 2.0).round() as i32
}

fn same_field(a: &ForcefieldPulse, axis: GuideAxis, pos: f32) -> bool {
    a.axis == axis && quantize(a.pos) == quantize(pos)
}

/// Spawn pulses for newly acquired guides; keep fading ones that already left.
/// A field that stays snapped does not retrigger after the pulse dies.
pub fn sync(state: &mut Forcefield, guides: &[SnapGuide], now: f64, tokens: BoardForcefieldTokens) {
    let life = tokens.lifetime() as f64;
    state.pulses.retain(|p| now - p.born < life);
    state.held.retain(|&(axis, q)| {
        guides
            .iter()
            .any(|g| g.axis == axis && quantize(g.pos) == q)
    });
    for g in guides {
        let key = (g.axis, quantize(g.pos));
        if state.held.iter().any(|&h| h == key) {
            continue;
        }
        if state.pulses.iter().any(|p| same_field(p, g.axis, g.pos)) {
            continue;
        }
        state.pulses.push(ForcefieldPulse {
            axis: g.axis,
            pos: g.pos,
            origin: g.origin,
            born: now,
        });
        state.held.push(key);
    }
}

/// Tuner lock: keep a crossing pair pulsing at `world` so sliders have a target.
pub fn sync_preview(state: &mut Forcefield, world: Pos2, now: f64, tokens: BoardForcefieldTokens) {
    let life = tokens.lifetime() as f64;
    state.pulses.retain(|p| now - p.born < life);
    state.held.clear();
    for (axis, pos, origin) in [
        (GuideAxis::Vertical, world.x, world.y),
        (GuideAxis::Horizontal, world.y, world.x),
    ] {
        let live = state
            .pulses
            .iter()
            .any(|p| same_field(p, axis, pos) && now - p.born < life);
        if !live {
            state.pulses.push(ForcefieldPulse {
                axis,
                pos,
                origin,
                born: now,
            });
        }
    }
}

pub fn paint(
    painter: &eframe::egui::Painter,
    xf: &BoardXf,
    canvas: Rect,
    state: &Forcefield,
    now: f64,
    tokens: BoardForcefieldTokens,
    accent: Color32,
) -> bool {
    let mut any = false;
    for pulse in &state.pulses {
        if paint_one(painter, xf, canvas, pulse, now, tokens, accent) {
            any = true;
        }
    }
    any
}

fn paint_one(
    painter: &eframe::egui::Painter,
    xf: &BoardXf,
    canvas: Rect,
    pulse: &ForcefieldPulse,
    now: f64,
    tokens: BoardForcefieldTokens,
    accent: Color32,
) -> bool {
    let age = (now - pulse.born) as f32;
    if age < 0.0 || age >= tokens.lifetime() {
        return false;
    }
    let expand_t = (age / tokens.expand_secs.max(1e-4)).clamp(0.0, 1.0);
    let fade = if age <= tokens.expand_secs {
        1.0
    } else {
        1.0 - ((age - tokens.expand_secs) / tokens.fade_secs.max(1e-4)).clamp(0.0, 1.0)
    };
    if fade <= 0.001 {
        return false;
    }
    // Ease-out cubic: shoots out fast, then settles as it leaves the page.
    let ease = 1.0 - (1.0 - expand_t).powi(3);

    let origin = match pulse.axis {
        GuideAxis::Vertical => xf.w2s(Pos2::new(pulse.pos, pulse.origin)),
        GuideAxis::Horizontal => xf.w2s(Pos2::new(pulse.origin, pulse.pos)),
    };
    let overshoot = match pulse.axis {
        GuideAxis::Vertical => canvas.height() * 0.08,
        GuideAxis::Horizontal => canvas.width() * 0.08,
    };
    let max_half = match pulse.axis {
        GuideAxis::Vertical => origin.y.max(canvas.bottom() - origin.y) + overshoot,
        GuideAxis::Horizontal => origin.x.max(canvas.right() - origin.x) + overshoot,
    };
    let half = max_half * ease;
    if half < 1.0 {
        return true;
    }
    let (a, b) = match pulse.axis {
        GuideAxis::Vertical => (
            Pos2::new(origin.x, origin.y - half),
            Pos2::new(origin.x, origin.y + half),
        ),
        GuideAxis::Horizontal => (
            Pos2::new(origin.x - half, origin.y),
            Pos2::new(origin.x + half, origin.y),
        ),
    };
    let center = accent.gamma_multiply(tokens.center_opacity * fade);
    let edge = accent.gamma_multiply(tokens.edge_opacity * fade);
    atlas_shell::taper::paint_tapered_ribbon_graded(
        painter,
        a,
        b,
        tokens.center_weight * 0.5,
        tokens.edge_weight * 0.5,
        center,
        edge,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_guide_spawns_one_pulse() {
        let mut state = Forcefield::default();
        let g = SnapGuide {
            axis: GuideAxis::Vertical,
            pos: 40.0,
            span_start: 0.0,
            span_end: 10.0,
            origin: 5.0,
        };
        let t = BoardForcefieldTokens::default();
        sync(&mut state, &[g], 1.0, t);
        assert_eq!(state.pulses.len(), 1);
        sync(&mut state, &[g], 1.02, t);
        assert_eq!(state.pulses.len(), 1, "same field must not retrigger");
    }

    #[test]
    fn pulse_dies_after_lifetime() {
        let mut state = Forcefield::default();
        let g = SnapGuide {
            axis: GuideAxis::Horizontal,
            pos: 10.0,
            span_start: 0.0,
            span_end: 8.0,
            origin: 4.0,
        };
        let t = BoardForcefieldTokens::default();
        sync(&mut state, &[g], 0.0, t);
        sync(&mut state, &[], t.lifetime() as f64 + 0.05, t);
        assert!(state.pulses.is_empty());
    }

    #[test]
    fn staying_snapped_does_not_loop() {
        let mut state = Forcefield::default();
        let g = SnapGuide {
            axis: GuideAxis::Vertical,
            pos: 12.0,
            span_start: 0.0,
            span_end: 4.0,
            origin: 2.0,
        };
        let t = BoardForcefieldTokens::default();
        sync(&mut state, &[g], 0.0, t);
        sync(&mut state, &[g], t.lifetime() as f64 + 0.05, t);
        assert!(state.pulses.is_empty(), "pulse must die");
        assert_eq!(state.pulses.len(), 0);
        // Still on the same field — no second flash.
        assert!(state.held.len() == 1);
        sync(&mut state, &[g], t.lifetime() as f64 + 0.10, t);
        assert!(state.pulses.is_empty(), "held field must not retrigger");
        // Leave, then re-acquire.
        sync(&mut state, &[], t.lifetime() as f64 + 0.15, t);
        sync(&mut state, &[g], t.lifetime() as f64 + 0.20, t);
        assert_eq!(state.pulses.len(), 1);
    }
}
