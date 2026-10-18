//! One neutral approval presenter for attached generators and conversations.
use super::*;

const FONT_UNITS: f32 = 11.0;
const CHOICE_HEIGHT: f32 = 24.0;
const GAP: f32 = 6.0;
const MAX_DESCRIPTION_ROWS: usize = 6;

fn description_layout(
    ctx: &egui::Context,
    description: &str,
    width: f32,
) -> std::sync::Arc<canvas_text::WorldLayout> {
    canvas_text::world_layout_rows(
        ctx,
        description,
        canvas_scale::font(FONT_UNITS, 1.0),
        width.max(1.0),
        egui::Align::LEFT,
        MAX_DESCRIPTION_ROWS,
    )
}

/// Exact visible height in board units; a long command retains its full tooltip.
pub fn height(ctx: &egui::Context, description: &str, width_world: f32) -> f32 {
    description_layout(ctx, description, width_world).height + GAP + CHOICE_HEIGHT
}

/// The caller owns authority, transport, and whether this surface takes input.
pub fn paint(
    ui: &egui::Ui,
    rect: Rect,
    description: &str,
    interactive: bool,
    zoom: f32,
    theme: Palette,
) -> Option<bool> {
    let layout = description_layout(ui.ctx(), description, rect.width() / zoom);
    let text = canvas_text::world_text(ui.ctx(), &layout, zoom);
    let text_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), text.size().y));
    text.paint(ui.painter(), rect.min, theme.ink);
    let response = ui.interact(
        text_rect,
        ui.id().with("approval-description"),
        Sense::hover(),
    );
    let _ = object_hover_text(response, description);
    let top = text_rect.bottom() + GAP * zoom;
    let gap = GAP * zoom;
    let width = ((rect.width() - gap) * 0.5).min(104.0 * zoom);
    let mut picked = None;
    for (index, (label, allow)) in [("Allow once", true), ("Deny", false)]
        .into_iter()
        .enumerate()
    {
        let choice = Rect::from_min_size(
            Pos2::new(rect.left() + index as f32 * (width + gap), top),
            Vec2::new(width, CHOICE_HEIGHT * zoom),
        );
        let response = ui.interact(
            choice,
            ui.id().with(("approval-choice", allow)),
            if interactive {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let ink = if interactive { theme.ink } else { theme.sub };
        ui.painter().rect(
            choice,
            4.0 * zoom,
            if response.hovered() && interactive {
                theme.card_hover
            } else {
                theme.card
            },
            Stroke::new(0.7 * zoom, theme.border),
            egui::StrokeKind::Inside,
        );
        canvas_text::text(
            ui.painter(),
            choice.center(),
            Align2::CENTER_CENTER,
            label,
            canvas_scale::font(FONT_UNITS, zoom),
            ink,
        );
        if interactive && response.clicked() {
            picked = Some(allow);
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_approval_choices_are_visible_clickable_and_scale_with_the_host() {
        for zoom in [0.5, 1.0, 2.0] {
            for allow in [true, false] {
                let ctx = egui::Context::default();
                let mut hit = Pos2::ZERO;
                let mut picked = None;
                let description =
                    "The provider requests permission to write the staged dashboard HTML.";
                for frame in 0..5 {
                    let events = match frame {
                        3 | 4 => vec![
                            egui::Event::PointerMoved(hit),
                            egui::Event::PointerButton {
                                pos: hit,
                                button: egui::PointerButton::Primary,
                                pressed: frame == 3,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        _ => vec![],
                    };
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(1200.0, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let width = 260.0;
                                let h = height(ctx, description, width);
                                let rect = Rect::from_min_size(
                                    Pos2::new(50.0, 50.0),
                                    Vec2::new(width, h) * zoom,
                                );
                                hit = Pos2::new(
                                    rect.left() + if allow { 52.0 } else { 162.0 } * zoom,
                                    rect.bottom() - CHOICE_HEIGHT * zoom * 0.5,
                                );
                                assert!(rect.contains(hit));
                                picked = paint(ui, rect, description, true, zoom, Palette::dark());
                            });
                        },
                    );
                }
                assert_eq!(picked, Some(allow));
            }
        }
    }
}
