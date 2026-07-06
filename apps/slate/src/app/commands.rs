//! Slate keyboard, mouse, and navigation commands.
//!
//! **Rule:** every user-facing shortcut or input command must be registered in
//! [`ENTRIES`] and will appear automatically in Advanced → Commands & shortcuts.
//! See `COMMANDS.md` before adding or changing bindings.

use eframe::egui::Ui;

pub use atlas_shell::commands::{CommandEntry, TurboPanState};

/// Canonical list of documented commands. Keep sorted by category, then name.
pub const ENTRIES: &[CommandEntry] = &[
    CommandEntry {
        category: "Navigation",
        name: "Pan (precise)",
        binding: "Left-drag or right-drag on canvas background",
    },
    CommandEntry {
        category: "Navigation",
        name: "Turbo pan",
        binding: "Ctrl + right-drag on canvas — pull away from the click point; speed scales with \
                  distance; returns to zero at the origin; locked to horizontal or vertical",
    },
    CommandEntry {
        category: "Navigation",
        name: "Zoom",
        binding: "Scroll wheel (pinch on trackpad)",
    },
    CommandEntry {
        category: "Navigation",
        name: "Pan (scroll)",
        binding: "Shift + scroll wheel",
    },
    CommandEntry {
        category: "Navigation",
        name: "Fit view",
        binding: "F",
    },
    CommandEntry {
        category: "Navigation",
        name: "Full-screen canvas (hide sidebar + bottom bar)",
        binding: "F11, or ⛶ in the canvas mini menu (lower-left), or View → Full-screen canvas",
    },
    CommandEntry {
        category: "Navigation",
        name: "Zoom in / out",
        binding: "+ / −",
    },
    CommandEntry {
        category: "Files",
        name: "Open host document",
        binding: "Double-click thumbnail",
    },
    CommandEntry {
        category: "Files",
        name: "Item context menu (assign tags)",
        binding: "Right-click thumbnail (without dragging)",
    },
    CommandEntry {
        category: "Files",
        name: "Add files to workbook",
        binding: "Drop files onto the window, or Workbook → Add files…",
    },
    CommandEntry {
        category: "Selection",
        name: "Toggle in selection",
        binding: "Ctrl + click thumbnail",
    },
    CommandEntry {
        category: "Selection",
        name: "Select all visible",
        binding: "Ctrl + A",
    },
    CommandEntry {
        category: "Selection",
        name: "Clear selection / dismiss",
        binding: "Escape",
    },
    CommandEntry {
        category: "Workbook",
        name: "New workbook tab",
        binding: "Ctrl + T",
    },
    CommandEntry {
        category: "Workbook",
        name: "Open workbook",
        binding: "Ctrl + O",
    },
    CommandEntry {
        category: "Workbook",
        name: "Save workbook",
        binding: "Ctrl + S",
    },
    CommandEntry {
        category: "Workbook",
        name: "Save workbook as…",
        binding: "Ctrl + Shift + S",
    },
    CommandEntry {
        category: "Board",
        name: "Select tool",
        binding: "V (Board view), or the combined Select/Pan toolbar button",
    },
    CommandEntry {
        category: "Board",
        name: "Pan tool (hand)",
        binding: "H, or middle-drag / Space + drag (Board view)",
    },
    CommandEntry {
        category: "Board",
        name: "Toggle Select ⇄ Pan",
        binding: "Click the combined Select/Pan toolbar button while it is active",
    },
    CommandEntry {
        category: "Board",
        name: "Frame tool (slides)",
        binding: "F, then click or drag (Board view)",
    },
    CommandEntry {
        category: "Board",
        name: "Shapes / Curve / Text tools",
        binding: "R / O / L / T (Board view); click or hover a toolbar button to open its submenu",
    },
    CommandEntry {
        category: "Board",
        name: "Text tool",
        binding: "T, then click (Board view); double-click text to edit",
    },
    CommandEntry {
        category: "Board",
        name: "Duplicate",
        binding: "Alt + drag selection, or Ctrl + D",
    },
    CommandEntry {
        category: "Board",
        name: "Resize (lock aspect ratio)",
        binding: "Shift + drag corner handle (Board view)",
    },
    CommandEntry {
        category: "Board",
        name: "Resize from center",
        binding: "Ctrl + drag corner handle (Board view)",
    },
    CommandEntry {
        category: "Board",
        name: "Draw square / circle",
        binding: "Shift + drag with Frame, Rectangle, or Ellipse tool",
    },
    CommandEntry {
        category: "Board",
        name: "Smart guides (align to objects)",
        binding: "On by default while moving or resizing; Alt temporarily disables",
    },
    CommandEntry {
        category: "Board",
        name: "Rotate object",
        binding: "Drag outside a corner handle (Board view); snaps at 45°; with 2+ selected \
                  rotates the whole group about its center",
    },
    CommandEntry {
        category: "Board",
        name: "Group resize (multi-selection)",
        binding: "With 2+ selected, drag a group bounding-box handle — scales all members \
                  about the opposite corner/edge; Shift locks aspect, Ctrl scales from center",
    },
    CommandEntry {
        category: "Board",
        name: "Crop image (enter crop mode)",
        binding: "Double-click an image, right-click → Crop image, or Selection \
                  inspector → Edit crop on canvas (images, PDF pages, video posters — \
                  not 3D viewports or text snippets)",
    },
    CommandEntry {
        category: "Board",
        name: "Crop: move the window / pan the content",
        binding: "In crop mode, drag the edge/corner handles to mask the image in \
                  place; drag inside the window (content grabber) to slide the \
                  image under the mask",
    },
    CommandEntry {
        category: "Board",
        name: "Finish cropping",
        binding: "Enter, Escape, or click outside the image (crop mode)",
    },
    CommandEntry {
        category: "Board",
        name: "Open image file",
        binding: "Right-click object → Open file (double-click enters crop mode instead)",
    },
    CommandEntry {
        category: "Board",
        name: "Finish text editing",
        binding: "Escape, or click anywhere outside the text box",
    },
    CommandEntry {
        category: "Board",
        name: "Board grid / snap to grid",
        binding: "Grid / Snap grid toggles in the board toolbar",
    },
    CommandEntry {
        category: "Board",
        name: "Align / distribute objects",
        binding: "Align menu in the board toolbar (2+ selected)",
    },
    CommandEntry {
        category: "Board",
        name: "Delete objects",
        binding: "Delete or Backspace",
    },
    CommandEntry {
        category: "Board",
        name: "Nudge objects",
        binding: "Arrow keys (Shift = ×10)",
    },
    CommandEntry {
        category: "Board",
        name: "Marquee select",
        binding: "Left-drag on empty board",
    },
    CommandEntry {
        category: "Board",
        name: "Pan board",
        binding: "H (hand tool), middle-drag, right-drag, or Space + left-drag",
    },
    CommandEntry {
        category: "Board",
        name: "Fit board content",
        binding: "Home (Board view)",
    },
    CommandEntry {
        category: "Board",
        name: "Object menu (z-order, tags, delete)",
        binding: "Right-click object",
    },
    CommandEntry {
        category: "Board",
        name: "Undo / Redo board edit",
        binding: "Ctrl + Z / Ctrl + Y (or Ctrl + Shift + Z)",
    },
    CommandEntry {
        category: "Board",
        name: "Unlock / lock 3D viewport",
        binding: "Double-click a locked .3dm model (or hover → click the padlock); \
                  live viewports auto-lock after 30 s idle (the frozen view becomes \
                  the slide image)",
    },
    CommandEntry {
        category: "Board",
        name: "Orbit 3D model",
        binding: "Drag inside an unlocked viewport (Rhino-style Z-up orbit)",
    },
    CommandEntry {
        category: "Board",
        name: "Pan 3D model",
        binding: "Shift + drag inside an unlocked viewport",
    },
    CommandEntry {
        category: "Board",
        name: "Zoom 3D model",
        binding: "Scroll inside an unlocked viewport",
    },
    CommandEntry {
        category: "Presentation",
        name: "Present frames as slides",
        binding: "F5, or ▶ Present in the board toolbar",
    },
    CommandEntry {
        category: "Presentation",
        name: "Navigate slides",
        binding: "← → / Space / PageUp / PageDown / Home / End; click sides",
    },
    CommandEntry {
        category: "Presentation",
        name: "Exit presentation",
        binding: "Escape",
    },
    CommandEntry {
        category: "Presentation",
        name: "Export HTML artifact",
        binding: "Ctrl + E, or Workbook → Export artifact…",
    },
];

/// Reference table for Advanced settings.
pub fn shortcuts_reference_ui(ui: &mut Ui) {
    atlas_shell::commands::shortcuts_reference_ui(ui, ENTRIES, "apps/slate/src/app/commands.rs");
}
