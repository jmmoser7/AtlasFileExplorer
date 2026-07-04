# Agent instructions — File Atlas (native)

Rust + egui Windows desktop app for visual file organization at scale.

## Architecture

Read `src/app/ARCHITECTURE.md` before UI changes. Module boundaries:

| Area | File(s) | Safe to edit in parallel |
|------|---------|--------------------------|
| Tab chrome | `src/app/ui/tabs.rs` | Yes |
| Left tools rail | `src/app/ui/tools.rs`, `chrome.rs` | Yes |
| Bottom readouts | `src/app/ui/readouts.rs` | Yes |
| Advanced window | `src/app/ui/advanced.rs` | Yes |
| Commands / shortcuts | `src/app/commands.rs` | Yes — read `src/app/COMMANDS.md` |
| Tree layout / hit-test | `src/tree.rs` | Yes |
| Thumbnails / cache | `src/thumbs.rs` | Yes (Windows APIs) |
| Canvas / app state | `src/app/mod.rs` | Integration point — coordinate here |
| Scanner / index | `src/scanner.rs`, `src/index.rs` | Yes |

Prefer small, focused PRs on one module. Match existing naming and egui patterns.

## Commands & shortcuts

Read `src/app/COMMANDS.md` before adding keyboard or mouse bindings. Every
user-facing command must be registered in `src/app/commands.rs` (`ENTRIES`) so
it appears in **Advanced → Commands & shortcuts**.

## Build & test (Windows — primary target)

```powershell
cd native-file-atlas   # repo root if cloned elsewhere
cargo test
cargo build --release
```

Release binary: `target/release/native-file-atlas.exe`. Requires `vendor/pdfium.dll` for PDF previews.

## Cursor Cloud specific instructions

Cloud agents run on **Linux VMs**. This crate targets **Windows** (Win32 shell thumbnails, `windows` crate). A full `cargo build` on Linux will fail until Windows-only code is gated — that is expected.

When working in the cloud:

1. Focus on logic, layout, and UI modules listed above.
2. Avoid large refactors to `thumbs.rs` Windows COM code unless explicitly requested.
3. Run `cargo fmt --all` and `cargo clippy --all-targets` where possible; syntax/type checks in isolated modules may still help.
4. Open a PR when done. The human reviewer verifies with `cargo test` and `cargo build --release` on Windows.

### Parallel cloud tasks (good split)

- Agent A: `src/app/ui/tabs.rs` — tab behavior
- Agent B: `src/app/ui/tools.rs` — filter/display panels
- Agent C: `src/tree.rs` — layout or hit-testing
- Agent D: `src/thumbs.rs` — cache tier logic (not shell extraction)

Each agent should use its **own branch** (`feature/...`) and a separate PR.
