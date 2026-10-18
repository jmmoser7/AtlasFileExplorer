//! A real frame and its linked image/caption travel through the existing owner.
use super::*;
use crate::app::tests::{painted_block, painted_strings};

pub(super) fn run() {
    let mut f = Fixture::new(-320.0);
    let image = f.h.app.doc().scene.node(f.images[0]).unwrap().clone();
    let prepared =
        f.h.app
            .prepare_targeted_command(
                "board.objects.create_frame",
                &[],
                &json!({"x":image.rect.x-20.0,"y":image.rect.y-20.0,
            "width":image.rect.w+40.0,"height":image.rect.h+60.0,"title":"Carried slide"}),
            )
            .unwrap();
    let frame_id = prepared.targets[0];
    f.h.app.commit_scene(prepared.cmds);
    let prepared =
        f.h.app
            .prepare_targeted_command(
                "board.objects.create_text",
                &[],
                &json!({"x":image.rect.x+4.0,"y":image.rect.y+image.rect.h+8.0,
            "width":image.rect.w-8.0,"height":24,"text":"Carried caption"}),
            )
            .unwrap();
    let caption_id = prepared.targets[0];
    f.h.app.commit_scene(prepared.cmds);
    f.h.app.board_sel.clear();
    let frame = f.h.app.doc().scene.node(frame_id).unwrap().clone();
    let caption = f.h.app.doc().scene.node(caption_id).unwrap().clone();
    let scene = f.h.app.doc().scene.clone();
    let items = f.h.app.doc().items.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    let out = f.step();
    assert!(painted_strings(&out).iter().any(|s| s == "Carried caption"));
    let caption_origin = painted_block(&out, "Carried caption", 1.0).unwrap().origin;
    f.draw(&out);
    assert_eq!(f.blue(&image), Some(true));
    f.save("frame-before", out);
    let (dx, dy) = (240.0, 100.0);
    f.invoke(
        0,
        "board.objects.move",
        &[frame_id],
        json!({"dx":dx,"dy":dy}),
    );
    let mut bands = [false; 3];
    let mut samples = Vec::new();
    let (mut travel, mut grab) = (false, false);
    let mut previous = 0.0;
    for _ in 0..80 {
        let out = f.step();
        if !f.pending() {
            assert!(painted_strings(&out).iter().any(|s| s == "Carried caption"));
            let painted_caption = painted_block(&out, "Carried caption", 1.0).unwrap().origin;
            assert!((painted_caption - caption_origin - egui::vec2(dx, dy)).length() < 0.1);
            f.save("frame-committed", out);
            break;
        }
        assert_eq!(f.h.app.doc().scene, scene);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        match f.sample().phase {
            MotionPhase::Travel => travel = true,
            MotionPhase::Grab => {
                assert!(travel);
                grab = true;
                let (x, y) = frame.rect.center();
                assert!((f.sample().position.x - f64::from(x)).abs() < 0.01);
                assert!((f.sample().position.y - f64::from(y)).abs() < 0.01);
            }
            MotionPhase::Work => {
                assert!(
                    travel && grab,
                    "approach and contact precede the carried frame"
                );
                let carried = f.h.app.presence_preview(frame_id).unwrap().clone();
                let visible_image = f.h.app.presence_preview(image.id).unwrap().clone();
                let visible_caption = f.h.app.presence_preview(caption_id).unwrap().clone();
                for (source, visible) in [(&image, &visible_image), (&caption, &visible_caption)] {
                    assert_eq!(visible.id, source.id);
                    assert_eq!(visible.kind, source.kind);
                    assert!(
                        (visible.rect.x - carried.rect.x - (source.rect.x - frame.rect.x)).abs()
                            < 0.01
                    );
                    assert!(
                        (visible.rect.y - carried.rect.y - (source.rect.y - frame.rect.y)).abs()
                            < 0.01
                    );
                }
                let progress = (carried.rect.x - frame.rect.x) / dx;
                assert!(progress >= previous && progress <= 1.0);
                previous = progress;
                if !(0.12..0.94).contains(&progress) {
                    continue;
                }
                let band = if progress < 0.35 {
                    0
                } else if progress < 0.7 {
                    1
                } else {
                    2
                };
                if bands[band] {
                    continue;
                }
                bands[band] = true;
                assert!(f
                    .h
                    .app
                    .canvas_rect
                    .contains_rect(f.h.app.board_xf().rect_w2s(carried.rect)));
                assert!(painted_strings(&out).iter().any(|s| s == "Carried caption"));
                let painted_caption = painted_block(&out, "Carried caption", 1.0).unwrap().origin;
                let displacement = egui::vec2(
                    visible_caption.rect.x - caption.rect.x,
                    visible_caption.rect.y - caption.rect.y,
                );
                assert!((painted_caption - caption_origin - displacement).length() < 0.1);
                f.draw(&out);
                assert_eq!(
                    f.blue(&visible_image),
                    Some(true),
                    "real opaque linked-image pixels at each carried position"
                );
                if visible_image.rect.x - image.rect.x > image.rect.w {
                    assert_eq!(
                        f.blue(&image),
                        Some(false),
                        "no opaque duplicate at the old image position"
                    );
                }
                samples.push(json!({"band":band,"progress":progress,"frame":carried.rect,
                    "image":visible_image.rect,"caption":visible_caption.rect,"opaqueImagePixels":true,
                    "paintedCaptionOrigin":[painted_caption.x,painted_caption.y]}));
                f.save(
                    [
                        "frame-carry-early",
                        "frame-carry-middle",
                        "frame-carry-late",
                    ][band],
                    out,
                );
            }
            _ => {}
        }
    }
    assert!(!f.pending());
    assert!(travel && grab && bands.into_iter().all(|b| b));
    assert_eq!(samples.len(), 3);
    for source in [&frame, &image, &caption] {
        let after = f.h.app.doc().scene.node(source.id).unwrap();
        assert_eq!(after.rect.x, source.rect.x + dx);
        assert_eq!(after.rect.y, source.rect.y + dy);
        assert_eq!(after.rect.w, source.rect.w);
        assert_eq!(after.rect.h, source.rect.h);
        assert_eq!(after.kind, source.kind);
    }
    assert_eq!(f.h.app.doc().items, items);
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        f.h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    assert_eq!(
        f.blue(f.h.app.doc().scene.node(image.id).unwrap()),
        Some(true)
    );
    let tab = f.h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(tab.doc.scene, scene);
    assert_eq!(tab.journal.undo_depth(), depth);
    f.record(
        "frame-carry-assertions",
        json!({"samples":samples,"singleAttributedCommit":true,
        "oneUndoRestoresScene":true,"provider":"synthetic local fixture; no provider launch"}),
    );
    f.finish();
}
