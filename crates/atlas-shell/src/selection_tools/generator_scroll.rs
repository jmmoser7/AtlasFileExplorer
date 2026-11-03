//! A transient overlay thumb. It never participates in text layout.
use super::*;

pub fn paint(
    ui: &egui::Ui,
    id: Id,
    viewport: Rect,
    content_height: f32,
    offset: f32,
    zoom: f32,
    theme: Palette,
) {
    if content_height <= viewport.height() + zoom || viewport.height() <= 0.0 {
        return;
    }
    let now = ui.input(|i| i.time);
    let user_scrolled = ui.input(|i| {
        (viewport.contains(
            i.pointer
                .hover_pos()
                .unwrap_or(Pos2::new(f32::MAX, f32::MAX)),
        ) && i.raw_scroll_delta != Vec2::ZERO)
            || [
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
                egui::Key::PageUp,
                egui::Key::PageDown,
                egui::Key::Home,
                egui::Key::End,
            ]
            .iter()
            .any(|key| i.key_pressed(*key))
    });
    let key = id.with("transient-scroll-time");
    if user_scrolled {
        ui.ctx().data_mut(|d| d.insert_temp(key, now));
    }
    let Some(last) = ui.ctx().data(|d| d.get_temp::<f64>(key)) else {
        return;
    };
    let idle = (now - last).max(0.0);
    let alpha = if idle <= 0.65 {
        0.65
    } else {
        (0.65 * (1.0 - (idle - 0.65) / 0.35)).max(0.0) as f32
    };
    if alpha <= 0.0 {
        return;
    }
    let height = (viewport.height() * viewport.height() / content_height)
        .max(24.0 * zoom)
        .min(viewport.height());
    let y = viewport.top()
        + (viewport.height() - height) * offset.clamp(0.0, content_height - viewport.height())
            / (content_height - viewport.height());
    let thumb = Rect::from_min_size(
        Pos2::new(viewport.right() - 5.0 * zoom, y),
        Vec2::new(3.0 * zoom, height),
    );
    ui.painter().with_clip_rect(viewport).rect_filled(
        thumb,
        2.0 * zoom,
        theme.sub.gamma_multiply(alpha),
    );
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(16));
}
