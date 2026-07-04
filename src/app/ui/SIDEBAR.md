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

## Control-type grouping (required)

Inside every expanded panel, **group controls by type**. Separate groups with
`sidebar_control_group(..., divider_before: true, ...)` and a faint horizontal
rule.

| Group type | Helper | Contents |
|------------|--------|----------|
| Actions | `sidebar_actions_column` + `sidebar_action_block` | Buttons stacked vertically; each has a **visible description line** and hover tooltip |
| Sliders | `sidebar_sliders_group` + `thin_sidebar_slider` | All numeric sliders; unified rail/handle styling |
| Checkboxes | inline `ui.checkbox` or `sidebar_checkbox_row` | All boolean toggles together (includes Dark mode) |
| Options | `sidebar_option_group` | Mutually exclusive `selectable_label` sets |
| Text input | plain `TextEdit` | Search fields — own group |
| Chips | `chip()` in `ScrollArea` | Tag pills |

**Do not** mix sliders, checkboxes, or buttons in the same group.

### Display settings group order (reference)

1. **Actions** — Fit, Flow (stacked; description under each control)
2. *divider*
3. **Sliders** — grid columns, portal threshold, row spacing
4. *divider*
5. **Checkboxes** — Dark, align image groups to lowest datum
6. *divider*
7. **Options** — leader lines

## Slider aesthetics (`SidebarSliderStyle`)

All sidebar sliders — single- or dual-handle — share one implementation in
`thin_sidebar_slider` via `apply_sidebar_slider_style`:

| Token | Value | Notes |
|-------|-------|-------|
| Rail height | 2.5px | Same stroke weight everywhere |
| Handle interact height | 10px | Matches SidePanel resize grab scale |
| Label-to-rail gap | 0.4px | Label row sits **above** the rail |
| Between sliders | 3px | Inside `sidebar_sliders_group` |

Layout per slider:

```
grid columns                    10 wide
[===========rail===============]
```

Right-click a display-settings slider to edit its **domain** (min/max). Row
spacing defaults to 40–300% but the max can be raised (e.g. 2000%) via that
popup. Domains live on `AtlasApp::display_slider_domains`.

## Action block aesthetics

Each action uses `sidebar_action_block`:

```
[ Fit ]
Fit the entire canvas in the current view (F)
        ↕ 8px
[ Flow → ]
Toggle branch flow direction (horizontal ↔ vertical)
```

Every control also gets `on_hover_text` with the same (or fuller) explanation.

## Section capsule tokens

| Token | Value |
|-------|-------|
| Corner radius | 6px |
| Outer border | none |
| Inner padding | 6×3.5 px |
| Gap between capsules | 4px |
| Group divider opacity | 0.22 (`theme.line`) |

## Adding a new panel

1. Add variant to `ToolPanel` in `chrome.rs`.
2. Implement with `sidebar_section(..., first, ...)`.
3. Split the body into type groups with `sidebar_control_group`.
4. Pick helpers from the grouping table above.

## Do / Don't

### Do

- Keep checkboxes together (Dark belongs with other checkboxes, not actions).
- Use `SidebarSliderStyle` for every slider; never one-off rail/handle sizes.
- Put slider labels directly above their rail.
- Right-click slider domain editing for numeric limits that users may want to extend.

### Don't

- Don't put Dark mode in the actions group.
- Don't mix control types within a group.
- Don't add outer borders to capsules.

## Related files

| File | Role |
|------|------|
| `ui/sidebar.rs` | Capsule + group + slider style primitives |
| `ui/widgets.rs` | `thin_sidebar_slider`, `gear_menu`, `chip` |
| `ui/tools.rs` | Panel implementations |
| `app/mod.rs` | `DisplaySliderDomains` |
