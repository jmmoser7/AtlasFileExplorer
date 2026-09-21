# File Atlas lens portal — interaction contract

Status: agreed
Family: portal
Portal class: **host** (Art. V.3 / P1.portal.frame) · Type: **lens** ·
Subtype: **file atlas**
Reference: File Atlas folder canvas (`atlas-core` scan / tree / thumbs),
interpreted inside a Slate portal frame — not a File Atlas app feature
Command: `board.portal.atlas` (placement) · Key: none · Palette: "file atlas"
(aliases: atlas lens, folder lens, folder portal)
Inherits: P0.* (all), P1.node, **P1.portal**, **P2.PortalPlace**,
**P2.PortalHost** — deviations flagged below.

Owner: `atlas-shell::folder_map` (paint) + `atlas-core` (scan / thumbs).
Forbidden forks: a second tree painter; importing `AtlasApp`. Known debt:
DV-19 (scan session), DV-20 (remaining bake helpers; the empty CTA is shared).

## What it is, and the 10% it implements

A journaled frame on a **Slate** board whose contents are one local folder,
drawn with File Atlas's map language (cards, collapse, streaming thumbs).
The real use (Art. III): keeping a live folder next to the drawings it
belongs with, without leaving the board and without growing File Atlas.

The 90% deliberately not implemented is in D15: this is not a File Atlas
window, not a fifth `ViewKind`, and not Edit-mode filesystem writes.

## Pushback before agreement (Art. XI)

1. **Article I — compose through core.** Slate must not import
   `apps/file-atlas` or instantiate `AtlasApp`. The inner map (camera,
   leaders, collapse grips, cards) lives in `atlas-shell::folder_map` on
   `atlas-core` models; File Atlas and the portal both call that module.
2. **Article X — no chrome divergence.** File Atlas window chrome (top bar,
   tools dock, readouts, Advanced) does not appear inside the frame. Frame
   chrome is Slate (`P1.portal.chrome`).
3. **Article IV / V.3 — honest export.** A folder of photos is not
   SVG-expressible geometry. Class is **host**: export is a poster plus a
   pointer to the folder, not a regenerated SVG of every thumbnail.
4. **Article IX.5 / File Atlas Edit.** View-only filesystem in v1. Agents
   never write the disk.
5. **Tagging is Slate-only.** The portal must not grow an Atlas tag model.

## Behavior matrix

