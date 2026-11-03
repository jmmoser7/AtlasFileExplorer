//! Component evidence: actual popup interaction, not ordinary-source acceptance.
use super::*;

struct Rows(bool);
impl SelectorRows for Rows {
    fn len(&self) -> usize {
        3
    }
    fn agent_datums(&self) -> bool {
        true
    }
    fn cyclic_keyboard(&self) -> bool {
        self.0
    }
    fn get(&self, index: usize) -> Option<SelectorRow<'_>> {
        Some(SelectorRow {
            label: ["Provider", "Disabled model", "API provider"][index],
            detail: None,
            submenu: (index == 2).then_some(Icon::ChevronRight),
            selected: false,
            checked: None,
            interactive: index != 1,
            danger: false,
            expanded: false,
            icon: None,
            secondary: index == 2,
        })
    }
}

struct Popup {
    ctx: egui::Context,
    open: bool,
    page: usize,
    enter_leaked: bool,
    cyclic: bool,
}
impl Popup {
    fn new() -> Self {
        Self {
            ctx: Default::default(),
            open: true,
            page: 0,
            enter_leaked: false,
            cyclic: true,
        }
    }
    fn id() -> Id {
        Id::new("keyboard-popup")
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> CapsuleMenu {
        let mut result = None;
        let _ = self.ctx.clone().run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    result = Some(rich_bare_menu(
                        ui,
                        RichBareMenu {
                            id: Self::id(),
                            scroll_id: Id::new(("page", self.page)),
                            rect: Rect::from_min_size(
                                Pos2::new(100.0, 100.0),
                                Vec2::new(120.0, 24.0),
                            ),
                            label: "Model",
                            rows: &Rows(self.cyclic),
                            open: &mut self.open,
                            zoom: 1.0,
                            theme: Palette::dark(),
                            label_danger: false,
                            label_hint: None,
                            label_detail: None,
                            trailing_caret: true,
                        },
                    ));
                    self.enter_leaked = ui.input(|input| input.key_pressed(egui::Key::Enter));
                });
            },
        );
        result.unwrap()
    }
    fn settle(&mut self) {
        for _ in 0..4 {
            self.frame(vec![]);
        }
    }
    fn key(&mut self, key: egui::Key) -> CapsuleMenu {
        let result = self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        let enter_leaked = self.enter_leaked;
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.enter_leaked = enter_leaked;
        result
    }
    fn focus(&self) -> Option<Id> {
        self.ctx.memory(|memory| memory.focused())
    }
}

#[test]
fn actual_picker_popup_focus_skips_disabled_and_visits_both_provider_targets() {
    let mut popup = Popup::new();
    popup.settle();
    assert_eq!(popup.focus(), Some(keyboard::primary(Popup::id(), 0)));
    popup.key(egui::Key::ArrowDown);
    assert_eq!(popup.focus(), Some(keyboard::primary(Popup::id(), 2)));
    popup.key(egui::Key::ArrowDown);
    assert_eq!(popup.focus(), Some(Popup::id().with(("secondary", 2usize))));
    popup.key(egui::Key::ArrowDown);
    assert_eq!(popup.focus(), Some(keyboard::primary(Popup::id(), 0)));
    popup.key(egui::Key::ArrowUp);
    let picked = popup.key(egui::Key::Enter);
    assert_eq!(picked.secondary_picked, Some(2));
    assert_eq!(picked.picked, None);
    assert!(
        !popup.enter_leaked,
        "the composer never receives picker activation"
    );
}

#[test]
fn popup_page_render_resets_first_focus_and_space_activates_primary() {
    let mut popup = Popup::new();
    popup.settle();
    popup.key(egui::Key::ArrowDown);
    popup.page = 1;
    popup.settle();
    assert_eq!(popup.focus(), Some(keyboard::primary(Popup::id(), 0)));
    assert_eq!(popup.key(egui::Key::Space).picked, Some(0));
    popup.open = true;
    popup.settle();
    popup.key(egui::Key::Escape);
    assert!(!popup.open);
    assert_eq!(popup.focus(), Some(Popup::id()));
}

#[test]
fn non_chat_model_picker_keeps_first_focus_without_cardinal_navigation() {
    let mut popup = Popup::new();
    popup.cyclic = false;
    popup.settle();
    let first = Some(keyboard::primary(Popup::id(), 0));
    for key in [
        egui::Key::ArrowDown,
        egui::Key::ArrowUp,
        egui::Key::ArrowLeft,
        egui::Key::ArrowRight,
    ] {
        popup.key(key);
        assert_eq!(
            popup.focus(),
            first,
            "non-Chat buttons do not acquire native spatial arrow navigation"
        );
    }
    popup.key(egui::Key::Tab);
    assert_eq!(popup.focus(), Some(keyboard::primary(Popup::id(), 2)));
    assert_eq!(popup.key(egui::Key::Enter).picked, Some(2));
    assert!(!popup.enter_leaked);
}
