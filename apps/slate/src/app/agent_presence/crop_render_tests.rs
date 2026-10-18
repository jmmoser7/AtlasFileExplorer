//! Real native-app paint of a witnessed crop; software evidence, not GPU acceptance.
use super::*;
use crate::app::{
    board_crop::{apply_gesture, CropGesture},
    tests::primary_button,
};
use slate_doc::scene::{Crop, PathData, PathSeg};

fn fixture() -> Fixture {
    let mut f = Fixture::multicolour(-100.0);
    let target = f.images[0];
    f.h.app.patch_nodes(&[target], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.crop = Crop {
                x: 0.25,
                y: 0.15,
                w: 0.5,
                h: 0.7,
            };
        }
        node.clip = Some(PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
                PathSeg::Line { to: [0.0, 1.0] },
            ],
            closed: true,
            ..Default::default()
        });
    });
    f
}

fn crop(node: &slate_doc::Node) -> Crop {
    match &node.kind {
        NodeKind::Image(image) => image.crop,
        _ => panic!("image"),
    }
}

fn start(f: &mut Fixture, sequence: u64) {
    let image = f.images[0];
    f.invoke(
        sequence,
        "board.objects.crop",
        &[image],
        json!({"mode":"pan","dx":48,"dy":0}),
    );
}

#[test]
#[ignore = "retains three real linked-image crop UV/pixel states and one attributed commit"]
fn witnessed_fixed_mask_pan_paints_three_intermediate_multicolour_uv_states_before_one_undoable_commit(
) {
    let mut f = fixture();
    let target = f.images[0];
    let before = f.h.app.doc().scene.node(target).unwrap().clone();
    let scene = f.h.app.doc().scene.clone();
    let items = f.h.app.doc().items.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    let probe = egui::pos2(
        before.rect.x + before.rect.w * 0.1,
        before.rect.y + before.rect.h * 0.8,
    );
    let out = f.step();
    f.draw(&out);
    let original_pixel = f.pixel(probe);
    f.save("crop-before", out);
    start(&mut f, 0);
    let mut grabbed = None;
    let mut samples = Vec::new();
    for _ in 0..100 {
        let out = f.step();
        if !f.pending() {
            f.save("crop-committed", out);
            break;
        }
        assert_eq!(f.h.app.doc().scene, scene);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        let sample = f.sample();
        if sample.phase == MotionPhase::Grab {
            grabbed = Some(sample.position);
        }
        let Some(preview) = f.h.app.presence_preview(target).cloned() else {
            continue;
        };
        assert_eq!(
            preview.rect, before.rect,
            "fixed mask never resizes during content pan"
        );
        if sample.phase != MotionPhase::Work || sample.progress < 0.1 || sample.progress > 0.9 {
            continue;
        }
        let slot = if sample.progress < 0.35 {
            0
        } else if sample.progress < 0.65 {
            1
        } else {
            2
        };
        if slot != samples.len() {
            continue;
        }
        let contact = grabbed.expect("actual visible body contact precedes crop movement");
        assert!(slate_doc::geom::point_in_node_outline(
            &before,
            contact.x as f32,
            contact.y as f32,
            0.05
        ));
        let input = CropGesture::Pan {
            delta_world: (
                (sample.position.x - contact.x) as f32,
                (sample.position.y - contact.y) as f32,
            ),
        };
        assert_eq!(apply_gesture(&before, input).as_ref(), Some(&preview));
        let uvs = f.image_uvs(&out, target);
        f.draw(&out);
        let pixel = f.pixel(probe);
        if let Some(previous) = samples.last() {
            let (_, old_crop, old_pixel, _): &(f32, Crop, [f32; 4], Vec<[f32; 2]>) = previous;
            assert!(crop(&preview).x < old_crop.x && pixel[0] < old_pixel[0] - 0.005);
        } else {
            assert!(pixel[0] < original_pixel[0] - 0.005);
        }
        f.save(&format!("crop-intermediate-{}", slot + 1), out);
        samples.push((sample.progress, crop(&preview), pixel, uvs));
    }
    assert!(!f.pending() && samples.len() == 3);
    assert!(
        samples.windows(2).all(|pair| pair[0].3 != pair[1].3),
        "actual linked-image UV meshes change in all three samples"
    );
    let expected = apply_gesture(
        &before,
        CropGesture::Pan {
            delta_world: (48.0, 0.0),
        },
    )
    .unwrap();
    assert_eq!(f.h.app.doc().scene.node(target), Some(&expected));
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        f.h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    assert_eq!(f.h.app.doc().items, items);
    f.record(
        "crop-uv-pixel-samples",
        json!({"originalPixel":original_pixel,"samples":samples,
        "originalNode":before,"finalNode":expected,"authoredSceneUnchangedUntilCommit":true}),
    );
    let tab = f.h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(tab.doc.scene.nodes, scene.nodes);
    assert!(tab.journal.redo(&mut tab.doc.scene));
    assert_eq!(tab.doc.scene.node(target), Some(&expected));
    f.finish();
}