Rows keyed to `DIMENSIONS.md` in registry order. Every row is mirrored in
`decisions.json` as `approved` (canvas accepted 2026-08-21; D12/D16/D17/D22 refined by explicit user request 2026-09-17).

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Slate board only. Command `board.portal.atlas`, board tabs only. Palette: "file atlas" (aliases: atlas lens, folder lens, folder portal). Portals flyout row next to Repository Lens. No single-key chord. Not a File Atlas app feature — `apps/file-atlas` does not grow a portal, a ViewKind, or a chrome path for this. Extra entry (OQ2 / **P1.portal.folder-drop**): dropping a folder opens a chooser; File Atlas lens is the default; other lenses that can read the folder are listed; "Place files on the board" dumps the folder's files as images. Alt keeps today's drop | stated | 100 |
| D02 | Stickiness & repeat | P2.PortalPlace.oneshot / P0.4: one-shot, commit returns to Select; Space/Enter re-arms | pattern | 90 |
| D03 | Gesture grammar | P2.PortalPlace.gesture: `Armed → Dragging(rect) → Committed(unbound)`. Release paints "Choose folder…". Binding is a separate non-modal step (D19). Folder drop through the chooser (File Atlas option) skips to `Committed(bound)` | pattern | 90 |
| D04 | Click vs drag rule | P2.PortalPlace.click: travel > `draft.drag_threshold` (4 px) = dragged rect. Below it places `portal.atlas.default_size` (960×540 world units) centred on the click. Deviates P2.DragShape on the existing portal precedent | precedent | 85 |
| D05 | Modifiers | P2.PortalPlace.aspect: Shift during the drag locks 16:9; unmodified drags are free-aspect. Alt while dropping a folder bypasses the chooser (P1.portal.folder-drop). Ctrl unassigned in v1 | precedent | 80 |
| D06 | Constraints & snapping | P2.PortalPlace.snap: F9 grid snap and smart guides apply to the frame rect (P1.node.move); F8 ortho is `n/a`. Inner Atlas cards never snap to the board grid | pattern | 85 |
| D07 | Direction / value locks | `n/a` — no directional parameter in a rect placement | pattern | 85 |
| D08 | Numeric / manual entry | No digit entry during placement. Typed frame dimensions stay a non-goal (D15). After bind, filter/search is typed in portal-local UI (D35), not during the place gesture | precedent | 80 |
| D09 | Preview & readouts | During the drag: frame outline in `Palette::portal` + live w×h in the dock readout. Bound and idle: folder name · file count (when known) · scan progress · health (`Ok`/`Unknown`/`Missing`). Dock readout when selected: locator · health · inner zoom · focused/unfocused | precedent | 80 |
| D10 | Cursor | Crosshair while armed. Over an unfocused portal: arrow. In contents-focus: File Atlas canvas cursors. Over the maximize square and frame chrome: arrow | precedent | 75 |
| D11 | Commit | One journaled `Add` of a portal node: `{ rect, class: Host, kind: FileAtlas, source: None }`. Frame and source (once bound) are journaled. Scan batches, tree, thumbs, inner camera, collapse, and selection are never journaled. One gesture = one undo. No `BoardLastStyle` consumed (D16) | pattern | 90 |
| D12 | Cancel | Esc cancels an active file carry first, without placing files or leaving contents focus; otherwise P0.1 / P1.portal.contents-focus applies. A primary click outside the focused body peels focus and belongs to the board. | stated | 100 |
| D13 | Selected presentation | P1.portal.pick / P1.node.transform. Windows-style hover resize — no prior selection. Contents expose no board grips. Portals stay axis-aligned: no rotate chrome. Maximize square per P1.portal.chrome (D33/D34) | pattern | 90 |
| D14 | Post-edit | Rebind and authored query knobs through the Portal inspector or `portal.atlas.*` commands; each is a journaled `Patch`. Inner camera / collapse / selection are re-edited by using the surface | pattern | 80 |
| D15 | Non-goals | Cut: any File Atlas app feature or chrome path; embedding `AtlasApp` / a second process / File Atlas window chrome inside the frame; a second copy of the folder-map painter; an Atlas-owned tag model; Edit-mode filesystem writes in v1; Cover Flow / Home / Advanced / destination assignment / export tray; a fifth tab-level `ViewKind`; authenticated anything. Not cut: place, bind a local folder, live `atlas-shell::folder_map` + atlas-core scan/thumbs, contents-focus, maximize, refresh, bake, Open in File Atlas (existing Slate second viewport) | stated | 100 |
| D16 | Create-style inheritance | P1.portal.style: no BoardLastStyle. Unauthored File Atlas fill follows `Palette::card` (slightly lighter than the board); unauthored stroke paints a 1-unit `Palette::border_strong` hairline so the window has an outline. Theme switches never mutate the scene. The Fill squircle authors `portal.fill`; the Stroke squircle authors `portal.stroke`. Cards and empty/status text still follow the active palette | stated | 100 |
| D17 | Hit-testing & pick | P1.portal.pick: frame rect including marquee. Unfocused clicks hit the frame. Contents-focus hits the shared Atlas hover/collapse grips; portal_frame.border_hit_px stays a Slate target. A carry begun inside focused contents owns input until release, including outside the portal. | stated | 100 |
| D18 | Portal class & authority | **Host** (Art. V.3 / P1.portal.frame). The folder's journal is the filesystem; Slate owns only the frame + source pointer. Named "lens" in the Portals flyout — class is still host because a live folder map is an inner surface | stated | 100 |
| D19 | Source binding | One `SourceUri` naming a **local folder**, stored relative-first (Art. IX.2). Bound by `portal.atlas.source`. Health `Ok`/`Missing`/`Unknown`. Refused: files, URLs, cloud accounts, a File Atlas window handle. Rebind is a journaled `Patch` | pattern | 90 |
| D20 | Query & parameters | v1 journaled query is `AtlasPortalQuery { sort }` only (default Name). Filter/search, if shown, is derived view-state (D31). Collapse, camera, scroll, and selection are never query fields | guess | 55 |
| D21 | Regeneration & staleness | Scan starts on bind, refresh, and watcher events. Work is generation-tagged; a stale batch is dropped (Art. II.3). Cards stream in while discovery is still running. Last-good tree stays painted across a refresh. Same cloud/dehydrate guards as atlas-core | pattern | 85 |
| D22 | Contents interaction | P1.portal.contents-focus. Double-click or Enter enters the Atlas canvas. In focus: shared FolderCam navigation; folder-card clicks collapse/expand and the shared incremental/full grips expand nested folders exactly as standalone Atlas. Click selects a file; Ctrl-click toggles selection. Left-drag on empty canvas, or Shift-drag, marquees files (`Tree::files_in_rect`; Ctrl additive). A real left-drag on a file carries the pressed file or its selection onto the primary Slate board, linking and placing through the same metadata-based recipient as detached Atlas (frame tags and one placement undo). Release inside the source portal or over UI cancels. Leaving the window hands off to atlas_core::shell_drag (copy/link, never move). Double-click opens a file in the OS. After blur the board owns navigation. | stated | 100 |
| D23 | Level of detail | P0.9: the inner camera is portal-local. Board zoom scales the entire map with the frame; screen scale is inner zoom × board zoom, in and out of contents-focus. Board pan and frame movement carry the map with them. Leaving contents preserves the inner view. LOD uses the composed screen scale; drop type when too small, never clamp to a screen constant | stated | 100 |
| D24 | Export serialization | Host (Art. V.3): `slate-artifact` emits a poster plus a pointer to the folder locator. Unbound/Missing export the state card. Not a regenerated SVG of every thumbnail | pattern | 80 |
| D25 | Bake | `portal.atlas.bake` emits one journaled `Add` of an authored Image (the current poster) plus a provenance Text node naming the folder. The portal stays live. v1 does not copy file bytes onto the board | guess | 55 |
| D26 | Collaboration & per-peer | P1.portal.sync. Each peer resolves the relative locator and runs atlas-core locally. A peer that cannot see the folder paints `Unknown` naming the locator. Inner camera and collapse do not sync (D31) | pattern | 85 |
| D27 | Agent surface | `board.portal.atlas` and `portal.atlas.*` are registry SPECs (Art. VII.1). Agent-issued frame/source/sort mutations stage for acceptance (Art. VII.6). An agent may never write the filesystem, never enter File Atlas Edit mode, and never initiate rename/move/copy/delete | stated | 100 |
| D28 | Determinism & provenance | Host: contents are not a deterministic extracted graph. Provenance caption names the folder locator + health + (when known) discovered file count. Two peers can legitimately see different thumbs while a scan is in flight | research | 75 |
| D29 | Performance envelope | Reuse `atlas-shell::folder_map` and atlas-core. Slate does not instantiate `AtlasApp` and does not import `apps/file-atlas` (Art. I). The inner camera is `FolderCam` (`screen = world × z + offset`, screen-pixel pan) — the same type File Atlas uses. Multiple portals on the same canonical folder share one index/thumb pool; camera/collapse stay per-portal. Nothing on the frame loop blocks on the network. `portal.atlas.live_pool` caps simultaneous live scans if needed | research | 70 |
| D30 | Failure & honesty states | Unbound: "Choose folder…". Missing: names the locator. Unknown: names the locator. Wrong kind (a file was bound): refuse and keep unbound. Empty folder: honest empty map. Partial scan: cards that have arrived, plus scan progress. Dehydrated cloud files: type icons, never a bulk hydrate | pattern | 85 |
| D31 | View-state ownership | Journaled authored intent: `rect`, `source`, `sort` (D20). Derived per-peer, never journaled: inner camera, collapse, selection, hover, filter/search, scan progress, thumb textures, scroll | pattern | 80 |
| D32 | Trust, sandbox & consent | Local folder only. No account, no host API, no webview. Same atlas-core cloud/dehydrate rules as File Atlas. Binding a folder the workbook can already see needs no extra prompt. No File Atlas process is spawned inside the frame | pattern | 85 |
| D33 | Portal chrome | P1.portal.chrome: no identity tab (web-only). Maximize square on the frame. Right-click: Maximize, Enter/Leave contents, Rebind folder, Refresh, Bake, Open in File Atlas (existing Slate second viewport — OQ4). No File Atlas top bar, tools dock, or readouts inside the frame (Art. X). Fill/Stroke/Formatting squircles live on the object property strip, not inside the window | stated | 100 |
| D34 | Portal maximize | P1.portal.maximize. Maximize fills the Slate canvas pane; Esc / the square restores. Inner camera is unchanged across maximize. This is Slate chrome, not a File Atlas window | pattern | 85 |
| D35 | Portal-local UI | P1.portal.local-ui. Root folder, sort, refresh, and rebind stay on this portal. Formatting (search, file-type radios, Ghost/Hide, Zoom to matches, Zoom to fit) is the object-strip `AtlasFormat` editor in `selection_tools` (`portal.atlas.fit`). Filter/search/camera remain derived view-state (D31). They do not appear on Document Settings or any board-wide panel | stated | 100 |

