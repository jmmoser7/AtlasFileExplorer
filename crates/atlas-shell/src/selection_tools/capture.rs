//! Capture controls inside the host-attached image/portal apron.
//! The caller owns its shared surface and Area; every control follows its zoom.
use super::*;

const STATUS_WIDTH: f32 = 128.0;
const LABEL_WIDTH: f32 = 58.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Recording,
    Paused,
    Saving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    StillCanvas,
    StillFile,
    Start,
    Pause,
    Resume,
    SaveFile,
    SaveCanvas,
    Discard,
}

pub struct Response {
    pub action: Option<Action>,
    /// The solid apron owns input, including the visible inset around controls.
    rects: [Rect; 1],
    rect_count: usize,
    controls: [Option<(Action, Rect)>; 4],
}

impl Response {
    fn new(rect: Rect, visible: bool) -> Self {
        Self {
            action: None,
            rects: [rect],
            rect_count: usize::from(visible),
            controls: [None; 4],
        }
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects[..self.rect_count]
    }

    pub fn controls(&self) -> impl Iterator<Item = &(Action, Rect)> {
        self.controls.iter().flatten()
    }

    pub fn bounds(&self) -> Rect {
        self.rects().first().copied().unwrap_or(Rect::NOTHING)
    }
}

fn body_height(rows: usize) -> f32 {
    2.0 * IMAGE_PANEL_INSET
        + rows as f32 * CORNER_HEIGHT
        + rows.saturating_sub(1) as f32 * STACK_GAP
}

/// Main-field height in board units, including the shared apron content inset.
pub fn launch_height(can_video: bool, _host_width_units: f32) -> f32 {
    body_height(if can_video { 2 } else { 1 })
}

/// Reflow depends on authored host width, so zoom never changes the topology.
pub fn active_height(phase: Phase, host_width_units: f32) -> f32 {
    let rows = if host_width_units < IMAGE_WIDE_THRESHOLD {
        match phase {
            Phase::Paused => 3,
            Phase::Starting | Phase::Recording => 2,
            Phase::Saving => 1,
        }
    } else {
        1
    };
    body_height(rows)
}

fn row(rect: Rect, index: usize, zoom: f32) -> Rect {
    let inset = canvas_scale::px(IMAGE_PANEL_INSET, zoom);
    Rect::from_min_size(
        rect.min
            + Vec2::new(
                inset,
                inset + canvas_scale::px(index as f32 * (CORNER_HEIGHT + STACK_GAP), zoom),
            ),
        Vec2::new(
            (rect.width() - 2.0 * inset).max(0.0),
            canvas_scale::px(CORNER_HEIGHT, zoom),
        ),
    )
}

pub fn elapsed_label(phase: Phase, elapsed: f32) -> String {
    let elapsed = if elapsed.is_finite() {
        elapsed.max(0.0)
    } else {
        0.0
    } as u64;
    let phase = match phase {
        Phase::Starting => "Starting…",
        Phase::Recording => "Recording",
        Phase::Paused => "Paused",
        Phase::Saving => "Saving…",
    };
    format!("{phase} {:02}:{:02}", elapsed / 60, elapsed % 60)
}

fn group(
    ui: &mut egui::Ui,
    rect: Rect,
    id: Id,
    actions: &[(Action, &str)],
    zoom: f32,
    theme: Palette,
    response: &mut Response,
) {
    let mut labels = [""; 4];
    for (index, (_, label)) in actions.iter().enumerate() {
        labels[index] = label;
    }
    let selected = segments(
        ui,
        rect,
        id,
        &labels[..actions.len()],
        usize::MAX,
        zoom,
        theme,
    );
    let share = rect.width() / actions.len().max(1) as f32;
    let start = response.controls().count();
    for (index, (action, _)) in actions.iter().enumerate() {
        response.controls[start + index] = Some((
            *action,
            Rect::from_min_size(
                rect.min + Vec2::new(index as f32 * share, 0.0),
                Vec2::new(share, rect.height()),
            ),
        ));
    }
    if let Some((action, _)) = actions.get(selected) {
        response.action = Some(*action);
    }
}

fn status(ui: &egui::Ui, rect: Rect, phase: Phase, elapsed: f32, zoom: f32, theme: Palette) {
    let dot = rect.left_center() + Vec2::new(canvas_scale::px(3.0, zoom), 0.0);
    ui.painter().circle_filled(
        dot,
        canvas_scale::px(2.6, zoom),
        if phase == Phase::Paused {
            theme.sub
        } else {
            theme.accent
        },
    );
    canvas_text::text(
        &ui.painter().with_clip_rect(rect),
        rect.left_center() + Vec2::new(canvas_scale::px(11.0, zoom), 0.0),
        Align2::LEFT_CENTER,
        elapsed_label(phase, elapsed),
        canvas_scale::font(CAPSULE_LABEL, zoom),
        theme.ink,
    );
}

