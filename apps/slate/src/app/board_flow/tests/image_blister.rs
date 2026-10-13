use super::*;
use crate::app::board_properties::Panel;

#[test]
fn image_agent_count_clicks_reach_the_authored_generator_settings() {
    let (mut h, id) = generator_with_output("count-clicks");
    h.app.patch_nodes(&[id], |node| {
        node.rect.w = 480.0;
        node.rect.h = 480.0;
    });
    h.app.board_sel = std::iter::once(id).collect();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    h.app.shape_properties.panel = Some(Panel::Agent);
    for _ in 0..12 {
        h.frame();
    }
    let xf = h.app.board_xf();
    let image = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let editor = atlas_shell::selection_tools::image_presentation_layout(
        image,
        xf.z,
        atlas_shell::selection_tools::ImagePanelKind::Custom {
            main_height: h
                .app
                .image_agent_body_height(&h.ctx.layer_painter(egui::LayerId::background()), id),
        },
    )
    .main;
    // Wide datum: play is 20, gap is 6, count's plus target is 13 wide.
    let inner = editor.shrink(12.0 * xf.z);
    let plus = Pos2::new(
        inner.right() - (20.0 + 6.0 + 6.5) * xf.z,
        inner.top() + 12.0 * xf.z,
    );
    for expected in [2, 3, 4] {
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(plus)));
        for pressed in [true, false] {
            h.frame_with(|input| {
                input.events.push(egui::Event::PointerButton {
                    pos: plus,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                })
            });
        }
        let settings = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .image;
        assert_eq!(settings.count, expected);
        assert_eq!(
            h.app
                .generation_request(id, false)
                .unwrap()
                .image
                .unwrap()
                .count,
            expected
        );
        h.frame();
    }
}

#[test]
fn a_running_frame_shows_what_it_is_doing_and_for_how_long() {
    let (mut h, generator) = generator_with_output("progress");
    h.app.patch_nodes(&[generator], |n| {
        // Keep the square source and its expanded datum inside the viewport.
        n.rect.w = 480.0;
        n.rect.h = 480.0;
        let a = slate_doc::agent_chat::agent_mut(n).unwrap();
        a.instruction = "a red barn".into();
        a.image.count = 3;
    });
    h.app.queue_generation(generator);
    h.app.queue_generation(generator);
    h.app.agent_blur();
    h.app.board_sel.clear();
    let r = h.app.doc().scene.node(generator).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    let shown = painted(&mut h, Pos2::new(2.0, 890.0));
    assert!(
        shown
            .iter()
            .any(|t| t.starts_with("Transforming image 1 of 3 · 0:0")),
        "progress without hovering: {shown:?}"
    );
    // Selected, the picture shows its dock.
    h.app.board_sel.insert(generator);
    painted(&mut h, Pos2::new(2.0, 890.0));
    h.app.shape_properties.panel = Some(Panel::Agent);
    for _ in 0..12 {
        h.frame();
    }
    painted(&mut h, Pos2::new(2.0, 890.0));
    let xf = h.app.board_xf();
    let image = xf.rect_w2s(h.app.doc().scene.node(generator).unwrap().rect);
    let editor = atlas_shell::selection_tools::image_presentation_layout(
        image,
        xf.z,
        atlas_shell::selection_tools::ImagePanelKind::Custom {
            main_height: h.app.image_agent_body_height(
                &h.ctx.layer_painter(egui::LayerId::background()),
                generator,
            ),
        },
    )
    .main;
    // The image blister's bare Play/Stop control occupies the top-right
    // of its datum row in the wide layout.
    let datum = editor.shrink(12.0);
    let play = Pos2::new(datum.right() - 10.0, datum.top() + 12.0);
    assert!(h.app.canvas_rect.contains(play), "visible Stop: {play:?}");
    assert!(h.app.agent_is_running(generator));
    assert_eq!(h.app.generations_waiting(generator), 1);
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(play)));
    let button = |pressed| egui::Event::PointerButton {
        pos: play,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.frame_with(|input| input.events.push(button(true)));
    h.frame_with(|input| input.events.push(button(false)));
    assert_eq!(
        h.app.generations_waiting(generator),
        0,
        "the bare Stop control cancels queued requests while the provider acknowledges its active stop"
    );
}
