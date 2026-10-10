//! Shape and sticky text layout tests.

use super::*;

#[test]
fn centered_shape_text_starts_mid_line() {
    let ctx = egui::Context::default();
    let mut glyph_x = None;
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        let galley = zoom_text_galley(
            ctx,
            "Hi",
            FontId::proportional(24.0),
            200.0,
            TextAlign::Center,
            Color32::WHITE,
            1.0,
        );
        glyph_x = galley
            .rows
            .first()
            .and_then(|row| row.glyphs.first().map(|glyph| glyph.pos.x));
    });
    let x = glyph_x.expect("glyph");
    assert!(x > 40.0, "centered glyph started at {x}");
}

#[test]
fn empty_centered_sticky_caret_sits_in_the_middle() {
    let ctx = egui::Context::default();
    let mut caret = None;
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        let laid = zoom_text_galley(
            ctx,
            "",
            FontId::proportional(24.0),
            200.0,
            TextAlign::Center,
            Color32::BLACK,
            1.0,
        );
        let mut owned = std::sync::Arc::try_unwrap(laid).unwrap_or_else(|arc| (*arc).clone());
        center_galley_vertically(&mut owned, 200.0);
        let galley = std::sync::Arc::new(owned);
        caret = galley.rows.first().map(|row| row.rect.center());
    });
    let caret = caret.expect("caret row");
    assert!(
        (caret.x - 100.0).abs() < 8.0,
        "horizontal center was {}",
        caret.x
    );
    assert!(
        (caret.y - 100.0).abs() < 16.0,
        "vertical center was {}",
        caret.y
    );
}

#[test]
fn short_sticky_keeps_authored_size_and_long_text_shrinks() {
    assert_eq!(fit_sticky_font(24.0, |_| true), 24.0);
    let fitted = fit_sticky_font(24.0, |size| size <= 12.0);
    assert!(fitted <= 12.5, "fitted {fitted}");
    assert!(fitted >= 11.0, "fitted {fitted}");
    let ctx = egui::Context::default();
    let mut sizes = None;
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        let short = measure_sticky_font(
            ctx,
            "Hi",
            Typeface::Sans,
            24.0,
            200.0,
            200.0,
            TextAlign::Center,
        );
        let long = measure_sticky_font(
            ctx,
            &"word ".repeat(80),
            Typeface::Sans,
            24.0,
            200.0,
            200.0,
            TextAlign::Center,
        );
        sizes = Some((short, long));
    });
    let (short, long) = sizes.expect("sizes");
    assert!((short - 24.0).abs() < 0.1, "short was {short}");
    assert!(long < short, "long {long} did not shrink below {short}");
    assert!(long >= slate_doc::scene::STICKY_FIT_MIN - 0.1);
}

#[test]
fn sticky_fit_is_the_same_world_size_at_every_zoom() {
    let zooms = [0.1_f32, 0.25, 0.37, 0.5, 0.73, 1.0, 1.5, 2.3, 4.0, 8.0];
    let text = "a note that has to shrink so the words stay inside the card";
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        canvas_text::clear_world_layout_cache();
        let before = canvas_text::world_layout_shapes();
        let fitted = measure_sticky_font(
            ctx,
            text,
            Typeface::Sans,
            24.0,
            160.0,
            90.0,
            TextAlign::Center,
        );
        let shaped = canvas_text::world_layout_shapes() - before;
        assert!(shaped >= 1, "fitting never shaped the note");
        let mut rows = None;
        for zoom in zooms {
            let again = measure_sticky_font(
                ctx,
                text,
                Typeface::Sans,
                24.0,
                160.0,
                90.0,
                TextAlign::Center,
            );
            assert!(
                (again - fitted).abs() < 1.0e-4,
                "fit {again} != {fitted} at zoom {zoom}"
            );
            let layout = canvas_text::world_layout(
                ctx,
                text,
                typeface_font(Typeface::Sans, fitted),
                160.0,
                egui::Align::Center,
            );
            let galley = canvas_text::zoom_galley(
                ctx,
                text,
                &layout,
                typeface_font(Typeface::Sans, fitted * zoom),
                Color32::BLACK,
                zoom,
            );
            let got: Vec<String> = galley.rows.iter().map(|row| row.text()).collect();
            match &rows {
                None => rows = Some(got),
                Some(expected) => assert_eq!(&got, expected, "breaks moved at zoom {zoom}"),
            }
        }
        assert_eq!(
            canvas_text::world_layout_shapes() - before,
            shaped,
            "zooming reshaped the fitted note"
        );
    });
}
