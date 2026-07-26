# Constitutional deviations ledger

Open gaps between `CONSTITUTION.md` and the code as built. One row per
deviation. **`cargo xtask metrics` counts the rows whose Status is `open`**, so
this file is machine-read as well as human-read — keep the table shape.

Rules:

- A deviation is opened by an audit, a review, or any agent that notices one.
- A deviation is closed only when the code conforms, with the closing commit
  recorded. Deleting a row is prohibited; set Status to `closed`.
- `accepted` means the user has ratified the deviation as intentional; it is not
  counted as debt but stays visible.

| ID | Article | Deviation | Status | Opened | Closes with | Closed |
|---|---|---|---|---|---|---|
| DV-01 | VI | `SceneCmd::{Add,Remove}` address nodes by `usize` index and `Patch` replaces a whole `Node`; the journal cannot converge, contradicting VI's "foundation for multiplayer synchronization later" | open | 2026-07-25 (audit №1 F4) | WI-2 | — |
| DV-02 | I | `crates/atlas-ai` contains `ui.rs` and depends on `eframe`/`atlas-shell`; the crate that grows into the agent surface is renderer-bound | open | 2026-07-25 (audit §2.1) | WI-6a | — |
| DV-03 | IX | `SlateItem` encodes identity as an absolute `PathBuf` + path/size/mtime cache key; a workbook is not portable between machines that mount the same material differently | open | 2026-07-25 (audit F2/F6) | WI-3 | — |
| DV-04 | IV | `Scene::members_of` can place one node in two overlapping frames, so `slate-artifact::collect_slides` emits it on two slides; the export is not an honest serialization of what the board shows | open | 2026-07-25 (survey) | T0.6 | — |
| DV-05 | VII.1 | `board.align` is a registered `CommandSpec` with no dispatch arm; `align_board_selection` / `distribute_board_selection` exist with zero call sites. A documented command that does nothing breaks "every human-performable action is a registered command" from the other direction | open | 2026-07-25 (survey) | T2.2 | — |
| DV-06 | II | No `.slate` write is guarded against a concurrent writer, and no external-change detection exists; with daily sharing (D1) last-save-wins silently destroys work | open | 2026-07-25 (audit E2) | WI-1 | — |
| DV-07 | IV.1 | No golden test asserts board-vs-artifact parity; parity is structural only (two interpreters, one model) and would not catch a divergence | open | 2026-07-25 (survey) | T3.1 | — |
| DV-08 | II.3 | `SceneJournal` is unbounded — no capacity limit on `done`/`undone`; a long session with large nodes grows without bound | open | 2026-07-25 (survey) | WI-2 | — |
| DV-09 | X | `Palette`'s fourteen semantic slots are hardcoded Rust constructors while `ui-tokens.toml` (which has a schema version and a live tuner) carries no colours; egui `Visuals` are set beside the palette rather than derived from it, so the two colour systems are hand-synchronised | open | 2026-07-25 (audit №2 R2) | T0.7 | — |
| DV-10 | II | `Scene.nodes` has no spatial index: every hit-test, marquee, and paint cull is a linear scan. Survivable at 10³ nodes, unusable at 10⁴ | closed | 2026-07-25 (audit №2 R6) | T0.8 | 41ef2bb |
| DV-11 | IV | Two wire routers for one concept: `lens.rs::lens_orthogonal_route` in the app and `connector_bezier` in `slate-doc`. One model, two implementations, no shared seam | open | 2026-07-25 (audit №2 R3) | Wave 3 (WI-4) | — |

## Deviation counts by article (maintained by the metrics tool)

Do not hand-edit; `cargo xtask metrics` rewrites the block below.

<!-- metrics:deviations:begin -->
open: 11 · accepted: 0 · closed: 0
<!-- metrics:deviations:end -->
