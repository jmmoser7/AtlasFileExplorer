# Fast development loop

Speeds up edit → see-result without changing app runtime behavior.
This is **auto rebuild + relaunch**, not in-process hot-patching.

## One-time setup

On Windows, install Visual Studio Build Tools with the **Desktop development
with C++** workload and a Windows SDK. From the repository root, select MSVC
for this checkout (this leaves other projects' Rust defaults unchanged):

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal --component rustfmt --component clippy
rustup override set stable-x86_64-pc-windows-msvc
cargo test --locked --workspace
.\scripts\build-release.ps1
```

The build/dev wrappers discover the installed Visual Studio C++ environment.
For a manually initialized terminal, run `. .\scripts\_msvc-env.ps1` first.
PDF previews use the checked-in `vendor/pdfium.dll` when launched from the
repository; keep it beside the executables if launching from another directory.

```powershell
cargo install --locked bacon
```

(`bacon` is a Cargo-adjacent watcher; it is not a project dependency.)

If Windows denies execution of generated helpers or compiler DLLs, follow
[Windows builds and execution auditing](windows-builds.md) before retrying.

## Day-to-day

From the repo root (MSVC/SDK env present if your machine needs it):

```powershell
bacon both           # Slate + File Atlas together (recommended for sessions)
bacon slate          # Slate only
bacon atlas          # File Atlas only
bacon check          # workspace check only (default job)
bacon test-slate     # slate crate tests on save
```

Or via wrappers (also set MSVC env on this machine when needed):

```powershell
.\scripts\dev-both.ps1
.\scripts\dev-slate.ps1
.\scripts\dev-atlas.ps1
```

Shortcuts inside bacon: `j`/`k` switch jobs, `r` re-run, `q` quit.
Each save kills the previous run and starts a fresh process — board state
is not preserved across edits (same as a manual relaunch). The `both` job
stops any prior `slate` / `native-file-atlas` processes before relaunching.

### Prefer the dev profile for logic work

Workspace `[profile.dev]` already tunes compile speed (`opt-level = 1` for
workspace crates, `2` for dependencies). Use `bacon slate` / `cargo run -p
slate` while iterating on board tools, hit-testing, and contracts.

Use release only when paint or frame-time matters:

```powershell
bacon slate-release
# or
cargo run --release -p slate
```

### Cargo aliases (no watcher)

```powershell
cargo slate          # = cargo run -p slate --
cargo atlas          # = cargo run -p native-file-atlas --
```

Both aliases end in `--`, so everything after them goes to **the app**, not to
cargo. Atlas takes an optional folder to open as its first argument, which makes
one mistake quietly misleading:

```powershell
cargo atlas --release                    # debug build, and "--release" is
                                         # handed to Atlas as the folder to open
cargo run --release -p native-file-atlas # what you meant
cargo atlas "C:\path\to\folder"          # open a folder in the debug build
```

### Script wrappers

```powershell
.\scripts\dev-both.ps1    # both apps (bacon both)
.\scripts\dev-slate.ps1   # Slate only
.\scripts\dev-atlas.ps1   # File Atlas only
```

These prefer `bacon` (with MSVC env fixups when needed). They do not change
how the apps behave.

## Desktop shortcuts

Create or refresh Windows Desktop shortcuts after a successful release build:

```powershell
.\scripts\build-release.ps1
```

The fast two-app dev loop also refreshes the same shortcuts after it builds,
pointing them at `target\debug`. The release wrapper repoints them at
`target\release`. Both targets are stable paths. Desktop shortcuts, Start menu
shortcuts, and an existing taskbar or Start pin whose target is `slate.exe` or
`native-file-atlas.exe` are repointed at that build. The scripts do not add,
remove, or reorder pins.

## Chrome visual tuning (already live)

For spacing/colors/geometry of shared chrome, keep using the existing
feature-gated tuner — that *is* true live reload:

```powershell
cargo run --release -p slate --features ui-tuner
```

The activity timeline is an Atlas readout, so dial its tokens from Atlas —
`bacon atlas-tuner` keeps the watch loop while adding the dashboard.

See `docs/ui-tuning-workflow.md`. Board-tool feel constants still require a
rebuild unless later promoted onto that token path.

## What this deliberately does not do

- No Subsecond / dylib hot-patch of Rust functions.
- No change to release binaries, journals, or document models.
- No agent-facing in-process plugin loading (Constitution Art. VII).
