//! Crop handles, crop mode, and repeat-last.

use super::*;

/// The W handle is hit along the whole left edge, a few px outside the box,
/// not only on the 18 px bar at the midpoint.
#[test]
fn crop_edge_hits_along_its_full_length_and_just_outside() {
    let (mut h, ids) = crop_board("crop_edge_full_length", 1);
    crop_via_command(&mut h);
    let xf = h.app.board_xf();
    // 40 px below the NW corner: past the corner zone, far from the bar,
    // and 6 px outside the box.
    let p = xf.w2s(Pos2::new(-6.0, 40.0));
    crop_pointer(&mut h, p, Some(true));
    crop_pointer(&mut h, p + EVec2::new(30.0, 0.0), None);
    assert_eq!(
        crop_drag_handle(&h).map(|(id, handle, _)| (id, handle)),
        Some((ids[0], board_handles::ResizeHandle::W as u8))
    );
    crop_pointer(&mut h, p + EVec2::new(30.0, 0.0), Some(false));
    assert!(h.app.board_crop.is_some());
}

/// Hovering a handle's slop shows its resize cursor.
#[test]
fn crop_handle_hover_shows_the_resize_cursor() {
    let (mut h, _) = crop_board("crop_hover_cursor", 1);
    crop_via_command(&mut h);
    let xf = h.app.board_xf();
    let cursor_at = |h: &mut Harness, p: Pos2| {
        let input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
            events: vec![egui::Event::PointerMoved(p)],
            ..Default::default()
        };
        let ctx = h.ctx.clone();
        let app = &mut h.app;
        ctx.run(input, |c| app.update_app(c))
            .platform_output
            .cursor_icon
    };
    cursor_at(&mut h, xf.w2s(Pos2::new(100.0, 75.0)));
    assert_eq!(
        cursor_at(&mut h, xf.w2s(Pos2::new(-6.0, 40.0))),
        egui::CursorIcon::ResizeWest
    );
    assert_eq!(
        cursor_at(&mut h, xf.w2s(Pos2::new(205.0, 155.0))),
        egui::CursorIcon::ResizeSouthEast
    );
    assert_eq!(
        cursor_at(&mut h, xf.w2s(Pos2::new(120.0, -6.0))),
        egui::CursorIcon::ResizeNorth
    );
}

/// A click that stays in a handle's outside slop is not a click on empty
/// canvas: crop mode and the selection stay.
#[test]
fn crop_click_in_outside_slop_keeps_crop_mode() {
    let (mut h, ids) = crop_board("crop_click_slop", 1);
    crop_via_command(&mut h);
    let xf = h.app.board_xf();
    let p = xf.w2s(Pos2::new(-5.0, 75.0));
    crop_pointer(&mut h, p, Some(true));
    crop_pointer(&mut h, p, Some(false));
    assert_eq!(h.app.board_crop, Some(ids[0]));
    assert!(h.app.board_sel.contains(&ids[0]));
}

/// The very first press on a handle after turning crop on from the Corners
/// panel grabs it. No hover frame precedes the press.
#[test]
fn crop_first_press_after_corners_crop_grabs_the_handle() {
    let (mut h, ids) = crop_board("crop_first_grab_panel", 1);
    crop_via_corners_panel(&mut h);
    let xf = h.app.board_xf();
    let east = xf.w2s(Pos2::new(200.0, 75.0));
    crop_pointer(&mut h, east, Some(true));
    crop_pointer(&mut h, east + EVec2::new(-20.0, 0.0), None);
    assert_eq!(
        crop_drag_handle(&h).map(|(id, handle, _)| (id, handle)),
        Some((ids[0], board_handles::ResizeHandle::E as u8)),
        "first press on the E bar must start the crop drag"
    );
    crop_pointer(&mut h, east + EVec2::new(-40.0, 0.0), None);
    crop_pointer(&mut h, east + EVec2::new(-40.0, 0.0), Some(false));
    let c = crop_of(&h, ids[0]);
    assert!((c.w - 0.8).abs() < 0.01, "crop.w {}", c.w);
    assert!(h.app.board_crop.is_some(), "crop stays on (D02)");
}

