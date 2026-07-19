# Unified top bar — chrome contract

Both File Atlas and Slate render **one** top chrome row. There is no separate
title bar and tab strip — they were merged into a single Chrome-style strip.

All painting, gradients, and interaction for this row lives in
`crates/atlas-shell` (`menubar.rs` + `tabs.rs`). Adjustable geometry,
typography, effects, and colors live in `crates/atlas-shell/ui-tokens.toml`.
Apps supply data through `UnifiedTopBarModel` and handle
`UnifiedTopBarResult`. **Never** paint menus, tabs, or window buttons inside
an app crate.

## Layout (left → right)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ [Icon] │ Tab₁ │ Tab₂ │ + │ ······· caption drag ······· │ — □ × │
└──────────────────────────────────────────────────────────────────────────┘
     ↑ portal trigger     ↑ inline tab strip
```

| Region | Module | Height |
|--------|--------|--------|
| Whole bar | `menubar::unified_top_bar` | `topbar.height` (30 px default) |
| Icon portal | `menubar.rs` | left zone, full bar height |
| Portal menu | `menubar.rs` | foreground overlay; never changes tab layout |
| Tabs | `tabs::tab_strip` | bottom-aligned inside bar |
| Caption | `menubar.rs` | draggable; double-click toggles maximize |
| Window buttons | `menubar.rs` | 44 px each, right edge |

Panel registration order in each app's `mod.rs::update`:

1. **Unified top bar first** — outermost panel, spans the entire viewport
2. Bottom readout bar (if not full-screen canvas)
3. Tools rail (if not full-screen canvas)

`egui` panels claim space in registration order. The top bar must therefore be
registered first; registering a side panel first crops the top bar and is a
chrome-contract violation.

## Icon portal and floating navigation

The app icon in the upper-left is the **menu portal**, not inert decoration.
Opening it must never resize, shift, or replace tabs.

| Gesture | Behavior |
|---------|----------|
| Hover icon | Floating navigation panel opens below the icon |
| Click icon | Panel opens and remains pinned |
| Hover a populated category | Submenu opens to the panel's right |
| Select an action | Action runs and both panels close |
| Click outside / Escape | Panels close |
| Leave a hover-opened panel | Panels close after a short tunable grace period |

The initial hierarchy is **File**, **Edit**, **View**, and **Preferences**.
Categories may be intentionally empty while their commands are designed.
Future categories extend `MenuSpec` in each app's `ui/menubar.rs` adapter.

### Portal visual language

- Rounded floating main panel and submenu with a fine border.
- Soft, broad drop shadow; no hard detached outline.
- Compact vertical rows with generous left/right breathing room.
- Hover uses a low-contrast rounded row fill.
- Labels align left; shortcuts align right; submenu chevrons use the right
  edge.
- Light mode uses a translucent near-white surface and dark neutral text.
- Dark mode uses a near-black neutral surface and slightly warm off-white
  text.
- Geometry, typography, shadow, close delay, and all light/dark colors live
  under `topbar.portal` in `ui-tokens.toml`.

Apps must **not** add a second always-visible File/View row or paint their own
portal panels.

## Tab strip aesthetics

Tabs use a Chrome-style visual language shared across light and dark mode.
Colors come from `TabChromeColors::from_palette` — never hardcode tab fills in
apps.

### Active tab

- Fill matches the canvas background (`Palette::bg`) so the tab **seamlessly
  blends** into the workspace below.
- A 4 px field remains between the absolute window top and tab top
  (`topbar.tab_top_inset`).
- Top corners: 5 px radius (`topbar.tab_top_radius`).
- Bottom shoulders: concave 7 px fillets that flare outward at the baseline
  (`topbar.tab_shoulder_radius`).
- Vertical gradient: slightly lighter at the top (`active_top` → `active`).
- Accent stroke: a three-pass `Palette::accent` glow (soft falloff, mid glow,
  crisp 1 px core) follows the complete top-and-shoulder silhouette.
- Inner highlight: a faint white emboss just beneath the top edge.

### Inactive tabs

- **No fill** — label text sits directly on the bar gradient (`Palette::sub`,
  brightening slightly on hover). Only the active tab gets a shaped fill.
- A subtle 1 px vertical divider appears **between adjacent inactive tabs**
  only (not beside the active tab).

### Bar background

- Subtle top-to-bottom gradient (`bar_top` → `bar`), distinct from the active
  tab fill.

### Light mode

Same structure; `TabChromeColors::from_palette` shifts to lighter greys. Accent
stroke still uses `Palette::accent`.

## App adapter pattern

Each app implements **one** function in `ui/menubar.rs`:

```rust
pub fn top_bar(app: &mut MyApp, ctx: &egui::Context) {
    let menus = [ /* MenuSpec … */ ];
    let tabs: Vec<TabSpec> = /* from app tab state */;
    let result = menubar::unified_top_bar(ctx, &palette, UnifiedTopBarModel { … });
    // match result.menu_clicked …
    // match result.tab_action …
}
```

`ui/mod.rs` exposes `draw_top_bar` only. Do not reintroduce separate
`draw_menu_bar` / `draw_top_chrome` calls.

## Full-screen canvas

When `ChromeConfig::canvas_fullscreen` is true (F11 / View menu / ⛶), the
tools rail and readout bar hide. The unified top bar **always** remains.

## Manual tokens and live tuning

The reusable project workflow is documented in
`docs/ui-tuning-workflow.md`.

The canonical, human-editable source is:

```
crates/atlas-shell/ui-tokens.toml
```

It contains every intentionally adjustable top-bar value: dimensions, fillet
radii, padding, tab width limits, text sizes, glow widths/opacities, and
light/dark RGBA values. Normal builds embed this file and contain no editor.

For an optimized build with the temporary live dashboard:

```powershell
cargo run --release -p native-file-atlas --features ui-tuner
cargo run --release -p slate --features ui-tuner
```

The dashboard opens automatically and provides:

- sliders with numeric entry for geometry, typography, and effects;
- color picker plus explicit R/G/B/A entries for both themes;
- live preview;
- **Lock portal preview open** at the start of every portal tuning section,
  with a File/View/Preferences submenu selector;
- **Revert to build defaults** (the TOML embedded by the current executable);
- **Factory reset** (safe unsaved defaults);
- **Save as project defaults** (writes `ui-tokens.toml`; rebuild to embed).

The application deliberately does not run Git commands. Review and commit the
token-file diff through the normal repository workflow. Builds without the
`ui-tuner` feature call a no-op and show no dashboard.

## Tests

- `menubar.rs` unit tests: window button geometry.
- App headless tests (`app::tests`): tab switch/close through the unified bar.

When changing top-bar layout or portal behavior, run:

```powershell
cargo test -p atlas-shell
cargo test -p native-file-atlas app::tests
cargo test -p slate app::tests
```
