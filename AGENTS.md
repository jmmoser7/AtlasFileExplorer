# Agent instructions — Atlas ecosystem (native)

Rust + egui Windows desktop apps for visual file organization at scale. The
repo is a **Cargo workspace** containing two launchable applications built on
shared crates:

- **File Atlas** (`apps/file-atlas`, binary `native-file-atlas`) — folder
  scanning, tree canvas, destination assignment / export workflow.
- **Slate** (`apps/slate`, binary `slate`) — tag-driven workbooks (`.slate`
  files) that link files (never copies) and present their thumbnails as a
  tag-grouped grid or literal Venn diagrams. Tagging lives *only* in Slate;
  Atlas offers Slate tags in its right-click menu during a linked session.

## Workspace layout

| Crate | Role | Safe to edit in parallel |
|-------|------|--------------------------|
| `crates/atlas-core` | UI-free backend: types, scanner, SQLite index, thumbnail pool + cache tiers, tree layout, journal, export, watcher | Yes |
| `crates/atlas-shell` | **Shared window chrome**: theme/Palette, tab strip, sidebar primitives, widgets, panel registry, command reference | Yes — but see the chrome rule below |
| `crates/atlas-session` | In-process bridge for linked Slate⇄Atlas sessions | Yes |
| `crates/slate-doc` | `.slate` document model + faceted tag system | Yes |
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
