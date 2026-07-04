# Agent instructions — File Atlas (native)

Rust + egui Windows desktop app for visual file organization at scale.

## Architecture

Read `src/app/ARCHITECTURE.md` before UI changes. Module boundaries:

| Area | File(s) | Safe to edit in parallel |
|------|---------|--------------------------|
| Tab chrome | `src/app/ui/tabs.rs` | Yes |
| Left tools rail | `src/app/ui/tools.rs`, `src/app/ui/sidebar.rs` (+ `SIDEBAR.md`) | Yes |
| Bottom readouts | `src/app/ui/readouts.rs`, `src/app/ui/activity_heatmap.rs` | Yes |
| Shared widgets | `src/app/ui/widgets.rs` | Yes |
| Advanced window | `src/app/ui/advanced.rs` | Yes |
| Panel registry | `src/app/chrome.rs` | Yes |
| Input bindings | `src/app/commands.rs` (+ `COMMANDS.md` — registry rule) | Yes |
| Canvas painting / camera | `src/app/canvas.rs` | Yes |
| Overlay windows / tray | `src/app/overlays.rs` | Yes |
| Theme / palette | `src/app/theme.rs` | Yes |
| Pre-warm | `src/app/prewarm.rs` | Yes |
| App state / lifecycle | `src/app/mod.rs` | Integration point — coordinate here |
| Multi-tab tests | `src/app/tests.rs` | Yes |
| Tree layout / hit-test | `src/tree.rs` | Yes |
| Thumbnails / cache | `src/thumbs.rs` | Yes (Windows APIs) |
| Scanner / index | `src/scanner.rs`, `src/index.rs` | Yes |
| Core types | `src/types.rs` (`FileEntry::rel` invariant — read its module docs) | Coordinate |

Prefer small, focused PRs on one module. Match existing naming and egui patterns.

## House rules (keep these true)

1. **Warning-free baseline.** `cargo clippy --all-targets` and `cargo check
   --all-targets` produce zero warnings on this repo. Fix or justify (with a
   comment) any warning you introduce; run `cargo fmt --all` before pushing.
2. **`mod.rs` stays lean.** New UI/painting code goes in `canvas.rs`,
   `overlays.rs`, or `ui/*`; new subsystems get their own `src/app/` child
   module (like `prewarm.rs`). `mod.rs` gains only state fields, lifecycle
   logic, and wiring. Cross-module `AtlasApp` methods are `pub(in crate::app)`.
3. **Multi-tab safety.** Any new field that carries entry ids or per-root
   state must be cleared in `reset_workspace()` (see the invariants section
   of `ARCHITECTURE.md`), and async results must be tagged with the
   generation/root/tab they belong to.
4. **Cross-platform.** Windows API calls are `#[cfg(windows)]`-gated with a
   non-Windows fallback so Linux builds and tests keep working. Shell
   integration lives in `src/app/platform.rs`. Path separators: `FileEntry::rel`
   and cache keys use `\` on every platform — construct entries via
   `FileEntry::from_abs`/`from_rel`, never by hand.
5. **Input bindings** must be registered in `src/app/commands.rs::ENTRIES`
   (see `src/app/COMMANDS.md`); the Advanced window renders the reference
   automatically. Don't duplicate shortcut lists elsewhere.
6. **Docs stay accurate.** If you add/move a module or change a documented
   pattern, update `src/app/ARCHITECTURE.md`, this table, and README's
   architecture list in the same PR.
7. **Tests:** backend modules keep inline `#[cfg(test)] mod tests` at the
   bottom of the file; app-level integration/stress tests go in
   `src/app/tests.rs` (headless harness). New tab/root lifecycle behavior
   needs a headless test.

## Build & test (Windows — primary target)

```powershell
cd native-file-atlas # repo root if cloned elsewhere
cargo test
cargo build --release
```

Release binary: `target/release/native-file-atlas.exe`. Requires `vendor/pdfium.dll` for PDF previews.

## Cursor Cloud specific instructions

Cloud agents run on **Linux VMs**. This crate targets **Windows** but builds
and tests cleanly on Linux: Windows-only code is `#[cfg(windows)]`-gated and
the `windows` crate is a target-specific dependency. `cargo test` and
`cargo clippy --all-targets` must pass on Linux before you open a PR.

When working in the cloud:

1. Focus on logic, layout, and UI modules listed above.
2. Avoid large refactors to `thumbs.rs` Windows COM code unless explicitly requested.
3. Run `cargo fmt --all` and `cargo clippy --all-targets`; keep the build warning-free.
4. Open a PR when done. The human reviewer verifies with `cargo test` and `cargo build --release` on Windows.

### Parallel cloud tasks (good split)

- Agent A: `src/app/ui/tabs.rs` — tab behavior
- Agent B: `src/app/ui/tools.rs` + `src/app/ui/sidebar.rs` — filter/display panels
- Agent C: `src/tree.rs` — layout or hit-testing
- Agent D: `src/thumbs.rs` — cache tier logic (not shell extraction)
- Agent E: `src/app/canvas.rs` — painting/LOD (coordinate with C on hit-testing)
- Agent F: `src/app/ui/readouts.rs` + `activity_heatmap.rs` — readout panels

Each agent should use its **own branch** (`feature/...`) and a separate PR.
`src/app/mod.rs` is the shared integration point — if two tasks both need it,
run them sequentially instead.
