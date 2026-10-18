//! Model presentation commands; the registry remains the command identity owner.

use super::*;

pub(super) const DISPLAY: CommandSpec = spec(
    "board.model_display",
    "Board",
    "3D display mode",
    "Selection strip → Viewport display: Shaded, Arctic (white clay), Hidden line, Material mask, or Z-buffer. One undo step, live or frozen; the frozen poster uses that mode. From the palette, cycles the selected model. The image generator still receives a shaded view and a depth pass",
    None,
    Repeat::Never,
    BOARD,
    &["arctic", "clay", "hidden line", "diagram", "material id", "z-buffer", "depth", "shaded", "viewport display"],
);

pub(super) const MATERIALS: CommandSpec = spec(
    "board.model_materials",
    "Board",
    "Toggle 3D materials",
    "Selection strip → Viewport display → Materials off / on. Uses imported surface colors in Shaded and Hidden line; Arctic stays white and diagnostic passes remain unchanged. One undo step, live or frozen. From the palette, toggles the selected model",
    None,
    Repeat::Never,
    BOARD,
    &["material", "surface color", "white model", "3d"],
);

pub(super) const MEASURE: CommandSpec = spec(
    "board.model_measure",
    "Board",
    "Measure 3D model",
    "Selection strip → Measure: enters the viewport if frozen, then click two points on the model (Rhino Distance). One measurement returns to navigate; Esc peels the pending point, then Measure, then the shown measurements. Measurements are not saved",
    None,
    Repeat::Never,
    BOARD,
    &["distance", "ruler", "dimension", "3d"],
);
