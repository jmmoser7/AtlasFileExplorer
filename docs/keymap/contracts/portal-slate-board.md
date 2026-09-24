# Slate board portal — interaction contract

Status: agreed
Family: portal
Portal class: **document** (Art. V.3) · Subtype: **nested workbook board**
Command: `board.portal.slate` (placement) · Key: none · Palette: "slate board"
(aliases: nested board, workbook portal)
Inherits: P0.* (all), P1.node, **P1.portal**, **P2.PortalPlace**, **P2.PortalHost** — deviations flagged below.
Canvas: `portal-slate-board-tool-contract` (volatile)

## What it is, and the 10% it implements

A journaled frame on a Slate board whose contents are another `.slate`
workbook's **Board** scene, fitted into the frame. This is the document
portal Article V.3 already names: the parent journal owns the frame; the
child workbook's journal owns that board. Spike S1 named this portal and
fixed version one: the frame loads and shows the child; double-click opens
the child as a tab; in-place editing comes later and must not require a
new coordinate system.

The 90% cut is in D15. A `.slate` file still never becomes a pool item.
A drop on a blank board opens a tab. A drop on a board that already has
nodes asks: open the file, or insert it as this portal.

## Pushback before agreement (Art. XI)

1. **Article V.3 — document, not host.** A nested board is not a poster of
   a foreign app. Mutations inside the child belong to the child's journal.
   `PortalClass` today has `Generated` and `Host` only; `Document` is the
   variant this portal adds.
2. **Article IX / the item guard.** `item_for_path` still refuses a `.slate`
   as a pool item. A drop on a blank board opens a tab. A drop on a board
   that already has nodes asks whether to open the file or insert it.
   A portal may not bind its own parent path, and a cycle (A contains B
   contains A) is refused.
3. **Article IV — one fit, two interpreters.** The board painter and
   `slate-artifact` both call one fit function in `slate-doc`. Export emits
   the child rendered, with a caption naming the locator.
4. **Article II / XII.** The child is loaded off the frame loop. Paint goes
   through the shared portal shell and the existing board-node painter.
   Do not paste `board_web.rs` into a new host file, and do not embed a
   second `SlateApp` or a WebView of the export.

## Fit (specified now, consumed by paint and by a later edit mode)

Not journaled. Both interpreters call the same function.

- `content` is the portal rect inset by the frame fillet.
- `child_bounds` is the axis-aligned union of the child's visible Board
  node rects. Hidden nodes are excluded. An empty or degenerate union
  paints the empty card and has no transform.
- `scale = min(content.width / child_bounds.width, content.height / child_bounds.height)`.
- The scaled child bounds are centered in `content`. Aspect is preserved.
- A parent point maps back by the inverse of that translation and scale.
  Version one does not hit-test child nodes with it.

Child links resolve against the **child** workbook directory.

## Behavior matrix

