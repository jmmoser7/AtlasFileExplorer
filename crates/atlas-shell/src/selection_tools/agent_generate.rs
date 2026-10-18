use super::*;

#[derive(Default)]
pub struct ImageGenerateResponse {
    pub count: Option<u8>,
    pub submit: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn image_generate_control(
    ui: &mut egui::Ui,
    play_rect: Rect,
    count_rect: Rect,
    count: u8,
    running: bool,
    live_active: bool,
    zoom: f32,
    theme: Palette,
) -> ImageGenerateResponse {
    let mut out = ImageGenerateResponse::default();
    // Register both targets every frame. Conditional registration based on a
    // parent hover response loses hover as soon as a child owns the pointer.
    let show_count = ui.rect_contains_pointer(count_rect.union(play_rect));
    {
        let minus =
            Rect::from_min_size(count_rect.min, Vec2::new(13.0 * zoom, count_rect.height()));
        let plus = Rect::from_min_size(
            Pos2::new(count_rect.right() - 13.0 * zoom, count_rect.top()),
            Vec2::new(13.0 * zoom, count_rect.height()),
        );
        for (rect, icon, name, delta) in [
            (minus, Icon::ChevronLeft, "Fewer images", -1i16),
            (plus, Icon::ChevronRight, "More images", 1i16),
        ] {
            let response = ui
                .interact(rect, ui.id().with(name), Sense::click())
                .on_hover_text(name);
            let glyph = Rect::from_center_size(rect.center(), Vec2::splat(6.0 * zoom));
            if show_count {
                icons::paint(
                    ui.painter(),
                    glyph,
                    icon,
                    if response.hovered() {
                        theme.ink
                    } else {
                        theme.sub
                    },
                );
            }
            if response.clicked() {
                out.count = Some((i16::from(count.max(1)) + delta).clamp(1, 8) as u8);
            }
        }
    }
    if show_count {
        canvas_text::text(
            ui.painter(),
            count_rect.center(),
            Align2::CENTER_CENTER,
            count.max(1).to_string(),
            canvas_scale::font(8.0, zoom),
            theme.sub,
        );
    }
    out.submit = generator::generator_play_control(
        ui,
        play_rect,
        running,
        live_active,
        ui.id().with("image-agent-play"),
        zoom,
        theme,
    );
    out
}
