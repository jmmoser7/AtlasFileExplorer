# Agent instructions — Atlas ecosystem (native)

## Governing documents

**`CONSTITUTION.md` (repo root) is the project's governing law** — read it
before any change touching architecture, document models, geometry, the
command system, exports, or the agent surface. It carries a pushback
mandate: if a request conflicts with an article, name the article and
propose an alternative or amendment instead of silently complying.
`ROADMAP.md` sequences the long-term build; `docs/facet-taxonomy.md` defines
how file types are classified. Where this file and the constitution
disagree, the constitution wins.

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
| `crates/atlas-core` | UI-free backend: types, scanner, SQLite index, thumbnail pool + cache tiers, tree layout, journal, export, watcher, time-selection math (`timeline.rs`) | Yes — but read `docs/performance.md` first for `scanner.rs`, `thumbs.rs`, `rasterthumb.rs`, `owners.rs`, `metadata.rs` |
| `crates/atlas-shell` | **Shared window chrome**: theme/Palette, tab strip, sidebar primitives, widgets, activity timeline, panel registry, command reference | Yes — but see the chrome rule below |
| `crates/atlas-session` | In-process bridge for linked Slate⇄Atlas sessions | Yes |
| `crates/atlas-ai` | AI / Cursor integration: shared AI-workspace config, Cursor launcher, live-link context beacon, the sidebar AI panel body | Yes |
| `crates/slate-doc` | `.slate` document model: faceted tag system + the board scene graph (`scene.rs`: nodes, SVG-ceiling styles, invertible + authored `SceneCmd` journal) | Yes |
| `crates/slate-kit` | Declarative board tools: gesture grammar (code, closed set of 9) + result recipe (data). `.slatekit` model, loader, and scope resolver; `builtin/core.slatekit` holds the board's own tool results. See `KITS.md` | Yes |
| `crates/slate-artifact` | HTML artifact writer: scene → slides, styles → CSS, embedded JS slide runtime. Export is serialization, not conversion | Yes |
| `crates/circle-pack` | Pure geometry: circle packing + Venn layout | Yes |
| `crates/vector-ink` | Pure vector geometry engine (kurbo): path flattening, variable-width stroking to feathered AA meshes, stroke outlines for SVG export, hit-testing, freehand fitting. No renderer deps (Constitution Art. I) | Yes |
| `crates/code-lens` | UI-free codebase analysis for Slate's Lens view: cargo workspace + Rust source extraction to a code graph, semantic-zoom layout, agent overlay/beacon contract | Yes |
| `crates/rhino-mesh` | Pure-Rust reader for cached render meshes in Rhino `.3dm` files (Slate's 3D board viewports) | Yes |
| `apps/file-atlas` | Atlas app: canvas + app state (`src/app/mod.rs` is the integration point) | Coordinate on `mod.rs` |
| `apps/slate` | Slate app: canvas, tagging sidebar, session host | Coordinate on `app/mod.rs` |

Read `apps/file-atlas/src/app/ARCHITECTURE.md` and
`apps/slate/src/app/ARCHITECTURE.md` before UI changes.

## The shared-chrome rule (no divergence)

Both apps must look and feel identical. This is enforced structurally:

1. **All chrome painting lives in `atlas-shell`** — tab shapes, palette,
   sidebar section cards, widgets, gear menus. The **unified top bar** (icon
   portal + inline tabs) is documented in `crates/atlas-shell/TOPBAR.md`.
   Apps supply *data* (tab specs, panel sets, command entries) and react to
   returned actions.
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

Interaction contracts for canvas tools and portal subtypes live in
`docs/keymap/contracts/` (registry, patterns, decisions database, one file per
contract) and are governed by the `.cursor/skills/tool-contract` skill.
`cargo xtask contracts` checks that the three artifacts agree — it also runs
inside `cargo test --workspace`.

## Board tools: grammar and recipe

A board tool is a **gesture grammar** (how input is read — code, a closed set of
nine in `slate-kit`) plus a **result recipe** (what the commit produces — data,
in a `.slatekit` file). `BoardTool::grammar()` states each shipped tool's
grammar once, and `finish_draw` asks `SlateApp::kits` what the gesture produces
rather than building nodes inline. When adding a tool, prefer expressing the
result as a recipe; only reach for new code when the grammar itself is new, which
is core work under Article III. Read `crates/slate-kit/KITS.md` first, and check
kit files with `cargo xtask kits` (also inside `cargo test --workspace`).

## Build & test (Windows — primary target)

```powershell
cargo test --workspace
cargo build --release -p native-file-atlas -p slate
```

The load-time benchmarks are `#[ignore]`d (they write large corpora) and are the
evidence behind `docs/performance.md` — run them before and after any change to
the scan or thumbnail hot paths:

```powershell
cargo test -p atlas-core --release --test thumb_bench -- --ignored --nocapture
cargo test -p atlas-core --release --test scan_bench  -- --ignored --nocapture
cargo test -p native-file-atlas --release load_jitter -- --ignored --nocapture
```

### Nothing whole-corpus on the batch path

`load_jitter` measures frame time while batches stream in, because that is where
smoothness is won or lost. A scan delivers a batch every ~30 ms, so **anything a
batch triggers runs on essentially every frame of a load**. One O(entries) pass
there — a filter recompute, a relayout, a rebuilt index, an owner re-tally — makes
panning judder worse the bigger the folder gets, which is the opposite of what the
user needs at that moment. Fold appended files in incrementally
(`absorb_new_entries`) or put the work on the tree's rebuild cadence, and check the
frame-time tail rather than the mean. `ATLAS_BENCH_LEGACY=1` reproduces the old
per-batch behavior for a same-machine comparison.

Likewise, **collapse state is a recorded decision, not a derived one**
(`AtlasApp::dir_collapsed`). Rebuilds happen constantly during a load and
`default_collapse` reads counts that are still growing, so anything that changes
collapse must record it — otherwise the next rebuild silently undoes the user.

When a real folder shows wrong or missing previews, run the pipeline against it
directly instead of guessing — `folder_probe` reports what each tier returned per
file, and `docs/performance.md` explains reading the result:

```powershell
$env:ATLAS_PROBE_DIR = "C:\path\to\folder"
cargo test -p atlas-core --release --test folder_probe -- --ignored --nocapture
```

**Bumping `CACHE_KEY_VERSION` is part of changing extraction**, not an optional
extra. Keys are `path + size + mtime`, so a bad cached entry is permanent
otherwise; `docs/performance.md` records the months-long icon bug that taught
this.

### Never read a cloud file's bytes in bulk

The reference machine's roots are OneDrive / SharePoint libraries where most files
are **dehydrated placeholders**: normal-looking directory entries whose content is
on a server, and reading one byte downloads the whole file. Thumbnailing such a
folder is a mass download — slow for the user and, on a managed network, loud
enough to get their access revoked. This is not hypothetical: it is why the app
appeared to cap out at 15 thumbnails/sec.

So, in code and in your own diagnostics alike:

- gate anything that reads file bytes on `atlas_core::cloud::is_dehydrated`, which
  reads only the directory entry and can never itself trigger a download;
- never write a script that opens or reads many files under a OneDrive path — not
  even a few KB each, since a partial read hydrates the whole file. Attributes
  from `Get-ChildItem` are safe; `[System.IO.File]::OpenRead` is not;
- `cargo test -p atlas-core --release --test cloud_guard batch` verifies the
  guarantee per file against a real folder, and is the thing to run after touching
  extraction.

`docs/performance.md` has the measurements, the `MAX_PATH` fail-open bug that
downloaded 502 files before it was caught, and why cloud-only files legitimately
show type icons.

For day-to-day board/UI iteration, prefer the **dev watch loop** (auto
rebuild + relaunch on save — no hot-patch): `bacon slate` after
`cargo install --locked bacon`. Details: `docs/dev-loop.md`. Chrome spacing
and color still live-tune via `--features ui-tuner`
(`docs/ui-tuning-workflow.md`).

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

1. **The scene model is constrained to the SVG ceiling.** `slate-doc::scene`
   only holds styling that SVG (including CSS) can express — the
   constitution's Article IV amended this from the original CSS-only rule to
   make room for paths, variable-width strokes, and blend modes (the code
   itself still reflects the CSS-only era until Roadmap Phase 2 lands).
   `apps/slate/src/app/board.rs` (egui painter) and `crates/slate-artifact`
   (HTML writer) are two interpreters of that one model, and `imagefx.rs`
   mirrors the CSS filter math on pixels. A new board style property must
   land in all interpreters, or not at all.
2. **All board mutations are invertible `SceneCmd`s** committed through the
   tab's `SceneJournal` (undo/redo now; the MCP agent surface later). UI code
   must not mutate `doc.scene` outside a journaled path
   (`patch_nodes` / `add_nodes` / `delete_board_nodes` / `commit_scene`).

Frames are slides (geometric membership, `order` = deck sequence, optional
tag assignments inherited by dropped images). Presentation mode
(`present.rs`) and the exported HTML runtime share navigation semantics.

Media kinds (`slate-doc::media`) decide what a placed file becomes, in both
renderers: images → `<img>`, web-safe video → `<video>` (time-trim via
`VideoOpts` → `#t=` media fragment + runtime guard; board shows the poster
with a ▶ badge), text → snippet card (same excerpt both sides), PDF/docs →
thumbnail-backed card linking to the copied original. **`.slate` files never
become items** — every add/drop path diverts them to open as tabs
(`pending_workbooks`), and `open_doc_at` dedupes by canonical path, which is
what makes workbook-in-workbook (and workbook-in-itself) recursion
impossible.

## The Lens (Slate's codebase view)

Slate's fourth `ViewKind` is **Lens**: a read-only, deterministic dependency
graph over a workbook's `lens_root` Cargo workspace. The graph is never
hallucinated — `crates/code-lens` extracts it from manifests and Rust source.
Semantic cluster labels come from Cursor agents via
`<ai-workspace>/.atlas-ai/lens/{graph.json,overlay.json}` per
`docs/lens-agent-contract.md`; Slate only consumes the `code-lens` contracts
for analysis, layout, overlay matching, and the AI beacon.

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
