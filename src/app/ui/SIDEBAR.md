# Sidebar design system

Authoritative rules for the left tools rail ([`ui/tools.rs`](tools.rs)).
Read [`ARCHITECTURE.md`](../ARCHITECTURE.md) Layer 1 before adding panels.

**Core principle:** every pixel fights for its life. No gratuitous padding, borders,
or decorative chrome. Use the minimum space needed for legibility and hit targets.

---

## Theme binding

Build `SidebarTheme` from the active app palette — never hardcode grays.

```rust
fn sidebar_theme(app: &AtlasApp) -> SidebarTheme {
    let p = app.palette();
    SidebarTheme {
        card: p.card,
        border: p.border,
        ink: p.ink,
        sub: p.sub,
        line: p.line,
    }
}
```

| Field | Role |
|-------|------|
| `card` | Capsule fill (elevated from `panel_fill`) |
| `border` | Slider rail stroke, structural accents |
| `ink` | Primary labels, active handles, section titles |
| `sub` | Secondary copy, slider captions, descriptions |
| `line` | Faint group dividers |

---

## Visibility state (`chrome.rs`)

Two independent toggles — do not conflate them.

| State | Storage | UI | Effect |
|-------|---------|-----|--------|
| Panel visible | `ChromeConfig.tools[]` | Gear menu ⚙ checkboxes | Entire capsule shown/hidden |
| Section expanded | `ChromeConfig.tools_expanded[]` | Capsule `+` / `−` header | Body collapsed to title row |

Register new panels in `ToolPanel` (`ALL`, `label`, `default_on`). Gear menu
auto-lists via `ToolPanel::ALL`.

---

## Token reference

### `SidebarTokens` — layout (`ui/sidebar.rs`)

| Constant | Value | Use |
|----------|-------|-----|
| `CORNER_RADIUS` | 6px | Capsule corners |
| `INNER_MARGIN_X` | 6px | Capsule horizontal inset |
| `INNER_MARGIN_Y` | 3.5px | Capsule vertical inset |
| `SECTION_GAP` | 4px | Space between capsules |
| `RAIL_TOP_LIFT` | 6.5px | First capsule pulled up (~30% of collapsed height) |
| `HEADER_HEIGHT` | 14px | Collapsed/expanded header row min height |
| `CONTROL_ROW_HEIGHT` | 18px | Checkbox, option, family, mode rows |
| `TOGGLE_SIZE` | 8px | Section `+` / `−` glyph |
| `ROW_GAP` | 2px | Trailing gap after checkbox rows |
| `OPTION_GAP` | 3px | Horizontal gap between option pills |
| `RIGHT_COL_WIDTH` | 60px | Right column in labeled rows |
| `GROUP_DIVIDER_OPACITY` | 0.22 | Multiplier on `theme.line` |
| `GROUP_DIVIDER_PAD` | 3px | Space above/below group divider |
| `ACTION_ITEM_PAD` | 8px | Vertical gap between action blocks |

### `SidebarSliderStyle` — all sliders (`ui/sidebar.rs`)

Shared by `thin_sidebar_slider` and `sidebar_date_timeline`. **Never override
per slider.**

| Constant | Value | Use |
|----------|-------|-----|
| `RAIL_HEIGHT` | 4px | Track thickness |
| `RAIL_STROKE` | 1.5px | Rail line weight |
| `HANDLE_RADIUS` | 4.5px | Circular handle (9px diameter) |
| `INTERACT_HEIGHT` | 11.25px | egui slider interact (`HANDLE_RADIUS × 2.5`) |
| `LABEL_GAP` | 0.4px | Caption-to-rail gap |
| `BETWEEN` | 3px | Gap between stacked sliders |

Helpers: `apply_sidebar_slider_style(ui)`, `sidebar_slider_rail_stroke(theme)`.

---

## API bindings — structure (`ui/sidebar.rs`)

| Function | Purpose |
|----------|---------|
| `sidebar_section(ui, id, title, subtitle, expanded, theme, first, body)` | Collapsible capsule; pass `first: true` for the first visible panel in the rail |
| `sidebar_control_group(ui, theme, divider_before, body)` | Type-group wrapper; set `divider_before: true` after the first group |
| `sidebar_group_divider(ui, theme)` | 1px faint rule between control-type groups |
| `sidebar_subtle_divider(ui, theme)` | Alias for `sidebar_group_divider` (used inside basic filters sub-regions) |
| `sidebar_region(ui, label, theme, body)` | Muted region heading + content (e.g. "File types", "Dates") |
| `sidebar_subsection_label(ui, label, theme)` | Muted label only, with top spacing |

