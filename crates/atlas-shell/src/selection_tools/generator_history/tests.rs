use super::*;

struct History {
    ctx: egui::Context,
    width: f32,
    zoom: f32,
    selected: usize,
    len: usize,
}

impl History {
    fn row(&self) -> Rect {
        Rect::from_min_size(
            Pos2::new(80.0, 80.0),
            Vec2::new(self.width, HEIGHT) * self.zoom,
        )
    }

    fn layout(&self) -> HistoryLayout {
        let row = self.row();
        HistoryLayout::new(
            row,
            row.center().y,
            self.len,
            Some(self.selected),
            self.zoom,
        )
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> GeneratorResponse {
        let row = self.row();
        let mut out = GeneratorResponse::default();
        let _ = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(4000.0, 1600.0))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let items = vec![
                        GeneratorHistoryItem {
                            label: "Preserved output".into(),
                            thumb: None
                        };
                        self.len
                    ];
                    out = history_row(
                        ui,
                        row,
                        row.center().y,
                        &items,
                        Some(self.selected),
                        self.zoom,
                        Palette::dark(),
                    );
                    out.submit = generator_play_control(
                        ui,
                        Rect::from_min_size(
                            Pos2::new(row.right() + 6.0 * self.zoom, row.top()),
                            Vec2::splat(20.0 * self.zoom),
                        ),
                        false,
                        false,
                        ui.id().with("adjacent-play"),
                        self.zoom,
                        Palette::dark(),
                    );
                });
            },
        );
        out
    }

    fn button(point: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click(&mut self, point: Pos2) -> GeneratorResponse {
        self.frame(vec![egui::Event::PointerMoved(point)]);
        let press = self.frame(vec![Self::button(point, true)]);
        assert!(press.clicked_history.is_none() && !press.submit);
        let out = self.frame(vec![Self::button(point, false)]);
        if let Some(index) = out.clicked_history {
            self.selected = index;
            // The app repaints after selection changes. Register the newly
            // centered chip identities before testing their next interaction.
            self.frame(vec![]);
        }
        out
    }
}

#[test]
fn output_navigation_repeatedly_reaches_both_ends_at_each_width_and_zoom() {
    for width in [56.0, 64.0, 96.0, 240.0, 900.0] {
        for zoom in [0.5, 1.0, 2.0] {
            let mut history = History {
                ctx: egui::Context::default(),
                width,
                zoom,
                selected: 0,
                len: 20,
            };
            for destination in [19, 0, 19, 0] {
                for _ in 0..40 {
                    if history.selected == destination {
                        break;
                    }
                    let layout = history.layout();
                    let (point, target) = if destination > history.selected {
                        let end = layout.start + layout.count;
                        if end < history.len {
                            (layout.later.unwrap().center(), end)
                        } else {
                            (
                                layout.chip(destination, history.row().center().y).center(),
                                destination,
                            )
                        }
                    } else if layout.start > 0 {
                        (layout.earlier.unwrap().center(), layout.start - 1)
                    } else {
                        (
                            layout.chip(destination, history.row().center().y).center(),
                            destination,
                        )
                    };
                    let out = history.click(point);
                    assert_eq!(
                        out.clicked_history,
                        Some(target),
                        "width={width}, zoom={zoom}"
                    );
                    assert!(out.drag_started_history.is_none() && !out.submit);
                }
                assert_eq!(
                    history.selected, destination,
                    "every preserved output stays reachable"
                );
                let layout = history.layout();
                let endpoint = if destination == 0 {
                    layout.earlier
                } else {
                    layout.later
                };
                if let Some(disabled) = endpoint {
                    let out = history.click(disabled.center());
                    assert!(
                        out.clicked_history.is_none()
                            && out.drag_started_history.is_none()
                            && !out.submit,
                        "an exhausted endpoint has no action: width={width}, zoom={zoom}, destination={destination}, clicked={:?}, drag={:?}, submit={}",
                        out.clicked_history, out.drag_started_history, out.submit
                    );
                    assert_eq!(history.selected, destination);
                }
            }
        }
    }
}