Rows keyed to `DIMENSIONS.md` in registry order. Mirrored in
`decisions.json` as `approved` (canvas accepted 2026-09-22; D19 altered
in chat: drop chooser on an occupied board, open-as-tab on a blank board).

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Slate board only. Command `board.portal.slate`. Palette name "slate board" (aliases: nested board, workbook portal). Portals flyout, no single-key chord. Not a File Atlas feature. | pattern | 90 |
| D02 | Stickiness & repeat | P2.PortalPlace.oneshot / P0.4. One-shot: commit returns to Select. Space or Enter re-arms. | pattern | 90 |
| D03 | Gesture grammar | P2.PortalPlace.gesture: Armed, then Dragging(rect), then Committed(unbound). Release paints "Choose workbook…". Binding is a separate step (D19). | pattern | 90 |
| D04 | Click vs drag rule | P2.PortalPlace.click. Travel under `draft.drag_threshold` (4 screen px) places 960×540 world units (`PORTAL_DEFAULT_W` × `PORTAL_DEFAULT_H`) centred on the click. Past the threshold, the drag is the rect. | precedent | 90 |
| D05 | Modifiers | P2.PortalPlace.aspect. Shift during the drag locks 16:9. An unmodified drag is free-aspect. Ctrl and Alt do nothing during placement. | precedent | 85 |
| D06 | Constraints & snapping | P2.PortalPlace.snap. Grid snap and smart guides apply to the frame rect. The child scene does not snap to the parent grid. No ortho lock. | pattern | 85 |
| D07 | Direction / value locks | n/a. Rect placement has no directional parameter. | pattern | 85 |
| D08 | Numeric / manual entry | No digit entry during placement. After bind, Rebind edits the path. Frame size uses the existing node resize, not a new numeric mode. | precedent | 80 |
| D09 | Preview & readouts | While dragging: portal outline and live width × height in the dock readout. Bound and idle: file name, health (Ok, Unknown, or Missing). Selected readout adds the relative locator and whether that workbook is an open tab. | precedent | 78 |
| D10 | Cursor | Crosshair while armed. Arrow over the frame, the maximize square, and the chrome. Version one has no contents-focus cursor. | precedent | 80 |
| D11 | Commit | One journaled Add: rect, class Document, kind Slate, source None. Binding later is a Patch. The child scene is not copied into the parent journal. One gesture is one undo. No BoardLastStyle (D16). | pattern | 90 |
| D12 | Cancel | Esc during the drag cancels placement and returns to Select (P0.1). Esc on a selected portal does not unbind it. Maximize, if up, peels first (D34). There is no contents-focus layer in version one. | pattern | 85 |
| D13 | Selected presentation | P1.portal.pick and P1.node.transform. Hover resize, axis-aligned, no rotate chrome. Child nodes are not grips. Maximize square per P1.portal.chrome. | pattern | 90 |
| D14 | Post-edit | Rebind is `portal.slate.source`, a journaled Patch that drops the cached child. Resize refits the same child (D20). Edits to the board happen in the child tab and stay on the child journal. | pattern | 82 |
| D15 | Non-goals | Cut in version one: in-place editing; contents-focus tools; the child's Grid, Venn, or Lens; a second Slate process; a WebView of the export; binding the parent path; a `.slate` drop on empty canvas becoming a portal; bake-merging the child into the parent; agent writes into the child; reading secrets. Not cut: place, bind a local `.slate`, fitted paint of the child's Board, open-in-tab, refresh, cycle refusal, export of the child rendered in the frame. | research | 78 |
| D16 | Create-style inheritance | P1.portal.style. No BoardLastStyle. Unauthored fill follows `Palette::card`, slightly lighter than the canvas in both light and dark. No outline unless `portal.stroke` is authored. Theme changes do not write the scene. | stated | 100 |
| D17 | Hit-testing & pick | P1.portal.pick. Click and marquee hit the frame rect. A click on the painted child selects the frame, not a child node. `border_hit_px` comes only from `portal_frame_tokens`. | pattern | 88 |
| D18 | Portal class & authority | Document (Art. V.3). The child workbook's journal owns mutations of that board. The parent journals the frame only: rect, title, source, fill, stroke. `PortalClass::Document` is new. Determinism is not required. | pattern | 92 |
| D19 | Source binding | Dropping a `.slate` file on a board whose scene already has nodes opens a chooser: Open (the existing tab path, `open_doc_at`) or Insert (a bound Slate portal, 960×540, centred on the drop). A blank board, meaning no board nodes, skips the chooser and opens the file as a tab. Browse on an unbound Slate portal, or `portal.slate.source`, still binds. One `SourceUri`, a local `.slate` file, stored relative-first (Art. IX.2). Health is Ok, Missing, or Unknown, and the card names the locator. Refused: empty, a directory, a URL, a non-`.slate` file, the parent workbook path, and a locator that closes a cycle. Rebind is a journaled Patch. Child-internal links resolve against the child workbook. | stated | 100 |
| D20 | Query & parameters | No journaled query in version one. The fitted region is the union of the child's visible Board node rects (see Fit). An empty union paints "Empty board". Frames are nodes in that union, not a paged deck. | guess | 52 |
| D21 | Regeneration & staleness | Load and reload run off the frame loop and are generation-tagged (Art. II.3). A stale load is dropped. Triggers: bind, `portal.slate.refresh`, and a changed child mtime. If that workbook is already an open tab, paint the live scene, including unsaved edits, instead of the disk copy. The last good scene stays up across a refresh. | guess | 60 |
| D22 | Contents interaction | Click selects the frame. Double-click or Enter (`portal.slate.open`) opens the child through `open_doc_at`, which focuses the tab when that canonical path is already open. Wheel, pan, and board tools stay with the parent (P0.5). No contents-focus in version one. The fit inverse in Fit is the later edit-mode map. | research | 78 |
| D23 | Level of detail | P0.9. Board zoom scales the portal and the fitted child together, with no screen-size floor. Type that is too small drops, using the board painter's existing LOD. The fit is in world space; screen size is that result times parent zoom. | pattern | 85 |
| D24 | Export serialization | Document class: `slate-artifact` renders the child's Board into the frame with the same fit function (Art. IV). A caption names the relative locator and health. Unbound, Missing, cycle, and empty export that state card. Depth cap matches D29. The child secret store and WebView profile are not packaged. | pattern | 85 |
| D25 | Bake | No bake command in version one. Copying the child's nodes into the parent is a document merge and stays deferred. The portal remains a link. | guess | 48 |
| D26 | Collaboration & per-peer | P1.portal.sync. Frame, source, fill, and stroke sync as journal deltas. Each peer resolves the relative locator and loads locally. A peer who cannot resolve it paints Unknown and names the locator. The loaded scene is not transmitted. | pattern | 85 |
| D27 | Agent surface | `board.portal.slate`, `portal.slate.source`, `portal.slate.refresh`, and `portal.slate.open` are registry commands (Art. VII.1). Agent frame and source mutations stage for acceptance (Art. VII.6). An agent may not write the child, read secrets, or bind a cycle or the parent path. The beacon carries locator and health, not the child scene. | pattern | 85 |
| D28 | Determinism & provenance | Not deterministic (Art. V.3, document class). The caption is the relative locator, health, and the child mtime when known. Two peers may differ while one has unsaved tab edits (D21). | pattern | 82 |
| D29 | Performance envelope | One cached child scene per canonical path, shared by every portal on that file; the fit stays per portal. Generation tags drop stale loads. Paint depth is 2: a Slate portal inside the child paints the grandchild, and anything deeper paints a card naming the locator without loading it. The frame loop does not read the child file. Child tessellation is cached with the scene (Art. II). | guess | 58 |
| D30 | Failure & honesty states | Unbound paints "Choose workbook…". Missing and Unknown name the locator. A wrong kind is refused and the frame stays unbound, with the reason. A cycle or a self-bind paints "This workbook already contains that board" and names both locators, without loading. An empty board paints "Empty board". An unreadable file paints the parse error. A failed child does not take down the parent board. | pattern | 80 |
| D31 | View-state ownership | Journaled: rect, title, source, fill, stroke. Derived, never journaled: loaded scene, fit transform, load generation, hover, maximize, open-tab hint. No inner camera in version one. | pattern | 85 |
| D32 | Trust, sandbox & consent | Local `.slate` only. No network fetch, no script, no WebView. Loading parses the scene and does not start the child's agent sidecars, does not open the child's web portals as live pages, and does not read secrets. Child host portals paint their frame and empty or poster state inside the nest. | research | 70 |
| D33 | Portal chrome | P1.portal.chrome. No identity tab (web-only). Maximize square at the upper right. Right-click: Open workbook, Rebind, Refresh, Maximize. No Slate top bar inside the frame. | pattern | 88 |
| D34 | Portal maximize | P1.portal.maximize. Maximize fills the Slate canvas pane with the same fitted board, still read-only. The node rect is not mutated. Esc peels maximize first. Maximize does not open a tab. | pattern | 80 |
| D35 | Portal-local UI | P1.portal.local-ui. Locator, health, Rebind, Refresh, and Open workbook stay on this portal's inspector and menu. They do not appear on Document Settings or any board-wide panel. | pattern | 90 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `PORTAL_DEFAULT_W` × `PORTAL_DEFAULT_H` | Click-place size | 960 × 540 world units |
| `draft.drag_threshold` | Click versus drag | 4 screen px (existing) |
| `portal.slate.paint_depth` | Nested Slate portals painted before a card | 2 |

