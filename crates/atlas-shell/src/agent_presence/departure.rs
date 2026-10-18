use super::{art, AvatarVisual, PresencePainter};
use eframe::egui::{self, Id, Painter, Pos2, Rect};

#[derive(Debug, Clone, Copy)]
struct Pose {
    center: Pos2,
    head_scale: f32,
    opacity: f32,
    puff: f32,
    sunset: f32,
    flap: f32,
}
fn remaining(progress: f32, start: f32) -> f32 {
    1.0 - ((progress - start) / (1.0 - start).max(0.001)).clamp(0.0, 1.0)
}
fn pose(
    start: Pos2,
    style: &str,
    p: f32,
    canvas: Rect,
    portal: Option<Rect>,
    reduced: bool,
) -> Pose {
    let mut out = Pose {
        center: start,
        head_scale: 1.0,
        opacity: 1.0 - p,
        puff: 0.0,
        sunset: 0.0,
        flap: 0.0,
    };
    if reduced {
        return out;
    }
    let style = if style == "portal" && portal.is_none() {
        "bow"
    } else {
        style
    };
    let definition = art::master()
        .departures
        .iter()
        .find(|d| d.id == style)
        .unwrap_or(&art::master().departures[0]);
    let ease = p * p * (3.0 - 2.0 * p);
    out.opacity = 1.0;
    match definition.id.as_str() {
        "sunset" => {
            let target = egui::pos2(
                canvas.right() - definition.target_inset[0],
                canvas.bottom() - definition.target_inset[1],
            );
            out.center = start
                + (target - start) * ease
                + egui::vec2(0.0, (p * definition.bob_turns).sin() * definition.bob);
            out.head_scale = 1.0 - p * definition.shrink;
            out.opacity = remaining(p, definition.fade_start);
            out.sunset = (p * std::f32::consts::PI).sin() * definition.effect_opacity;
        }
        "edge" => {
            let margin = definition.edge_margin;
            let candidates = [
                egui::pos2(canvas.left() - margin, start.y),
                egui::pos2(canvas.right() + margin, start.y),
                egui::pos2(start.x, canvas.top() - margin),
                egui::pos2(start.x, canvas.bottom() + margin),
            ];
            let target = candidates
                .into_iter()
                .min_by(|a, b| a.distance_sq(start).total_cmp(&b.distance_sq(start)))
                .unwrap();
            out.center = start
                + (target - start) * ease
                + egui::vec2(0.0, (p * definition.bob_turns).sin() * definition.bob);
        }
        "portal" => {
            let target = portal.unwrap().right_bottom()
                - egui::vec2(definition.corner_inset[0], definition.corner_inset[1]);
            out.center = start + (target - start) * ease;
            out.head_scale =
                1.0 - (1.0 - remaining(p, definition.shrink_start)) * definition.shrink;
            out.opacity = remaining(p, definition.fade_start);
            out.flap = if p > definition.effect_start && p < definition.effect_end {
                definition.effect_opacity
            } else {
                0.0
            };
        }
        _ => {
            let bow = (p / definition.bow_until.max(0.01)).min(1.0) * std::f32::consts::PI;
            out.center.y += bow.sin() * definition.drop;
            out.head_scale = 1.0 - bow.sin() * definition.shrink;
            out.opacity = remaining(p, definition.fade_start);
            out.puff = (((p - definition.effect_start) / (1.0 - definition.effect_start))
                .clamp(0.0, 1.0)
                * std::f32::consts::PI)
                .sin()
                * definition.effect_opacity;
        }
    }
    out
}
impl PresencePainter {
    /// Backend authority must already have stopped. These ephemeral flourishes
    /// never create geometry in the authored scene or affect the human camera.
    #[allow(clippy::too_many_arguments)]
    pub fn paint_departure(
        &mut self,
        painter: &Painter,
        visual: AvatarVisual<'_>,
        style: &str,
        progress: f32,
        now: f64,
        reduced: bool,
        canvas: Rect,
        portal: Option<Rect>,
    ) -> bool {
        self.paint_departure_scaled(
            painter,
            visual,
            style,
            progress,
            now,
            reduced,
            canvas,
            portal,
            1.0,
            art::master().head_points,
        )
    }
    /// Same departure model, sampled in world units and scaled by the camera.
    #[allow(clippy::too_many_arguments)]
    pub fn paint_world_departure(
        &mut self,
        painter: &Painter,
        visual: AvatarVisual<'_>,
        style: &str,
        progress: f32,
        now: f64,
        reduced: bool,
        canvas: Rect,
        portal: Option<Rect>,
        zoom: f32,
    ) -> bool {
        if !zoom.is_finite() || zoom <= 0.0 {
            return false;
        }
        self.paint_departure_scaled(
            painter, visual, style, progress, now, reduced, canvas, portal, zoom, 36.0,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn paint_departure_scaled(
        &mut self,
        painter: &Painter,
        mut visual: AvatarVisual<'_>,
        style: &str,
        progress: f32,
        now: f64,
        reduced: bool,
        canvas: Rect,
        portal: Option<Rect>,
        zoom: f32,
        head: f32,
    ) -> bool {
        let inverse = egui::emath::TSTransform::from_scaling(1.0 / zoom);
        let canvas = inverse * canvas;
        let portal = portal.map(|r| inverse * r);
        visual.center = inverse * visual.center;
        let p = progress.clamp(0.0, 1.0);
        let portal = portal.filter(|r| r.is_finite() && r.intersects(canvas));
        let resolved = if style == "portal" && portal.is_none() {
            "bow"
        } else {
            style
        };
        let definition = art::master()
            .departures
            .iter()
            .find(|d| d.id == resolved)
            .unwrap_or(&art::master().departures[0]);
        let pose = pose(visual.center, style, p, canvas, portal, reduced);
        let id = Id::new(("presence-departure", visual.id));
        if pose.sunset > 0.0 {
            self.art.effect(
                painter,
                id.with("sunset"),
                "sunset",
                egui::emath::TSTransform::from_scaling(zoom)
                    * egui::pos2(
                        canvas.right() - definition.effect_inset[0],
                        canvas.bottom() - definition.effect_inset[1],
                    ),
                definition.effect_points * zoom,
                pose.sunset * visual.opacity,
            );
        }
        visual.center = egui::emath::TSTransform::from_scaling(zoom) * pose.center;
        visual.opacity *= pose.opacity;
        visual.prop = "none";
        visual.expression = "pleased";
        visual.orientation = if style == "bow" { "down" } else { "forward" };
        self.paint_scaled(painter, visual, now, reduced, head, pose.head_scale * zoom);
        if pose.puff > 0.0 {
            self.art.effect(
                painter,
                id.with("puff"),
                "puff",
                egui::emath::TSTransform::from_scaling(zoom) * pose.center,
                definition.effect_points
                    * zoom
                    * (definition.effect_scale_from + p * definition.effect_scale_travel),
                pose.puff,
            );
        }
        if pose.flap > 0.0 {
            let corner = portal.unwrap().right_bottom();
            self.art.effect(
                painter,
                id.with("flap"),
                "flap",
                egui::emath::TSTransform::from_scaling(zoom)
                    * (corner - egui::Vec2::splat(definition.effect_points / 2.0)),
                definition.effect_points * zoom,
                pose.flap,
            );
        }
        p < 1.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn departures_are_continuous_and_reduced_motion_stays_at_the_start() {
        let canvas = Rect::from_min_size(Pos2::ZERO, egui::vec2(1100.0, 720.0));
        let start = egui::pos2(400.0, 500.0);
        let portal = Some(Rect::from_min_size(
            egui::pos2(805.0, 485.0),
            egui::vec2(230.0, 118.0),
        ));
        for style in ["bow", "sunset", "edge", "portal"] {
            let mut before = pose(start, style, 0.0, canvas, portal, false);
            assert_eq!(before.center, start);
            for n in 1..=100 {
                let next = pose(start, style, n as f32 / 100.0, canvas, portal, false);
                assert!(next.center.is_finite());
                assert!(next.center.distance(before.center) < 16.0);
                assert!((0.0..=1.0).contains(&next.opacity));
                before = next;
            }
            let simple = pose(start, style, 0.5, canvas, portal, true);
            assert_eq!(simple.center, start);
            assert_eq!(simple.opacity, 0.5);
            assert_eq!(simple.flap, 0.0);
        }
    }
    #[test]
    fn portal_crawl_requires_a_visible_real_portal() {
        let canvas = Rect::from_min_size(Pos2::ZERO, egui::vec2(1100.0, 720.0));
        let p = pose(egui::pos2(100.0, 100.0), "portal", 0.7, canvas, None, false);
        assert_eq!(p.flap, 0.0);
        assert!(p.puff > 0.0);
    }
}
