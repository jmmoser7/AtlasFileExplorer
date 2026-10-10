//! Labels and screen-sized text shape in units of their font size, so their
//! glyphs, breaks, and cuts follow the zoom exactly.

use super::tests::{row_text, with_ctx, ZOOM_SWEEP};
use super::*;

/// Glyph lefts of `scaled` on screen, row by row.
fn screen_glyph_x(scaled: &Scaled) -> Vec<Vec<f32>> {
    let galley = scaled.galley();
    let rows = galley.rows.iter();
    rows.map(|row| {
        row.glyphs
            .iter()
            .map(|g| g.pos.x * scaled.scale())
            .collect()
    })
    .collect()
}

#[test]
fn a_single_line_label_scales_its_glyphs_in_proportion() {
    let word = "Quarterly figures, final";
    with_ctx(|ctx| {
        let painter = ctx.layer_painter(egui::LayerId::debug());
        for base in [8.0_f32, 11.0, 14.0] {
            let at = |z: f32| {
                let font = FontId::proportional(base * z);
                layout_no_wrap(&painter, word.to_owned(), font, Color32::WHITE)
            };
            let want = screen_glyph_x(&at(1.0));
            assert_eq!(want[0].len(), word.chars().count());
            for zoom in ZOOM_SWEEP.into_iter().filter(|z| legible(base * z)) {
                let got = screen_glyph_x(&at(zoom));
                for (g, w) in got.iter().flatten().zip(want.iter().flatten()) {
                    let err = (g - w * zoom).abs();
                    assert!(
                        err < 0.05,
                        "{base}px label glyph off by {err}px at zoom {zoom}"
                    );
                }
            }
        }
    });
}

#[test]
fn a_screen_wrap_that_scales_with_the_font_breaks_in_the_same_places() {
    let text = "a wrapped canvas caption whose width is given on screen";
    with_ctx(|ctx| {
        let painter = ctx.layer_painter(egui::LayerId::debug());
        let at = |z: f32| {
            let font = FontId::proportional(12.0 * z);
            let laid = layout(&painter, text.to_owned(), font, Color32::WHITE, 120.0 * z);
            row_text(&laid.galley())
        };
        let want = at(1.0);
        assert!(want.len() > 1, "sample did not wrap");
        for zoom in ZOOM_SWEEP {
            assert_eq!(at(zoom), want, "breaks changed at zoom {zoom}");
        }
    });
}

#[test]
fn a_label_is_cut_at_the_same_glyph_at_every_zoom() {
    let name = "Quarterly_report_final_revised_v12_for_review.pdf";
    with_ctx(|ctx| {
        let spec = WorldSpec::label(FontId::proportional(11.0), 120.0);
        let layout = world_layout_spec(ctx, name, &spec);
        assert!(layout.elided && layout.lines.len() == 1);
        assert!(layout.width <= 120.0 + 0.5, "label overran its width");
        let want = row_text(&world_text(ctx, &layout, 1.0).galley());
        assert!(want[0].ends_with('…'), "{want:?}");
        for zoom in ZOOM_SWEEP {
            let again = world_layout_spec(ctx, name, &spec);
            assert!(Arc::ptr_eq(&again, &layout), "a zoom reshaped the label");
            let got = row_text(&world_text(ctx, &layout, zoom).galley());
            assert_eq!(got, want, "cut moved at zoom {zoom}");
        }
        let fits = world_layout_spec(ctx, "short", &spec);
        assert!(
            !fits.elided && fits.width < 60.0,
            "a short label is as wide as its text"
        );
    });
}
