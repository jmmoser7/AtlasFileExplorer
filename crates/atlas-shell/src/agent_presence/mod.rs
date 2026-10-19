//! Shared ephemeral participant chrome. Apps supply data and handle actions;
//! this module never reads a workbook, connects a provider, or commits a command.
mod art;
mod art_geometry;
mod avatar;
mod controls;
mod conversation;
mod departure;
mod departure_selection;
mod hats;
mod head;
mod model;
mod motion;
mod people;
mod profile;
mod rig;
mod roster;
mod schedule;
mod state;
mod transcript;
mod ui;
pub use departure_selection::{DepartureSequence, RANDOM_DEPARTURE};
pub use model::*;
pub use motion::system_reduced_motion;
pub use people::{people_setup_ui, PeopleSetupAction, PeopleSetupState};
pub use roster::*;
pub use schedule::{schedule_editor_ui, ScheduleAction, ScheduleEditor};
pub use state::RosterState;
pub fn departure_duration(style: &str) -> f64 {
    art::master()
        .departures
        .iter()
        .find(|d| d.id == style)
        .map_or(1.5, |d| d.duration)
}

pub fn participant_name(
    painter: &eframe::egui::Painter,
    center: eframe::egui::Pos2,
    name: &str,
    palette: &crate::theme::Palette,
) {
    painter.text(
        center + eframe::egui::vec2(0.0, 27.0),
        eframe::egui::Align2::CENTER_TOP,
        name,
        eframe::egui::FontId::proportional(10.0),
        palette.sub,
    );
}
use eframe::egui::{Painter, Pos2};
use std::sync::Arc;

/// Cached local meshes move through an egui layer transform, so a moving agent
/// does not allocate and retessellate its head, hat or prop on each frame.
fn cached_mesh(
    painter: &Painter,
    id: eframe::egui::Id,
    mesh: Arc<eframe::egui::Mesh>,
    transform: eframe::egui::emath::TSTransform,
    opacity: f32,
) {
    let layer = eframe::egui::LayerId::new(painter.layer_id().order, id);
    painter.ctx().set_sublayer(painter.layer_id(), layer);
    let parent = painter
        .ctx()
        .layer_transform_to_global(painter.layer_id())
        .unwrap_or(eframe::egui::emath::TSTransform::IDENTITY);
    painter.ctx().set_transform_layer(layer, parent * transform);
    let mut local = painter.clone().with_layer_id(layer);
    // The clone inherits a screen-space clip. Replace it in local coordinates;
    // with_clip_rect would intersect two different coordinate spaces.
    local.set_clip_rect(transform.inverse() * painter.clip_rect());
    local.set_opacity(opacity.clamp(0.0, 1.0));
    local.add(eframe::egui::Shape::Mesh(mesh));
}

pub struct AvatarVisual<'a> {
    pub id: &'a str,
    pub center: Pos2,
    pub expression: &'a str,
    pub orientation: &'a str,
    pub hat: &'a str,
    pub prop: &'a str,
    pub opacity: f32,
}

#[derive(Default)]
pub struct PresencePainter {
    avatars: avatar::AvatarCache,
    art: art::ArtCache,
}
impl PresencePainter {
    /// Head diameter is screen-space points, independent of board zoom.
    /// Returns whether another paint is needed for an active face/prop transition.
    pub fn paint_avatar(
        &mut self,
        painter: &Painter,
        visual: AvatarVisual<'_>,
        now: f64,
        reduced: bool,
    ) -> bool {
        self.paint_sized(painter, visual, now, reduced, art::master().head_points)
    }
    /// Board collaborators are 36 world units, including their hats and props.
    /// The canonical meshes stay cached while the camera changes their layer scale.
    pub fn paint_world_avatar(
        &mut self,
        painter: &Painter,
        visual: AvatarVisual<'_>,
        now: f64,
        reduced: bool,
        zoom: f32,
    ) -> bool {
        if !zoom.is_finite() || zoom <= 0.0 {
            return false;
        }
        self.paint_scaled(painter, visual, now, reduced, 36.0, zoom)
    }
    fn paint_sized(
        &mut self,
        painter: &Painter,
        visual: AvatarVisual<'_>,
        now: f64,
        reduced: bool,
        head: f32,
    ) -> bool {
        self.paint_scaled(painter, visual, now, reduced, head, 1.0)
    }
    fn paint_scaled(
        &mut self,
        painter: &Painter,
        visual: AvatarVisual<'_>,
        now: f64,
        reduced: bool,
        head: f32,
        scale: f32,
    ) -> bool {
        if !visual.center.is_finite() || !now.is_finite() {
            return false;
        }
        let (active, look) = self
            .avatars
            .paint(painter, &visual, now, reduced, head, scale);
        let animated = self
            .art
            .paint_pose(painter, &visual, now, reduced, head, scale, look);
        active || animated
    }
    /// Drop ephemeral geometry for sessions no longer on this board.
    pub fn retain(&mut self, ids: &[&str]) {
        self.avatars.retain(ids);
    }
}

pub fn world_participant_name(
    painter: &Painter,
    center: Pos2,
    name: &str,
    palette: &crate::theme::Palette,
    zoom: f32,
    family: Option<&eframe::egui::FontFamily>,
) {
    let mut font = crate::canvas_scale::font(10.0, zoom);
    font.family = ui::resolved_family(painter.ctx(), family);
    crate::canvas_text::text(
        painter,
        center + eframe::egui::vec2(0.0, crate::canvas_scale::px(26.0, zoom)),
        eframe::egui::Align2::CENTER_TOP,
        name,
        font,
        ui::presentation_palette(palette).sub,
    );
}
