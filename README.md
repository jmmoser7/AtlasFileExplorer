# File Atlas (native)

A native Windows tool for visually organizing tens of thousands of files.
Rust + egui, GPU-rendered, fully non-destructive.

## What it does

- **Instant scans.** A parallel native directory walker streams results into the
  canvas from the first frame (~500k files/sec on local disk). Every folder you
  open is remembered in a SQLite index, so revisiting is instant: the canvas
  paints from the index in milliseconds while a background rescan re-verifies.
- **Browser-style tabs.** The top bar holds one tab per directory; the + adds
  more. Each tab remembers its folder and camera. Undo/Redo sit in the top
  chrome; everything else belongs to the active tab workspace.
- **Modular toolbars.** Left **Tools** rail (Basic filters, Display settings,
  Workflow, Tags) and bottom **Readouts** (metrics) each have a gear menu to
  show/hide sub-panels. Advanced settings (pre-warm, shared cache) open as a
  floating window from the tools gear. See `src/app/ARCHITECTURE.md`.
- **Infinite canvas, branching by folder.** The folder hierarchy is drawn as a
  branching tree on a pan/zoom canvas (scroll = zoom at cursor, drag = pan).
  Files cluster around their parent folder; folders with more than ten files
  pack them into a dashed grid block. Click a folder node to collapse or expand
  its branch. Large collapsed folders become violet "portal" cards with a 3×3
  thumbnail mosaic — click to fly in. The Flow button switches between
  left-to-right and top-down branching.
- **Level of detail.** Far out, every file is a block in its own average color
  so the whole tree reads as a heat map; mid zoom shows color slabs; close up,
  full cards with thumbnails, names, sizes, ages, and tag chips.
- **Thumbnails for everything Windows can preview** — images, MP4, PSD,
  Office, and Rhino `.3dm` (via Rhino's shell handler) — plus built-in
  fallbacks that need no extra software: `.3dm` embedded previews, PowerPoint/
  Word/Excel embedded thumbnails (read straight out of the zip), and PDFs
  rendered with the bundled pdfium engine (`pdfium.dll` next to the exe).
  Thumbnails persist in a disk cache keyed by path+size+mtime; on network
  roots the worker pool grows and a throttled background pass pre-warms the
  cache for cold folders.
- **Shared per-project cache.** When the opened folder sits inside a project
  that follows the firm template (contains `02 DESIGN\05 RESOURCES\03 DATA`),
  a second cache tier is kept at `…\03 DATA\.atlas-cache` inside the project
  itself. Cache keys are project-root-relative, so every machine that opens
  any part of the project reads and writes the same entries — the first
  person to browse (or pre-warm) a project makes it fast for everyone.
  Thumbnails publish into `.atlas-cache` automatically whenever they are
  viewed or warmed; opening a folder also syncs any existing local cache
  entries into the project cache.
- **Structure-only map.** Uncheck every file-type box and the canvas keeps
  the full folder skeleton — nodes, branches, counts — with zero thumbnails,
  a lightweight way to map out a folder's shape. (With some boxes checked,
  hide mode still prunes non-matching branches as before.)
- **Overnight pre-warm.** Tools gear → Advanced settings → "Pre-warm a folder…"
  walks any directory in the background at lowest priority, publishing into
  the shared project cache. Every project found under the picked folder gets
  its `.atlas-cache` repository created on the spot — pick a folder above
  many projects and they are all warmed. While a run is active a temporary
  dashboard docks above the readout bar with live discovery counts, thumbnail
  progress, transfer speed and ETA, a parallel-jobs speed control (1–8), and
  a Cancel button. Leave the app open overnight.
- **Non-destructive organizing.** Tag files (drag tag chips onto cards, or
  right-click → Tag & assign), stage them to destination folders, rename on
  export. Sources are never touched. Export copies files to a destination you
  pick, and writes a JSON manifest documenting every source→dest mapping.
- **The journal is the undo system.** Every action is a reversible entry in a
  ledger (Ctrl+Z / Ctrl+Shift+Z). The journal panel UI is temporarily hidden
  while its permanent toolbar home is decided; undo/redo remain in the top bar.

## Controls

| Input | Action |
|---|---|
| Ctrl+O / drop a folder | Open folder |
| Scroll | Zoom at cursor |
| Drag empty canvas | Pan |
| Shift+drag | Rubber-band select |
| Click folder node | Collapse / expand branch |
| Click / Ctrl+click | Select file |
| Right-click file | Tag / assign / open menu |
| Double-click file | Detail view |
| Double-click empty canvas | Zoom in |
| F | Fit whole tree · +/− zoom |
| F2 | Tag & assign panel |
| Ctrl+A | Select all (filtered) |
| Ctrl+Z / Ctrl+Shift+Z | Undo / redo |
| Esc | Close panel / clear selection |

## Build

Requires Rust (GNU or MSVC toolchain). From this directory:

```
cargo build --release
```

Binary lands at `target/release/native-file-atlas.exe`. Optionally pass a
folder path as the first argument to open it on launch.

For PDF previews, place `pdfium.dll` in `vendor/` before building — the build
script copies it next to the exe automatically. You can also copy it manually
beside `native-file-atlas.exe`. Without pdfium, PDFs only preview when a shell
PDF handler is installed and Explorer has already cached a real thumbnail.

Run tests with `cargo test`.

## Data locations

- Index + tags + journal: `%LOCALAPPDATA%\NativeFileAtlas\atlas.db`
- Thumbnail cache: `%LOCALAPPDATA%\NativeFileAtlas\thumbs\`
- Shared project cache: `<project>\02 DESIGN\05 RESOURCES\03 DATA\.atlas-cache\`

Deleting any of these is always safe; they will be rebuilt. The scanner and
watcher ignore `.atlas-cache` folders, so the shared cache never shows up in
the canvas or triggers rescans.

## Architecture

- `src/scanner.rs` — parallel streaming directory walker (8 workers)
- `src/index.rs` — SQLite persistence on a dedicated thread
- `src/tree.rs` — folder hierarchy + tidy-tree layout (orientation, grid-pack,
  portals, collapse, hit testing)
- `src/thumbs.rs` — shell thumbnail workers (LIFO priority) + JPEG disk cache;
  tries Explorer's thumbnail cache before extracting
- `src/threedm.rs` — `.3dm` embedded-preview fallback parser
- `src/office.rs` — Office Open XML embedded-thumbnail extractor
- `src/pdf.rs` — PDF page-1 renderer via dynamically loaded pdfium
- `src/journal.rs` — reversible action ledger (undo/redo)
- `src/export.rs` — copy-only export engine + manifest + undo-by-manifest
- `src/watcher.rs` — filesystem watcher keeping the index live
- `src/app/mod.rs` — application shell, canvas, tab workspace orchestration
- `src/app/ARCHITECTURE.md` — UI layer boundaries and extension points
- `src/app/ui/` — top tabs, tools rail, readouts bar, advanced window
- `src/app/chrome.rs` — gear-menu panel registry
