//! A node lying on a chat card owns the press over it, even where the card's
//! transcript would scroll.

use super::train_setup::{board, train};
use super::*;

fn picture_over(h: &mut super::super::tests::Harness, host: slate_doc::scene::WorldRect) -> NodeId {
    let path = h.base.join("cover.png");
    image::RgbaImage::from_pixel(12, 8, image::Rgba([200, 40, 40, 255]))
        .save(&path)
        .unwrap();
    let item = h.app.item_for_path(&path).unwrap();
    let rect = slate_doc::scene::WorldRect::new(
        host.x + host.w * 0.5 - 60.0,
        host.y + host.h * 0.5 - 40.0,
        120.0,
        80.0,
    );
    let node = h.app.doc_mut().scene.build_node(
        rect,
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    h.app.add_nodes(vec![node])[0]
}

#[test]
fn a_picture_on_a_scrolling_transcript_is_dragged_not_scrolled() {
    let mut h = board("cover_transcript");
    let card = train(&mut h, Pos2::ZERO, "local");
    h.frame();
    let turns = (0..24)
        .map(|i| AgentTurn {
            role: if i % 2 == 0 { "user" } else { "assistant" }.into(),
            text: format!(
                "Message {i}: the courtyard keeps its two plane trees by the north wall."
            ),
            at: i,
        })
        .collect();
    h.app.agents.local_turns.insert(card, turns);
    h.app.patch_nodes(&[card], |n| {
        n.rect.w = 360.0;
        n.rect.h = 300.0;
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.size = Some([360.0, 300.0]);
        }
    });
    h.app.agents.output_epoch += 1;
    let host = h.app.doc().scene.node(card).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(host.x + host.w * 0.5, host.y + host.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    for _ in 0..6 {
        h.frame();
    }
    assert!(
        h.app
            .agents
            .card_overflow
            .get(&card)
            .is_some_and(|m| *m > 0.5),
        "the card's transcript overflows, so it would take a drag"
    );

    let picture = picture_over(&mut h, host);
    h.frame();
    let before = h.app.doc().scene.node(picture).unwrap().rect;
    let from = Pos2::new(before.x + before.w * 0.5, before.y + before.h * 0.5);
    let path: Vec<Pos2> = (0..=6)
        .map(|i| from + egui::vec2(25.0 * i as f32, 0.0))
        .collect();
    super::super::tests::press_drag_release_frames(&mut h, &path, egui::Modifiers::NONE, |_| {});
    let after = h.app.doc().scene.node(picture).unwrap().rect;
    assert!(
        (after.x - before.x - 150.0).abs() < 2.0,
        "the picture on top moves with the drag: {} -> {}",
        before.x,
        after.x
    );
    assert_eq!(
        h.app.doc().scene.node(card).unwrap().rect,
        host,
        "the card underneath stays"
    );
}
