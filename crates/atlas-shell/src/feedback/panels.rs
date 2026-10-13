//! The one shared chooser and bug/feature form presentation.
use super::{FeedbackHub, FeedbackKind, FeedbackPhase, FeedbackUiOutput};
use crate::icons::{self, Icon};
use crate::theme::Palette;
use crate::tokens::{self, FeedbackTokens};
use eframe::egui::{self, Color32, CornerRadius, Id, Margin, Rect, RichText, Stroke, Vec2};

pub(super) fn color(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

pub(super) fn bounds(ctx: &egui::Context, hub: &FeedbackHub, t: &FeedbackTokens) -> Rect {
    let canvas = hub
        .canvas_rect
        .filter(|r| r.is_positive())
        .unwrap_or(ctx.available_rect());
    canvas.intersect(ctx.screen_rect()).shrink(
        t.canvas_buffer
            .min(canvas.width().min(canvas.height()) * 0.1),
    )
}

pub(super) fn show(
    ctx: &egui::Context,
    p: &Palette,
    hub: &mut FeedbackHub,
    app: &str,
    version: &str,
) -> FeedbackUiOutput {
    let mut out = FeedbackUiOutput {
        command: None,
        toast: None,
    };
    if matches!(hub.phase, FeedbackPhase::Closed | FeedbackPhase::Recording) {
        return out;
    }
    let tokens = tokens::current();
    let t = &tokens.feedback;
    let dark = ctx.style().visuals.dark_mode;
    let panel = color(if dark { t.panel_dark } else { t.panel_light });
    let field = color(if dark { t.field_dark } else { t.field_light });
    let (action, action_ink) = tokens.dock.inline.primary_colors(dark, false);
    let area = bounds(ctx, hub, t);
    let width = (t.width - 2.0).min((area.width() - 2.0).max(1.0));
    let picker = hub.phase == FeedbackPhase::KindPicker;
    let bug = hub.kind == Some(FeedbackKind::Bug);
    let height = area.height().min(if picker { 300.0 } else { 560.0 });
    let default_pos = area.center() - Vec2::new(width, height) * 0.5;
    let dialog_pos = hub.dialog_pos.unwrap_or(default_pos);
    let (title, subtitle) = if picker {
        ("Feedback", "Help improve the app.")
    } else if bug {
        ("Report a bug", "Describe what went wrong.")
    } else {
        ("Request a feature", "Describe what would help your work.")
    };
    let mut close = false;
    let mut header_drag = Vec2::ZERO;
    let shown = egui::Window::new(title)
        .id(Id::new(("feedback-dialog", app, picker, bug)))
        .title_bar(false)
        .collapsible(false)
        .resizable([false, true])
        .movable(true)
        .constrain_to(area)
        .current_pos(dialog_pos)
        .default_width(width)
        .default_height(height)
        .min_height(area.height().min(if picker { 240.0 } else { 280.0 }))
        .min_width(width)
        .max_width(width)
        .max_height(area.height())
        .vscroll(true)
        .frame(
            egui::Frame::new()
                .fill(panel)
                .stroke(Stroke::new(1.0_f32, p.border))
                .corner_radius(CornerRadius::same(t.radius))
                .inner_margin(0),
        )
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(13.0));
            ui.visuals_mut().override_text_color = Some(p.ink);
            ui.spacing_mut().item_spacing = Vec2::new(7.0, 0.0);
            ui.spacing_mut().button_padding = Vec2::new(12.0, 7.0);
            ui.visuals_mut().widgets.inactive.weak_bg_fill = field;
            ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
            ui.visuals_mut().widgets.inactive.corner_radius = CornerRadius::same(6);
            header_drag = header(
                ui,
                p,
                title,
                subtitle,
                t.inset,
                tokens.dock.inline.primary_colors(dark, true),
                &mut close,
            );
            rule(ui, p);
            egui::Frame::new()
                .inner_margin(Margin::same(t.inset as i8))
                .show(ui, |ui| {
                    if picker {
                        chooser(ui, p, hub);
                    } else {
                        form(ui, p, hub, t, field, bug);
                        ui.add_space(t.section_gap);
                        rule(ui, p);
                        ui.add_space(15.0);
                        ui.label(
                            RichText::new("Saved locally with app details and any recorded steps.")
                                .size(12.0)
                                .color(p.sub),
                        );
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if ui.button("Back").clicked() {
                                hub.phase = FeedbackPhase::KindPicker;
                            }
                            let ready = !hub.description.trim().is_empty();
                            let (save, email) = ui
                                .with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center)
                                        .with_main_wrap(true),
                                    |ui| {
                                        let save = ui
                                            .add_enabled(
                                                ready,
                                                egui::Button::new(
                                                    RichText::new("Save report").color(action_ink),
                                                )
                                                .fill(action),
                                            )
                                            .clicked();
                                        let email = ui
                                            .add_enabled(ready, egui::Button::new("Email…"))
                                            .clicked();
                                        (save, email)
                                    },
                                )
                                .inner;
                            if save || email {
                                match super::ui::submit(hub, app, version) {
                                    Ok(dir) => {
                                        hub.set_last_bundle(dir.clone());
                                        if email {
                                            if let Some(body) = hub.mailto_summary(app, version) {
                                                super::ui::open_mailto(
                                                    &hub.mailto_subject(app),
                                                    &body,
                                                );
                                            }
                                        }
                                        out.toast = Some(if email {
                                            format!(
                                                "Saved report — attach files from {}",
                                                dir.display()
                                            )
                                        } else {
                                            format!("Saved report to {}", dir.display())
                                        });
                                        hub.reset_after_submit();
                                    }
                                    Err(error) => hub.attach_note = Some(error),
                                }
                            }
                        });
                    }
                });
        });
    if let Some(shown) = shown {
        hub.dialog_pos = Some(shown.response.rect.min + header_drag);
    }
    if close {
        hub.phase = FeedbackPhase::Closed;
    }
    out
}

