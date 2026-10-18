//! Curve gesture commands; durable curve styles remain in slate-doc.
use super::*;

pub(super) const FADE: CommandSpec = spec(
    "board.curve.fade",
    "Board",
    "Fade curve ink",
    "Curve cap menu → Fade: ink stays solid for 0.5 seconds, then fades over 0.5 seconds",
    None,
    Repeat::Never,
    BOARD,
    &["fade", "temporary", "intent ink"],
);

pub(super) const ARROW_SIZE: CommandSpec = spec(
    "board.path.arrow_size",
    "Board",
    "Size curve arrowhead",
    "Direct selection: drag an arrowhead base for length or wing for width; Escape restores",
    None,
    Repeat::Never,
    BOARD_SEL,
    &["arrowhead", "arrow", "length", "width"],
);
