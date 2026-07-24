//! Slate keyboard, mouse, and navigation commands.
//!
//! **Rule:** every user-facing shortcut or input command must be registered in
//! [`SPECS`] and will appear automatically in Advanced → Commands & shortcuts.
//! See `COMMANDS.md` and `docs/keymap/` before adding or changing bindings.
//!
//! Commands are data ([`atlas_commands::CommandSpec`]): the reference UI, the
//! canvas palette, keyboard dispatch (`dispatch.rs`), and the F2 history all
//! read this one table, so they can never disagree (Constitution Art. VII).
//! Rows with `chord: None` are documentation for mouse gestures or keys that
//! are handled specially (Space/Enter repeat, Esc, Tab, arrows) or locally in
//! a view (`F` fit and `+`/`−` zoom stay in `canvas.rs`/`lens.rs` because they
//! need the freshly computed layout).

use atlas_commands::{Availability, Chord, CommandId, CommandSpec, Key, Registry, Repeat};
use eframe::egui::Ui;

pub use atlas_shell::commands::{CommandEntry, TurboPanState};

const GLOBAL: Availability = Availability::GLOBAL;
const BOARD: Availability = Availability::BOARD_VIEW;
const BOARD_SEL: Availability =
    Availability(Availability::BOARD_VIEW.0 | Availability::NEEDS_SELECTION.0);

/// Shorthand spec constructor keeping the table readable (one row per
/// argument mirrors the `CommandSpec` fields; the arg count is the point).
#[allow(clippy::too_many_arguments)]
const fn spec(
    id: &'static str,
    category: &'static str,
    name: &'static str,
    binding: &'static str,
    chord: Option<Chord>,
    repeat: Repeat,
    when: Availability,
    aliases: &'static [&'static str],
) -> CommandSpec {
    CommandSpec {
        id: CommandId(id),
        name,
        category,
        binding,
        chord,
        repeat,
        when,
        aliases,
    }
}

const fn ctrl_shift(key: Key) -> Chord {
    Chord {
        key,
        ctrl: true,
        shift: true,
        alt: false,
    }
}

