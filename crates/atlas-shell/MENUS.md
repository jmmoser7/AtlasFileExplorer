# Shared menus — chrome contract

Every dropdown and right-click menu in File Atlas and Slate is one visual
language. Painting lives in `crates/atlas-shell/src/menu.rs`. Apps supply
labels, icons, and actions; they must not invent a second menu chrome.

The icon-portal flyout (`menubar.rs`) uses the same language. Portal tokens
under `[topbar.portal]` only place the flyout (width, gap, offset, close
delay). Look — fill, type, icons, dividers, shadow — is `[menu]`.

## Visual invariants

- Filleted rectangle. No visible outer border; the panel is fill + a soft
  drop shadow.
- Thin inset section rules. Dividers do not touch the panel edge.
- Each row is `[icon] [label] [shortcut | chevron]`. Icons are thin
  monochrome line-art. Submenus carry a right chevron.
- Destructive actions use the danger color on both icon and label.
- Hover is a low-contrast rounded row fill, not a hard outline.
- Light: off-white panel, charcoal type. Dark: charcoal panel, warm off-white
  type.

Window chrome and pointer-attached ghosts are the named exceptions in
`P0.9`. Menus are window chrome — they stay screen-sized.

## How to build a menu

Hand-built panels (right-click, portal):

```rust
atlas_shell::menu::frame(dark).show(ui, |ui| {
    ui.set_min_width(atlas_shell::menu::tokens().min_width);
    atlas_shell::menu::heading(ui, "3 file(s)", dark);
    atlas_shell::menu::separator(ui, dark);
    if atlas_shell::menu::item(ui, MenuIcon::Open, "Open", dark).clicked() { … }
    if atlas_shell::menu::item_danger(ui, MenuIcon::Trash, "Delete", dark).clicked() { … }
});
```

egui-built menus (`menu_button`, `context_menu`, combo boxes) inherit the
panel (radius, shadow, fill, no border) from `menu::apply_style`, called
from each app's `apply_theme`. Inside the menu closure, call
`menu::prepare(ui, dark)` so row hover and type match. Prefer `menu::item*`
over `ui.button` for new rows.

## Tuner

`[menu]` in `ui-tokens.toml`. The live editor (either app, `--features
ui-tuner`) exposes **Menus · Geometry & spacing**, **Typography**,
**Shadow**, and light/dark colors first in the dashboard.

Each of those sections starts with **Lock menu preview open** and a panel
selector (portal File / View / Preferences, or a sample right-click). Hold
one open while the pointer is on the sliders — same pattern as the dock
popover lock. Future tuner sections for transient UI must do this too
(`docs/ui-tuning-workflow.md`).

Do not add a second set of look knobs under the portal section.
