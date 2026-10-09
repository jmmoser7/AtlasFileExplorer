# Theme sweep — October 2026

Branch: `feature/theme-sweep`. Goal: board tools and canvas-attached UI read
semantic colours from `atlas-shell` in both light and dark mode.

## New tokens

### `[theme.light]` / `[theme.dark]` (`Palette` slots)

| Slot | Role |
|------|------|
| `handle_hot` | Fillet grip and crop-handle hover fill |
| `link` / `link_hover` | Agent picker primary buttons |
| `success` | Agent/flow progress bars, live dot, Atlas journal applied marker |

### `[board_agent.light]` / `[board_agent.dark]`

Port-role chip colours: `prompt`, `geometry`, `image`, `style` (via
`Palette::agent_roles()`).

### Helpers

- `Palette::alpha(color, a)` — same RGB, explicit alpha for marquees and borders.
- `Palette::agent_roles()` — resolved agent port colours.

## Moved to palette (approx. 38 constructor literals)

| Area | Change |
|------|--------|
| `board_agent.rs` | Port roles, picker buttons, progress/live dot, pick-list rows, agent shell border |
| `board_handles.rs` | Grip hover via `palette.handle_hot` passed into paint helpers |
| `board_flow.rs` | Indeterminate progress sweep |
| `board_web.rs` | Focused/unfocused portal border; empty-state headline/detail |
| `board_tip_hud.rs` | Tip palette backdrop, idle ring, label ink |
| `board_color.rs` | Colour-wheel backdrop and snap ring |
| `present.rs` | Letterbox fill and slide chrome text |
| `ui/readouts.rs` | Missing-link warning |
| `file-atlas/mod.rs` | Staging chips, journal dot, rubber/zoom marquee fill, hit-debug harness |

Behaviour in the mode each control was originally tuned for is preserved where
reasonable; dark mode gains explicit values for handles and link blues.

## Allowlisted (`xtask/theme-allowlist.toml`)

Remaining `Color32::from_*` in scoped paths are **data or generated content**,
not chrome:

- Scene/authored rgba → `Color32` (board, path, inspector, tip preview ink)
- Colour-wheel HSV tessellation and recent swatches
- Tag colours from Slate session / workbook
- File Atlas chip fill derived from tag base + alpha
- Atlas portal mosaic hashes, web Win32 BGRA decode, web still luminance ramp
- Property-panel test fixture stroke

Unit tests under `mod tests { … }` are skipped by the linter.

## Guard

- `cargo xtask theme`
- `cargo test -p xtask --test theme` (also runs under `cargo test --workspace`)

## Human eyeball checklist

**Light mode**

- Board: select node (handle hover), fillet grip, crop mode brackets
- Agent node: port chips, “+ New chat” / “Just build” buttons, run progress bar
- Web portal: focused vs unfocused border, empty-state text
- Right-drag tip palette row
- Colour wheel (backdrop + snap ticks)
- Presentation mode letterbox
- Readout: missing links line (danger)
- File Atlas: staging chips, rubber-band selection, journal panel applied dot

**Dark mode**

- Same surfaces; confirm handle_hot and link blues read on `#0e1013` board paper
- Agent pick-list row hover/fill
- Web portal unfocused border (sub @ 150 alpha)
