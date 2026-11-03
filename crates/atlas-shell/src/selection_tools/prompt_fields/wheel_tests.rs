use super::*;

struct Field {
    ctx: egui::Context,
    text: String,
    transform: egui::emath::TSTransform,
    clip: Rect,
    visible: bool,
}
impl Field {
    fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            text: String::new(),
            transform: Default::default(),
            clip: Rect::from_min_max(Pos2::ZERO, Pos2::new(460.0, 260.0)),
            visible: true,
        }
    }
    fn id() -> Id {
        Id::new("actual-wheel-textedit")
    }
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, Id::new("prompt-wheel-area"))
    }
    fn frame(&mut self, pointer: Pos2, events: Vec<egui::Event>) -> bool {
        let mut owned = false;
        let ctx = self.ctx.clone();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
                events: std::iter::once(egui::Event::PointerMoved(pointer))
                    .chain(events)
                    .collect(),
                ..Default::default()
            },
            |ctx| {
                owned = crate::menu_wheel::wheel_owned(ctx); // same pre-paint ordering as canvas navigation
                if !self.visible {
                    return;
                }
                ctx.set_transform_layer(Self::layer(), self.transform);
                egui::Area::new(Self::layer().id)
                    .order(egui::Order::Middle)
                    .fixed_pos(Pos2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_min_size(Vec2::new(460.0, 260.0));
                        ui.set_clip_rect(self.clip);
                        image_prompt_field(
                            ui,
                            Self::id(),
                            Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(220.0, 52.0)),
                            1.0,
                            &mut self.text,
                            "Message",
                            true,
                            false,
                            Palette::dark(),
                        );
                    });
            },
        );
        owned
    }
    fn click_field(&mut self) -> Pos2 {
        for _ in 0..3 {
            self.frame(Pos2::ZERO, vec![]);
        }
        let response = self
            .ctx
            .read_response(Self::id())
            .expect("actual shared TextEdit");
        let viewport = Rect::from_min_size(Pos2::new(40.0, 40.0), Vec2::new(220.0, 52.0));
        let visible = response
            .interact_rect
            .intersect(viewport)
            .intersect(self.clip);
        let transform = self
            .ctx
            .layer_transform_to_global(response.layer_id)
            .unwrap_or_default();
        let point = transform * visible.center();
        let before = self.ctx.memory(|memory| memory.focused());
        assert!(response.enabled() && visible.is_positive(),
            "settled editable TextEdit required: enabled={}, rect={:?}, interact={:?}, viewport={viewport:?}, clip={:?}, visible={visible:?}, focus={before:?}, lines={}",
            response.enabled(), response.rect, response.interact_rect, self.clip, self.text.lines().count());
        assert_eq!(self.ctx.layer_id_at(point), Some(response.layer_id),
            "actual visible click must target its own topmost layer: point={point:?}, visible={visible:?}, transform={transform:?}");
        for pressed in [true, false] {
            self.frame(
                point,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(
            self.ctx.memory(|m| m.has_focus(Self::id())),
            "actual pointer click focuses TextEdit: before={before:?}, after={:?}, point={point:?}, rect={:?}, interact={:?}, visible={visible:?}, lines={}",
            self.ctx.memory(|memory| memory.focused()), response.rect, response.interact_rect, self.text.lines().count()
        );
        point
    }
}

#[test]
fn actual_plain_prompt_focus_short_overflow_blur_and_hidden_wheel_lifecycle() {
    let mut field = Field::new();
    let point = Pos2::new(60.0, 46.0);
    for _ in 0..3 {
        assert!(!field.frame(point, vec![]), "unfocused hover zooms canvas");
    }
    let point = field.click_field();
    assert!(
        field.frame(point, vec![]),
        "focused empty field captures before paint"
    );
    assert!(field.frame(point, vec![egui::Event::Text("scroll check".into())]));
    assert_eq!(field.text, "scroll check");
    assert!(
        field.frame(point, vec![]),
        "short nonoverflow field captures"
    );
    field.frame(
        point,
        vec![egui::Event::Text(
            " more words
"
            .repeat(30),
        )],
    );
    assert!(field.text.lines().count() > 20);
    assert!(field.frame(point, vec![]), "overflow field captures too");
    assert!(
        !field.frame(Pos2::new(420.0, 220.0), vec![]),
        "focus does not capture outside field"
    );
    let outside = Pos2::new(600.0, 500.0);
    for pressed in [true, false] {
        field.frame(
            outside,
            vec![egui::Event::PointerButton {
                pos: outside,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    assert!(!field.ctx.memory(|m| m.has_focus(Field::id())));
    assert!(
        !field.frame(point, vec![]),
        "blur invalidates prior claim immediately"
    );
    field.click_field();
    field.visible = false;
    field.frame(point, vec![]);
    assert!(
        !field.frame(point, vec![]),
        "closed/hidden widget leaves no stale wheel owner"
    );
}

#[test]
fn actual_prompt_claim_uses_transformed_clipped_field_and_topmost_layer() {
    let mut field = Field::new();
    field.transform = egui::emath::TSTransform::new(Vec2::new(130.0, 90.0), 1.5);
    field.clip = Rect::from_min_max(Pos2::ZERO, Pos2::new(160.0, 200.0));
    let point = field.click_field();
    assert!(field.frame(point, vec![]));
    let clipped = field.transform * Pos2::new(210.0, 46.0);
    assert!(
        !field.frame(clipped, vec![]),
        "invisible portion cannot capture wheel"
    );
    let ctx = field.ctx.clone();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::PointerMoved(point)],
            ..Default::default()
        },
        |ctx| {
            egui::Area::new(Id::new("covering-popup"))
                .order(egui::Order::Foreground)
                .fixed_pos(point - Vec2::splat(20.0))
                .show(ctx, |ui| {
                    ui.set_min_size(Vec2::splat(80.0));
                });
            assert!(
                !crate::menu_wheel::wheel_owned(ctx),
                "covered focused field cannot capture through top layer"
            );
        },
    );
}