fn rule(ui: &mut egui::Ui, p: &Palette) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, p.border));
}

fn header(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    subtitle: &str,
    inset: f32,
    colors: (Color32, Color32),
    close: &mut bool,
) -> Vec2 {
    let response = egui::Frame::new()
        .inner_margin(Margin::symmetric(inset as i8, 20))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (circle, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
                ui.painter().circle_filled(circle.center(), 16.0, colors.0);
                icons::paint(ui.painter(), circle.shrink(7.0), Icon::Feedback, colors.1);
                ui.add_space(5.0);
                let text_width = (ui.available_width() - 31.0).max(40.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(text_width, 36.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(RichText::new(title).size(17.0).strong());
                        ui.add_space(2.0);
                        ui.label(RichText::new(subtitle).size(12.0).color(p.sub));
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().button_padding = Vec2::ZERO;
                    if ui
                        .add(
                            egui::Button::new("×")
                                .frame(false)
                                .min_size(Vec2::splat(24.0)),
                        )
                        .on_hover_text("Close feedback")
                        .clicked()
                    {
                        *close = true;
                    }
                });
            });
        })
        .response;
    // `Window::title_bar(false)` gives the dialog its bespoke header, so the
    // corresponding drag affordance must also be supplied here. Keep the
    // close-button end out of this interaction rectangle.
    let drag_rect = Rect::from_min_max(response.rect.min, response.rect.max - Vec2::new(40.0, 0.0));
    ui.interact(
        drag_rect,
        ui.id().with("feedback-header-drag"),
        egui::Sense::drag(),
    )
    .drag_delta()
}

