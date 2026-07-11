# Slate — UI architecture

Slate mirrors File Atlas's shell exactly (same `atlas-shell` chrome) with a
different workspace model: instead of a filesystem root per tab, each tab owns
one **workbook** (`.slate` document — links to files plus a faceted tag
system, never file copies).

## Layer 0 — Top chrome (`ui/menubar.rs` + `ui/tabs.rs`)

The custom title bar (topmost, full width; the window runs without OS
decorations): app icon — no app-name text — then the File/View menus, a
draggable caption area, and the minimize/maximize/close buttons, all on one
row. Below it, browser-style workbook tabs. All painting comes from
`atlas_shell::menubar` /
`atlas_shell::tabs`; these modules only adapt `SlateApp` state to
`MenuSpec`s / `TabSpec`s and apply actions. The tools rail is registered
*before* the tab strip so the rail runs from the readout bar up to the menu
bar, with tabs nested in the remaining width. Full-screen canvas
(`ChromeConfig::canvas_fullscreen`; F11, View → Full-screen canvas, or ⛶ in
the canvas mini menu) suppresses the tools rail and readout bar.

## Layer 1 — Tab workspace

| Region | Module | Role |
|--------|--------|------|
| Left tools rail | `ui/tools.rs` | **Tags** (hierarchical group editor), **Display** (Board/Grid/Venn/Lens, cell size, theme), **Selection** (dynamic inspector, `ui/inspector.rs`), **Workbook** (open/save, add files, artifact export, Atlas link), **AI** (Cursor launcher + AI workspace; body shared with Atlas via `crates/atlas-ai`), **Lens** (code root, depth, edge filters, search, overlay legend) |
| Canvas | `canvas.rs` | Grid + Venn presentations, selection, right-click tag assignment |
| Lens | `lens.rs` | Code-dependency graph canvas: worker pump, painting, focus/expand gestures |
| Board | `board.rs` | Authored open-world canvas: frames, shapes, text, placed images, draw tools, gestures |
| Presentation | `present.rs` | Fullscreen slide playback of the board's frames |
| Image filters | `imagefx.rs` | CSS-filter math on pixels (board preview parity with the HTML artifact) |
| 3D viewports | `model3d.rs` | Rhino `.3dm` viewport lifecycle: off-thread mesh parse (`crates/rhino-mesh`), offscreen glow render, lock/unlock + poster cache |
| Previews | `preview.rs` | Lazy full-resolution texture tier above the thumbnails (see below) |
| Settings | `settings.rs` | Persisted UI settings (`slate-settings.json` next to the index DB) |
| Bottom readouts | `ui/readouts.rs` | Item/tag counts, link health, zoom |
| Advanced | `ui/advanced.rs` | Floating window (canvas preview settings, workbook info, commands reference) |
| Commands | `commands.rs` | Canonical bindings; see `COMMANDS.md` |

## The tag model (`slate-doc`)

- A workbook holds **tag groups** (facets). Tags **within** a group are
  mutually exclusive on a file (Big/Medium/Small); tags **across** groups
  combine freely (Big + Red). `SlateItem.assignments: BTreeMap<GroupId, TagId>`
  enforces this structurally.
- Items with no assignments are **uncategorized**: they render in a separate
  tray and never appear inside Venn circles.
- `SlateDoc::combination_buckets` drives both presentations: grid sections are
  tag-combination buckets; Venn regions are subsets of focused tags.

## Presentations

- **Grid** (`canvas.rs::grid_layout`) — sections per tag combination,
  uncategorized last.
- **Venn** (`canvas.rs::venn_layout_now`) — literal circles per focused tag
  (`crates/circle-pack::venn_layout`); thumbnails render as circle-cropped
  textured meshes packed inside their set circles, shared files sit in the
  lens overlaps. Tag focus is toggled from the Tags panel.