## Open questions

None. The four canvas questions were answered on 2026-08-21: host class
(live atlas-core canvas); drop folder opens a lens chooser (File Atlas
default, plus Place files on the board); view-only filesystem in v1;
Open in File Atlas uses the existing Slate-hosted File Atlas viewport.

## Golden paths

- **GP1.** Portals flyout → File Atlas → click the board → 960×540 unbound
  frame → picker binds a local folder → cards stream in.
- **GP2.** Drop a folder → chooser; File Atlas (default) binds a host
  portal. Place files on the board dumps images. Alt+drop skips the chooser.
- **GP3.** Click selects the frame. Double-click enters contents-focus;
  wheel zooms the inner camera; Esc or a click outside the body returns
  to the frame. After peel, the board owns the wheel again.
- **GP4.** Two portals on the same canonical folder share the atlas-core
  index/thumbs; each keeps its own camera and collapse.
- **GP5.** Missing / dehydrated / empty folder → honest state card or empty
  map; no bulk hydrate.
- **GP6.** Maximize fills the Slate pane; Esc restores; inner camera unchanged.
- **GP7.** Bake → authored poster + provenance text; portal stays live.
- **GP8.** Agent may stage place/bind/sort. Agent cannot rename, move, or
  delete a file.
- **GP9.** Enter contents, then leave → board wheel scales the frame and
  cards together; board pan carries both. The inner camera stays unchanged.
  Initial fit is identical at different board zooms. At non-unit board zoom,
  clicking a painted card selects it, inner zoom stays pointer-anchored, and
  a right-drag moves the map by the pointer's screen delta. Maximize/restore
  preserves the local view and focused maximize routes the same navigation.

