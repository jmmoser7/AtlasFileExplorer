//! Context actions adapt to the existing selector popup without a trigger widget.
use super::*;

#[derive(Clone, Copy)]
pub enum StudioActionStyle {
    Image,
    Element,
    Plain,
}

pub struct StudioActionRow<'a> {
    pub label: &'a str,
    pub icon: Option<crate::menu::MenuIcon>,
    pub hint: Option<&'a str>,
    pub checked: Option<bool>,
    pub interactive: bool,
    pub danger: bool,
    pub divider_before: bool,
}

pub struct StudioActionResponse {
    pub picked: Option<usize>,
    pub popup: Option<Rect>,
    pub dismissed_outside: bool,
}

struct Rows<'a> {
    rows: &'a [StudioActionRow<'a>],
    style: StudioActionStyle,
}
impl SelectorRows for Rows<'_> {
    fn hint(&self, index: usize) -> Option<&str> {
        self.rows.get(index).and_then(|row| row.hint)
    }
    fn action_icon(&self, index: usize) -> Option<crate::menu::MenuIcon> {
        self.rows.get(index).and_then(|row| row.icon)
    }
    fn len(&self) -> usize {
        self.rows.len()
    }
    fn get(&self, index: usize) -> Option<SelectorRow<'_>> {
        let row = self.rows.get(index)?;
        Some(SelectorRow {
            label: row.label,
            detail: None,
            submenu: None,
            selected: false,
            checked: row.checked,
            interactive: row.interactive,
            danger: row.danger,
            expanded: false,
            icon: None,
            secondary: false,
        })
    }
    fn divider_before(&self, index: usize) -> bool {
        self.rows.get(index).is_some_and(|row| row.divider_before)
    }
    fn presentation(&self) -> Presentation {
        match self.style {
            StudioActionStyle::Image => Presentation::ImageOptions,
            StudioActionStyle::Element => Presentation::ElementOptions,
            StudioActionStyle::Plain => Presentation::PlainActions,
        }
    }
}

/// Screen-space anchor only: no hidden button, trigger painter or second popup state.
#[allow(clippy::too_many_arguments)]
pub fn studio_action_popup(
    ui: &mut egui::Ui,
    id: Id,
    at: Pos2,
    rows: &[StudioActionRow<'_>],
    open: &mut bool,
    style: StudioActionStyle,
    theme: Palette,
) -> StudioActionResponse {
    let out = CapsuleMenu {
        picked: None,
        secondary_picked: None,
        popup: None,
        opened: false,
        dismissed_outside: false,
    };
    let out = if *open {
        popup_selector(
            ui,
            id,
            Rect::from_min_size(at, Vec2::ZERO),
            &[],
            usize::MAX,
            open,
            1.0,
            theme,
            Some(&Rows { rows, style }),
            None,
            None,
            out,
        )
    } else {
        out
    };
    StudioActionResponse {
        picked: out.picked,
        popup: out.popup,
        dismissed_outside: out.dismissed_outside,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_popup_has_no_trigger_and_uses_the_existing_rows_for_actual_clicks() {
        for style in [
            StudioActionStyle::Image,
            StudioActionStyle::Element,
            StudioActionStyle::Plain,
        ] {
            let ctx = egui::Context::default();
            let mut open = true;
            let rows = [
                StudioActionRow {
                    label: "Graft",
                    icon: None,
                    hint: None,
                    checked: Some(false),
                    interactive: true,
                    danger: false,
                    divider_before: false,
                },
                StudioActionRow {
                    label: "Unavailable",
                    icon: None,
                    hint: None,
                    checked: None,
                    interactive: false,
                    danger: false,
                    divider_before: false,
                },
            ];
            let anchor = Pos2::new(100.0, 100.0);
            let mut frame = |events| {
                let mut out = None;
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            out = Some(studio_action_popup(
                                ui,
                                Id::new("port-popup"),
                                anchor,
                                &rows,
                                &mut open,
                                style,
                                Palette::dark(),
                            ));
                        });
                    },
                );
                out.unwrap()
            };
            for _ in 0..4 {
                frame(vec![]);
            }
            let popup = frame(vec![]).popup.unwrap();
            assert!((popup.width() - 232.0).abs() < 0.1);
            let point = popup.min + Vec2::new(80.0, 18.5);
            frame(vec![egui::Event::PointerMoved(point)]);
            frame(vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }]);
            let chosen = frame(vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }]);
            assert_eq!(chosen.picked, Some(0));
            drop(frame);
            assert!(!open);
            assert!(
                ctx.read_response(Id::new("port-popup")).is_none(),
                "anchor geometry is never a hidden trigger widget"
            );
        }
    }
}
