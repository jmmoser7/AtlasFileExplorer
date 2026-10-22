use super::*;

/// Delete reaches the hovered card and updates the canvas after recycling.
/// Recycle Bin is a Windows shell verb; this is the Windows proof.
#[test]
fn delete_key_removes_the_card_under_the_cursor() {
    let mut h = Harness::new("edit_delete");
    let root = h._base.join("delete_proj");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("doomed.jpg"), vec![b'x'; 32]).unwrap();
    std::fs::write(root.join("keeper.jpg"), vec![b'y'; 32]).unwrap();
    h.app.set_root(root.clone());
    h.pump_until_idle();
    h.app.set_edit_mode(EditMode::Edit);
    // Exercise routing after the human has approved this small deletion.
    h.app.suppress_delete_confirm = true;
    h.frame();
    let doomed = *h.app.rel_to_id.get("doomed.jpg").expect("scanned");
    let fixture_path = root.join("doomed.jpg");
    let scanned_path = &h.app.entries[doomed as usize].path;
    assert_eq!(
        scanned_path, &fixture_path,
        "scanned generated fixture path"
    );
    let metadata = std::fs::symlink_metadata(&fixture_path).unwrap();
    eprintln!(
        "Delete generated fixture: scanned={scanned_path:?}, expected={fixture_path:?}, absolute={}, file={}, symlink={}, len={}, readonly={}",
        scanned_path.is_absolute(), metadata.is_file(), metadata.file_type().is_symlink(),
        metadata.len(), metadata.permissions().readonly()
    );
    #[cfg(windows)]
    {
        use std::os::windows::{ffi::OsStrExt, fs::MetadataExt};
        let wide: Vec<u16> = scanned_path.as_os_str().encode_wide().collect();
        eprintln!(
            "Delete generated fixture Windows: attributes=0x{:x}, utf16_len={}, utf16_units={wide:?}",
            metadata.file_attributes(), wide.len()
        );
    }
    let center = h.app.w2s(
        h.app.tree.as_ref().unwrap().file_pos[doomed as usize]
            .rect()
            .center(),
    );
    h.frame_with_events(vec![egui::Event::PointerMoved(center)]);
    assert_eq!(h.app.hovered_file, Some(doomed));
    assert!(h.app.selection.is_empty(), "nothing is selected");
    h.frame_with_events(vec![egui::Event::Key {
        key: egui::Key::Delete,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    let started_operation = h.app.fs_op.as_ref().map(|op| op.op.clone());
    eprintln!("Delete generated fixture started operation: {started_operation:?}");
    assert!(
        h.app.fs_op.is_some() || !root.join("doomed.jpg").exists(),
        "Delete did not start or finish: focus={:?}, wants_keyboard={}, hover={:?}, mode={:?}, confirmation={}, toasts={:?}",
        h.ctx.memory(|m| m.focused()), h.ctx.wants_keyboard_input(),
        h.app.hovered_file, h.app.edit_mode, h.app.delete_plan.is_some(), h.app.toasts
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.app.fs_op.is_some() && Instant::now() < deadline {
        h.frame();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !root.join("doomed.jpg").exists(),
        "the file is still there: operation={:?}, toasts={:?}, started_operation={:?}",
        h.app
            .fs_op
            .as_ref()
            .map(|op| (&op.label, op.done, op.total, &op.op)),
        h.app.toasts,
        started_operation
    );
    assert!(h.app.entries[doomed as usize].dead);
    // The next rebuild agrees with the completed operation.
    h.frame();
    h.frame();
    assert_eq!(
        h.app.tree.as_ref().unwrap().file_pos[doomed as usize].place,
        atlas_core::tree::FilePlace::Hidden,
        "the deleted card is still placed on the canvas"
    );
    assert!(root.join("keeper.jpg").exists());
}