## API bindings — controls (`ui/sidebar.rs`)

| Function | Control type |
|----------|--------------|
| `sidebar_actions_column(ui, body)` | Vertical stack of action blocks, 8px apart |
| `sidebar_action_block(ui, theme, description, body)` | One button + visible description line below |
| `sidebar_checkbox_row(ui, value, label)` | Full-width checkbox, 18px row, 2px trailing gap |
| `sidebar_family_row(ui, value, swatch_color, label)` | 16px checkbox column + ■ swatch + label |
| `sidebar_option_group(ui, label, theme, body)` | Muted label, then horizontal `selectable_label` row |
| `sidebar_mode_row(ui, selected, mode_label, brief, hover_detail, theme)` | Option pill + inline brief + hover tooltip |
| `sidebar_labeled_row(ui, label, theme, body)` | Label left, custom control in 60px right column |
| `sidebar_sliders_group(ui, body)` | Vertical stack of sliders, 3px apart |

## API bindings — widgets (`ui/widgets.rs`)

| Function | Control type |
|----------|--------------|
| `thin_sidebar_slider(ui, id, value, range, label, unit, hover, sub_color)` | Numeric slider; label above rail; right-click domain popup |
| `sidebar_date_timeline(ui, id, span_min, span_max, mode, …, theme)` | Single- or dual-handle date filter |
| `chip(ui, text, active, base_color)` | Tag/owner pill; draggable |
| `gear_menu(ui, id, body)` | 8px ⚙ panel visibility menu |
| `group_digits(n)` | Comma-formatted counts in labels |

## App state bindings (`app/mod.rs`)

| Type | Field | Purpose |
|------|-------|---------|
| `DisplaySliderDomains` | `grid_cols`, `portal_threshold`, `row_spacing` | `RangeInclusive<usize>` per display slider; editable via right-click popup |
| `AtlasApp` | `display_slider_domains` | Session storage for slider domains |
| `LayoutConfig` | `row_spacing_max` | Upper clamp derived from `row_spacing` domain end |

Default domains: grid `2..=30`, portal `10..=1000`, row spacing `40..=300`.

---

## Graphic presentation — by element

Each section is normative. Match these layouts and measurements exactly.

### 1. Tools rail header

```
⚙  Tools                    ← gear 8px; "Tools" small, theme.sub
│  1px gap below
```

- Gear: `gear_menu` — icon `RichText::size(8.0)`.
- Label: `RichText::new("Tools").small().color(theme.sub)`.
- Spacing after row: **1px** (`ui.add_space(1.0)`).

---

### 2. Section capsule (collapsed)

```
┌─────────────────────────────┐  ← fill: theme.card, radius 6, NO outer stroke
│ + Display settings          │  ← +/− 8px theme.sub; title strong theme.ink
└─────────────────────────────┘  ← inner pad 6×3.5px; header min-height 14px
        4px gap
┌─────────────────────────────┐
│ + Basic filters             │
└─────────────────────────────┘
```

- First capsule: top gap = `max(0, SECTION_GAP − RAIL_TOP_LIFT)` → pulls stack up.
- Header: click `+`/`−` or title toggles `tools_expanded[]`.
- Optional subtitle: right-aligned, small, `theme.sub` (Tags panel only today).

**Binding:** `sidebar_section(..., first, |ui| { ... })`

---

### 3. Group divider (inside expanded body)

```
  … controls above …
        3px
  ───────────────────────────  ← 1px rect, theme.line × 0.22
        3px
  … controls below …
```

- **Binding:** `sidebar_control_group(ui, theme, divider_before: true, ...)`.
- Never use `ui.separator()` between type groups.

---

### 4. Action button block

```
[ Fit ]                        ← default egui button, left-aligned
Fit the entire canvas… (F)     ← small, theme.sub — ALWAYS visible

        8px

[ Flow → ]
Toggle branch flow direction…
```

- Stack via `sidebar_actions_column` + `sidebar_action_block`.
- Every button **must** have:
  1. Visible description line (`theme.sub`, small).
  2. `on_hover_text` with same or fuller explanation.
- **Do not** put checkboxes or sliders in the actions group.
- **Do not** lay out action buttons on one horizontal row.

**Binding example:**

```rust
sidebar_action_block(ui, theme, "Fit the entire canvas in the current view (F)", |ui| {
    if ui.button("Fit")
        .on_hover_text("Fit the entire canvas in the current view (F)")
        .clicked()
    { /* … */ }
});
```

