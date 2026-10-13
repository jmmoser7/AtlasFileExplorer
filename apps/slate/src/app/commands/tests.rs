use super::*;

#[test]
fn registry_validates() {
    registry().validate().expect("SPECS table is consistent");
}

#[test]
fn alias_chords_resolve_and_do_not_collide() {
    let reg = registry();
    for (chord, id) in ALIAS_CHORDS {
        let spec = reg.by_id(*id).expect("alias target exists");
        // An alias must not shadow a primary chord that is live in any of
        // the target's views.
        if let Some(other) = reg.by_chord(*chord, spec.when.union(Availability::GLOBAL)) {
            panic!(
                "alias {:?} for `{}` collides with primary chord of `{}`",
                chord, id.0, other.id.0
            );
        }
    }
}

#[test]
fn never_repeat_set_matches_spec() {
    // The command-registry spec's never-repeat set: undo/redo/save/save-as/
    // open/new-tab/escape/delete/zoom/fit/select-all/history/help/repeat.
    let reg = registry();
    for id in [
        "app.undo",
        "app.redo",
        "app.save",
        "app.save_as",
        "app.open",
        "app.new_tab",
        "app.cancel",
        "board.delete",
        "canvas.zoom_in",
        "board.fit",
        "app.select_all",
        "app.history",
        "app.help",
        "app.feedback.open",
        "app.feedback.finish_recording",
        "app.repeat_last",
        "app.session.mark",
        "app.session.reveal",
    ] {
        let spec = reg.by_id(CommandId(id)).expect(id);
        assert_eq!(spec.repeat, Repeat::Never, "{id} must be never-repeat");
    }
}
