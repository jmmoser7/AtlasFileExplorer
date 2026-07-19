# Live UI tuning workflow

Use this workflow when a visual feature is structurally correct but needs
human fine-tuning of spacing, geometry, typography, color, or effects.

The goal is a short loop:

1. agent creates a tokenized implementation;
2. human adjusts it against the running application;
3. the tuner saves readable project defaults;
4. normal builds embed those defaults without shipping the tuner.

## Current implementation: shared top bar

The checked-in source of truth is:

```
crates/atlas-shell/ui-tokens.toml
```

It controls the shared File Atlas and Slate top bar. Both applications consume
the same tokens; app-specific copies are forbidden.

The runtime plumbing is:

| File | Responsibility |
|------|----------------|
| `crates/atlas-shell/src/tokens.rs` | Typed token model, validation, embedded defaults |
| `crates/atlas-shell/src/tuning.rs` | Feature-gated floating editor and save action |
| `crates/atlas-shell/src/tabs.rs` | Tab rendering driven by tokens |
| `crates/atlas-shell/src/menubar.rs` | Portal/window-control rendering driven by tokens |
| `crates/atlas-shell/TOPBAR.md` | Permanent design and behavior contract |

Floating canvas docks follow the same workflow:

| File | Responsibility |
|------|----------------|
| `crates/atlas-shell/src/dock.rs` | Squircle icon dock, popover host, hover/click pinning |
| `crates/atlas-shell/DOCK.md` | Permanent dock design and behavior contract |
| `[dock]` in `ui-tokens.toml` | Icon size/gap, squircle exponent, popover size, shadows, colors |

## Fine-tuning session

### 1. Build an optimized tuner executable

Use one application as the visual workbench:

```powershell
cargo run --release -p slate --features ui-tuner
```

or:

```powershell
cargo run --release -p native-file-atlas --features ui-tuner
```

The dashboard opens automatically. The release profile is intentional: visual
and interaction performance should match the real application.

### 2. Tune live

- Change one visual concern at a time.
- Check both light and dark mode.
- Check short, long, active, inactive, hover, and multi-tab states.
- Check the smallest supported window before accepting widths or padding.
- For docks, check hover-open and click-pinned states and verify popovers do
  not reserve or shift canvas layout.
- Prefer the smallest adjustment that fixes the observed problem.

Changes apply immediately to the running process. Separate File Atlas and
Slate processes do not share live memory; they share the saved token file.

### 3. Save project defaults

Click **Save as project defaults**.

This writes `crates/atlas-shell/ui-tokens.toml`. It does not invoke Git. The
current process continues using the live values; a rebuild embeds them for
both applications.

Other actions:

- **Revert to build defaults** — restores values embedded when the executable
  was built.
- **Factory reset** — loads safe original values without saving.
- **Lock … preview open** — keeps transient UI such as dropdowns visible while
  the pointer is operating tuner controls; use the adjacent selector to choose
  which submenu/state is previewed.
- Closing the dashboard discards no already-saved values.

### 4. Build normal release applications

```powershell
cargo build --release -p native-file-atlas -p slate
```

Do not pass `ui-tuner`. Normal builds compile the dashboard to a no-op while
retaining the saved visual defaults.

### 5. Verify parity

Launch both apps and verify:

- the tuned component looks the same in File Atlas and Slate;
- light and dark mode remain usable;
- window resizing and full-screen behavior still work;
- hit targets remain large enough even if the visible graphics are slender;
- no debug dashboard appears in the normal build.

Then review the token-file diff and commit it through the normal repository
workflow.

## Adding another tunable UI area

Do not start by putting sliders over scattered constants. First establish a
clean token boundary.

1. **Identify the owner.** Shared chrome belongs in `atlas-shell`; Slate-only
   board UI belongs in Slate; backend behavior is not a UI token.
2. **Add a named token section.** Extend the appropriate checked-in TOML file
   and typed token structs. Use semantic names such as
   `section_vertical_gap`, not implementation names such as `offset_2`.
3. **Move every intended adjustment out of rendering code.** The renderer
   should consume tokens and retain only structural math.
4. **Define safe bounds and normalization.** Hand-edited files and live
   sliders must not produce negative sizes, inverted ranges, or invalid
   opacity.
5. **Add a focused editor section.** Group controls by Geometry, Typography,
   Effects, and Light/Dark Colors. Provide numeric values alongside sliders.
6. **Keep persistence explicit.** Live edits remain temporary until the user
   chooses **Save as project defaults**.
7. **Feature-gate the editor.** Normal builds must not expose tuning controls
   or source-writing behavior.
8. **Document the visual contract.** Tokens describe adjustable values; an
   architecture/design Markdown file describes invariants that sliders must
   not violate.
9. **Test both configurations.** Check the owner crate with and without the
   tuner feature, then build all affected applications.

## Guardrails

- A shared component has one token source, never per-app copies.
- The tuner must not execute `git add`, `git commit`, or `git push`.
- Do not make functional behavior, data integrity, accessibility, or command
  bindings freely tunable as visual parameters.
- Do not expose values whose independent adjustment can break layout without
  also defining normalization constraints.
- Saved TOML should remain human-readable and use rounded values.
- A screenshot is feedback, not the specification. Preserve the accompanying
  Markdown contract and interaction rules.
- Experimental controls should be deleted when the corresponding design is
  locked, unless they remain useful for future theme or accessibility work.

## Verification commands

```powershell
cargo fmt --all
cargo test -p atlas-shell --features ui-tuner
cargo check -p atlas-shell --no-default-features
cargo build --release -p native-file-atlas -p slate
```

On Windows, run these from a Visual Studio developer shell when the MSVC/SDK
environment is not already present.
