//! Canvas text never reflows when the board zooms (ledger TX1).

use super::*;

const SWEEP: [f32; 15] = [
    0.25, 0.3, 0.37, 0.5, 0.61, 0.73, 0.88, 1.0, 1.17, 1.5, 1.9, 2.3, 2.8, 3.3, 4.0,
];

/// Glyph pens may differ by float error only: 0.02 world units is a fifth of
/// a pixel at zoom 4.
const TOL: f32 = 0.02;

fn sweep(h: &mut Harness, node: NodeId, mark: &str, what: &str, failures: &mut Vec<String>) {
    let out = frame_on(h, node, 1.0);
    let reference = painted_block(&out, mark, 1.0).unwrap_or_else(|| {
        panic!(
            "{what} did not paint at zoom 1; painted {:?}",
            painted_strings(&out)
        )
    });
    assert!(reference.rows.len() > 1, "{what} must wrap: {reference:?}");
    for zoom in SWEEP {
        let out = frame_on(h, node, zoom);
        match painted_block(&out, mark, zoom) {
            // Too small to read is dropped (P0.9 LOD), never clamped.
            None if zoom < 0.5 => {}
            None => failures.push(format!("{what} @ {zoom}: not painted")),
            Some(got) => {
                if let Some(drift) = block_drift(&reference, &got, TOL) {
                    failures.push(format!("{what} @ {zoom}: {drift}"));
                }
            }
        }
    }
}

#[test]
fn canvas_text_keeps_its_breaks_and_line_offsets_through_a_zoom_sweep() {
    let mut h = Harness::new("tx1_zoom_sweep");
    let board = text_zoom_board(&mut h);
    let mut failures = Vec::new();
    sweep(
        &mut h,
        board.chat,
        "Chat reply",
        "chat reply",
        &mut failures,
    );
    sweep(&mut h, board.chat, "Chat ask", "chat ask", &mut failures);
    set_chat_streaming(&mut h, board.chat, true);
    sweep(
        &mut h,
        board.chat,
        "Chat reply",
        "streaming reply",
        &mut failures,
    );
    sweep(&mut h, board.text, "Text node", "text node", &mut failures);
    sweep(
        &mut h,
        board.shape,
        "Shape text",
        "shape text",
        &mut failures,
    );
    sweep(
        &mut h,
        board.snippet,
        "Snippet card",
        "snippet card",
        &mut failures,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