#[test]
#[ignore = "retains fixed-frame crop cancellation and human takeover with no visible crop jump"]
fn witnessed_crop_cancel_and_takeover_preserve_visible_crop_geometry_and_camera() {
    for takeover in [false, true] {
        let mut f = fixture();
        let target = f.images[0];
        let before = f.h.app.doc().scene.node(target).unwrap().clone();
        let depth = f.h.app.tab().journal.undo_depth();
        let camera = (f.h.app.tab().cam.offset, f.h.app.tab().cam.z);
        start(&mut f, 0);
        let mut acted = false;
        for _ in 0..100 {
            let out = f.step();
            let Some(preview) = f.h.app.presence_preview(target).cloned() else {
                continue;
            };
            if crop(&preview) == crop(&before) || f.sample().progress < 0.35 {
                continue;
            }
            assert_eq!(
                preview.rect, before.rect,
                "crop-only pan must still be handed over"
            );
            f.save("crop-before-interrupt", out);
            if takeover {
                let world = egui::pos2(
                    before.rect.x + before.rect.w * 0.12,
                    before.rect.y + before.rect.h * 0.8,
                );
                let at = f.h.app.board_xf().w2s(world);
                f.h.frame_with(primary_button(at, true, false));
                assert!(!f.pending());
                assert_eq!(f.h.app.doc().scene.node(target), Some(&preview));
                assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
                assert_eq!(f.h.app.tab().journal.last_author(), Some(&CmdAuthor::Human));
                f.h.frame_with(primary_button(at, false, false));
            } else {
                f.h.app.stop_presence(&f.who, false).unwrap();
                assert_eq!(f.h.app.doc().scene.node(target), Some(&before));
                assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
            }
            assert!(f.h.app.presence_preview(target).is_none());
            assert!(!f.h.app.presence.holds_node(target));
            assert_eq!((f.h.app.tab().cam.offset, f.h.app.tab().cam.z), camera);
            let out = f.step();
            f.save(
                if takeover {
                    "crop-human-takeover"
                } else {
                    "crop-cancel-restored"
                },
                out,
            );
            if takeover {
                let tab = f.h.app.tab_mut();
                assert!(tab.journal.undo(&mut tab.doc.scene));
                assert_eq!(tab.doc.scene.node(target), Some(&before));
                assert!(tab.journal.redo(&mut tab.doc.scene));
                assert_eq!(tab.doc.scene.node(target), Some(&preview));
            }
            acted = true;
            break;
        }
        assert!(acted);
        f.finish();
    }
}

fn visible_crop(f: &mut Fixture) -> slate_doc::Node {
    start(f, 0);
    for _ in 0..100 {
        let out = f.step();
        if f.sample().phase == MotionPhase::Work && f.sample().progress > 0.35 {
            let visible = f.h.app.presence_preview(f.images[0]).unwrap().clone();
            f.draw(&out);
            return visible;
        }
    }
    panic!("crop never painted an intermediate preview");
}

fn widget_center(f: &Fixture, id: egui::Id) -> egui::Pos2 {
    let response = f.h.ctx.read_response(id).expect("real foreground widget");
    let transform =
        f.h.ctx
            .layer_transform_to_global(response.layer_id)
            .unwrap_or_default();
    let rect = transform * response.rect;
    assert!((transform * response.interact_rect).contains(rect.center()));
    assert_eq!(
        f.h.ctx.layer_id_at(rect.center()),
        Some(response.layer_id),
        "the real foreground target must own its visible center"
    );
    rect.center()
}

