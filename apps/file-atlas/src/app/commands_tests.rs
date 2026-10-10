use super::*;

#[test]
fn registry_validates() {
    REGISTRY
        .validate()
        .expect("SPECS has duplicate ids or ambiguous chords");
}

#[test]
fn shared_never_repeat_set_is_honored() {
    for id in [
        "app.undo",
        "app.redo",
        "app.open",
        "app.select_all",
        "app.cancel",
        "canvas.zoom_in",
        "canvas.zoom_out",
        "canvas.fit",
        "app.help",
        "app.feedback.open",
        "app.feedback.finish_recording",
        "app.preferences",
        "app.new_tab",
        "app.repeat_last",
        "app.session.mark",
        "app.session.reveal",
    ] {
        let spec = REGISTRY.by_id(CommandId(id)).expect(id);
        assert!(
            matches!(spec.repeat, Repeat::Never),
            "`{id}` must be Repeat::Never"
        );
    }
}

#[test]
fn atlas_repeatables_are_repeatable() {
    for id in ["atlas.assign", "atlas.open_selected", "app.properties"] {
        let spec = REGISTRY.by_id(CommandId(id)).expect(id);
        assert!(
            matches!(spec.repeat, Repeat::Repeatable),
            "`{id}` must be Repeat::Repeatable"
        );
    }
}

#[test]
fn selection_commands_require_selection() {
    let no_sel = Availability::ATLAS | Availability::GLOBAL;
    let with_sel = no_sel | Availability::NEEDS_SELECTION;
    // F2 only resolves while something is selected.
    assert!(REGISTRY.by_chord(Chord::bare(Key::F2), no_sel).is_none());
    assert_eq!(
        REGISTRY
            .by_chord(Chord::bare(Key::F2), with_sel)
            .unwrap()
            .id
            .0,
        "atlas.assign"
    );
    // Ctrl+C likewise (a focused text field keeps its own copy anyway).
    assert!(REGISTRY.by_chord(Chord::ctrl(Key::C), no_sel).is_none());
}
