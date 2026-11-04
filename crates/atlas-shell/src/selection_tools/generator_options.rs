//! One compact ellipsis menu for generator-owned options.
use super::super::menu_selector::{
    rich_bare_menu, Presentation, RichBareMenu, SelectorRow, SelectorRows,
};
use super::*;
#[path = "generator_options/custom.rs"]
mod custom;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::selection_tools) enum OptionsPage {
    #[default]
    Root,
    Aspect,
    Mode,
    Seed,
}

pub(in crate::selection_tools) struct OptionsData<'a> {
    pub preserve: Option<bool>,
    pub aspects: &'a [&'a str],
    pub aspect: usize,
    pub custom_aspect: Option<[u16; 2]>,
    pub custom_supported: bool,
    pub live: Option<bool>,
    pub seed_locked: Option<bool>,
    pub seed_supported: bool,
    pub legacy: &'a [&'a str],
    pub legacy_selected: usize,
}

impl Default for OptionsData<'_> {
    fn default() -> Self {
        Self {
            preserve: None,
            aspects: &[],
            aspect: 0,
            custom_aspect: None,
            custom_supported: false,
            live: None,
            seed_locked: None,
            seed_supported: false,
            legacy: &[],
            legacy_selected: 0,
        }
    }
}

#[derive(Default)]
pub(in crate::selection_tools) struct OptionsResponse {
    pub menu: Option<menu_selector::CapsuleMenu>,
    pub page: OptionsPage,
    pub preserve: Option<bool>,
    pub aspect: Option<usize>,
    pub custom_aspect: Option<[u16; 2]>,
    pub live: Option<bool>,
    pub seed_locked: Option<bool>,
    pub legacy: Option<usize>,
}

#[derive(Clone, Copy)]
enum Action {
    Page(OptionsPage),
    Preserve(bool),
    Aspect(usize),
    Custom,
    Live(bool),
    Seed(bool),
    Legacy(usize),
}