- **Lens** (`lens.rs`) — interactive code-dependency graph over a workbook's
  `lens_root`. Deterministic analysis and layout come from `code-lens`; Slate
  paints containers/chips/wires on the shared camera, runs analysis on a
  background thread, and writes `graph.json` / reads `overlay.json` through
  `code_lens::LensBeacon` when an AI workspace is configured.
- New presentations should follow the same pattern: pure geometry in a crate,
  a `*_layout` builder producing `Placed` items, painting + hit-testing on the
  shared camera.

## Lazy full-resolution previews (`preview.rs`)

All item painting goes through `SlateApp::item_texture(item, desired_px)`
(`desired_px` = on-screen size in physical pixels). It returns the best
GPU-resident texture — full-res preview, else thumbnail, else `None` — and
only *queues* upgrades; it never blocks a paint. The pipeline:

1. The 192 px thumbnail (shared disk cache, `atlas_core::thumbs`) is always
   the instant tier.
2. When an item's on-screen size outgrows its thumbnail, a capped decode of
   the original is queued on `atlas_core::preview::PreviewPool` (LIFO — what
   the user looks at now decodes first; a few requests start per frame).
   Rasters decode via the `image` crate, PDFs render through pdfium at the
   requested size, everything else asks the platform shell (Windows).
3. Target sizes are quantized to a power-of-two ladder capped by the user's
   max-resolution setting (Advanced → Canvas previews), so continuous zoom
   re-decodes a bounded number of times.
4. Resident previews live in `preview_cache`, an LRU bounded by the user's
   memory budget; least-recently-viewed entries fall back to thumbnails.
   Sources that can't beat their thumbnail land in `preview_failed` and are
   never re-requested (until "Unload all").

Board specifics: unadjusted images sharpen through the same path; images
with non-identity `ImageAdjust` intentionally stay on thumbnail-based FX
textures (CPU filter math over multi-megapixel previews would stall the
canvas). Presentation mode inherits full-res automatically because it paints
through `paint_board_node` at fullscreen sizes.
- **Board** (`board.rs`) — the *authored* view: unlike Grid/Venn (generated
  arrangements of the pool), the Board is a persistent scene the user
  composes. See below.

## The Board (authored canvas + presentation generator)

The board's scene model lives in `slate-doc::scene` and is serialized inside
the workbook. Two invariants carry the whole design:

1. **CSS-expressible styling only.** Every node property (stroke/dash,
   rounded or chamfered corners, crop, opacity, the CSS-filter adjustment
   set, font choices) maps 1:1 onto HTML+CSS. The egui board painter
   (`board.rs`) and the HTML writer (`crates/slate-artifact`) are two
   interpreters of one model, so the exported artifact shows exactly what
   the board shows. `imagefx.rs` implements the same filter math on pixels
   for the live preview. Do not add board styling that CSS cannot express.
2. **Every mutation is an invertible `SceneCmd`.** Gestures mutate the scene
   live but journal their net effect on release (`SceneJournal`); inspector
   scrubs coalesce into single undo steps. This command layer is the same
   surface a future MCP agent will drive.

Other board rules:

- **Frames are slides.** Membership is geometric (a node belongs to the frame
  containing its center); moving a frame moves its members. `FrameNode.order`
  is the deck sequence. Frames can carry tag assignments; images dropped into
  a tagged frame inherit those tags (drops elsewhere stay uncategorized).
- **Presentation mode** (`present.rs`) plays frames fullscreen with the same
  navigation keys as the exported HTML runtime.
- **Export is serialization** (`slate-artifact::export_html`): frames become
  `<section>` slides, assets are copied beside `index.html` or base64-inlined
  (Workbook panel toggle).
- The **Selection panel** (`ui/inspector.rs`) reshapes per node kind; sections
  must funnel edits through `SlateApp::patch_nodes` so they stay undoable.

### Media kinds (what a placed file becomes)

`slate-doc::media::media_kind` is the single extension taxonomy both
renderers consult — the board and the artifact must never disagree about
what a file *is*:

