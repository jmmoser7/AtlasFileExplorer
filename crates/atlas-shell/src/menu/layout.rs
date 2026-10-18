//! Measured menu geometry without changing the global chrome tokens.
use super::*;

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub menu_width: f32,
    pub row_height: f32,
    pub icon_size: f32,
    pub panel_inset: f32,
    pub divider_height: f32,
    pub corner_radius: f32,
}

impl Metrics {
    fn tokens(self) -> MenuTokens {
        MenuTokens {
            min_width: self.menu_width - 2.0 * self.panel_inset,
            row_height: self.row_height,
            row_gap: 0.0,
            icon_size: self.icon_size,
            icon_gap: 13.0,
            icon_stroke: 1.5,
            row_pad_x: 10.0,
            text_size: 14.0,
            letter_spacing: 0.0,
            panel_padding: self.panel_inset,
            corner_radius: self.corner_radius,
            hover_radius: 3.0,
            divider_inset: 10.0,
            divider_gap: self.divider_height - 1.0,
            divider_thickness: 1.0,
            ..tokens()
        }
    }

    pub fn frame(self, dark: bool) -> Frame {
        frame_with_tokens(dark, &self.tokens())
    }

    pub fn prepare(self, ui: &mut Ui, dark: bool) {
        let tokens = self.tokens();
        prepare_with_tokens(ui, dark, &tokens);
        ui.set_width(tokens.min_width);
        ui.spacing_mut().item_spacing = Vec2::ZERO;
    }

    /// Keep egui's menu state and dismissal, with measured outer chrome.
    pub fn context_menu(
        self,
        response: &egui::Response,
        dark: bool,
        add_contents: impl FnOnce(&mut Ui),
    ) -> Option<egui::InnerResponse<()>> {
        let previous = response.ctx.style();
        response.ctx.style_mut(|style| {
            style.spacing.menu_margin = Margin::same(self.panel_inset.clamp(0.0, 127.0) as i8);
            style.visuals.menu_corner_radius =
                CornerRadius::same(self.corner_radius.clamp(0.0, 255.0) as u8);
        });
        let shown = response.context_menu(|ui| {
            self.prepare(ui, dark);
            add_contents(ui);
        });
        response.ctx.set_style(previous);
        shown
    }

    pub fn row(self, ui: &mut Ui, mut spec: Row<'_>, dark: bool) -> egui::Response {
        spec.truncate = true;
        rows::row(ui, spec, dark, &self.tokens())
    }

    pub fn separator(self, ui: &mut Ui, dark: bool) {
        separator_with_tokens(ui, dark, &self.tokens());
    }

    pub fn heading(self, ui: &mut Ui, text: impl AsRef<str>, dark: bool) {
        let mut tokens = self.tokens();
        tokens.row_height = 26.0 / 0.72;
        heading_with_tokens(ui, text, dark, &tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_menu_does_not_modify_global_tokens() {
        let metrics = Metrics {
            menu_width: 256.0,
            row_height: 33.0,
            icon_size: 18.0,
            panel_inset: 8.0,
            divider_height: 12.0,
            corner_radius: 10.0,
        };
        let before = tokens();
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                metrics.frame(true).show(ui, |ui| {
                    metrics.prepare(ui, true);
                    let first = metrics.row(ui, Row::new(MenuIcon::Open, "Open file"), true);
                    let second = metrics.row(ui, Row::new(MenuIcon::Copy, "Copy"), true);
                    assert_eq!(first.rect.height(), 33.0);
                    assert_eq!(first.rect.width(), 240.0);
                    assert_eq!(second.rect.top(), first.rect.bottom());
                });
            });
        });
        assert_eq!(tokens().row_height, before.row_height);
        assert_eq!(tokens().icon_size, before.icon_size);
    }

    #[test]
    fn measured_context_menu_uses_outer_metrics_without_leaking_style() {
        let metrics = Metrics {
            menu_width: 256.0,
            row_height: 33.0,
            icon_size: 18.0,
            panel_inset: 8.0,
            divider_height: 12.0,
            corner_radius: 10.0,
        };
        let ctx = egui::Context::default();
        apply_style(&ctx, true);
        let previous = ctx.style();
        let mut at = Pos2::ZERO;
        let mut menu_rect = None;
        for frame in 0..5 {
            let events = if frame == 2 || frame == 3 {
                vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Secondary,
                        pressed: frame == 2,
                        modifiers: Default::default(),
                    },
                ]
            } else {
                vec![]
            };
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(800.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (_, response) =
                            ui.allocate_exact_size(Vec2::new(100.0, 40.0), Sense::click());
                        at = response.rect.center();
                        if let Some(popup) = metrics.context_menu(&response, true, |ui| {
                            assert_eq!(ui.style().spacing.menu_margin, Margin::same(8));
                            assert_eq!(ui.visuals().menu_corner_radius, CornerRadius::same(10));
                            metrics.row(ui, Row::new(MenuIcon::View, "Visible"), true);
                            if frame == 4 {
                                ui.close_menu();
                            }
                        }) {
                            menu_rect = Some(popup.response.rect);
                        }
                    });
                },
            );
            assert_eq!(
                ctx.style().spacing.menu_margin,
                previous.spacing.menu_margin
            );
            assert_eq!(
                ctx.style().visuals.menu_corner_radius,
                previous.visuals.menu_corner_radius
            );
        }
        assert_eq!(menu_rect.expect("native context menu").width(), 256.0);
    }
}