---

### 5. Numeric slider (`thin_sidebar_slider`)

```
grid columns              10 wide   ← label left, value+unit right; small theme.sub
         0.4px
    ●━━━━━━━━━━━━━━○               ← rail 4px; handle r=4.5px; stroke 1.5px
```

| Part | Spec |
|------|------|
| Label row | `small`, `theme.sub`; value right-aligned via `Layout::right_to_left` |
| Label → rail | **0.4px** (`SidebarSliderStyle::LABEL_GAP`) |
| Rail | Height **4px**; stroke **1.5px** at `theme.border × 0.9` |
| Handle | Circle **4.5px** radius; egui `INTERACT_HEIGHT` **11.25px** |
| Between sliders | **3px** inside `sidebar_sliders_group` |
| Hover | `on_hover_text` on slider response |
| Right-click | Opens domain popup (`id.with("domain")`); min/max `DragValue` + Apply |

**Binding example:**

```rust
sidebar_sliders_group(ui, |ui| {
    layout_changed |= thin_sidebar_slider(
        ui,
        Id::new("slider_grid_cols"),
        &mut app.grid_cols,
        &mut app.display_slider_domains.grid_cols,
        "grid columns",
        "wide",
        "Maximum controlled dimension of thumbnail grids",
        theme.sub,
    );
});
```

---

### 6. Date timeline (`sidebar_date_timeline`)

```
[+]  ●━━━━━━━━━━●               ← +/− small_button toggles single ↔ range mode
     Jan 3, 2024 — Mar 12, 2024  ← caption small theme.sub; 0.4px below track row
```

| Part | Spec |
|------|------|
| Mode toggle | `small_button` `+` / `−`; hover explains single vs range |
| Track row | Min height **22px**; rail uses **same** `SidebarSliderStyle` as numeric sliders |
| Range fill | `theme.ink × 0.18` between handles when in range mode |
| Handles | **4.5px** radius circles; idle `theme.sub`, hover/drag `theme.ink` |
| Hit target | **9×9px** square centered on handle |
| Caption | Formatted date(s), small `theme.sub`, **0.4px** gap after |

**Binding:** used inside `sidebar_region(ui, "Dates", theme, ...)`.

---

### 7. Checkbox — standard row

```
☑ align image groups to lowest datum    ← min row height 18px
        2px gap (when using sidebar_checkbox_row)
```

| Part | Spec |
|------|------|
| Row height | **18px** min |
| Label | Inline with checkbox (default egui body text) |
| Trailing gap | **2px** when via `sidebar_checkbox_row` |
| Hover | `on_hover_text` for non-obvious settings |
| Grouping | All checkboxes in **one** group — includes Dark mode |

**Bindings:** `sidebar_checkbox_row` (workflow) or bare `ui.checkbox` (display
settings checkbox group — stacked with no extra wrapper between siblings).

```
☑ Dark
☑ align image groups to lowest datum
```

Dark mode **must not** appear in the actions group.

---

### 8. Family filter row (`sidebar_family_row`)

```
☑  ■  Images (1,234)     ← 16px checkbox col; 3px gap; ■ uses Family::color()
```

| Part | Spec |
|------|------|
| Checkbox column | Fixed **16px** width, empty label |
| Swatch | `RichText::new("■").color(fam.color())` |
| Label | Default body text with comma-formatted count |
| Row height | **18px** min |
| Sub-groups | Indented ext-group checkboxes below parent family (`ui.indent`) |

---

### 9. Option group (`sidebar_option_group`)

```
leader lines                   ← small theme.sub
        1px
  bezier   orthogonal          ← selectable_label row; 3px horizontal gap
        2px
```

| Part | Spec |
|------|------|
| Group label | Small `theme.sub` |
| Options | Horizontal `selectable_label`; **3px** gap |
| Row height | **18px** min |

Used for: leader lines (display), created/modified toggle (dates region).

---

### 10. Mode row (`sidebar_mode_row`) — ghost / hide

```
● ghost   Dim unchecked items on the canvas
○ hide    Remove unchecked items from the layout
        2px between rows
```

| Part | Spec |
|------|------|
| Layout | `selectable_label` (mode name) + brief description inline, small `theme.sub` |
| Hover | Full explanation via `on_hover_text(hover_detail)` on the row |
| Row height | **18px** min |

---

### 11. Region label (`sidebar_region`)

```
File types                     ← subsection label: small theme.sub, 2px top pad
☑  ■  Images (1,234)
…
```