#[test]
fn history_controls_are_isolated_from_chips_and_adjacent_play_and_keep_dragging() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut history = History {
            ctx: egui::Context::default(),
            width: 96.0,
            zoom,
            selected: 10,
            len: 20,
        };
        let layout = history.layout();
        let earlier = layout.earlier.unwrap();
        let later = layout.later.unwrap();
        for index in layout.start..layout.start + layout.count {
            let chip = layout.chip(index, history.row().center().y);
            assert!(!chip.intersects(earlier) && !chip.intersects(later));
        }
        let out = history.click(earlier.center());
        assert_eq!(out.clicked_history, layout.start.checked_sub(1));
        assert!(out.hovered_history.is_none() && out.drag_started_history.is_none() && !out.submit);
        let layout = history.layout();
        let chip_index = layout.start;
        let point = layout.chip(chip_index, history.row().center().y).center();
        assert_eq!(
            history
                .frame(vec![egui::Event::PointerMoved(point)])
                .hovered_history,
            Some(chip_index)
        );
        history.frame(vec![History::button(point, true)]);
        let moved = point + Vec2::new(24.0 * zoom, 0.0);
        let out = history.frame(vec![egui::Event::PointerMoved(moved)]);
        assert_eq!(out.drag_started_history, Some(chip_index));
        assert_eq!(out.history_drag_position, Some(moved));
        assert!(out.clicked_history.is_none() && !out.submit);
        history.frame(vec![History::button(moved, false)]);
        let play = Pos2::new(
            history.row().right() + 16.0 * zoom,
            history.row().top() + 10.0 * zoom,
        );
        let out = history.click(play);
        assert!(out.submit && out.clicked_history.is_none() && out.drag_started_history.is_none());
    }
}

#[test]
fn all_fit_history_stays_right_aligned_without_navigation_targets() {
    for zoom in [0.5, 1.0, 2.0] {
        let row = Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(240.0, HEIGHT) * zoom);
        let layout = HistoryLayout::new(row, row.center().y, 3, Some(1), zoom);
        assert!(layout.earlier.is_none() && layout.later.is_none());
        assert_eq!(layout.chip(2, row.center().y).right(), row.right());
        assert_eq!(layout.diameter, HEIGHT * zoom);
    }
    assert!(fits(26.0, 1));
    assert!(fits(55.0, 2));
    assert!(!fits(55.0, 3));
    assert!(fits(64.0, 20));
}

#[test]
fn a_large_indexed_album_materializes_only_the_visible_global_indices() {
    struct Album(std::cell::RefCell<Vec<usize>>);
    impl GeneratorHistory for Album {
        fn len(&self) -> usize {
            1_000_000
        }
        fn get(&self, index: usize) -> Option<GeneratorHistoryItem<'_>> {
            if index >= self.len() {
                return None;
            }
            self.0.borrow_mut().push(index);
            Some(GeneratorHistoryItem {
                label: "Borrowed output".into(),
                thumb: None,
            })
        }
    }
    for width in [64.0, 96.0, 900.0] {
        for zoom in [0.5, 1.0, 2.0] {
            let ctx = egui::Context::default();
            let album = Album(Default::default());
            let selected = 712_345;
            let row = Rect::from_min_size(Pos2::new(80.0, 80.0), Vec2::new(width, HEIGHT) * zoom);
            let expected =
                HistoryLayout::new(row, row.center().y, album.len(), Some(selected), zoom);
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(4000.0, 1600.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        history_row(
                            ui,
                            row,
                            row.center().y,
                            &album,
                            Some(selected),
                            zoom,
                            Palette::dark(),
                        );
                    });
                },
            );
            assert_eq!(
                *album.0.borrow(),
                (expected.start..expected.start + expected.count).collect::<Vec<_>>()
            );
            assert!(
                album.0.borrow().len() < 32,
                "album size does not become frame work"
            );
        }
    }
}