- **GP10.** At non-unit board zoom, click a folder card to collapse it. The full
  grip expands its nested folders; the incremental grip opens one level. Click
  a child’s grip to descend. Rebuild after new scan entries preserves those
  decisions. A press/click never starts a Windows drag.
- **GP11.** Press a file, then drag across the portal boundary in one frame.
  Release over the board: link at the correct board coordinate, inherit frame
  tags, undo/redo placement as one group. Release inside the portal or press Esc:
  add nothing. Switching workbooks/rebinding cancels. Leaving the window hands
  the same paths to Windows; it never moves source files.
- **GP12.** Toggle dark → light → dark: an unauthored File Atlas window follows
  `Palette::card` (slightly lighter than the board) so its outline is visible;
  cards and empty/status text follow the palette. An authored fill or stroke
  stays put. The serialized scene does not change.
- **GP13.** Select the portal: Fill, Stroke, and Formatting squircles appear on
  the object strip. Fill authors `portal.fill`. Stroke authors `portal.stroke`
  (width 0 = none) and paints the window border. Formatting is the File Atlas
  filter menu (search, type radios, Ghost/Hide, Zoom to matches) plus Zoom to
  fit (`portal.atlas.fit`). Filter/search/camera stay derived view-state.
- **GP14.** Enter contents-focus. Drag a box on empty canvas, or Shift-drag,
  to marquee multiple files (`Tree::files_in_rect`; Ctrl additive). A real
  left-drag on a file still carries the pressed file or its selection onto
  the board.

## Feel constants (proposed)

| Token | Meaning | Value |
|-------|---------|-------|
| `portal.atlas.default_size` | click placement size | 960 × 540 |
| `portal_frame.border_hit_px` | frame band that stays a Slate target while focused | 6 |
| `portal.atlas.live_pool` | max simultaneous live folder scans | 2 |
| `portal.atlas.lod_drop_px` | drop inner type below this on-screen size | File Atlas existing LOD |