Used to subdivide **within** a capsule (basic filters). Separate regions with
`sidebar_subtle_divider`, not type-group dividers, when crossing filter categories
(search → file types → owner → dates → ghost/hide).

---

### 12. Text input (search)

```
┌ Search names…──────────────┐  ← full width TextEdit, default egui chrome
└────────────────────────────┘
        4px
File types
…
```

- Full capsule width: `desired_width(ui.available_width())`.
- Own block at top of basic filters — not mixed with checkboxes.

---

### 13. Chip / pill (`chip`)

```
 ┌─────────────┐
 │ tag (42)    │  ← radius 10; text 11px white; fill base or base@α90
 └─────────────┘
```

| State | Fill |
|-------|------|
| Active | `base` full opacity |
| Inactive | `base` at alpha **90** |
| Tags | `#375a7a` |
| Owners | `#5c6b8a` |

- Sense: `click_and_drag` (tags draggable onto canvas).
- Used in scroll areas (tags panel, owner region).

---

### 14. Small utility buttons

```
 all   none                     ← small_button; horizontal pair under family list
 clear tag filter              ← small_button when filter active
```

- Default egui `small_button` sizing.
- Group with related control (family toggles, chip filters).

---

## Control-type grouping (required)

Inside every expanded capsule, **one group per control type**. Order groups top
to bottom; separate with `sidebar_control_group(..., divider_before: true, ...)`.

| Type | Bindings | Example panel |
|------|----------|---------------|
| Text input | `TextEdit` | Basic filters — search |
| Regions | `sidebar_region` + content | Basic filters — file types, owner, dates |
| Actions | `sidebar_actions_column` + `sidebar_action_block` | Display — Fit, Flow |
| Sliders | `sidebar_sliders_group` + `thin_sidebar_slider` | Display — layout sliders |
| Checkboxes | `ui.checkbox` / `sidebar_checkbox_row` / `sidebar_family_row` | Display, workflow, families |
| Options | `sidebar_option_group` / `sidebar_mode_row` | Leader lines, ghost/hide, date field |
| Date timeline | `sidebar_date_timeline` | Basic filters — dates |
| Chips | `chip` in `ScrollArea` | Tags, owners |

**Never** mix sliders and checkboxes in the same group. **Never** put Dark mode
in the actions group.

### Display settings — canonical group order

1. Actions — Fit, Flow  
2. *divider*  
3. Sliders — grid columns, portal threshold, row spacing  
4. *divider*  
5. Checkboxes — Dark, align image groups to lowest datum  
6. *divider*  
7. Options — leader lines (bezier / orthogonal)

---

## Adding a new panel — checklist

1. Add `ToolPanel` variant in [`chrome.rs`](../chrome.rs); extend `tools[]` and
   `tools_expanded[]` array sizes.
2. In [`tools.rs`](tools.rs): implement `{panel}(app, ui, theme, first)` using
   `sidebar_section`.
3. Split body into type groups with `sidebar_control_group`.
4. Pass `first` through from `left_panel` (only first **visible** panel gets
   `*first == true`, then `*first = false`).
5. Pick bindings from the tables above — do not invent one-off layout in `tools.rs`.
6. Extend this doc if you introduce a new control pattern (add binding + graphic spec).

---

## Do / Don't

### Do

- Source all muted text from `theme.sub`, titles from `theme.ink`.
- Use `SidebarSliderStyle` for every slider and the date timeline.
- Put slider labels **above** the rail with **0.4px** gap.
- Give every action button a visible description **and** hover text.
- Right-click display sliders to edit domains when limits may need extending.

### Don't

- Don't add outer borders to capsules.
- Don't use `ui.separator()` between type groups inside a capsule.
- Don't lay out Fit / Flow / Dark on one row.
- Don't override `slider_rail_height`, handle size, or rail stroke per control.
- Don't add custom `Frame` styling in `tools.rs` — extend [`sidebar.rs`](sidebar.rs).

---

## Related files

| File | Role |
|------|------|
| [`ui/sidebar.rs`](sidebar.rs) | Tokens, capsules, groups, row helpers, `SidebarSliderStyle` |
| [`ui/widgets.rs`](widgets.rs) | Sliders, date timeline, chip, gear |
| [`ui/tools.rs`](tools.rs) | Panel implementations (reference layouts) |
| [`chrome.rs`](../chrome.rs) | `ToolPanel`, visibility + collapse state |
| [`app/mod.rs`](../mod.rs) | `DisplaySliderDomains`, `palette()` |
