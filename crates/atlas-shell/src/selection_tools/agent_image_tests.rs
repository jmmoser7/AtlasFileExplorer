use super::*;

fn editor_frame(
    ctx: &egui::Context,
    rect: Rect,
    zoom: f32,
    count: u8,
    events: Vec<egui::Event>,
) -> ImageAgentEditResponse {
    let mut response = ImageAgentEditResponse::default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(3000.0, 2000.0))),
        events,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut prompt = String::new();
            let (mut model_open, mut aspect_open, mut settings_open) = (false, false, false);
            response = image_agent_editor(
                ui,
                rect,
                AgentEdit {
                    modes: &["Image"],
                    mode: 0,
                    image: true,
                    prompt: &mut prompt,
                    hint: "Describe the image",
                    models: &["Auto"],
                    model: 0,
                    model_open: &mut model_open,
                    count,
                    aspects: &["Source"],
                    aspect: 0,
                    aspect_open: &mut aspect_open,
                    seed_locked: false,
                    live: false,
                    take_focus: false,
                },
                ImageAgentOptions {
                    history: &[],
                    selected_history: None,
                    running: false,
                    live_active: false,
                    status: None,
                    settings_open: &mut settings_open,
                    live_open: &mut false,
                    chrome: Default::default(),
                },
                zoom,
                Palette::dark(),
            );
        });
    });
    response
}

#[test]
fn image_agent_count_arrows_accept_repeated_clicks_at_each_layout_and_zoom() {
    for width in [160.0, 360.0, 900.0] {
        for zoom in [0.5, 1.0, 2.0] {
            let ctx = egui::Context::default();
            let rect = Rect::from_min_size(Pos2::new(30.0, 30.0), Vec2::new(width, 240.0) * zoom);
            let datum = image_agent_datum_layout(rect.shrink(12.0 * zoom), zoom);
            let plus = Pos2::new(datum.count.right() - 6.5 * zoom, datum.count.center().y);
            let minus = Pos2::new(datum.count.left() + 6.5 * zoom, datum.count.center().y);
            let mut count = 1;
            editor_frame(&ctx, rect, zoom, count, vec![]);
            for (pointer, expected) in [(plus, 2), (plus, 3), (minus, 2), (minus, 1), (minus, 1)] {
                editor_frame(
                    &ctx,
                    rect,
                    zoom,
                    count,
                    vec![egui::Event::PointerMoved(pointer)],
                );
                let click = |pressed| egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                let pressed = editor_frame(&ctx, rect, zoom, count, vec![click(true)]);
                assert!(!pressed.submit);
                let released = editor_frame(&ctx, rect, zoom, count, vec![click(false)]);
                assert!(!released.submit);
                count = released.count.expect("an arrow click returns a count edit");
                assert_eq!(count, expected, "width={width}, zoom={zoom}");
                // Keep hovering the child target across frames: this used to
                // remove its click region when the parent's hover disappeared.
                editor_frame(&ctx, rect, zoom, count, vec![]);
            }
            editor_frame(&ctx, rect, zoom, 8, vec![egui::Event::PointerMoved(plus)]);
            for pressed in [true, false] {
                let output = editor_frame(
                    &ctx,
                    rect,
                    zoom,
                    8,
                    vec![egui::Event::PointerButton {
                        pos: plus,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                if !pressed {
                    assert_eq!(output.count, Some(8));
                }
            }
        }
    }
}
