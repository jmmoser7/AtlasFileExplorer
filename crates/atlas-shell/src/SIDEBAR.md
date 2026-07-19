# Sidebar design system (legacy panel bodies)

The permanent left tools rail has been replaced by floating canvas docks. Read
`crates/atlas-shell/DOCK.md` before adding or moving tool surfaces.

This document still describes the reusable section/body primitives that many
tool popovers render inside their dock panels. Do not use it as permission to
reintroduce a layout-reserving rail.

Rules for legacy panel bodies (`ui/tools.rs`). Read `ARCHITECTURE.md` Layer 1
and `DOCK.md` before adding panels.

## Two levels of visibility

| Mechanism | Where | Effect |
|-----------|-------|--------|
| Gear menu (`ChromeConfig.tools[]`) | Upper-left ⚙ | Show or hide an entire panel |
| Section `+` / `−` toggle (`ChromeConfig.tools_expanded[]`) | Panel header | Collapse to title row or expand body |

Gear toggles are registered in `chrome::ToolPanel`. Section collapse is per-tab
session state (not persisted to disk yet).

## Adding a new panel

1. Add a variant to `ToolPanel` in `chrome.rs` (`ALL`, `label`, `default_on`).
2. Extend `tools[]` and `tools_expanded[]` array sizes in `ChromeConfig`.
3. Implement a section function in `tools.rs` using `sidebar_section`.
4. Call it from `left_panel` behind `chrome.tool(ToolPanel::YourPanel)`.
5. The gear menu picks up the new panel automatically via `ToolPanel::ALL`.

## Section card anatomy

Each panel is a bordered card via `sidebar_section` in `sidebar.rs`:

```
┌─────────────────────────────┐
│ +  Section title   (hint)   │  ← always visible header (18px)
├─────────────────────────────┤
│  controls…                  │  ← body when expanded
└─────────────────────────────┘
```

### Tokens (`SidebarTokens`)

| Token | Value | Notes |
|-------|-------|-------|
| Corner radius | 6px | Matches canvas tag radius |
| Border | 1px `theme.border` | Inside stroke on card fill |
| Fill | `theme.card` | Elevated from `panel_fill` |
| Inner padding | 8×6 px | Horizontal × vertical |
| Gap between cards | 6px | Replaces `ui.separator()` |
| Header height | 18px | Toggle + title |
| Control row height | 20px | Checkboxes, option rows |
| Toolbar row height | 22px | Primary action buttons |
| Toggle glyph size | 8px | `+` collapsed, `−` expanded |
| Row gap | 4px | Between controls inside body |
| Muted text | `theme.sub` | Labels, slider captions |
| Primary text | `theme.ink` | Section titles |

Build theme from the app palette:

```rust
let p = app.palette();
let theme = SidebarTheme {
    card: p.card,
    border: p.border,
    ink: p.ink,
    sub: p.sub,
};
```

## Control-type decision tree

```
Is it a one-shot canvas action (Fit, Flow)?
  → sidebar_toolbar_row + ui.button

Is it a boolean filter/setting with a short label?
  → sidebar_checkbox_row

Is it a numeric range?
  → sidebar_slider_block + thin_sidebar_slider(..., theme.sub)

Is it a mutually exclusive pair/small set of modes?
  → sidebar_option_group(label, theme, |ui| selectable_label …)

Is it a subsection label only?
  → sidebar_subsection_label

Is it a family/type row with a color swatch?
  → sidebar_family_row

Is it free-form text input?
  → full-width TextEdit at top of section body

Is it a list of draggable pills (tags)?
  → chip() inside ScrollArea (Tags panel pattern)
```

## Layout rules

### Do

- Keep section bodies vertically stacked with consistent 4px rhythm.
- Put primary actions in a toolbar row at the top of Display-style panels.
- Use `sidebar_option_group` for muted label + horizontal option pills.
- Put slider value readouts on the right (handled by `thin_sidebar_slider`).
- Use `theme.sub` for all secondary copy; never hardcode `gray(120)`.

### Don't

- Don't use `ui.separator()` between rail sections — card gaps replace them.
- Don't mix toolbar buttons and sliders on the same horizontal row.
- Don't use empty checkbox labels with separate text labels (use
  `sidebar_family_row` or inline checkbox labels).
- Don't add custom `Frame` styling in `tools.rs` — extend `sidebar.rs` instead.

## Example: minimal panel

```rust
fn my_panel(app: &mut AtlasApp, ui: &mut egui::Ui, theme: SidebarTheme) {
    let mut expanded = app.active_chrome().tool_expanded(ToolPanel::MyPanel);
    if sidebar_section(
        ui,
        Id::new("tools_my_panel"),
        "My panel",
        None,
        &mut expanded,
        theme,
        |ui| {
            if sidebar_checkbox_row(ui, &mut app.my_flag, "Enable thing") {
                app.filter_dirty = true;
            }
        },
    ) {
        app.active_chrome_mut()
            .set_tool_expanded(ToolPanel::MyPanel, expanded);
    }
}
```

## Visual review (Windows)

After changing sidebar layout, verify on Windows:

1. Border contrast between card fill and rail background.
2. Toggle hit target and glyph legibility at 200px rail width.
3. Display settings toolbar alignment (Fit / Flow / Dark).
4. Collapsed vs expanded header density.
5. Tags panel scroll area inside the card frame.

## Related files

| File | Role |
|------|------|
| `ui/sidebar.rs` | Primitives and tokens |
| `ui/tools.rs` | Panel implementations |
| `ui/widgets.rs` | `gear_menu`, `thin_sidebar_slider`, `chip` |
| `chrome.rs` | Panel registry and visibility state |