fn reach_ready(f: &mut Fixture) {
    let motion_id = f.h.app.presence.agents[0].motion.clone();
    let motion = f.h.app.presence.roster.session_mut(&motion_id).unwrap();
    // Advance the canonical motion to its terminal boundary without replacing
    // the last painted preview. An input frame must defer terminal resolution.
    for _ in 0..40 {
        if motion.advance(std::time::Duration::from_millis(50)).phase == MotionPhase::ReadyToCommit
        {
            return;
        }
    }
    panic!("motion never reached its commit boundary");
}

#[test]
fn primary_press_takes_over_the_last_painted_crop_even_at_the_ready_boundary() {
    for ready_boundary in [false, true] {
        let mut f = fixture();
        let target = f.images[0];
        let before = f.h.app.doc().scene.node(target).unwrap().clone();
        let depth = f.h.app.tab().journal.undo_depth();
        let camera = (f.h.app.tab().cam.offset, f.h.app.tab().cam.z);
        let visible = visible_crop(&mut f);
        assert_eq!(f.h.app.doc().scene.node(target), Some(&before));
        if ready_boundary {
            reach_ready(&mut f);
            assert_eq!(f.sample().phase, MotionPhase::ReadyToCommit);
        }
        let at = f.h.app.board_xf().w2s(egui::pos2(
            before.rect.x + before.rect.w * 0.12,
            before.rect.y + before.rect.h * 0.8,
        ));
        f.h.frame_with(primary_button(at, true, false));
        assert!(!f.pending());
        assert_eq!(f.h.app.doc().scene.node(target), Some(&visible));
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
        assert_eq!(f.h.app.tab().journal.last_author(), Some(&CmdAuthor::Human));
        assert_eq!((f.h.app.tab().cam.offset, f.h.app.tab().cam.z), camera);
        f.h.frame_with(primary_button(at, false, false));
    }
}

#[test]
fn foreground_composer_press_holds_crop_without_takeover_or_terminal_commit() {
    for ready_boundary in [false, true] {
        let mut f = fixture();
        // Joining Dot changes this right-pivoted Area's width. Let its actual
        // layout settle before clicking response geometry from the prior pass.
        for _ in 0..3 {
            f.step();
        }
        let chat = egui::Id::new(("presence-chat", "self"));
        let at = widget_center(&f, chat);
        f.step();
        assert_eq!(widget_center(&f, chat), at, "roster target must be stable");
        f.h.frame_with(primary_button(at, true, false));
        f.h.frame_with(primary_button(at, false, false));
        f.step();
        assert_eq!(f.h.app.presence.chrome.active_chat(), Some("self"));
        let composer = egui::Id::new(("presence-composer", "self"));
        let at = widget_center(&f, composer);
        let world = f.h.app.board_xf().s2w(at);
        let target = f.images[0];
        f.h.app.patch_nodes(&[target], |node| {
            node.rect.x = world.x - node.rect.w * 0.12;
            node.rect.y = world.y - node.rect.h * 0.8;
        });
        let before = f.h.app.doc().scene.node(target).unwrap().clone();
        let depth = f.h.app.tab().journal.undo_depth();
        let visible = visible_crop(&mut f);
        assert!(slate_doc::geom::point_in_node_outline(
            &visible, world.x, world.y, 0.05
        ));
        if ready_boundary {
            reach_ready(&mut f);
        }
        let sample = f.sample();
        f.h.frame_with(primary_button(at, true, false));
        assert!(f.pending(), "the foreground composer owns this press");
        assert_eq!(f.h.app.presence_preview(target), Some(&visible));
        assert_eq!(f.h.app.doc().scene.node(target), Some(&before));
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        assert_eq!(
            f.sample(),
            sample,
            "the input frame holds all motion phases"
        );
        f.h.app.stop_presence(&f.who, false).unwrap();
        f.h.frame_with(primary_button(at, false, false));
        assert_eq!(f.h.ctx.memory(|memory| memory.focused()), Some(composer));
    }
}

