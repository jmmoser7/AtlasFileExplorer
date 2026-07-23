//! Keyboard, mouse, and navigation commands.
//!
//! **Rule:** every user-facing shortcut or input command must be registered in
//! [`SPECS`] and will appear automatically in Advanced → Commands & shortcuts.
//! The same table drives keyboard dispatch (`mod.rs` → `hotkeys`) through
//! [`REGISTRY`], so bindings can never drift from documentation. See
//! `COMMANDS.md` and `docs/keymap/KEYMAP.md` before adding or changing
//! bindings.

use atlas_commands::{Availability, Chord, CommandId, CommandSpec, Key, Registry, Repeat};
use eframe::egui;

pub use atlas_shell::commands::{CommandEntry, TurboPanState};

const ATLAS: Availability = Availability::ATLAS;
const GLOBAL: Availability = Availability::GLOBAL;
const ATLAS_SEL: Availability = Availability::ATLAS.union(Availability::NEEDS_SELECTION);

#[allow(clippy::too_many_arguments)]
const fn spec(
    id: &'static str,
    name: &'static str,
    category: &'static str,
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

/// A mouse gesture or otherwise non-key-drivable command: reference-UI row
/// only, never dispatched by chord and never a repeat target.
const fn gesture(
    id: &'static str,
    name: &'static str,
    category: &'static str,
    binding: &'static str,
    when: Availability,
) -> CommandSpec {
    spec(id, name, category, binding, None, Repeat::Never, when, &[])
}

/// Canonical command table. Grouped by category (Navigation, Files, Filters,
/// Selection, Workflow) — the Advanced reference renders groups in this
/// declaration order.
pub const SPECS: &[CommandSpec] = &[
    // ---- Navigation ----
    gesture(
        "canvas.pan",
        "Pan (precise)",
        "Navigation",
        "Left-drag on canvas background, or right-drag anywhere (even over a thumbnail)",
        ATLAS,
    ),
    gesture(
        "canvas.pan_turbo",
        "Turbo pan",
        "Navigation",
        "Ctrl + right-drag on canvas — pull away from the click point; speed scales with \
         distance; returns to zero at the origin; locked to horizontal or vertical",
        ATLAS,
    ),
    gesture(
        "canvas.zoom_wheel",
        "Zoom",
        "Navigation",
        "Scroll wheel (pinch on trackpad)",
        ATLAS,
    ),
    gesture(
        "canvas.pan_scroll",
        "Pan (scroll)",
        "Navigation",
        "Shift + scroll wheel",
        ATLAS,
    ),
    // Arrow panning is held-key motion, applied per frame in `hotkeys` —
    // chord `None` keeps it out of the press-event dispatch path.
    gesture(
        "canvas.pan_arrows",
        "Pan (arrow keys)",
        "Navigation",
        "Arrow keys (Shift = ×4 speed)",
        ATLAS,
    ),
    gesture(
        "canvas.zoom_to_point",
        "Zoom to point",
        "Navigation",
        "Double-click empty canvas",
        ATLAS,
    ),
    spec(
        "canvas.fit",
        "Fit view",
        "Navigation",
        "F",
        Some(Chord::bare(Key::F)),
        Repeat::Never,
        ATLAS,
        &[],
    ),
    spec(
        "app.fullscreen",
        "Full-screen canvas (hide sidebar + bottom bar)",
        "Navigation",
        "F11, or ⛶ in the canvas mini menu (lower-left), or View → Full-screen canvas",
        Some(Chord::bare(Key::F11)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "canvas.zoom_in",
        "Zoom in",
        "Navigation",
        "+",
        Some(Chord::bare(Key::Plus)),
        Repeat::Never,
        ATLAS,
        &[],
    ),
    spec(
        "canvas.zoom_out",
        "Zoom out",
        "Navigation",
        "−",
        Some(Chord::bare(Key::Minus)),
        Repeat::Never,
        ATLAS,
        &[],
    ),
    spec(
        "canvas.minimap",
        "Minimap",
        "Navigation",
        "M",
        Some(Chord::bare(Key::M)),
        Repeat::Repeatable,
        ATLAS,
        &["map", "overview"],
    ),
    spec(
        "canvas.tool.zoom",
        "Zoom tool",
        "Navigation",
        "Z — click = zoom in, Alt + click = out, drag = zoom window; Esc or Z disarms",
        Some(Chord::bare(Key::Z)),
        Repeat::Never,
        ATLAS,
        &["magnify"],
    ),
    spec(
        "canvas.cycle_next",
        "Cycle to next filtered file",
        "Navigation",
        "Tab",
        Some(Chord::bare(Key::Tab)),
        Repeat::Never,
        ATLAS,
        &[],
    ),
    spec(
        "canvas.cycle_prev",
        "Cycle to previous filtered file",
        "Navigation",
        "Shift + Tab",
        Some(Chord {
            key: Key::Tab,
            ctrl: false,
            shift: true,
            alt: false,
        }),
        Repeat::Never,
        ATLAS,
        &[],
    ),
    // ---- Files ----
    spec(
        "atlas.open_selected",
        "Open host document",
        "Files",
        "Double-click thumbnail (repeatable for the selected file)",
        None,
        Repeat::Repeatable,
        ATLAS_SEL,
        &["open"],
    ),
    gesture(
        "atlas.context_menu",
        "File context menu",
        "Files",
        "Right-click file or folder (without dragging)",
        ATLAS,
    ),
    spec(
        "atlas.copy_paths",
        "Copy file paths",
        "Files",
        "Ctrl + C",
        Some(Chord::ctrl(Key::C)),
        Repeat::Repeatable,
        ATLAS_SEL,
        &["copy", "clipboard"],
    ),
    spec(
        "app.properties",
        "Details (single selected file)",
        "Files",
        "F3",
        Some(Chord::bare(Key::F3)),
        Repeat::Repeatable,
        ATLAS_SEL,
        &["details", "inspector"],
    ),
    // ---- Filters ----
    gesture(
        "filters.date_fit",
        "Date timeline fit",
        "Filters",
        "Double-click date timeline",
        ATLAS,
    ),
    gesture(
        "filters.date_pan",
        "Date timeline pan",
        "Filters",
        "Drag date timeline background",
        ATLAS,
    ),
    gesture(
        "filters.date_scrub",
        "Date range scrub",
        "Filters",
        "Drag between the two date handles (keeps the window width)",
        ATLAS,
    ),
    gesture(
        "filters.date_zoom",
        "Date timeline zoom",
        "Filters",
        "Scroll wheel over date timeline (zooms around cursor, down to minute marks)",
        ATLAS,
    ),
    gesture(
        "filters.date_range",
        "Filter by date range",
        "Filters",
        "Drag either date handle on the timeline",
        ATLAS,
    ),
    spec(
        "canvas.search",
        "Focus search",
        "Filters",
        "Ctrl + F (Esc returns focus to the canvas)",
        Some(Chord::ctrl(Key::F)),
        Repeat::Never,
        ATLAS,
        &["find", "filter"],
    ),
    // ---- Selection ----
    gesture(
        "atlas.select_rubber",
        "Rubber-band select",
        "Selection",
        "Shift + left-drag on canvas",
        ATLAS,
    ),
    gesture(
        "atlas.select_toggle",
        "Toggle in selection",
        "Selection",
        "Ctrl + click file",
        ATLAS,
    ),
    gesture(
        "atlas.select_range",
        "Range select",
        "Selection",
        "Shift + click file",
        ATLAS,
    ),
    spec(
        "app.select_all",
        "Select all visible",
        "Selection",
        "Ctrl + A",
        Some(Chord::ctrl(Key::A)),
        Repeat::Never,
        ATLAS,
        &[],
    ),
    // Escape is dispatched through the cancel stack in `hotkeys`, not the
    // chord table (it must fall through layer by layer) — chord stays None.
    spec(
        "app.cancel",
        "Cancel (menu → edit panel → details → zoom tool → selection)",
        "Selection",
        "Escape",
        None,
        Repeat::Never,
        GLOBAL,
        &["escape", "dismiss"],
    ),
    // ---- Workflow ----
    gesture(
        "app.home",
        "Home",
        "Workflow",
        "Icon portal → File → Home",
        GLOBAL,
    ),
    spec(
        "app.open",
        "Open folders",
        "Workflow",
        "Ctrl + O",
        Some(Chord::ctrl(Key::O)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.new_tab",
        "New tab",
        "Workflow",
        "Ctrl + N",
        Some(Chord::ctrl(Key::N)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "atlas.assign",
        "Assign selection",
        "Workflow",
        "F2",
        Some(Chord::bare(Key::F2)),
        Repeat::Repeatable,
        ATLAS_SEL,
        &["destination", "stage"],
    ),
    spec(
        "atlas.export",
        "Export assigned files",
        "Workflow",
        "Bottom tray → Export",
        None,
        Repeat::Never,
        ATLAS,
        &[],
    ),
    spec(
        "app.undo",
        "Undo",
        "Workflow",
        "Ctrl + Z",
        Some(Chord::ctrl(Key::Z)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.redo",
        "Redo",
        "Workflow",
        "Ctrl + Y (Ctrl + Shift + Z)",
        Some(Chord::ctrl(Key::Y)),
        Repeat::Never,
        GLOBAL,
        &[],
    ),
    spec(
        "app.repeat_last",
        "Repeat last command",
        "Workflow",
        "Space (tap) / Enter (idle)",
        None,
        Repeat::Never,
        GLOBAL,
        &["again"],
    ),
    spec(
        "app.history",
        "Command history",
        "Workflow",
        "Advanced → Command history",
        None,
        Repeat::Never,
        GLOBAL,
        &["log"],
    ),
    spec(
        "app.help",
        "Commands & shortcuts",
        "Workflow",
        "F1",
        Some(Chord::bare(Key::F1)),
        Repeat::Never,
        GLOBAL,
        &["help", "shortcuts"],
    ),
    spec(
        "app.preferences",
        "Advanced settings",
        "Workflow",
        "Ctrl + Shift + P",
        Some(Chord {
            key: Key::P,
            ctrl: true,
            shift: true,
            alt: false,
        }),
        Repeat::Never,
        GLOBAL,
        &["settings", "advanced"],
    ),
];

/// The registry every consumer reads: chord dispatch, the Advanced
/// reference, history repeat, and (later) the palette/agent surface.
pub const REGISTRY: Registry = Registry::new(SPECS);

/// Map an egui key to the registry's renderer-free key enum (Constitution
/// Art. I: `atlas-commands` never sees egui types).
pub fn map_key(key: egui::Key) -> Option<Key> {
    use egui::Key as E;
    Some(match key {
        E::A => Key::A,
        E::B => Key::B,
        E::C => Key::C,
        E::D => Key::D,
        E::E => Key::E,
        E::F => Key::F,
        E::G => Key::G,
        E::H => Key::H,
        E::I => Key::I,
        E::J => Key::J,
        E::K => Key::K,
        E::L => Key::L,
        E::M => Key::M,
        E::N => Key::N,
        E::O => Key::O,
        E::P => Key::P,
        E::Q => Key::Q,
        E::R => Key::R,
        E::S => Key::S,
        E::T => Key::T,
        E::U => Key::U,
        E::V => Key::V,
        E::W => Key::W,
        E::X => Key::X,
        E::Y => Key::Y,
        E::Z => Key::Z,
        E::F1 => Key::F1,
        E::F2 => Key::F2,
        E::F3 => Key::F3,
        E::F4 => Key::F4,
        E::F5 => Key::F5,
        E::F6 => Key::F6,
        E::F7 => Key::F7,
        E::F8 => Key::F8,
        E::F9 => Key::F9,
        E::F10 => Key::F10,
        E::F11 => Key::F11,
        E::F12 => Key::F12,
        E::ArrowUp => Key::ArrowUp,
        E::ArrowDown => Key::ArrowDown,
        E::ArrowLeft => Key::ArrowLeft,
        E::ArrowRight => Key::ArrowRight,
        E::Space => Key::Space,
        E::Enter => Key::Enter,
        E::Escape => Key::Escape,
        E::Tab => Key::Tab,
        E::Delete => Key::Delete,
        E::Backspace => Key::Backspace,
        E::Home => Key::Home,
        E::End => Key::End,
        E::PageUp => Key::PageUp,
        E::PageDown => Key::PageDown,
        E::OpenBracket => Key::OpenBracket,
        E::CloseBracket => Key::CloseBracket,
        E::Comma => Key::Comma,
        E::Period => Key::Period,
        // Both the dedicated + key and Shift+= reach zoom-in.
        E::Plus | E::Equals => Key::Plus,
        E::Minus => Key::Minus,
        _ => return None,
    })
}

/// Reference table for Advanced settings, rendered straight from [`SPECS`].
pub fn shortcuts_reference_ui(ui: &mut egui::Ui) {
    let rows: Vec<CommandEntry> = SPECS
        .iter()
        .map(|s| CommandEntry {
            category: s.category,
            name: s.name,
            binding: s.binding,
        })
        .collect();
    atlas_shell::commands::shortcuts_reference_ui(ui, &rows, "apps/file-atlas/src/app/commands.rs");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_validates() {
        REGISTRY
            .validate()
            .expect("SPECS has duplicate ids or ambiguous chords");
    }

    #[test]
    fn shared_never_repeat_set_is_honored() {
        for id in [
            "app.undo",
            "app.redo",
            "app.open",
            "app.select_all",
            "app.cancel",
            "canvas.zoom_in",
            "canvas.zoom_out",
            "canvas.fit",
            "app.help",
            "app.preferences",
            "app.new_tab",
            "app.repeat_last",
        ] {
            let spec = REGISTRY.by_id(CommandId(id)).expect(id);
            assert!(
                matches!(spec.repeat, Repeat::Never),
                "`{id}` must be Repeat::Never"
            );
        }
    }

    #[test]
    fn atlas_repeatables_are_repeatable() {
        for id in ["atlas.assign", "atlas.open_selected", "app.properties"] {
            let spec = REGISTRY.by_id(CommandId(id)).expect(id);
            assert!(
                matches!(spec.repeat, Repeat::Repeatable),
                "`{id}` must be Repeat::Repeatable"
            );
        }
    }

    #[test]
    fn selection_commands_require_selection() {
        let no_sel = Availability::ATLAS | Availability::GLOBAL;
        let with_sel = no_sel | Availability::NEEDS_SELECTION;
        // F2 only resolves while something is selected.
        assert!(REGISTRY.by_chord(Chord::bare(Key::F2), no_sel).is_none());
        assert_eq!(
            REGISTRY
                .by_chord(Chord::bare(Key::F2), with_sel)
                .unwrap()
                .id
                .0,
            "atlas.assign"
        );
        // Ctrl+C likewise (a focused text field keeps its own copy anyway).
        assert!(REGISTRY.by_chord(Chord::ctrl(Key::C), no_sel).is_none());
    }
}
