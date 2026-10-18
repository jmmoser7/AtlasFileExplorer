//! Registered drawing and scoped portal actions. The main SPECS table is the one registry.
use super::*;

pub(super) const BRUSH_STROKE: CommandSpec = spec(
    "board.brush.stroke",
    "Board",
    "Brush stroke",
    "Drag with the Brush tool",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const PEN_LINE: CommandSpec = spec(
    "board.pen.line",
    "Board",
    "Pen straight line",
    "Shift+drag with the Pen (45° steps, Tab locks); Shift+click connects",
    None,
    Repeat::Never,
    BOARD,
    &["straight", "shift line"],
);

pub(super) const ERASER_STROKE: CommandSpec = spec(
    "board.eraser.stroke",
    "Board",
    "Eraser stroke",
    "Drag with the Eraser tool",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const JOIN: CommandSpec = spec(
    "board.path.join",
    "Board",
    "Join paths",
    "Ctrl + J — open paths join at nearest endpoints; closed shapes \
     boolean-union; an open curve in that set is treated as a ribbon \
     of its stroke weight. First selected style wins.",
    Some(Chord::ctrl(Key::J)),
    Repeat::Repeatable,
    BOARD_SEL,
    &["join", "close path", "merge paths", "union"],
);

pub(super) const DRAW: CommandSpec = spec(
    "board.paths.draw",
    "Board",
    "Draw agent paths",
    "Visibly draw Pen, Brush, line, polyline, Bezier, Arc and parametric shapes through shared result owners",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const PORTAL: CommandSpec = spec(
    "board.objects.create_portal",
    "Board",
    "Place a source portal",
    "Place a Web or File Atlas portal with explicit source authority",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const ATLAS_LIST: CommandSpec = spec(
    "portal.atlas.list",
    "Board",
    "Read Explorer entries",
    "Read a bounded page of already enumerated File Atlas entries",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const ATLAS_HIGHLIGHT: CommandSpec = spec(
    "portal.atlas.highlight",
    "Board",
    "Show an analyzed file",
    "Highlight an exact File Atlas file without changing the human selection",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const ATLAS_READ: CommandSpec = spec(
    "portal.atlas.read_file",
    "Board",
    "Read an Explorer file",
    "Read bounded content for a granted enumerated file without hydrating cloud-only files",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const WEB_READ: CommandSpec = spec(
    "portal.web.read_text",
    "Board",
    "Read visible web text",
    "Read visible page text through the existing consented portal host",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const WEB_CAPTURE: CommandSpec = spec(
    "portal.web.capture",
    "Board",
    "Capture current web view",
    "Capture the actual current consented page asynchronously",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const WEB_NAVIGATE: CommandSpec = spec(
    "portal.web.navigate",
    "Board",
    "Navigate a web portal",
    "Navigate the existing consented host without changing its authored locator",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const WEB_CLICK: CommandSpec = spec(
    "portal.web.click",
    "Board",
    "Click within a web portal",
    "Witnessed page input independent of the human cursor",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const WEB_SCROLL: CommandSpec = spec(
    "portal.web.scroll",
    "Board",
    "Scroll within a web portal",
    "Scroll the existing portal host",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const WEB_TYPE: CommandSpec = spec(
    "portal.web.type",
    "Board",
    "Type within a web portal",
    "Insert literal text in the focused portal page control",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const WEB_KEY: CommandSpec = spec(
    "portal.web.key",
    "Board",
    "Send a web portal key",
    "Send a bounded supported key through the existing portal input route",
    None,
    Repeat::Never,
    BOARD,
    &[],
);