| Kind | Board | Artifact |
|------|-------|----------|
| Image | thumbnail texture (crop/filters) | `<img>` (crop/filters as CSS) |
| Video (web-safe: mp4/webm/ogv/m4v) | poster thumbnail + ▶ badge | `<video>` with `VideoOpts` attrs; trim → `#t=start,end` fragment + runtime guard |
| Video (mov/avi/mkv…) | poster + ▶ + ext badge | thumbnail card linking to the copied original |
| 3D model (`.3dm`) | interactive viewport (unlocked) or frozen-camera poster (locked) | poster card from `ExportOptions::model_posters` (per node — the saved perspective) + link |
| Text (txt/md/csv/code…) | snippet card (`snippets` cache) | `.textcard` — same excerpt (`slate_artifact::read_snippet`), linked original |
| PDF / Doc | shell thumbnail + ext badge | thumbnail-backed card (poster from `ExportOptions::thumbs`, supplied by `export_thumb_map` from the shared cache) + link |
| Workbook (`.slate`) | **never an item** | n/a |

Video playback happens in the artifact, not on the board (egui has no
decoder); the ▶ badge is the honest marker of that divergence. Spatial video
cropping reuses the image `Crop`; time cropping is `VideoOpts { start, end }`
edited in the inspector's Video section.

### 3D model viewports (`model3d.rs`)

Placed `.3dm` files are **viewport nodes**: the node's `ModelCamera`
(document state on `ImageNode`, like `VideoOpts`) selects the view. Locked
nodes paint a disk-cached poster rendered from that pose — no mesh in
memory. Unlocking (padlock on hover) parses the file's cached render meshes
off-thread (`crates/rhino-mesh`), uploads to the GPU, and renders offscreen
(glow MSAA framebuffer → egui texture) with Rhino-style controls. Live
viewports are capped (`MAX_LIVE`) and auto-lock after 30 s idle; locking
re-renders the poster at presentation quality and journals the camera as
one undo step. Duplicating a model node duplicates the *pose*, so one model
can sit on several slides from different saved perspectives while only ever
loading once (and not at all while locked). Crop/filter adjustments don't
apply to model nodes; camera framing replaces them in both renderers.

### Workbook-in-workbook guards

Adding a `.slate` file to a workbook — file dialog, OS drop, Atlas drag, or
double-click — never creates an item. All add paths divert workbooks into
`pending_workbooks`, drained once per frame (after drop placement) into
`open_doc_at`, which dedupes by canonical path: re-opening an open workbook
(including the active one, i.e. "load into itself") just focuses its tab.
No item can reference a workbook, so board/export recursion cannot occur.

## Linked Atlas sessions (`session.rs`)

"Open File Atlas" hosts Atlas as a second **viewport of the Slate process**
(`show_viewport_immediate`), bridged by `crates/atlas-session`:

1. Slate publishes the active workbook's tag groups + its window rect.
2. Atlas shows those tags in its right-click menu (multi-assign per menu
   instance) and supports click-hold-drag of thumbnails toward Slate.
3. Slate drains the assignment inbox into the document each frame and resolves
   released drags by screen-point-in-window; untagged drops arrive
   uncategorized.

Thumbnails are never re-extracted: `SessionFile.cache_key` reuses Atlas's
thumbnail cache (`atlas_core::thumbs`), which both apps read.

## Workbook lifecycle invariants

1. `tabs` is never empty; `active_tab` always in bounds.
2. Every document mutation goes through `SlateApp::doc_mut()` (sets `dirty`).
3. Dirty tabs refuse to close (toast, no data loss).
4. `selection` only holds live `ItemId`s; tag/group removal strips
   assignments inside `slate-doc`, and dead ids are dropped on use.
5. Saves are atomic (temp file + rename in `slate-doc`).

`tests.rs` drives the real frame loop headlessly over these invariants.