/// C enters crop, then the next frame presses a handle with no hover.
#[test]
fn crop_first_press_after_c_key_grabs_the_handle() {
    let (mut h, ids) = crop_board("crop_first_grab_key", 1);
    crop_key(&mut h, egui::Key::C, true);
    let xf = h.app.board_xf();
    let west = xf.w2s(Pos2::new(0.0, 75.0));
    h.frame_with(|i| {
        i.events.push(egui::Event::Key {
            key: egui::Key::C,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        i.events.push(egui::Event::PointerMoved(west));
        i.events.push(egui::Event::PointerButton {
            pos: west,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
    });
    assert_eq!(h.app.board_crop, Some(ids[0]), "C entered crop mode");
    crop_pointer(&mut h, west + EVec2::new(20.0, 0.0), None);
    assert_eq!(
        crop_drag_handle(&h).map(|(_, handle, _)| handle),
        Some(board_handles::ResizeHandle::W as u8)
    );
    crop_pointer(&mut h, west + EVec2::new(20.0, 0.0), Some(false));
    assert!((crop_of(&h, ids[0]).x - 0.1).abs() < 0.01);
}

/// Every selected image stays in crop mode for the whole gesture with the
/// Corners squircle up, the drag writes the same crop on all of them, and
/// one undo restores them.
#[test]
fn crop_multi_images_stay_in_crop_mode_and_commit_one_group() {
    for n in [3, 5] {
        let (mut h, ids) = crop_board(&format!("crop_multi_{n}"), n);
        crop_via_corners_panel(&mut h);
        crop_idle_frames(&mut h, n, "after entering crop");
        let xf = h.app.board_xf();
        let west_b = xf.w2s(Pos2::new(260.0, 75.0));
        crop_pointer(&mut h, west_b, Some(true));
        assert_crop_holds(&h, n, "press");
        for dx in [10.0, 20.0, 30.0] {
            crop_pointer(&mut h, west_b + EVec2::new(dx, 0.0), None);
            assert_crop_holds(&h, n, "mid-drag");
            assert_eq!(
                crop_drag_handle(&h),
                Some((ids[1], board_handles::ResizeHandle::W as u8, n - 1)),
                "the grabbed image drives every peer"
            );
        }
        crop_pointer(&mut h, west_b + EVec2::new(30.0, 0.0), Some(false));
        crop_idle_frames(&mut h, n, "after release");
        for id in &ids {
            let c = crop_of(&h, *id);
            assert!(
                (c.x - 0.15).abs() < 0.01 && (c.w - 0.85).abs() < 0.01,
                "{id:?}: {c:?}"
            );
        }
        h.app.board_undo();
        for id in &ids {
            assert!(crop_of(&h, *id).is_full(), "one undo restores {id:?}");
        }
    }
}

/// A click on each selected image in turn during crop mode keeps every
/// image in crop mode and the Corners squircle up (D17: crop mode does not
/// clear the selection).
#[test]
fn crop_click_on_each_peer_keeps_every_image_in_crop_mode() {
    for n in [3, 5] {
        let (mut h, ids) = crop_board(&format!("crop_multi_click_{n}"), n);
        crop_via_corners_panel(&mut h);
        crop_idle_frames(&mut h, n, "after entering crop");
        let xf = h.app.board_xf();
        for i in 0..n {
            let inside = xf.w2s(Pos2::new(i as f32 * 260.0 + 100.0, 75.0));
            crop_pointer(&mut h, inside, Some(true));
            assert_crop_holds(&h, n, &format!("press on image {i}"));
            crop_pointer(&mut h, inside, Some(false));
            crop_idle_frames(&mut h, n, &format!("after click on image {i}"));
        }
        for id in &ids {
            assert!(h.app.board_sel.contains(id));
        }
    }
}

/// The strip keys on which nodes are selected, not on the order the
/// selection set happens to iterate in.
#[test]
fn crop_panel_survives_selection_set_reordering() {
    for n in [3, 5] {
        let (mut h, _) = crop_board(&format!("crop_multi_reorder_{n}"), n);
        crop_via_corners_panel(&mut h);
        for round in 0..24 {
            h.app.board_sel = h.app.board_sel.iter().copied().collect();
            h.frame();
            assert_crop_holds(&h, n, &format!("reordered set, round {round}"));
        }
    }
}

/// After a crop, selecting another image and tapping the repeat key (P0.4)
/// enters crop mode on it, the same as invoking Crop.
#[test]
fn crop_repeat_last_reenters_crop_on_the_next_image() {
    for key in [egui::Key::Space, egui::Key::Enter] {
        let (mut h, ids) = crop_board("crop_repeat", 2);
        h.app.board_sel = [ids[0]].into_iter().collect();
        h.frame();
        crop_via_corners_panel(&mut h);
        let xf = h.app.board_xf();
        let east = xf.w2s(Pos2::new(200.0, 75.0));
        crop_pointer(&mut h, east, Some(true));
        crop_pointer(&mut h, east + EVec2::new(-30.0, 0.0), None);
        crop_pointer(&mut h, east + EVec2::new(-30.0, 0.0), Some(false));
        assert!(!crop_of(&h, ids[0]).is_full(), "first image cropped");
        // Esc closes the Corners panel, then leaves crop mode (P0.1).
        for _ in 0..2 {
            crop_key(&mut h, egui::Key::Escape, true);
            crop_key(&mut h, egui::Key::Escape, false);
        }
        assert!(h.app.board_crop.is_none(), "Esc left crop mode");
        h.app.board_sel = [ids[1]].into_iter().collect();
        h.frame();
        crop_key(&mut h, key, true);
        crop_key(&mut h, key, false);
        assert_eq!(h.app.board_crop, Some(ids[1]), "{key:?} repeats crop");
    }
}