struct Rows<'a> {
    data: OptionsData<'a>,
    page: OptionsPage,
    custom_label: String,
}
impl Rows<'_> {
    fn root_len(&self) -> usize {
        usize::from(!self.data.aspects.is_empty())
            + usize::from(self.data.live.is_some())
            + 1
            + usize::from(self.data.seed_locked.is_some())
            + if self.data.preserve.is_none() {
                self.data.legacy.len()
            } else {
                0
            }
    }
    fn root_at(&self, mut index: usize) -> Option<(&str, Option<&str>, Action)> {
        if !self.data.aspects.is_empty() {
            if index == 0 {
                return Some((
                    "Aspect Ratio",
                    if self.data.aspect == self.data.aspects.len()
                        && self.data.custom_aspect.is_some()
                    {
                        Some(self.custom_label.as_str())
                    } else {
                        self.data.aspects.get(self.data.aspect).copied()
                    },
                    Action::Page(OptionsPage::Aspect),
                ));
            }
            index -= 1;
        }
        if let Some(live) = self.data.live {
            if index == 0 {
                return Some((
                    "Mode",
                    Some(if live {
                        "Live Mode"
                    } else {
                        "Single Image Mode"
                    }),
                    Action::Page(OptionsPage::Mode),
                ));
            }
            index -= 1;
        }
        if self.data.seed_locked.is_some() {
            if index == 0 {
                return Some(("Settings", None, Action::Page(OptionsPage::Seed)));
            }
            index -= 1;
        }
        if index == 0 {
            return Some((
                "Preserve model history",
                None,
                Action::Preserve(!self.data.preserve.unwrap_or(false)),
            ));
        }
        index -= 1;
        if self.data.preserve.is_none() {
            return self
                .data
                .legacy
                .get(index)
                .map(|label| (*label, None, Action::Legacy(index)));
        }
        None
    }
    fn expanded(&self) -> Option<usize> {
        (0..self.root_len()).find(|&i| {
            self.root_at(i)
                .is_some_and(|(_, _, a)| matches!(a,Action::Page(page) if page==self.page))
        })
    }
    fn row_action(&self, index: usize) -> Option<(SelectorRow<'_>, Action)> {
        let expanded = self.expanded();
        let prefix = if self.page == OptionsPage::Root {
            self.root_len()
        } else {
            expanded.map_or(self.root_len(), |i| i + 1)
        };
        if index < prefix {
            let (label, detail, action) = self.root_at(index)?;
            let is_page = matches!(action, Action::Page(_));
            let selected = (matches!(action, Action::Preserve(_))
                && self.data.preserve == Some(true))
                || matches!(action,Action::Legacy(index) if index == self.data.legacy_selected);
            let interactive =
                !matches!(action, Action::Preserve(_)) || self.data.preserve.is_some();
            let icon = if self.data.aspects.is_empty() {
                None
            } else {
                Some(match action {
                    Action::Page(OptionsPage::Aspect) => Icon::MenuCrop,
                    Action::Page(OptionsPage::Mode) => Icon::MenuMode,
                    Action::Page(OptionsPage::Seed) => Icon::MenuSettings,
                    Action::Preserve(_) => Icon::MenuLayers,
                    _ => Icon::MenuImage,
                })
            };
            let action = match action {
                Action::Page(page) if page == self.page => Action::Page(OptionsPage::Root),
                other => other,
            };
            return Some((
                SelectorRow {
                    label,
                    detail,
                    submenu: is_page.then_some(Icon::ChevronRight),
                    selected,
                    checked: matches!(action, Action::Preserve(_))
                        .then_some(self.data.preserve == Some(true)),
                    interactive,
                    danger: false,
                    expanded: is_page && expanded == Some(index),
                    icon,
                    secondary: false,
                },
                action,
            ));
        }
        let choice = index - prefix;
        let (label, selected, action) = match self.page {
            OptionsPage::Aspect if choice == self.data.aspects.len() => (
                "Custom…",
                self.data.custom_aspect.is_some() && self.data.aspect == self.data.aspects.len(),
                Action::Custom,
            ),
            OptionsPage::Aspect => (
                *self.data.aspects.get(choice)?,
                choice == self.data.aspect,
                Action::Aspect(choice),
            ),
            OptionsPage::Mode => (
                *["Single Image Mode", "Live Mode"].get(choice)?,
                choice == usize::from(self.data.live?),
                Action::Live(choice == 1),
            ),
            OptionsPage::Seed => (
                *["Seed unlocked", "Seed locked"].get(choice)?,
                choice == usize::from(self.data.seed_locked?),
                Action::Seed(choice == 1),
            ),
            OptionsPage::Root => return None,
        };
        Some((
            SelectorRow {
                label,
                detail: None,
                submenu: None,
                selected,
                checked: Some(selected),
                interactive: match action {
                    Action::Custom => self.data.custom_supported,
                    Action::Seed(true) => self.data.seed_supported,
                    _ => true,
                },
                danger: false,
                expanded: false,
                icon: Some(Icon::MenuImage),
                secondary: false,
            },
            action,
        ))
    }
}
impl SelectorRows for Rows<'_> {
    fn hint(&self, index: usize) -> Option<&str> {
        self.row_action(index).and_then(|(_, action)| match action {
            Action::Seed(true) if !self.data.seed_supported => {
                Some("This provider does not expose a repeatable seed")
            }
            Action::Custom => Some(if self.data.custom_supported {
                "Custom ratio for Comfy generation; 1:4 to 4:1, dimensions rounded to 8px"
            } else {
                "Custom aspect requires Comfy generation without a source image or model"
            }),
            Action::Preserve(_) => Some(if self.data.aspects.is_empty() {
                "Keep the dialogue here; its history accompanies a downstream wire."
            } else {
                "Keep conversation in this image; dragging its output carries history downstream."
            }),
            _ => None,
        })
    }
    fn presentation(&self) -> Presentation {
        if self.data.aspects.is_empty() {
            Presentation::ElementOptions
        } else {
            Presentation::ImageOptions
        }
    }
    fn divider_before(&self, index: usize) -> bool {
        match self.page {
            OptionsPage::Root => {
                !self.data.aspects.is_empty()
                    && self
                        .root_at(index)
                        .is_some_and(|(_, _, a)| matches!(a, Action::Preserve(_)))
            }
            _ => self
                .expanded()
                .is_some_and(|expanded| index == expanded + 1),
        }
    }
    fn len(&self) -> usize {
        if self.page == OptionsPage::Root {
            return self.root_len();
        }
        self.expanded().map_or(self.root_len(), |i| i + 1)
            + match self.page {
                OptionsPage::Aspect => self.data.aspects.len() + 1,
                OptionsPage::Mode | OptionsPage::Seed => 2,
                OptionsPage::Root => 0,
            }
    }
    fn get(&self, index: usize) -> Option<SelectorRow<'_>> {
        self.row_action(index).map(|(r, _)| r)
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::selection_tools) fn paint(
    ui: &mut egui::Ui,
    rect: Rect,
    id: Id,
    open: &mut bool,
    page: OptionsPage,
    data: OptionsData<'_>,
    zoom: f32,
    theme: Palette,
) -> OptionsResponse {
    let custom_label = data
        .custom_aspect
        .map_or(String::new(), |[width, height]| format!("{width}:{height}"));
    let rows = Rows {
        data,
        page,
        custom_label,
    };
    let custom_seed = rows.data.custom_aspect.unwrap_or([16, 9]);
    let menu = rich_bare_menu(
        ui,
        RichBareMenu {
            id,
            scroll_id: id.with(page as u8),
            rect,
            label: "",
            rows: &rows,
            open,
            zoom,
            theme,
            label_danger: false,
            label_hint: Some("Generator options"),
            label_detail: None,
            trailing_caret: false,
        },
    );
    // Keep the existing menu hit target and state, but paint native geometry:
    // the workbook font need not contain an ellipsis glyph.
    if let Some(response) = ui.ctx().read_response(id) {
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                ui.is_enabled(),
                "Generator options",
            )
        });
        let size = rect.width().min(rect.height());
        let ink = if response.hovered() || *open {
            theme.ink
        } else {
            theme.sub
        };
        crate::icons::paint_more(ui.painter(), rect.center(), size * 0.3, size * 0.07, ink);
    }
    let mut out = OptionsResponse {
        page,
        ..Default::default()
    };
    if let Some((_, action)) = menu.picked.and_then(|index| rows.row_action(index)) {
        match action {
            Action::Page(p) => out.page = p,
            Action::Preserve(v) => out.preserve = Some(v),
            Action::Aspect(v) => {
                out.aspect = Some(v);
                out.page = OptionsPage::Root
            }
            Action::Custom => custom::open(ui.ctx(), id, custom_seed),
            Action::Live(v) => {
                out.live = Some(v);
                out.page = OptionsPage::Root
            }
            Action::Seed(v) => {
                out.seed_locked = Some(v);
                out.page = OptionsPage::Root
            }
            Action::Legacy(v) => out.legacy = Some(v),
        }
        if matches!(action, Action::Page(_) | Action::Preserve(_)) {
            *open = true;
        }
    }
    out.menu = Some(menu);
    out.custom_aspect = custom::paint(ui.ctx(), id, theme);
    if !*open {
        out.page = OptionsPage::Root;
    }
    out
}

