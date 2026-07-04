# Sidebar design system

Rules for the left tools rail (`ui/tools.rs`). Read `ARCHITECTURE.md` Layer 1
before adding panels.

**Core principle:** every pixel fights for its life. No gratuitous padding or
borders.

## Control-type grouping (required)

Group controls by type inside expanded panels. Separate groups with faint
horizontal rules via `sidebar_control_group(..., divider_before: true, ...)`.

| Group | Helper | Contents |
|-------|--------|----------|
| Actions | `sidebar_action_block` | Buttons stacked with visible description + hover |
| Sliders | `sidebar_sliders_group` + `thin_sidebar_slider` | All numeric sliders |
| Checkboxes | `ui.checkbox` / `sidebar_checkbox_row` | Dark mode lives here, not in actions |
| Options | `sidebar_option_group` | `selectable_label` sets |
| Date timeline | `sidebar_date_timeline` | Dual-handle date filter in basic filters |

## Slider aesthetics (`SidebarSliderStyle`)

**All sidebar sliders share one spec** — including the date timeline handles
in Basic filters and the display-settings sliders:

| Token | Value |
|-------|-------|
| Rail height | 4px |
| Rail stroke | 1.5px (`theme.border` × 0.9) |
| Handle radius | 4.5px |
| Label-to-rail gap | 0.4px (label row **above** rail) |

Display sliders: right-click → edit min/max domain. Row spacing default max
300% but can be raised via popup (`DisplaySliderDomains` on `AtlasApp`).

## Display settings group order

1. Actions — Fit, Flow (stacked + descriptions)
2. *divider*
3. Sliders
4. *divider*
5. Checkboxes — Dark, align image groups…
6. *divider*
7. Options — leader lines

## Related files

| File | Role |
|------|------|
| `ui/sidebar.rs` | Capsule + group + `SidebarSliderStyle` |
| `ui/widgets.rs` | `thin_sidebar_slider`, `sidebar_date_timeline` |
| `ui/tools.rs` | Panel bodies |
| `app/mod.rs` | `DisplaySliderDomains` |