## Golden paths

- **GP1.** Portals flyout, Slate board, click the canvas. A 960×540 unbound frame appears. Browse binds a local `.slate`. The child's visible Board nodes fit inside the frame, aspect preserved.
- **GP2.** Bind the parent workbook's own path. The bind is refused and the frame stays unbound.
- **GP3.** Workbook A binds B, and B binds A. The nested portal paints the cycle card and does not load again.
- **GP4.** Double-click opens the child tab. A second double-click focuses that same tab.
- **GP5.** The bound file is missing. The frame names the locator. The parent board stays usable.
- **GP6.** Drop a `.slate` on a blank board (no board nodes). It opens a tab. No chooser. It does not become a pool item.
- **GP7.** Drop a `.slate` on a board that already has nodes. The chooser offers Open and Insert. Insert places a bound portal at the drop point. Open uses the tab path.
- **GP8.** A child that itself contains a Slate portal paints that grandchild. One level deeper paints a card and does not load.
- **GP9.** HTML export draws the child Board in the frame with the same fit, plus a caption of the locator. Secrets are absent from the package.
- **GP10.** Undo of placement removes the portal and does not modify the child file.

## Open questions

None. Accepted 2026-09-22. D20 fits the whole visible board. D25 has no bake. D29 paints to depth 2. D21 paints the live tab, including unsaved edits. D19 is the drop chooser above: Open or Insert when the board already has nodes, and open-as-tab on a blank board.

## Implementation reuse (required when Family: portal)

Owner: `slate-doc` for `PortalClass::Document`, `PortalKind::Slate`, the fit function, and the cycle check. Paint is a body hook on the shared portal shell in `board_portal_chrome`, drawing child nodes with the existing board-node painter under that fit. The load cache lives in the Slate app, off the frame loop. `slate-artifact` calls the same fit and walks the child scene.

Forbidden forks: a `board_slate.rs` pasted from `board_web.rs` or `board_atlas.rs`; a second `SlateApp`; a WebView of the exported HTML; a per-kind `resolve_source`.