/// Canonical command table. Keep grouped by category (the reference UI groups
/// on category changes), documentation rows first within their historical
/// order, dispatchable additions after.
pub static SPECS: &[CommandSpec] = &[
    // ----- Navigation -------------------------------------------------------
    spec(
        "nav.pan",
        "Navigation",
        "Pan (precise)",
        "Left-drag or right-drag on canvas background",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "nav.turbo_pan",
        "Navigation",
        "Turbo pan",
        "Ctrl + right-drag on canvas — pull away from the click point; speed scales with \
         distance; returns to zero at the origin; locked to horizontal or vertical",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "canvas.zoom",
        "Navigation",
        "Zoom",
        "Scroll wheel (pinch on trackpad)",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "canvas.pan_scroll",
        "Navigation",
        "Pan (scroll)",
        "Shift + scroll wheel",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "canvas.fit",
        "Navigation",
        "Fit view",
        "F (Grid / Venn / Lens)",
        None, // handled per-view (needs the freshly computed layout)
        Repeat::Never,
        Availability(Availability::GRID_VENN.0 | Availability::LENS.0),
        &["fit view", "zoom extents"],
    ),
    spec(
        "app.fullscreen",
        "Navigation",
        "Full-screen canvas (hide sidebar + bottom bar)",
        "F11, or ⛶ in the canvas mini menu (lower-left), or View → Full-screen canvas",
        Some(Chord::bare(Key::F11)),
        Repeat::Repeatable,
        GLOBAL,
        &[],
    ),
    spec(
        "canvas.zoom_in",
        "Navigation",
        "Zoom in / out",
        "+ / −",
        None, // handled per-view next to fit
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "canvas.minimap",
        "Navigation",
        "Minimap",
        "M",
        Some(Chord::bare(Key::M)),
        Repeat::Repeatable,
        GLOBAL,
        &["map", "overview"],
    ),
    spec(
        "canvas.tool.zoom",
        "Navigation",
        "Zoom tool",
        "Z — click = zoom in, Alt + click = out, drag = zoom window; \
         Esc or Z disarms (Board / Grid / Venn)",
        Some(Chord::bare(Key::Z)),
        Repeat::Never,
        Availability(Availability::BOARD_VIEW.0 | Availability::GRID_VENN.0),
        &["magnify", "zoom window"],
    ),
    spec(
        "canvas.search",
        "Navigation",
        "Search canvas",
        "Ctrl + F — Enter / Shift+Enter cycle matches, Esc closes",
        Some(Chord::ctrl(Key::F)),
        Repeat::Repeatable,
        GLOBAL,
        &["find", "filter"],
    ),
    spec(
        "canvas.cycle_next",
        "Navigation",
        "Cycle objects (reading order)",
        "Tab / Shift+Tab",
        None, // handled specially (Shift variant + egui focus interplay)
        Repeat::Repeatable,
        GLOBAL,
        &[],
    ),
    spec(
        "canvas.pan_arrows",
        "Navigation",
        "Pan with arrows (nothing selected)",
        "Arrow keys with no selection (Shift = faster, Board view)",
        None, // arrows are handled specially (nudge vs pan)
        Repeat::Never,
        BOARD,
        &[],
    ),
    // ----- Files ------------------------------------------------------------
    spec(
        "files.open_host",
        "Files",
        "Open host document",
        "Double-click thumbnail",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "files.item_menu",
        "Files",
        "Item context menu (assign tags)",
        "Right-click thumbnail (without dragging)",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.add_files",
        "Files",
        "Add files to workbook",
        "Drop files onto the window, or Workbook → Add files…",
        None,
        Repeat::Repeatable,
        GLOBAL,
        &["import"],
    ),
    // ----- Selection ----------------------------------------------------------
    spec(
        "select.toggle",
        "Selection",
        "Toggle in selection",
        "Ctrl + click thumbnail",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.select_all",
        "Selection",
        "Select all visible",
        "Ctrl + A",
        Some(Chord::ctrl(Key::A)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.cancel",
        "Selection",
        "Clear selection / dismiss",
        "Escape (pops one layer: draft → tool → selection → menus)",
        None, // Esc runs the cancel stack in dispatch.rs
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    // ----- Workbook -----------------------------------------------------------
    spec(
        "app.home",
        "Workbook",
        "Home",
        "Icon portal → File → Home",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.new_tab",
        "Workbook",
        "New workbook tab",
        "Ctrl + T (or Ctrl + N)",
        Some(Chord::ctrl(Key::T)),
        Repeat::Never,
        GLOBAL,
        &["new"],
    ),
    spec(
        "app.open",
        "Workbook",
        "Open workbook",
        "Ctrl + O",
        Some(Chord::ctrl(Key::O)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.save",
        "Workbook",
        "Save workbook",
        "Ctrl + S",
        Some(Chord::ctrl(Key::S)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.save_as",
        "Workbook",
        "Save workbook as…",
        "Ctrl + Shift + S",
        Some(ctrl_shift(Key::S)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    // ----- Board --------------------------------------------------------------
    spec(
        "board.tool.select",
        "Board",
        "Select tool",
        "V (Board view), or the combined Select/Pan bottom-dock button",
        Some(Chord::bare(Key::V)),
        Repeat::Repeatable,
        BOARD,
        &["arrow"],
    ),
    spec(
        "board.tool.pan",
        "Board",
        "Pan tool (hand)",
        "H, or middle-drag / Space + drag (Board view)",
        Some(Chord::bare(Key::H)),
        Repeat::Repeatable,
        BOARD,
        &["hand"],
    ),
    spec(
        "board.tool_swap",
        "Board",
        "Toggle Select ⇄ Pan",
        "Click the combined Select/Pan bottom-dock button while it is active",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.tool.frame",
        "Board",
        "Frame tool (slides)",
        "F, then click or drag (Board view)",
        Some(Chord::bare(Key::F)),
        Repeat::Repeatable,
        BOARD,
        &["slide", "artboard"],
    ),
    spec(
        "board.tools_row",
        "Board",
        "Shapes / Curve / Text tools",
        "R / O / L / T (Board view); click or hover a bottom-dock button to open its submenu",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.tool.rect",
        "Board",
        "Rectangle tool",
        "R (Board view)",
        Some(Chord::bare(Key::R)),
        Repeat::Repeatable,
        BOARD,
        &["box", "square"],
    ),
    spec(
        "board.tool.ellipse",
        "Board",
        "Ellipse tool",
        "O (Board view)",
        Some(Chord::bare(Key::O)),
        Repeat::Repeatable,
        BOARD,
        &["circle", "oval"],
    ),
    spec(
        "board.tool.line",
        "Board",
        "Line tool",
        "L (Board view)",
        Some(Chord::bare(Key::L)),
        Repeat::Repeatable,
        BOARD,
        &["segment"],
    ),
    spec(
        "board.tool.text",
        "Board",
        "Text tool",
        "T, then click (Board view); double-click text to edit",
        Some(Chord::bare(Key::T)),
        Repeat::Repeatable,
        BOARD,
        &["note", "label"],
    ),
    spec(
        "board.tool.pen",
        "Board",
        "Pen tool (freehand path)",
        "P (Board view)",
        Some(Chord::bare(Key::P)),
        Repeat::Repeatable,
        BOARD,
        &["draw", "freehand"],
    ),
    spec(
        "board.duplicate",
        "Board",
        "Duplicate",
        "Alt + drag selection, or Ctrl + D",
        Some(Chord::ctrl(Key::D)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["copy in place"],
    ),
    spec(
        "board.resize_free",
        "Board",
        "Resize (free aspect / distort)",
        "Shift + drag corner handle (corners scale proportionally by default); \
         Shift + drag edge handle locks aspect instead",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.resize_center",
        "Board",
        "Resize from center",
        "Ctrl + drag corner handle (Board view)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.draw_square",
        "Board",
        "Draw square / circle",
        "Shift + drag with Frame, Rectangle, or Ellipse tool",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.smart_guides",
        "Board",
        "Smart guides (align to objects)",
        "On by default while moving or resizing; Alt temporarily disables",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.rotate",
        "Board",
        "Rotate object",
        "Drag outside a corner handle (Board view); snaps at 45°; with 2+ selected \
         rotates the whole group about its center",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.group_resize",
        "Board",
        "Group resize (multi-selection)",
        "With 2+ selected, drag a group bounding-box handle — scales all members \
         about the opposite corner/edge; corners scale proportionally, Shift \
         distorts, Ctrl scales from center",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.crop_enter",
        "Board",
        "Crop image (enter crop mode)",
        "Double-click an image, right-click → Crop image, or Selection \
         inspector → Edit crop on canvas (images, PDF pages, video posters — \
         not 3D viewports or text snippets)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.crop",
        "Board",
        "Crop selected image",
        "C with a single croppable image selected (Board view)",
        Some(Chord::bare(Key::C)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["mask"],
    ),
    spec(
        "board.crop_window",
        "Board",
        "Crop: move the window / pan the content",
        "In crop mode, drag the edge/corner handles to mask the image in \
         place; drag inside the window (content grabber) to slide the \
         image under the mask",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.crop_finish",
        "Board",
        "Finish cropping",
        "Enter, Escape, or click outside the image (crop mode)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.open_file",
        "Board",
        "Open image file",
        "Right-click object → Open file (double-click enters crop mode instead)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.text_finish",
        "Board",
        "Finish text editing",
        "Escape, or click anywhere outside the text box",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.grid",
        "Board",
        "Board grid",
        "G or F7, or the Grid toggle in the board bottom dock",
        Some(Chord::bare(Key::G)),
        Repeat::Repeatable,
        BOARD,
        &["dots"],
    ),
    spec(
        "board.snap_grid",
        "Board",
        "Snap to grid",
        "F9, or the Snap toggle in the board bottom dock",
        Some(Chord::bare(Key::F9)),
        Repeat::Repeatable,
        BOARD,
        &["grid snap"],
    ),
    spec(
        "board.ortho",
        "Board",
        "Ortho (45° constraint)",
        "F8 — the constraint applies to drags in a later wave; Shift inverts while held",
        Some(Chord::bare(Key::F8)),
        Repeat::Repeatable,
        BOARD,
        &["orthogonal", "axis lock"],
    ),
    spec(
        "board.align",
        "Board",
        "Align / distribute objects",
        "Align menu in the board bottom dock (2+ selected)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.delete",
        "Board",
        "Delete objects",
        "Delete or Backspace",
        Some(Chord::bare(Key::Delete)),
        Repeat::Never,
        BOARD_SEL,
        &["remove"],
    ),
    spec(
        "board.nudge",
        "Board",
        "Nudge objects",
        "Arrow keys (Shift = ×10)",
        None, // arrows handled specially (nudge vs pan)
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.marquee",
        "Board",
        "Marquee select",
        "Left-drag on empty board",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.pan_gestures",
        "Board",
        "Pan board",
        "H (hand tool), middle-drag, right-drag, or Space + left-drag",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.fit",
        "Board",
        "Fit board content",
        "Home (Board view)",
        Some(Chord::bare(Key::Home)),
        Repeat::Never,
        BOARD,
        &["fit board"],
    ),
    spec(
        "board.menu",
        "Board",
        "Object menu (z-order, tags, delete)",
        "Right-click object",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "app.undo",
        "Board",
        "Undo board edit",
        "Ctrl + Z",
        Some(Chord::ctrl(Key::Z)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.redo",
        "Board",
        "Redo board edit",
        "Ctrl + Y (or Ctrl + Shift + Z)",
        Some(Chord::ctrl(Key::Y)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "board.model_lock",
        "Board",
        "Unlock / lock 3D viewport",
        "Double-click a locked .3dm model (or hover → click the padlock); \
         live viewports auto-lock after 30 s idle (the frozen view becomes \
         the slide image)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.model_orbit",
        "Board",
        "Orbit 3D model",
        "Drag inside an unlocked viewport (Rhino-style Z-up orbit)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.model_pan",
        "Board",
        "Pan 3D model",
        "Shift + drag inside an unlocked viewport",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.model_zoom",
        "Board",
        "Zoom 3D model",
        "Scroll inside an unlocked viewport",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.path_finish",
        "Board",
        "Finish path",
        "Enter or double-click (Polyline / Bezier tools)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.path_cancel",
        "Board",
        "Cancel path",
        "Escape while drawing a path",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.to_front",
        "Board",
        "Bring to front",
        "PageUp (Board view)",
        Some(Chord::bare(Key::PageUp)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["front", "raise"],
    ),
    spec(
        "board.to_back",
        "Board",
        "Send to back",
        "PageDown or Ctrl + B (Board view)",
        Some(Chord::bare(Key::PageDown)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["back", "lower"],
    ),
    spec(
        "board.copy",
        "Board",
        "Copy objects",
        "Ctrl + C (Board view; connectors between copied nodes come along)",
        Some(Chord::ctrl(Key::C)),
        Repeat::Repeatable,
        BOARD_SEL,
        &[],
    ),
    spec(
        "board.cut",
        "Board",
        "Cut objects",
        "Ctrl + X (Board view)",
        Some(Chord::ctrl(Key::X)),
        Repeat::Repeatable,
        BOARD_SEL,
        &[],
    ),
    spec(
        "board.paste",
        "Board",
        "Paste objects",
        "Ctrl + V — at the pointer when over the canvas, else at the view center; \
         repeated pastes step +24, +24",
        Some(Chord::ctrl(Key::V)),
        Repeat::Repeatable,
        BOARD,
        &[],
    ),
    spec(
        "board.paste_in_place",
        "Board",
        "Paste in place",
        "Ctrl + Shift + V — paste at the source coordinates",
        Some(ctrl_shift(Key::V)),
        Repeat::Repeatable,
        BOARD,
        &[],
    ),
    spec(
        "board.image.adjust",
        "Board",
        "Image adjustments (popover)",
        "Ctrl + U with image(s) selected (Board view)",
        Some(Chord::ctrl(Key::U)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["hue", "saturation", "brightness", "filters"],
    ),
    spec(
        "board.image.invert",
        "Board",
        "Invert image colors",
        "Ctrl + I with image(s) selected (Board view)",
        Some(Chord::ctrl(Key::I)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["negative"],
    ),
    spec(
        "board.palette",
        "Board",
        "Canvas palette",
        "Double-click empty board — type to search commands, Enter places/executes",
        None, // opened from the double-click site in board.rs
        Repeat::Never,
        BOARD,
        &["command palette"],
    ),
    // ----- board: ink tools + color state (wave 2b) -------------------------
    spec(
        "board.tool.brush",
        "Board",
        "Brush tool (expressive ink)",
        "B — freehand stroke in the foreground color; Shift+click chains a \
         straight segment from the last stroke end; Alt = eyedropper while armed",
        Some(Chord::bare(Key::B)),
        Repeat::Repeatable,
        BOARD,
        &["ink", "paint"],
    ),
    spec(
        "board.tool.eraser",
        "Board",
        "Eraser tool",
        "E — drag across ink/shape strokes to remove them (whole-stroke; \
         images, text, frames, and wires are never erased); Esc cancels",
        Some(Chord::bare(Key::E)),
        Repeat::Repeatable,
        BOARD,
        &["erase", "remove strokes"],
    ),
    spec(
        "board.tool.eyedropper",
        "Board",
        "Eyedropper",
        "I — click samples the node color under the cursor into the \
         foreground; Alt+click samples into the background",
        Some(Chord::bare(Key::I)),
        Repeat::Repeatable,
        BOARD,
        &["sample", "color picker"],
    ),
    spec(
        "board.tool.sticky",
        "Board",
        "Sticky note",
        "N, then click to place; typing starts immediately — Tab/Shift+Tab \
         while editing spawns an adjacent sticky",
        Some(Chord::bare(Key::N)),
        Repeat::Repeatable,
        BOARD,
        &["sticky", "postit"],
    ),
    spec(
        "board.tool.direct_select",
        "Board",
        "Direct select (anchors)",
        "A — click a path to edit anchors: drag anchors/segments/handles \
         (Alt breaks handle symmetry), double-click toggles corner/smooth, \
         arrows nudge (Shift ×10)",
        Some(Chord::bare(Key::A)),
        Repeat::Repeatable,
        BOARD,
        &["anchors", "direct selection", "path edit"],
    ),
    spec(
        "board.colors.default",
        "Board",
        "Default colors",
        "D — reset foreground/background to the theme ink/paper",
        Some(Chord::bare(Key::D)),
        Repeat::Repeatable,
        BOARD,
        &["reset colors"],
    ),
    spec(
        "board.colors.swap",
        "Board",
        "Swap colors",
        "X — swap foreground ⇄ background",
        Some(Chord::bare(Key::X)),
        Repeat::Repeatable,
        BOARD,
        &["exchange colors"],
    ),
    spec(
        "board.brush.width_down",
        "Board",
        "Brush / eraser width −",
        "[ — steps by the Photoshop screen-px tiers (<10:1 · 10–50:5 · \
         50–100:10 · >100:25); adjusts the eraser while E is armed",
        Some(Chord::bare(Key::OpenBracket)),
        Repeat::Repeatable,
        BOARD,
        &["thinner"],
    ),
    spec(
        "board.brush.width_up",
        "Board",
        "Brush / eraser width +",
        "] — same tiers as [",
        Some(Chord::bare(Key::CloseBracket)),
        Repeat::Repeatable,
        BOARD,
        &["thicker"],
    ),
    spec(
        "board.path.join",
        "Board",
        "Join paths",
        "Ctrl + J — two selected endpoints (A) merge or bridge; one open \
         path closes; several open paths join at nearest endpoints keeping \
         the first path's style",
        Some(Chord::ctrl(Key::J)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["join", "close path", "merge paths"],
    ),
    // ----- board: scene flags (wave 2b) --------------------------------------
    spec(
        "board.group",
        "Board",
        "Group selection",
        "Ctrl + G (2+ objects) — click any member selects the whole group; \
         Ctrl+Shift+click selects a single member",
        Some(Chord::ctrl(Key::G)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["group"],
    ),
    spec(
        "board.ungroup",
        "Board",
        "Ungroup selection",
        "Ctrl + Shift + G",
        Some(ctrl_shift(Key::G)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["ungroup"],
    ),
    spec(
        "board.hide",
        "Board",
        "Hide selection",
        "Ctrl + H — hidden objects leave paint, hit-testing, cycling, \
         present mode, and the export",
        Some(Chord::ctrl(Key::H)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["hide"],
    ),
    spec(
        "board.show_all",
        "Board",
        "Show all hidden",
        "Ctrl + Shift + H (also in the right-click menu on empty canvas)",
        Some(ctrl_shift(Key::H)),
        Repeat::Repeatable,
        BOARD,
        &["show hidden", "unhide"],
    ),
    spec(
        "board.lock",
        "Board",
        "Lock selection",
        "Ctrl + L — locked objects paint normally and stay snap targets, \
         but leave selection and edits; Ctrl+Shift+click force-selects one",
        Some(Chord::ctrl(Key::L)),
        Repeat::Repeatable,
        BOARD_SEL,
        &["lock"],
    ),
    spec(
        "board.unlock_all",
        "Board",
        "Unlock all",
        "Ctrl + Shift + L (also in the right-click menu on empty canvas)",
        Some(ctrl_shift(Key::L)),
        Repeat::Repeatable,
        BOARD,
        &["unlock"],
    ),
    spec(
        "board.subselect",
        "Board",
        "Sub-object select",
        "Ctrl + Shift + click — a single member inside a group, or a locked \
         object (grayed handles, one-off edit)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    // ----- board: connector wires (wave 2b) ----------------------------------
    spec(
        "board.wire.add",
        "Board",
        "Draw connector (wire)",
        "Hover a node edge (Select tool) → drag from a side grip; snaps to \
         grips/edges within 14 px; release on empty canvas opens the palette \
         to place-and-connect; Shift also adds; Esc cancels",
        None,
        Repeat::Never,
        BOARD,
        &["wire", "connector", "connect", "arrow"],
    ),
    spec(
        "board.wire.detach",
        "Board",
        "Detach / rewire a wire",
        "Ctrl + drag from a grip with wires (or drag a selected connector's \
         endpoint dot) — release on a grip/edge rewires, on empty canvas \
         frees the end",
        None,
        Repeat::Never,
        BOARD,
        &["rewire", "disconnect"],
    ),
    spec(
        "board.wire.move_all",
        "Board",
        "Move all wires on a grip",
        "Ctrl + Shift + drag from a grip — every wire end follows; release \
         on a target grip re-anchors all (release on empty cancels)",
        None,
        Repeat::Never,
        BOARD,
        &[],
    ),
    spec(
        "board.wire.label",
        "Board",
        "Connector label / options",
        "Double-click a connector to edit its label; right-click for \
         arrowheads, faint display, and delete",
        None,
        Repeat::Never,
        BOARD,
        &["label", "arrowhead", "faint"],
    ),
    // ----- Presentation ---------------------------------------------------------
    spec(
        "app.present",
        "Presentation",
        "Present frames as slides",
        "F5, or View → Present",
        Some(Chord::bare(Key::F5)),
        Repeat::Repeatable,
        GLOBAL,
        &["slideshow"],
    ),
    spec(
        "present.navigate",
        "Presentation",
        "Navigate slides",
        "← → / Space / PageUp / PageDown / Home / End; click sides",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "present.exit",
        "Presentation",
        "Exit presentation",
        "Escape",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.export",
        "Presentation",
        "Export HTML artifact",
        "Ctrl + E, or File → Export HTML artifact…",
        Some(Chord::ctrl(Key::E)),
        Repeat::Never, // opens a modal folder picker, like open/save
        GLOBAL,
        &[],
    ),
    // ----- Lens -------------------------------------------------------------------
    spec(
        "lens.view",
        "Lens",
        "Switch to Lens view",
        "View → Lens, or the View dock popover",
        None,
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "lens.root",
        "Lens",
        "Choose code root",
        "Lens panel → Choose…, or empty-state button",
        None,
        Repeat::Never,
        Availability::LENS,
        &[],
    ),
    spec(
        "lens.rescan",
        "Lens",
        "Rescan codebase",
        "Lens panel → Rescan",
        None,
        Repeat::Never,
        Availability::LENS,
        &[],
    ),
    spec(
        "lens.focus",
        "Lens",
        "Focus node",
        "Click a node or container header",
        None,
        Repeat::Never,
        Availability::LENS,
        &[],
    ),
    spec(
        "lens.clear_focus",
        "Lens",
        "Clear focus",
        "Escape (Lens view)",
        None,
        Repeat::Never,
        Availability::LENS,
        &[],
    ),
    spec(
        "lens.expand",
        "Lens",
        "Expand / collapse container",
        "Double-click an expandable container",
        None,
        Repeat::Never,
        Availability::LENS,
        &[],
    ),
    spec(
        "lens.open_source",
        "Lens",
        "Open source file",
        "Double-click a file or item leaf",
        None,
        Repeat::Never,
        Availability::LENS,
        &[],
    ),
    spec(
        "lens.fit",
        "Lens",
        "Fit graph to view",
        "F (Lens view)",
        None, // handled in lens.rs (needs the laid-out graph)
        Repeat::Never,
        Availability::LENS,
        &[],
    ),
    spec(
        "lens.nav",
        "Lens",
        "Pan / zoom graph",
        "Same as Grid/Venn (drag, scroll, Shift+scroll, Ctrl+right-drag turbo pan, +/−)",
        None,
        Repeat::Never,
        Availability::LENS,
        &[],
    ),
    // ----- Commands (registry meta) -------------------------------------------------
    spec(
        "app.repeat_last",
        "Commands",
        "Repeat last command",
        "Space (tap) or Enter (idle)",
        None, // Space/Enter are handled specially in dispatch.rs
        Repeat::Never,
        GLOBAL,
        &["again"],
    ),
    spec(
        "app.help",
        "Commands",
        "Commands & shortcuts",
        "F1 — opens the Advanced window on the command reference",
        Some(Chord::bare(Key::F1)),
        Repeat::Never,
        GLOBAL,
        &["help", "shortcuts", "keymap"],
    ),
    spec(
        "app.history",
        "Commands",
        "Command history window",
        "F2",
        Some(Chord::bare(Key::F2)),
        Repeat::Never,
        GLOBAL,
        &["log"],
    ),
    spec(
        "app.preferences",
        "Commands",
        "Advanced settings",
        "Ctrl + Shift + P, or Preferences → Advanced settings…",
        Some(ctrl_shift(Key::P)),
        Repeat::Never,
        GLOBAL,
        &["settings", "options"],
    ),
    spec(
        "app.properties",
        "Commands",
        "Selection inspector panel",
        "F3 — toggles the Selection dock panel",
        Some(Chord::bare(Key::F3)),
        Repeat::Repeatable,
        GLOBAL,
        &["inspector", "properties"],
    ),
];

/// Secondary chords: (chord, target command). Checked after the primary
/// registry lookup so one command can own several keys without duplicate ids.
pub static ALIAS_CHORDS: &[(Chord, CommandId)] = &[
    (Chord::ctrl(Key::N), CommandId("app.new_tab")),
    (ctrl_shift(Key::Z), CommandId("app.redo")),
    (Chord::ctrl(Key::B), CommandId("board.to_back")),
    (Chord::bare(Key::F7), CommandId("board.grid")),
    (Chord::bare(Key::Backspace), CommandId("board.delete")),
];

/// The app's registry view over [`SPECS`].
pub fn registry() -> Registry {
    Registry::new(SPECS)
}

/// Reference table for Advanced settings, rendered from [`SPECS`] so the
/// window and the dispatcher can never disagree.
pub fn shortcuts_reference_ui(ui: &mut Ui) {
    let entries: Vec<CommandEntry> = SPECS
        .iter()
        .map(|s| CommandEntry {
            category: s.category,
            name: s.name,
            binding: s.binding,
        })
        .collect();
    atlas_shell::commands::shortcuts_reference_ui(ui, &entries, "apps/slate/src/app/commands.rs");
}

#[cfg(test)]
mod tests {
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
            "canvas.fit",
            "board.fit",
            "app.select_all",
            "app.history",
            "app.help",
            "app.repeat_last",
        ] {
            let spec = reg.by_id(CommandId(id)).expect(id);
            assert_eq!(spec.repeat, Repeat::Never, "{id} must be never-repeat");
        }
    }
}
