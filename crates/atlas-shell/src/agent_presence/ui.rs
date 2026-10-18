//! Small presentation primitives shared by the roster, profiles and conversation.
use super::{AvatarVisual, Participant, PresencePainter};
use crate::{
    icons::{self, Icon},
    theme::Palette,
};
use eframe::egui::{
    self, Align2, Color32, FontId, Id, Painter, Pos2, Rect, Response, Sense, Ui, Vec2,
};

/// Keep the approved slate-muted hierarchy readable, while following user themes.
pub(super) fn presentation_palette(p: &Palette) -> Palette {
    Palette {
        sub: p.sub.lerp_to_gamma(p.ink, 0.22).lerp_to_gamma(p.link, 0.10),
        ..*p
    }
}
pub(super) fn resolved_family(
    ctx: &egui::Context,
    requested: Option<&egui::FontFamily>,
) -> egui::FontFamily {
    requested
        .filter(|family| ctx.fonts(|fonts| fonts.families().contains(family)))
        .cloned()
        .unwrap_or(egui::FontFamily::Proportional)
}
pub(super) fn configure(ui: &mut Ui, requested: Option<&egui::FontFamily>, p: &Palette) {
    let family = resolved_family(ui.ctx(), requested);
    let style = ui.style_mut();
    for font in style.text_styles.values_mut() {
        font.family = family.clone();
    }
    style.override_font_id = None;
    style.visuals.override_text_color = Some(p.ink);
}
pub(super) fn font(ui: &Ui, size: f32) -> FontId {
    FontId::new(size, egui::TextStyle::Body.resolve(ui.style()).family)
}
pub(super) fn shadow(p: &Palette) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: p.ink.gamma_multiply(if p.dark_mode { 0.26 } else { 0.12 }),
    }
}
pub(super) fn surface(painter: &Painter, rect: Rect, radius: u8, p: &Palette, lift: bool) {
    if lift {
        let shadow = egui::epaint::Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: p.ink.gamma_multiply(if p.dark_mode { 0.18 } else { 0.08 }),
        };
        painter.add(shadow.as_shape(rect, egui::CornerRadius::same(radius)));
    }
    painter.rect_filled(rect, radius, p.card);
}
pub(super) fn label(ui: &Ui, rect: Rect, text: &str, size: f32, color: Color32, align: Align2) {
    if !rect.is_positive() {
        return;
    }
    let painter = ui.painter();
    let mut job = egui::text::LayoutJob::simple(text.into(), font(ui, size), color, rect.width());
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = painter.layout_job(job);
    let pos = align.align_size_within_rect(galley.size(), rect).min;
    painter.with_clip_rect(rect).galley(pos, galley, color);
}
pub(super) fn chevron(p: &Painter, center: Pos2, color: Color32) {
    p.add(egui::Shape::line(
        vec![
            center + egui::vec2(-3.0, -1.5),
            center + egui::vec2(0.0, 1.5),
            center + egui::vec2(3.0, -1.5),
        ],
        egui::Stroke::new(1.15_f32, color),
    ));
}
pub(super) fn participant_face(
    ui: &Ui,
    painter: &mut PresencePainter,
    p: &Participant<'_>,
    slot: &str,
    rect: Rect,
    now: f64,
    reduced: bool,
) {
    if p.human {
        ui.painter().circle_filled(
            rect.center(),
            rect.width() * 0.5,
            ui.visuals().widgets.inactive.bg_fill,
        );
        if let Some(texture) = p.photo_texture {
            egui::Image::from_texture(texture)
                .fit_to_exact_size(rect.size())
                .corner_radius(255)
                .paint_at(ui, rect);
        } else {
            let initial = p
                .name
                .chars()
                .next()
                .unwrap_or('U')
                .to_uppercase()
                .to_string();
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                initial,
                font(ui, rect.height() * 0.36),
                ui.visuals().text_color(),
            );
        }
    } else {
        let id = super::avatar::slot_id(p.id, slot);
        painter.paint_sized(
            ui.painter(),
            AvatarVisual {
                id: &id,
                center: rect.center(),
                expression: "neutral",
                orientation: "forward",
                hat: p.hat,
                prop: "none",
                opacity: 1.0,
            },
            now,
            reduced,
            rect.height(),
        );
    }
}
pub(super) fn provider_icon(id: &str) -> Option<Icon> {
    match id {
        "codex" => Some(Icon::ProviderCodex),
        "cursor" => Some(Icon::ProviderCursor),
        "ollama" => Some(Icon::ProviderOllama),
        _ => None,
    }
}
pub(super) fn provider(ui: &Ui, rect: Rect, id: &str, color: Color32) {
    icons::paint(
        ui.painter(),
        rect,
        provider_icon(id).unwrap_or(Icon::Agent),
        color,
    );
}
#[derive(Clone, Copy)]
pub(super) enum Glyph {
    Plus,
    Close,
    Pause,
    Play,
    Stop,
    Locate,
    More,
    Send,
    Exit,
    Chat,
    Clone,
    Lock,
    Schedule,
    Hats,
}
pub(super) fn glyph_button(
    ui: &Ui,
    id: Id,
    rect: Rect,
    glyph: Glyph,
    title: &str,
    p: &Palette,
) -> Response {
    let response = ui.interact(rect, id, Sense::click()).on_hover_text(title);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), title)
    });
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, rect.height() as u8 / 2, p.card_hover);
    }
    draw_glyph(ui.painter(), rect.shrink(rect.width() * 0.24), glyph, p.ink);
    response
}
pub(super) fn draw_glyph(p: &Painter, r: Rect, glyph: Glyph, color: Color32) {
    let center = r.center();
    let s = r.width().min(r.height());
    let stroke = egui::Stroke::new(1.5_f32, color);
    match glyph {
        Glyph::Stop => icons::paint(p, r, Icon::Stop, color),
        Glyph::Locate => icons::paint(p, r, Icon::SnapCenter, color),
        Glyph::Chat => icons::paint(p, r, Icon::ChatWindow, color),
        Glyph::Schedule => icons::paint(p, r, Icon::Clock, color),
        Glyph::Clone => {
            icons::paint(p, r.translate(Vec2::new(-2.0, -2.0)), Icon::Pages, color);
        }
        Glyph::Plus | Glyph::Close => {
            let d = s * 0.3;
            let (a, b) = if matches!(glyph, Glyph::Close) {
                (Vec2::new(d, d), Vec2::new(d, -d))
            } else {
                (Vec2::new(d, 0.0), Vec2::new(0.0, d))
            };
            p.line_segment([center - a, center + a], stroke);
            p.line_segment([center - b, center + b], stroke);
        }
        Glyph::Pause => {
            for x in [-s * 0.2, s * 0.2] {
                p.line_segment(
                    [
                        center + Vec2::new(x, -s * 0.3),
                        center + Vec2::new(x, s * 0.3),
                    ],
                    egui::Stroke::new(3.0_f32, color),
                );
            }
        }
        Glyph::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    center + Vec2::new(-s * 0.2, -s * 0.34),
                    center + Vec2::new(s * 0.32, 0.0),
                    center + Vec2::new(-s * 0.2, s * 0.34),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        Glyph::More => {
            for x in [-s * 0.3, 0.0, s * 0.3] {
                p.circle_filled(center + Vec2::new(x, 0.0), s * 0.07, color);
            }
        }
        Glyph::Send => {
            p.line_segment(
                [
                    center + Vec2::new(0.0, s * 0.3),
                    center - Vec2::new(0.0, s * 0.3),
                ],
                stroke,
            );
            p.add(egui::Shape::line(
                vec![
                    center + Vec2::new(-s * 0.25, -s * 0.05),
                    center - Vec2::new(0.0, s * 0.3),
                    center + Vec2::new(s * 0.25, -s * 0.05),
                ],
                stroke,
            ));
        }
        Glyph::Exit => {
            // A small setting sun: retirement is a reversible departure.
            p.line_segment(
                [
                    Pos2::new(r.left(), center.y + s * 0.2),
                    Pos2::new(r.right(), center.y + s * 0.2),
                ],
                stroke,
            );
            let points = (0..17)
                .map(|n| {
                    let a = std::f32::consts::PI + n as f32 * std::f32::consts::PI / 16.0;
                    center + Vec2::new(a.cos() * s * 0.26, a.sin() * s * 0.26 + s * 0.2)
                })
                .collect();
            p.add(egui::Shape::line(points, stroke));
            for a in [-2.65_f32, -1.57, -0.49] {
                let v = Vec2::angled(a);
                p.line_segment(
                    [
                        center + Vec2::new(0.0, s * 0.2) + v * s * 0.4,
                        center + Vec2::new(0.0, s * 0.2) + v * s * 0.5,
                    ],
                    stroke,
                );
            }
        }
        Glyph::Lock => {
            p.rect_stroke(
                Rect::from_center_size(
                    center + Vec2::new(0.0, s * 0.15),
                    Vec2::new(s * 0.65, s * 0.55),
                ),
                2,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.circle_stroke(center - Vec2::new(0.0, s * 0.15), s * 0.23, stroke);
        }
        Glyph::Hats => {
            p.add(egui::Shape::line(
                vec![
                    center + Vec2::new(-s * 0.45, s * 0.2),
                    center + Vec2::new(-s * 0.25, s * 0.05),
                    center + Vec2::new(-s * 0.2, -s * 0.3),
                    center + Vec2::new(s * 0.22, -s * 0.3),
                    center + Vec2::new(s * 0.27, s * 0.05),
                    center + Vec2::new(s * 0.45, s * 0.2),
                ],
                stroke,
            ));
        }
    }
}
pub(super) fn menu_row(ui: &mut Ui, p: &Palette, glyph: Glyph, title: &str) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 8, p.card_hover);
    }
    draw_glyph(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(rect.left() + 16.0, rect.center().y),
            Vec2::splat(18.0),
        ),
        glyph,
        p.sub,
    );
    label(
        ui,
        Rect::from_min_max(rect.min + Vec2::new(37.0, 0.0), rect.max),
        title,
        14.0,
        p.ink,
        Align2::LEFT_CENTER,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    response.clicked()
}
