//! Image crop, save and result actions, including range selection.
use super::*;

pub(super) const CROP: CommandSpec = spec(
    "board.crop",
    "Board",
    "Crop selected image",
    "C with one or more croppable images selected (Board view)",
    Some(Chord::bare(Key::C)),
    Repeat::Repeatable,
    BOARD_SEL,
    &["mask"],
);

pub(super) const COPY: CommandSpec = spec(
    "portal.agent.result.copy",
    "Image results",
    "Copy selected image results",
    "Click or right-click a result thumbnail → Copy; offers image pixels, linked files and an editable Slate copy",
    None,
    Repeat::Never,
    BOARD,
    &["copy images", "album", "clipboard"],
);
pub(super) const SAVE: CommandSpec = spec(
    "portal.agent.result.save_as",
    "Image results",
    "Save selected image results as",
    "Result thumbnail → Save as…; several images first open batch naming, then the native folder picker",
    None,
    Repeat::Never,
    BOARD,
    &["save images", "batch name", "album"],
);
pub(super) const IMAGE_SAVE: CommandSpec = spec(
    "board.image.save_as",
    "Board",
    "Save image as",
    "Right-click an image → Save image as…; save its displayed crop, filters and paint to a new numbered file",
    None,
    Repeat::Never,
    BOARD,
    &["save photo", "save picture", "export image"],
);
pub(super) const CANVAS: CommandSpec = spec(
    "portal.agent.result.copy_to_canvas",
    "Image results",
    "Copy selected results to canvas",
    "Result thumbnail → Copy to canvas, or drag a result from either history surface onto the board",
    None,
    Repeat::Never,
    BOARD,
    &["result", "duplicate", "drag image"],
);
pub(super) const RANGE: CommandSpec = spec(
    "portal.agent.result.select_range",
    "Image results",
    "Select a range of image results",
    "Shift-click a result thumbnail: highlight the contiguous range from the previous pick; Copy and Save as act on the range",
    None,
    Repeat::Never,
    BOARD,
    &["shift", "range", "multiple images"],
);
