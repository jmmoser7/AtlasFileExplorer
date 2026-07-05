# Agent instructions — Atlas ecosystem (native)

Rust + egui Windows desktop apps for visual file organization at scale. The
repo is a **Cargo workspace** containing two launchable applications built on
shared crates:

- **File Atlas** (`apps/file-atlas`, binary `native-file-atlas`) — folder
  scanning, tree canvas, destination assignment / export workflow.
- **Slate** (`apps/slate`, binary `slate`) — tag-driven workbooks (`.slate`
  files) that link files (never copies) and present their thumbnails as a
  tag-grouped grid, literal Venn diagrams, or an authored **Board** (frames,
  shapes, text, placed images) that presents as slides and exports an HTML
  artifact. Tagging lives *only* in Slate; Atlas offers Slate tags in its
  right-click menu during a linked session.

## Workspace layout

| Crate | Role | Safe to edit in parallel |
|-------|------|--------------------------|
| `crates/atlas-core` | UI-free backend: types, scanner, SQLite index, thumbnail pool + cache tiers, tree layout, journal, export, watcher | Yes |
| `crates/atlas-shell` | **Shared window chrome**: theme/Palette, tab strip, sidebar primitives, widgets, panel registry, command reference | Yes — but see the chrome rule below |
| `crates/atlas-session` | In-process bridge for linked Slate⇄Atlas sessions | Yes |
| `crates/atlas-ai` | AI / Cursor integration: shared AI-workspace config, Cursor launcher, live-link context beacon, the sidebar AI panel body | Yes |
| `crates/slate-doc` | `.slate` document model: faceted tag system + the board scene graph (`scene.rs`: nodes, CSS-constrained styles, invertible `SceneCmd` journal) | Yes |
| `crates/slate-artifact` | HTML artifact writer: scene → slides, styles → CSS, embedded JS slide runtime. Export is serialization, not conversion | Yes |
| `crates/circle-pack` | Pure geometry: circle packing + Venn layout | Yes |
| `apps/file-atlas` | Atlas app: canvas + app state (`src/app/mod.rs` is the integration point) | Coordinate on `mod.rs` |
| `apps/slate` | Slate app: canvas, tagging sidebar, session host | Coordinate on `app/mod.rs` |

Read `apps/file-atlas/src/app/ARCHITECTURE.md` and
`apps/slate/src/app/ARCHITECTURE.md` before UI changes.

## The shared-chrome rule (no divergence)

Both apps must look and feel identical. This is enforced structurally:

1. **All chrome painting lives in `atlas-shell`** — tab shapes, palette,
   sidebar section cards, widgets, gear menus. Apps supply *data* (tab specs,
   panel sets, command entries) and react to returned actions.
2. **Never define chrome colors, tab painting, or sidebar layout primitives
   inside an app crate.** If an app needs a new chrome capability, add it to
   `atlas-shell` so the other app gets it too.
3. Panel *sets* (which sections exist) and canvas internals are app-specific
   by design; their *rendering primitives* are not.
4. Both apps must stay on the same egui/eframe version — dependency versions
   are pinned once in the workspace `Cargo.toml` (`[workspace.dependencies]`);
   member crates must use `{ workspace = true }`.

## Commands & shortcuts

Read the app's `COMMANDS.md` before adding keyboard or mouse bindings. Every
user-facing command must be registered in that app's `commands.rs` (`ENTRIES`)
so it appears in **Advanced → Commands & shortcuts**.

## Build & test (Windows — primary target)

```powershell
cargo test --workspace
cargo build --release -p native-file-atlas -p slate
```

Release binaries: `target/release/native-file-atlas.exe` and
`target/release/slate.exe`. Atlas requires `vendor/pdfium.dll` for PDF
previews. Slate registers the `.slate` file association (per-user, HKCU) on
first run and embeds `apps/slate/assets/slate.ico`.

## Linked sessions (Slate ⇄ Atlas)

"Open File Atlas" inside Slate hosts Atlas as a **second viewport of the
Slate process** (`egui` multi-viewport). The apps communicate through
`crates/atlas-session` (`SharedSession`): Slate publishes tag groups, Atlas
queues tag assignments and cross-window drag payloads. Both binaries still
run standalone; the bridge is `None` outside sessions.

## AI integration (Cursor)

Both apps expose an optional, collapsible **AI** panel in the left tools rail.
Its body is rendered by `crates/atlas-ai` (`ui::ai_body`) so the panel stays
identical in both apps — extend it there, never per-app. The crate owns:

- the shared **AI workspace** folder (persisted in `ai-config.json` next to
  the index DB; the user must establish it before the first Cursor launch, and
  it becomes Cursor's working directory when launched from either app);
- the **live link**: each app writes `<workspace>/.atlas-ai/<app>-context.json`
  (open root/workbook, selection, in-view files) — the contract future MCP
  servers read to give Cursor full view of Atlas/Slate state.

## The Board (Slate's presentation generator)

Two structural rules keep the board honest — hold both when extending it:

1. **The scene model is constrained to CSS.** `slate-doc::scene` only holds
   styling that HTML+CSS can express; `apps/slate/src/app/board.rs` (egui
   painter) and `crates/slate-artifact` (HTML writer) are two interpreters of
   that one model, and `imagefx.rs` mirrors the CSS filter math on pixels.
   A new board style property must land in all three, or not at all.
2. **All board mutations are invertible `SceneCmd`s** committed through the
   tab's `SceneJournal` (undo/redo now; the MCP agent surface later). UI code
   must not mutate `doc.scene` outside a journaled path
   (`patch_nodes` / `add_nodes` / `delete_board_nodes` / `commit_scene`).

Frames are slides (geometric membership, `order` = deck sequence, optional
tag assignments inherited by dropped images). Presentation mode
(`present.rs`) and the exported HTML runtime share navigation semantics.

## Cursor Cloud specific instructions

Cloud agents run on **Linux VMs**. These crates target **Windows** (Win32
shell thumbnails, `windows` crate), but non-Windows stubs keep
`cargo check/test --workspace` green on Linux — use them.

When working in the cloud:

1. Focus on logic, layout, and UI modules listed above.
2. Avoid large refactors to `atlas-core/src/thumbs.rs` Windows COM code unless explicitly requested.
3. Run `cargo fmt --all` and `cargo clippy --workspace --all-targets` where possible.
4. Open a PR when done. The human reviewer verifies with `cargo test` and `cargo build --release` on Windows.

### Parallel cloud tasks (good split)

- Agent A: `apps/file-atlas/src/app/ui/*` — Atlas panels
- Agent B: `apps/slate/src/app/ui/*` or `canvas.rs` — Slate panels/views
- Agent C: `crates/atlas-core/src/tree.rs` — layout or hit-testing
- Agent D: `crates/circle-pack` / `crates/slate-doc` — geometry / document model
- Shared chrome changes (`crates/atlas-shell`) should be a dedicated task, not
  mixed into app work.

Each agent should use its **own branch** (`feature/...`) and a separate PR.