#[test]
fn masked_handle_contact_survives_completion_stop_retirement_and_next_travel() {
    for (rotation, ending) in [0.0, 37.0, 90.0]
        .into_iter()
        .flat_map(|rotation| ["commit", "stop", "retire"].map(|ending| (rotation, ending)))
    {
        let mut f = fixture();
        let target = f.images[0];
        f.h.app.patch_nodes(&[target], |node| {
            node.rotation_deg = rotation;
            if let NodeKind::Image(image) = &mut node.kind {
                image.flip_x = true;
                image.flip_y = true;
            }
        });
        let before = f.h.app.doc().scene.node(target).unwrap().clone();
        let depth = f.h.app.tab().journal.undo_depth();
        let request = before.rect.rotate_point([-900.0, 1200.0], rotation);
        let gesture = CropGesture::Handle {
            handle: 2,
            world: (request[0], request[1]),
        };
        let expected = apply_gesture(&before, gesture).unwrap();
        f.invoke(
            0,
            "board.objects.crop",
            &[target],
            json!({"mode":"handle","handle":2,"x":request[0],"y":request[1]}),
        );
        let motion_id = f.h.app.presence.agents[0].motion.clone();
        let mut contact = None;
        for _ in 0..80 {
            let out = f.step();
            let sample = f.sample();
            if sample.phase != MotionPhase::Work || sample.progress < 0.75 {
                continue;
            }
            let preview = f.h.app.presence_preview(target).unwrap();
            let (x, y) = crate::app::board_crop::gesture_contact(preview, gesture).unwrap();
            let physical = point(x, y);
            assert_ne!(
                physical, sample.position,
                "cover translation separates grip and control"
            );
            assert_eq!(
                f.h.app.presence.agents[0].position(&f.h.app.presence.roster),
                Some(physical)
            );
            assert_eq!(preview.rect, before.rect);
            assert_eq!(preview.clip, before.clip);
            assert_eq!(f.h.app.doc().scene.node(target), Some(&before));
            assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
            f.draw(&out);
            contact = Some(physical);
            break;
        }
        let physical =
            contact.expect("actual full-app crop must paint a separated grip before completion");
        match ending {
            "stop" => {
                let at =
                    f.h.app
                        .board_xf()
                        .w2s(egui::pos2(physical.x as f32, physical.y as f32));
                f.h.frame_with(primary_button(at, true, false));
                assert!(
                    f.h.app.presence.avatar_hold.is_some(),
                    "painted grip avatar owns the press"
                );
                assert!(f.pending());
                assert_eq!(f.h.app.doc().scene.node(target), Some(&before));
                f.h.app.stop_presence(&f.who, false).unwrap();
                let session = f.h.app.presence.roster.session(&motion_id).unwrap();
                assert_eq!(session.sample().position, physical);
                assert_eq!(session.sample().phase, MotionPhase::Idle);
                assert_eq!(f.h.app.doc().scene.node(target), Some(&before));
                assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
                assert!(f.h.app.presence_preview(target).is_none());
                f.h.frame_with(primary_button(at, false, false));
            }
            "retire" => {
                f.h.app.retire_presence(&f.who, "bow").unwrap();
                assert_eq!(
                    f.h.app.presence.agents[0].departure.as_ref().unwrap().2,
                    physical
                );
                assert_eq!(f.h.app.doc().scene.node(target), Some(&before));
                assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
                assert!(f.h.app.presence.roster.session(&motion_id).is_none());
            }
            "commit" => {
                for _ in 0..20 {
                    f.step();
                    if !f.pending() {
                        break;
                    }
                }
                assert!(!f.pending());
                assert_eq!(f.h.app.doc().scene.node(target), Some(&expected));
                assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
                assert_eq!(
                    f.h.app.tab().journal.last_author(),
                    Some(&CmdAuthor::Agent("Dot".into()))
                );
                let (x, y) = crate::app::board_crop::gesture_contact(&expected, gesture).unwrap();
                let final_contact = point(x, y);
                assert_eq!(f.sample().position, final_contact);
                f.invoke(1, "board.objects.move", &[target], json!({"dx":20,"dy":0}));
                f.step();
                let session = f.h.app.presence.roster.session(&motion_id).unwrap();
                assert_eq!(session.current_plan().unwrap().start, final_contact);
            }
            _ => unreachable!(),
        }
    }
}