fn transport_actions(phase: Phase) -> &'static [(Action, &'static str)] {
    match phase {
        Phase::Starting => &[(Action::Discard, "Discard")],
        Phase::Saving => &[],
        Phase::Recording => &[(Action::Pause, "Pause"), (Action::Discard, "Discard")],
        Phase::Paused => &[
            (Action::SaveFile, "Save File…"),
            (Action::SaveCanvas, "Place Board"),
            (Action::Resume, "Resume"),
            (Action::Discard, "Discard"),
        ],
    }
}

/// Paint transport controls into a shared `ImagePanelKind::Custom` main field.
pub fn active(
    ui: &mut egui::Ui,
    rect: Rect,
    phase: Phase,
    elapsed: f32,
    zoom: f32,
    theme: Palette,
) -> Response {
    let visible = !canvas_scale::too_small(12.0 * zoom);
    let mut response = Response::new(rect, visible);
    if !visible {
        return response;
    }
    let actions = transport_actions(phase);
    let top = row(rect, 0, zoom);
    let stacked = rect.width() / zoom.max(f32::EPSILON) < IMAGE_WIDE_THRESHOLD;
    let status_rect = if stacked || actions.is_empty() {
        top
    } else {
        Rect::from_min_size(
            top.min,
            Vec2::new(canvas_scale::px(STATUS_WIDTH, zoom), top.height()),
        )
    };
    status(ui, status_rect, phase, elapsed, zoom, theme);
    for (index, chunk) in actions.chunks(if stacked { 2 } else { 4 }).enumerate() {
        let controls = if stacked {
            row(rect, index + 1, zoom)
        } else {
            Rect::from_min_max(
                Pos2::new(
                    status_rect.right() + canvas_scale::px(IMAGE_PANEL_INSET, zoom),
                    top.top(),
                ),
                top.max,
            )
        };
        group(
            ui,
            controls,
            ui.id().with(("capture_transport", index)),
            chunk,
            zoom,
            theme,
            &mut response,
        );
    }
    ui.advance_cursor_after_rect(rect);
    response
}

/// Paint capture launch controls on the existing apron, with no second tray.
pub fn launch(
    ui: &mut egui::Ui,
    rect: Rect,
    can_video: bool,
    zoom: f32,
    theme: Palette,
) -> Response {
    let visible = !canvas_scale::too_small(12.0 * zoom);
    let mut response = Response::new(rect, visible);
    if !visible {
        return response;
    }
    for index in 0..if can_video { 2 } else { 1 } {
        let row = row(rect, index, zoom);
        let (icon, label, actions): (_, _, &[(Action, &str)]) = if index == 0 {
            (
                Icon::Image,
                "Still",
                &[
                    (Action::StillCanvas, "Place Board"),
                    (Action::StillFile, "Save File…"),
                ],
            )
        } else {
            (Icon::Video, "MP4", &[(Action::Start, "Start recording")])
        };
        icons::paint(
            ui.painter(),
            Rect::from_center_size(
                row.left_center() + Vec2::new(canvas_scale::px(6.0, zoom), 0.0),
                Vec2::splat(canvas_scale::px(12.0, zoom)),
            ),
            icon,
            theme.ink,
        );
        canvas_text::text(
            ui.painter(),
            row.left_center() + Vec2::new(canvas_scale::px(18.0, zoom), 0.0),
            Align2::LEFT_CENTER,
            label,
            canvas_scale::font(CAPSULE_LABEL, zoom),
            theme.sub,
        );
        let controls = Rect::from_min_max(
            row.min + Vec2::new(canvas_scale::px(LABEL_WIDTH, zoom), 0.0),
            row.max,
        );
        group(
            ui,
            controls,
            ui.id().with(("capture_launch", index)),
            actions,
            zoom,
            theme,
            &mut response,
        );
        if index == 1 {
            object_hover_text(
                ui.interact(
                    Rect::from_min_max(row.min, controls.left_bottom()),
                    ui.id().with("capture_video_hint"),
                    Sense::hover(),
                ),
                "Silent MP4 · up to 1080p / 30 fps",
            );
        }
    }
    ui.advance_cursor_after_rect(rect);
    response
}

#[cfg(test)]
mod tests;
