# Sidebar design system

Rules for the left tools rail (`ui/tools.rs`). Read `ARCHITECTURE.md` Layer 1
before adding panels.

**Core principle:** every pixel fights for its life. No gratuitous padding or
borders. Use the minimum space needed for legibility and hit targets.

## Two levels of visibility

| Mechanism | Where | Effect |
|-----------|-------|--------|
| Gear menu (`ChromeConfig.tools[]`) | Upper-left ⚙ | Show or hide an entire panel |
| Section `+` / `−` toggle (`ChromeConfig.tools_expanded[]`) | Panel header | Collapse to title row or expand body |

## Panel anatomy

```
┌─────────────────────────────┐
│ +  Section title   (hint)   │  ← collapsed capsule (tight vertical padding)
│ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─  │  ← faint group divider (inside expanded body only)
│  [same-type controls…]      │
└─────────────────────────────┘
```

Capsules use **fill only** — no outer border stroke. Group boundaries inside an
expanded panel use a **faint horizontal rule** (`sidebar_group_divider`).

### Tokens (`SidebarTokens`)

| Token | Value | Notes |
|-------|-------|-------|
| Corner radius | 6px | Soft capsule corners |
| Outer border | none | Fill distinguishes capsule from rail |
| Fill | `theme.card` | Elevated from `panel_fill` |
| Inner padding | 6×3.5 px | Horizontal × vertical (40% tighter than v1) |
| Gap between capsules | 4px | |
| Rail top lift | 6.5px | First capsule sits ~30% higher |
| Header min height | 14px | Text size unchanged; padding reduced |
| Control row height | 18px | Checkboxes, options |
| Toggle glyph | 8px | `+` / `−` |
| Row gap (body) | 2px | Between rows inside a group |
| Group divider opacity | 0.22 | `theme.line` |
| Group divider pad | 3px | Above/below the rule |
| Action stack gap | 2px | Vertically stacked buttons/checkboxes |
| Muted text | `theme.sub` | Labels, slider captions |
| Primary text | `theme.ink` | Section titles |

```rust
let p = app.palette();
let theme = SidebarTheme {
    card: p.card,
    border: p.border,
    ink: p.ink,
    sub: p.sub,
    line: p.line,
};
```

## Control-type grouping (required)

Inside every expanded panel, **group controls by type**. Separate groups with
`sidebar_control_group(..., divider_before: true, ...)`.

| Group type | Helper | Contents |
|------------|--------|------------|
| Actions | `sidebar_actions_column` | One-shot buttons + mode toggles stacked vertically, left-aligned, hover tooltips |
| Sliders | `sidebar_sliders_group` | All numeric sliders, tight vertical stack |
| Checkboxes | `sidebar_checkbox_row` | All boolean toggles together |
| Options | `sidebar_option_group` | Mutually exclusive `selectable_label` sets |
| Text input | plain `TextEdit` | Search fields — own group at top of filter panels |
| Chips | `chip()` in `ScrollArea` | Tag pills |

**Do not** mix sliders and checkboxes in the same group. **Do not** put sliders
on the same row as buttons.

### Display settings group order (reference)

1. **Actions** — Fit, Flow, Dark (stacked, left-aligned, hover explains each)
2. *divider*
3. **Sliders** — grid columns, portal threshold, row spacing
4. *divider*
5. **Checkboxes** — align image groups…
6. *divider*
7. **Options** — leader lines (bezier / orthogonal)

### Slider aesthetics

- Rail height: 2px
- Handle: ~60% smaller than v1 (`interact_size.y = 2.4`)
- Label row sits 1px below the rail; value readout right-aligned
- 1px gap between consecutive sliders in a group

### Action column aesthetics

- Vertical stack, `Align::Min` (left)
- 2px between items
- Every control gets a descriptive `on_hover_text`

## Adding a new panel

1. Add variant to `ToolPanel` in `chrome.rs`.
2. Implement section in `tools.rs` with `sidebar_section(..., first, ...)`.
3. Inside the body, use `sidebar_control_group` per control type.
4. Pass `first: bool` through `left_panel` so the first visible capsule gets
   `RAIL_TOP_LIFT`.

## Control-type decision tree

```
One-shot canvas action or orientation toggle?
  → sidebar_actions_column (stack vertically, hover tooltip)

Numeric range?
  → sidebar_sliders_group + thin_sidebar_slider(..., theme.sub)

Boolean filter/setting?
  → sidebar_checkbox_row (keep all checkboxes in one group)

Mutually exclusive modes?
  → sidebar_option_group

Color swatch + family label?
  → sidebar_family_row

Free-form text?
  → TextEdit in its own control group

Draggable tag pills?
  → chip() inside ScrollArea
```

## Do / Don't

### Do

- Pass `first` to the first visible `sidebar_section` in the rail.
- Put faint dividers between every control-type group in expanded bodies.
- Keep collapsed capsules as slim as possible (padding, not font size).
- Use hover tooltips on action controls.

### Don't

- Don't add outer borders to capsules.
- Don't use `ui.separator()` between rail capsules.
- Don't lay out Fit / Flow / Dark on one horizontal row.
- Don't interleave sliders with checkboxes or buttons.
- Don't add custom frames in `tools.rs` — extend `sidebar.rs`.

## Example

```rust
fn my_panel(app: &mut AtlasApp, ui: &mut egui::Ui, theme: SidebarTheme, first: &mut bool) {
    let mut expanded = app.active_chrome().tool_expanded(ToolPanel::MyPanel);
    if sidebar_section(
        ui,
        Id::new("tools_my_panel"),
        "My panel",
        None,
        &mut expanded,
        theme,
        *first,
        |ui| {
            sidebar_control_group(ui, theme, false, |ui| {
                if sidebar_checkbox_row(ui, &mut app.my_flag, "Enable thing") {
                    app.filter_dirty = true;
                }
            });
        },
    ) {
        app.active_chrome_mut()
            .set_tool_expanded(ToolPanel::MyPanel, expanded);
    }
    *first = false;
}
```

## Related files

| File | Role |
|------|------|
| `ui/sidebar.rs` | Primitives and tokens |
| `ui/tools.rs` | Panel implementations |
| `ui/widgets.rs` | `gear_menu`, `thin_sidebar_slider`, `chip` |
| `chrome.rs` | Panel registry and visibility state |
