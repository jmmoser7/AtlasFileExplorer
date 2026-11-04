//! Presentation selector and creation controls.
use super::*;

impl SlateApp {
    pub(super) fn deck_selector(&mut self, ui: &mut Ui, menu: Rect, z: f32, bounds: Rect) {
        let p = ui.painter().clone();
        let theme = self.palette();
        p.rect(
            menu,
            (9.0 * z).round() as u8,
            theme.card,
            Stroke::new(z, theme.border),
            StrokeKind::Inside,
        );
        let row = Rect::from_min_size(menu.min + vec2(5.0, 5.0) * z, vec2(192.0, 28.0) * z);
        let read_only = self.tab().read_only;
        if let Some(name) = &mut self.frame_presentation.name {
            let edit = ui.put(
                row,
                egui::TextEdit::singleline(name)
                    .hint_text("Presentation name")
                    .char_limit(80)
                    .font(canvas_scale::font(12.0, z)),
            );
            if ui.input(|i| !i.pointer.any_pressed()) && !edit.has_focus() {
                edit.request_focus();
            }
            let create = chrome::control(
                ui,
                row.translate(vec2(0.0, 30.0) * z),
                Id::new("frame-deck-create"),
                "Create",
                z,
                theme,
                false,
            )
            .clicked()
                || ui.input(|i| i.key_pressed(Key::Enter));
            if create && !name.trim().is_empty() && !read_only {
                let name = name.trim().to_owned();
                let tab = self.tab().id;
                let id = self
                    .doc()
                    .scene
                    .presentations()
                    .iter()
                    .map(|d| d.id)
                    .max()
                    .unwrap_or(0)
                    + 1;
                if let Some(cmd) = self.doc().scene.presentation_command(|d| {
                    d.push(Presentation {
                        id,
                        name,
                        frames: Vec::new(),
                    })
                }) {
                    if self.commit_scene(vec![cmd]) {
                        self.frame_presentation.active.insert(tab, id);
                    }
                }
                self.frame_presentation.selector = false;
                self.frame_presentation.name = None;
                self.frame_presentation.chosen.clear();
            }
            return;
        }
        let active = self.active_frame_deck();
        let decks = self
            .doc()
            .scene
            .presentations()
            .iter()
            .map(|d| (Some(d.id), d.name.clone()))
            .collect::<Vec<_>>();
        let decks = if decks.is_empty() {
            vec![(None, "Board order".into())]
        } else {
            decks
        };
        for (i, (id, name)) in decks.iter().enumerate() {
            let r = row.translate(vec2(0.0, 30.0 * i as f32) * z);
            let hit = chrome::control(
                ui,
                r,
                Id::new(("frame-deck-option", id)),
                name,
                z,
                theme,
                *id == active,
            );
            if hit.hovered() {
                paint_map(&p, menu, bounds, &self.doc().scene, *id, z, theme);
            }
            if hit.clicked() {
                let tab = self.tab().id;
                if let Some(id) = id {
                    self.frame_presentation.active.insert(tab, *id);
                }
                self.frame_presentation.selector = false;
                self.frame_presentation.chosen.clear();
                self.frame_presentation.anchor = None;
                self.frame_presentation.reveal = true;
            }
        }
        let new = row.translate(vec2(0.0, 30.0 * decks.len() as f32) * z);
        if chrome::control(
            ui,
            new,
            Id::new("frame-deck-new"),
            "+ New presentation",
            z,
            theme,
            false,
        )
        .clicked()
            && !read_only
        {
            self.frame_presentation.name = Some(String::new());
        }
    }
}
