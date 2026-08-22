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
DV-19 (scan session), DV-20 (empty CTA scale).

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
`decisions.json` as `approved` (canvas accepted 2026-08-21).

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
| D12 | Cancel | Esc peels one layer per press (P0.1): contents-focus → drag draft → armed tool → selection. A primary click outside the focused body (or on another node) also peels contents-focus; that click then belongs to the board (P1.portal.contents-focus) | pattern | 85 |
| D13 | Selected presentation | P1.portal.pick / P1.node.transform. Windows-style hover resize — no prior selection. Contents expose no board grips. Portals stay axis-aligned: no rotate chrome. Maximize square per P1.portal.chrome (D33/D34) | pattern | 90 |
| D14 | Post-edit | Rebind and authored query knobs through the Portal inspector or `portal.atlas.*` commands; each is a journaled `Patch`. Inner camera / collapse / selection are re-edited by using the surface | pattern | 80 |
| D15 | Non-goals | Cut: any File Atlas app feature or chrome path; embedding `AtlasApp` / a second process / File Atlas window chrome inside the frame; a second copy of the folder-map painter; an Atlas-owned tag model; Edit-mode filesystem writes in v1; Cover Flow / Home / Advanced / destination assignment / export tray; a fifth tab-level `ViewKind`; authenticated anything. Not cut: place, bind a local folder, live `atlas-shell::folder_map` + atlas-core scan/thumbs, contents-focus, maximize, refresh, bake, Open in File Atlas (existing Slate second viewport) | stated | 100 |
| D16 | Create-style inheritance | P1.portal.style: **No.** The frame does not consume `BoardLastStyle` | pattern | 90 |
| D17 | Hit-testing & pick | P1.portal.pick: the frame picks on its rect, including marquee. Unfocused: every click hits the frame. Contents-focus: hits go to the Atlas canvas (`atlas-shell::folder_map` hover / collapse grips), with a border band (`portal.atlas.border_hit_px`) that stays a Slate target. A primary click outside the body peels focus; the click then belongs to the board | precedent | 80 |
| D18 | Portal class & authority | **Host** (Art. V.3 / P1.portal.frame). The folder's journal is the filesystem; Slate owns only the frame + source pointer. Named "lens" in the Portals flyout — class is still host because a live folder map is an inner surface | stated | 100 |
| D19 | Source binding | One `SourceUri` naming a **local folder**, stored relative-first (Art. IX.2). Bound by `portal.atlas.source`. Health `Ok`/`Missing`/`Unknown`. Refused: files, URLs, cloud accounts, a File Atlas window handle. Rebind is a journaled `Patch` | pattern | 90 |
| D20 | Query & parameters | v1 journaled query is `AtlasPortalQuery { sort }` only (default Name). Filter/search, if shown, is derived view-state (D31). Collapse, camera, scroll, and selection are never query fields | guess | 55 |
| D21 | Regeneration & staleness | Scan starts on bind, refresh, and watcher events. Work is generation-tagged; a stale batch is dropped (Art. II.3). Cards stream in while discovery is still running. Last-good tree stays painted across a refresh. Same cloud/dehydrate guards as atlas-core | pattern | 85 |
| D22 | Contents interaction | P1.portal.contents-focus (host). Click selects the frame. Double-click or Enter enters the Atlas canvas. Esc or a primary click outside the body leaves. In focus: File Atlas camera (wheel / pinch zoom, right/middle pan), collapse/expand via the same grips as the standalone app, select files (readout only). Double-click a file opens it in the OS viewer. After peel, the inner camera must not keep the wheel. Board drawing tools never reach the cards | precedent | 75 |
| D23 | Level of detail | P0.9 on the inner surface: cards, type, icons, badges scale with inner zoom. Board zoom only shrinks the clip. When on-screen type is too small, drop it. Never clamp to a screen constant | pattern | 80 |
| D24 | Export serialization | Host (Art. V.3): `slate-artifact` emits a poster plus a pointer to the folder locator. Unbound/Missing export the state card. Not a regenerated SVG of every thumbnail | pattern | 80 |
| D25 | Bake | `portal.atlas.bake` emits one journaled `Add` of an authored Image (the current poster) plus a provenance Text node naming the folder. The portal stays live. v1 does not copy file bytes onto the board | guess | 55 |
| D26 | Collaboration & per-peer | P1.portal.sync. Each peer resolves the relative locator and runs atlas-core locally. A peer that cannot see the folder paints `Unknown` naming the locator. Inner camera and collapse do not sync (D31) | pattern | 85 |
| D27 | Agent surface | `board.portal.atlas` and `portal.atlas.*` are registry SPECs (Art. VII.1). Agent-issued frame/source/sort mutations stage for acceptance (Art. VII.6). An agent may never write the filesystem, never enter File Atlas Edit mode, and never initiate rename/move/copy/delete | stated | 100 |
| D28 | Determinism & provenance | Host: contents are not a deterministic extracted graph. Provenance caption names the folder locator + health + (when known) discovered file count. Two peers can legitimately see different thumbs while a scan is in flight | research | 75 |
| D29 | Performance envelope | Reuse `atlas-shell::folder_map` and atlas-core. Slate does not instantiate `AtlasApp` and does not import `apps/file-atlas` (Art. I). The inner camera is `FolderCam` (`screen = world × z + offset`, screen-pixel pan) — the same type File Atlas uses. Multiple portals on the same canonical folder share one index/thumb pool; camera/collapse stay per-portal. Nothing on the frame loop blocks on the network. `portal.atlas.live_pool` caps simultaneous live scans if needed | research | 70 |
| D30 | Failure & honesty states | Unbound: "Choose folder…". Missing: names the locator. Unknown: names the locator. Wrong kind (a file was bound): refuse and keep unbound. Empty folder: honest empty map. Partial scan: cards that have arrived, plus scan progress. Dehydrated cloud files: type icons, never a bulk hydrate | pattern | 85 |
| D31 | View-state ownership | Journaled authored intent: `rect`, `source`, `sort` (D20). Derived per-peer, never journaled: inner camera, collapse, selection, hover, filter/search, scan progress, thumb textures, scroll | pattern | 80 |
| D32 | Trust, sandbox & consent | Local folder only. No account, no host API, no webview. Same atlas-core cloud/dehydrate rules as File Atlas. Binding a folder the workbook can already see needs no extra prompt. No File Atlas process is spawned inside the frame | pattern | 85 |
| D33 | Portal chrome | P1.portal.chrome: no identity tab (web-only). Maximize square on the frame. Right-click: Maximize, Enter/Leave contents, Rebind folder, Refresh, Bake, Open in File Atlas (existing Slate second viewport — OQ4). No File Atlas top bar, tools dock, or readouts inside the frame (Art. X) | precedent | 80 |
| D34 | Portal maximize | P1.portal.maximize. Maximize fills the Slate canvas pane; Esc / the square restores. Inner camera is unchanged across maximize. This is Slate chrome, not a File Atlas window | pattern | 85 |
| D35 | Portal-local UI | P1.portal.local-ui. Root folder, sort, refresh, rebind, and (if shown) filter live on this portal's inspector / empty state. They do not appear on Document Settings or any board-wide panel | pattern | 90 |

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

## Feel constants (proposed)

| Token | Meaning | Value |
|-------|---------|-------|
| `portal.atlas.default_size` | click placement size | 960 × 540 |
| `portal.atlas.border_hit_px` | frame band that stays a Slate target while focused | 6 |
| `portal.atlas.live_pool` | max simultaneous live folder scans | 2 |
| `portal.atlas.lod_drop_px` | drop inner type below this on-screen size | File Atlas existing LOD |