#[cfg(test)]
mod check_tests {
    use super::*;
    struct Toggle {
        ctx: egui::Context,
        on: bool,
        open: bool,
    }
    impl Toggle {
        fn frame(&mut self, events: Vec<egui::Event>) -> (Rect, egui::FullOutput) {
            let mut menu = OptionsResponse::default();
            let output = self.ctx.clone().run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
                    events,
                    time: Some(self.ctx.input(|i| i.time) + 1.0 / 30.0),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        menu = paint(
                            ui,
                            Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(32.0, 24.0)),
                            Id::new("preserve-check-test"),
                            &mut self.open,
                            OptionsPage::Root,
                            OptionsData {
                                preserve: Some(self.on),
                                ..Default::default()
                            },
                            1.0,
                            Palette::light(),
                        );
                    });
                },
            );
            if let Some(on) = menu.preserve {
                self.on = on;
            }
            (menu.menu.unwrap().popup.unwrap(), output)
        }
        fn click(&mut self, at: Pos2) {
            for pressed in [true, false] {
                self.frame(vec![
                    egui::Event::PointerMoved(at),
                    egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
        }
    }
    fn has_leading_check(output: &egui::FullOutput, popup: Rect) -> bool {
        let check = Rect::from_min_max(
            Pos2::new(popup.left() + 7.0, popup.top()),
            Pos2::new(popup.left() + 30.0, popup.bottom()),
        );
        output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "✓" && check.contains_rect(text.visual_bounding_rect())))
    }
    #[test]
    fn preserve_toggle_paints_check_after_click_and_keeps_the_options_open() {
        let mut menu = Toggle {
            ctx: egui::Context::default(),
            on: false,
            open: true,
        };
        menu.frame(vec![]);
        let (popup, off) = menu.frame(vec![]);
        assert!(!has_leading_check(&off, popup));
        menu.click(popup.center());
        let (popup, on) = menu.frame(vec![]);
        assert!(menu.on);
        assert!(menu.open);
        assert!(has_leading_check(&on, popup));
        menu.click(popup.center());
        let (popup, off) = menu.frame(vec![]);
        assert!(!menu.on);
        assert!(menu.open);
        assert!(!has_leading_check(&off, popup));
    }
}

#[cfg(test)]
#[path = "generator_options/header_glyph_tests.rs"]
mod header_glyph_tests;