fn chooser(ui: &mut egui::Ui, p: &Palette, hub: &mut FeedbackHub) {
    for (kind, title, detail) in [
        (
            FeedbackKind::Bug,
            "Report a bug",
            "Something isn’t working as expected.",
        ),
        (
            FeedbackKind::Feature,
            "Request a feature",
            "An idea to make the app more useful.",
        ),
    ] {
        let text = RichText::new(format!("{title}\n{detail}")).size(13.0);
        if ui
            .add_sized([ui.available_width(), 68.0], egui::Button::new(text))
            .clicked()
        {
            hub.begin_form(kind);
        }
        ui.add_space(10.0);
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new("Reports are saved on your computer.\nYou choose when to share them.")
            .size(12.0)
            .color(p.sub),
    );
}

fn label(ui: &mut egui::Ui, p: &Palette, text: &str, note: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).size(12.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(note).size(12.0).color(p.sub));
        });
    });
}

fn field(
    ui: &mut egui::Ui,
    p: &Palette,
    fill: Color32,
    text: &mut String,
    hint: &str,
    height: f32,
    id: &'static str,
) {
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(text)
                    .id_salt(id)
                    .frame(false)
                    .font(egui::FontId::proportional(13.0))
                    .hint_text(hint)
                    .desired_width(f32::INFINITY)
                    .min_size(Vec2::new(0.0, height - 20.0)),
            );
        });
}

fn form(
    ui: &mut egui::Ui,
    p: &Palette,
    hub: &mut FeedbackHub,
    t: &FeedbackTokens,
    fill: Color32,
    bug: bool,
) {
    label(
        ui,
        p,
        if bug {
            "What happened?"
        } else {
            "What would you like to do?"
        },
        "Required",
    );
    ui.add_space(t.label_gap);
    field(
        ui,
        p,
        fill,
        &mut hub.description,
        if bug {
            "What did you expect, and what happened instead?"
        } else {
            "Describe the idea and how it would help."
        },
        t.description_height,
        "feedback-description",
    );
    ui.add_space(t.section_gap);
    label(ui, p, "Images", "Optional");
    ui.add_space(t.label_gap);
    let ctx = ui.ctx().clone();
    super::attachments::show(ui, p, hub, &ctx);
    ui.add_space(t.section_gap);
    label(ui, p, "Links", "Optional · one per line");
    ui.add_space(t.label_gap);
    field(
        ui,
        p,
        fill,
        &mut hub.links_text,
        "Paste supporting links…",
        t.links_height,
        "feedback-links",
    );
    if bug {
        ui.add_space(t.section_gap);
        rule(ui, p);
        ui.add_space(15.0);
        ui.checkbox(
            &mut hub.reproduce_wanted,
            RichText::new("Include reproduction steps")
                .size(12.0)
                .strong(),
        );
        if hub.reproduce_wanted {
            ui.add_space(9.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(if hub.recorded_steps.is_empty() {
                        "Show the sequence that causes the issue.".into()
                    } else {
                        format!("{} steps recorded", hub.recorded_steps.len())
                    })
                    .size(12.0)
                    .color(p.sub),
                );
                if ui
                    .button(if hub.recorded_steps.is_empty() {
                        "Record steps"
                    } else {
                        "Record again"
                    })
                    .clicked()
                {
                    hub.start_recording();
                }
            });
            let mut remove = None;
            for (i, step) in hub.recorded_steps.iter().enumerate() {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{:02}:{:02}",
                            step.at_ms / 60_000,
                            step.at_ms / 1_000 % 60
                        ))
                        .size(12.0)
                        .color(p.sub),
                    );
                    let width = (ui.available_width() - 32.0).max(20.0);
                    ui.allocate_ui(Vec2::new(width, 16.0), |ui| {
                        ui.add(egui::Label::new(RichText::new(&step.label).size(12.0)).wrap());
                    });
                    if ui.small_button("×").on_hover_text("Remove step").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                hub.recorded_steps.remove(i);
            }
        }
    }
}

#[cfg(test)]
mod tests;
