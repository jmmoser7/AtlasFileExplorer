//! Navigation command descriptions.
use super::*;
pub(super) const TURBO_PAN: CommandSpec = spec(
    "nav.turbo_pan",
    "Navigation",
    "Turbo pan",
    "Ctrl + right-drag on canvas — pull away from the click point; speed scales with \
         distance; returns to zero at the origin; locked to horizontal or vertical",
    None,
    Repeat::Never,
    GLOBAL,
    &[],
);

pub(super) const ZOOM: CommandSpec = spec(
    "canvas.zoom",
    "Navigation",
    "Zoom",
    "Scroll wheel (pinch on trackpad)",
    None,
    Repeat::Never,
    GLOBAL,
    &[],
);

pub(super) const PAN_SCROLL: CommandSpec = spec(
    "canvas.pan_scroll",
    "Navigation",
    "Pan (scroll)",
    "Shift + scroll wheel",
    None,
    Repeat::Never,
    GLOBAL,
    &[],
);

pub(super) const ZOOM_TOOL: CommandSpec = spec(
    "canvas.tool.zoom",
    "Navigation",
    "Zoom tool",
    "Z — click = zoom in, Alt + click = out, drag = zoom window; \
         Esc or Z disarms (Board)",
    Some(Chord::bare(Key::Z)),
    Repeat::Never,
    Availability::BOARD_VIEW,
    &["magnify", "zoom window"],
);
