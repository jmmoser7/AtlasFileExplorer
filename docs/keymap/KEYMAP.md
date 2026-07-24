# The canvas keymap — master classification

This is the governing keymap document for the **canvas command project**: the
adoption of a professional-grade default keymap (drawn from Miro, Photoshop,
Illustrator, Rhino, and Grasshopper) across **Slate** and **File Atlas**.

- Architecture: `docs/keymap/ARCHITECTURE.md` (command registry, tool modes,
  cancel stack, connectors).
- Source-app UX research: `docs/keymap/research/*.md` (Stage-2 inputs).
- Implementation specs: `docs/keymap/specs/*.md`.
- Per-app canonical binding tables remain `apps/*/src/app/commands.rs`
  (`ENTRIES`) per each app's `COMMANDS.md` — this document decides *what* is
  bound; `ENTRIES` remains the runtime source of truth the Advanced window
  reads.

## Constitutional ground rules

Every decision below was held against `CONSTITUTION.md`:

- **Art. I / VII (command registry, parity):** every binding maps to a
  *registered, dispatchable command*, not an ad-hoc key handler. The
  Rhino-inspired features (repeat-last, command history, canvas palette,
  radial menu) are front-ends to that registry — the same registry the
  Phase-4 MCP surface will expose.
- **Art. III (the 10% rule):** bindings that exist in a source app but have
  no real reach-for-it use on our canvases are **rejected**, with reasons.
- **Art. IV (SVG ceiling):** new visual capabilities (connectors, sticky
  notes, image invert) are admitted only because SVG/CSS can express them;
  each lands in the egui painter *and* `slate-artifact` together.
- **Art. VI (journal-only mutation):** every mutating command commits
  through `SceneJournal` (Slate) or the atlas-core `Journal` (Atlas).
- **Art. VIII (bandwidth):** keyboard, mouse gestures, palette, and radial
  menu are all *adapters* compiling to the same commands.
- **Art. X (no chrome divergence):** shared surfaces (minimap, palette,
  radial menu, history window) are built once in `atlas-shell`.

## Status legend

| Status | Meaning |
|--------|---------|
| ✅ exists | Already implemented and registered; keymap confirms it |
| 🟢 adopt | New, build in this project (P1) |
| 🟡 adopt-P2 | Agreed direction, build after P1 lands |
| 🔵 adapt | Adopted with deliberately changed semantics (noted) |
| ⛔ reject | Not adopted — reason + article cited |

---

## Letters (single-key tools — Slate Board view unless noted)

Single-key tool switches remain **Board-view only** and suppressed while
typing or presenting (existing convention). "Both" = Slate all views + Atlas.

| Key | Source | Action here | Scope | Status | Notes |
|-----|--------|-------------|-------|--------|-------|
| A | Illustrator | **Direct Selection** — anchor/segment editing on `Path` shapes | Board | ✅ exists | Completes Roadmap Phase 2. Hollow/filled anchor convention, handle editing, marquee over anchors. |
| B | Photoshop | **Brush** — freehand variable-width ink stroke in the foreground color | Board | ✅ exists | Distinct from `P` (precise pen/curve family). Consumes the shared color state (see `D`/`I`/`X`). Brushes are declarative presets (Art. VII.3). |
| C | Photoshop | **Crop** — enter existing crop mode on the selected croppable image | Board | ✅ exists | Pure binding; crop mode already exists (double-click). |
| D | Photoshop | **Default colors** — reset foreground/background to ink/paper | Board | ✅ exists | Requires the fg/bg color state. `X` (swap fg/bg) adopted as the natural companion. |
| E | Photoshop | **Eraser** — drag across strokes to remove them | Board | ✅ exists | Vector semantics: stroke-level erase (hit-tested whole-stroke delete) first; segment splitting is 🟡 P2. Journaled `Remove`s. |
| F | Miro | **Frame tool** | Board | ✅ exists | Fit-view stays `Home` on Board, `F` elsewhere (existing, deliberate). |
| G | Miro | **Toggle board grid** | Board | ✅ exists | Grid toggle exists in dock; this adds the key. `F7` = alias (Rhino muscle memory). |
| H | Miro | **Hand tool** | Board | ✅ exists | Atlas needs no hand mode — every drag already pans (⛔ for Atlas, no-op value). |
| I | Photoshop | **Eyedropper** — sample color under cursor into foreground | Board | ✅ exists | Samples node fill/stroke/text color; image-pixel sampling 🟡 P2. Alt+click samples into background. |
| L | Rhino | **Line** — parametric two-point line (click-move-click or press-drag-release) | Board | ✅ exists | Contract shipped 2026-07-23: `contracts/line.md` (P2.RhinoDraft grammar — Tab direction lock, typed length, one-shot; endpoint grips, no bbox). Replaced the bbox drag line; legacy `.slate` bbox lines convert on load. Also in the `P` precise-family flyout. |
| M | Miro | **Minimap toggle** | **Both** | ✅ exists | Shared `atlas-shell` overlay widget (Art. X). Click/drag to navigate; viewport rectangle indicator. Atlas trees are huge — both apps win. |
| N | Miro | **Sticky note** | Board | ✅ exists | *Not* a new node kind: a Text-node preset (fill, padding, autosize) under the SVG ceiling. Color cycling via brush/palette state. |
| O | Miro | **Ellipse tool** | Board | ✅ exists | |
| P | Miro | **Pen (curve family)** | Board | ✅ exists | Freehand pen exists; `B` takes over expressive inking, `P` stays the precise family (Line/Arc/Polyline/Bezier flyout). |
| R | Miro | **Rectangle tool** | Board | ✅ exists | |
| S | Illustrator | **Scale tool** — set origin, then drag to scale selection about it | Board | 🟡 adopt-P2 | Bounding-box handles cover 90% today; the set-origin scale earns its place for precise composition. Research decides final shape. |
| T | Miro | **Text tool** | Board | ✅ exists | |
| V | Miro | **Select tool** | Board | ✅ exists | |
| Z | Photoshop | **Zoom tool** — click = zoom in, Alt+click = out, drag = zoom window | **Both** | ✅ exists | Also absorbs Rhino's `Ctrl+W` Zoom Window (⛔ as a chord — see rejects). Slate: Board/Grid/Venn (Lens keeps its own camera keys); a transient mode — the previous tool re-arms on disarm. |

## Special keys

| Key | Source | Action here | Scope | Status | Notes |
|-----|--------|-------------|-------|--------|-------|
| Space (tap) | Rhino | **Repeat last command** | Both | 🔵 adapt | Tap = repeat; **hold+drag remains pan** (Miro). Repeat draws from the registry history and honors a never-repeat list (undo/redo/repeat/save…). |
| Enter | Rhino | **Repeat last command** (idle only) | Both | 🔵 adapt | Only when no draft/crop/edit is active — Enter keeps finishing paths, crops, text. |
| Esc | Rhino | **Cancel stack** | Both | ✅/🔵 | Already layered ad-hoc; formalized as an explicit cancel stack in the registry (tool op → draft → crop → selection → menus → tool=Select). |
| Tab / Shift+Tab | Miro | **Cycle selection through objects** | Board + Atlas | ✅ exists | Board: reading-order traversal with camera nudge; groups count as one stop. Atlas: cycles filtered file matches. During a P2.RhinoDraft gesture, Tab instead locks the pending segment's direction (`contracts/line.md` D07). |
| Delete | Rhino | **Delete object** | Board | ✅ exists | ⛔ for Atlas: Atlas never deletes real files (its mutations are assign/export only). |
| Arrows | Miro | **Nudge selection / pan canvas** | Both | 🔵 adapt | Board nudge exists (Shift = ×10). New: with nothing selected, arrows pan the canvas — and this is the Atlas behavior. |
| [ / ] | Photoshop | **Brush width − / +** | Board | ✅ exists | Live width-circle cursor preview while stepping. Also steps the eraser while E is armed. |
| , / . | Photoshop | **Previous / next brush preset** | Board | 🟡 adopt-P2 | Presets are user-space data assets (Art. VII.3). |
| Page Up / Down | Miro | **Bring to front / send to back** | Board | ✅ exists | `reorder_nodes` exists; this adds keys. Present mode owns its own keyboard (no conflict). |

## Function keys

| Key | Source | Action here | Scope | Status | Notes |
|-----|--------|-------------|-------|--------|-------|
| F1 | Rhino | **Help** — open Advanced → Commands & shortcuts | Both | ✅ exists | |
| F2 | Rhino | **Command history** window | Slate | 🔵 adapt | Registry execution log (author-attributed, per Art. VI). **Atlas keeps `F2` = Assign selection** (established muscle memory); Atlas reaches history via Advanced. |
| F3 | Rhino | **Properties** — toggle selection inspector | Both | ✅ exists | Atlas: opens Details for the selected file. Also absorbs Illustrator's F11 Attributes (⛔ — F11 is fullscreen). |
| F6 | Photoshop | **Color panel** — fg/bg color popover | Board | 🟡 adopt-P2 | Folded into the color-state work; the dock Colors chips cover P1. |
| F7 | Rhino | **Grid toggle** (alias of `G`) | Board | ✅ exists | |
| F8 | Rhino | **Toggle Ortho** — persistent 45° constraint on draw/move | Board | ✅ exists | Shift *inverts* the current ortho state while held (Rhino semantics). |
| F9 | Rhino | **Toggle snap-to-grid** | Board | ✅ exists | Dock toggle exists; adds the key. |
| F11 | Illustrator | Attributes panel | — | ⛔ reject | `F11` is canvas fullscreen in both apps (registered, shipped). Attributes content lives in the `F3` inspector. |
| F12 | Rhino | DigClick | — | ⛔ reject | Digitizer hardware command; no analog here (Art. III). |

## Shift + key

| Key | Source | Action here | Scope | Status | Notes |
|-----|--------|-------------|-------|--------|-------|
| Shift+drag | Photoshop | **Constrain** — square/circle draw, 45° move | Board | ✅ exists | Draw constraint + 45° move (one-shot ortho inversion of `F8`) both live. |
| Shift+click | Photoshop | **Add to selection** | Board | 🟢 adopt | Atlas keeps Shift+click = range select (✅ exists, better fit for a file canvas). |
| Shift+Tab | Miro | Cycle backward | Board + Atlas | ✅ exists | With Tab. |
| Shift+Arrow | Miro | Nudge ×10 | Board | ✅ exists | |
| Shift+C | Photoshop | **Cycle tool family** (general `Shift+<tool key>` pattern) | Board | 🟡 adopt-P2 | e.g. Shift+P cycles Line→Arc→Polyline→Bezier; Shift+R cycles Rect→Ellipse. |
| Shift+F5 | Illustrator | **Graphic styles** — saved style presets | Board | 🟡 adopt-P2 | Styles as declarative user-space assets (Art. VII.3); apply-to-selection is one journaled patch. `F5` alone stays Present. |

## Ctrl + key

| Key | Source | Action here | Scope | Status | Notes |
|-----|--------|-------------|-------|--------|-------|
| Ctrl+A | Rhino | Select all | Both | ✅ exists | |
| Ctrl+B | Grasshopper | **Send to back** | Board | ✅ exists | Alias of PageDown behavior. |
| Ctrl+C | Rhino | **Copy** | Board + Atlas | ✅ exists | Board: node clipboard (serialized nodes). Atlas: copy selected file paths to the OS clipboard (real daily-driver use). |
| Ctrl+D | Miro | Duplicate | Board | ✅ exists | |
| Ctrl+F | Miro | **Search** | Both | ✅ exists | Slate: canvas search overlay (items, text nodes, frame titles, tags) with zoom-to-result. Atlas: focuses the existing filter search box. |
| Ctrl+G | Rhino | **Group** | Board | ✅ exists | New `group` field on nodes (flat groups first; nesting 🟡 P2). Selection expands to group; `Ctrl+Shift+click` selects a member (Rhino sub-object convention). |
| Ctrl+H | Rhino | **Hide selected** | Board | ✅ exists | New `hidden` node flag — skipped by paint and hit-test, journaled patch. |
| Ctrl+I | Photoshop | **Invert image** | Board | ✅ exists | New `invert` in `ImageAdjust`; CSS `invert()` keeps it inside the SVG ceiling; lands in painter + artifact together (Art. IV). |
| Ctrl+J | Rhino | **Join** — join selected open paths at nearest endpoints | Board | ✅ exists | Natural Phase-2 vector capability; also closes a single near-closed path. |
| Ctrl+L | Rhino | **Lock selected** | Board | ✅ exists | New `locked` node flag — visible, grayed handles, excluded from selection/drag; still snappable. |
| Ctrl+N | Rhino | **New** workbook tab / Atlas tab | Both | ✅ exists | Alias of Ctrl+T (kept). |
| Ctrl+O | Rhino | Open | Both | ✅ exists | |
| Ctrl+P | Rhino | Print | — | ⛔ defer | Print-faithful sheet/PDF export is **Roadmap Phase 5**; binding reserved until then (Art. III — no real path to use it yet). |
| Ctrl+R | Photoshop | **Rulers + guides** | Board | 🟡 adopt-P2 | Smart guides + grid cover most alignment today; rulers/guides earn their place with presentation work. |
| Ctrl+S | Rhino | Save | Slate | ✅ exists | ⛔ for Atlas — no document to save (index persists itself). |
| Ctrl+T | Rhino | Trim | — | ⛔ reject | `Ctrl+T` = new tab (shipped in both apps). Curve trimming is beyond the moodboard/markup 10% (Art. III); `Ctrl+J` join covers the real need. |
| Ctrl+U | Photoshop | **Hue/Saturation** — adjust popover for selected images | Board | ✅ exists | Opens the existing `ImageAdjust` controls (CSS-filter math already shipped); slider scrubs coalesce in the journal. |
| Ctrl+V | Rhino | **Paste** (at pointer, offset on repeat) | Board | ✅ exists | |
| Ctrl+W | Rhino | Zoom Window | — | ⛔ reject | Absorbed by `Z`-drag (zoom window). `Ctrl+W` reserved for close-tab. |
| Ctrl+X | Rhino | **Cut** | Board | ✅ exists | Copy + journaled delete. |
| Ctrl+Y / Ctrl+Z | Rhino | Redo / Undo | Both | ✅ exists | Never-repeatable (see repeat rules). |

## Ctrl+Shift + key

| Key | Source | Action here | Scope | Status | Notes |
|-----|--------|-------------|-------|--------|-------|
| Ctrl+Shift+S | Photoshop | Save As | Slate | ✅ exists | |
| Ctrl+Shift+G | Rhino | **Ungroup** | Board | ✅ exists | |
| Ctrl+Shift+L | Rhino | **Unlock all** | Board | ✅ exists | Locked nodes aren't click-selectable, so unlock-all is the practical form; one-off unlock via Ctrl+Shift+click force-selection. |
| Ctrl+Shift+V | Photoshop | **Paste in place** | Board | ✅ exists | |
| Ctrl+Shift+P | Grasshopper | **Preferences** — open Advanced window | Both | ✅ exists | |
| Ctrl+Shift+H | Rhino | **Show all hidden** | Board | ✅ exists | Rhino's ShowSelected needs a "show ghosts" picker — 🟡 P2; show-all is the P1 form. |

## Alt & mouse + modifier

| Gesture | Source | Action here | Scope | Status | Notes |
|---------|--------|-------------|-------|--------|-------|
| Alt+click | Photoshop | **Sample as background** (eyedropper/brush active) | Board | ✅ exists | Eyedropper tool: Alt+click → bg. Brush: held Alt spring-loads the eyedropper (samples fg). |
| Scroll wheel | Rhino | Zoom | Both | ✅ exists | |
| Space+drag | Miro | Pan | Both | ✅ exists | Coexists with Space-tap = repeat. |
| Ctrl+RMB drag | Rhino | Zoom view | — | ⛔ reject | **Ctrl+right-drag is turbo pan** — a shipped, documented, signature gesture in both apps. Zoom-drag lives on `Z`. |
| Shift+RMB drag | Rhino | Pan | Both | ✅ exists | Right-drag already pans everywhere. |
| Alt+drag | Miro | Duplicate object | Board | ✅ exists | |
| Alt+scroll / Alt+RMB | Rhino | Pan perspective | — | ⛔ reject | 3D-viewport gesture; our placed `.3dm` viewports already implement Rhino nav internally. |
| Ctrl+click | Rhino | Remove from selection | Both | ✅ exists | Ctrl+click toggles — removal included. |
| Ctrl+MMB drag | Rhino | Pan | Board | ✅ exists | Middle-drag pans. |
| Ctrl+Shift+click | Rhino | **Select sub-object** — member inside a group; also force-selects a locked node | Board | ✅ exists | Landed with groups + direct selection. |
| Drag from edge grip | Grasshopper/Miro | **Draw connector (wire)** | Board | ✅ exists | See connector spec — hover a node's edge to reveal 4 side grips; drag to another node's grip/edge; release on empty opens the palette to place-and-connect. |
| Ctrl+drag (wire) | Grasshopper | **Remove/redraw a wire** | Board | ✅ exists | Drag an existing wire end off its grip to disconnect; drop elsewhere to rewire (or drag a selected connector's endpoint dot). |
| Shift+drag (wire) | Grasshopper | **Add wire without erasing** | Board | ✅ exists | Multiple connectors per grip are always legal on a whiteboard; Shift keeps the *gesture grammar* parity so muscle memory transfers. |
| Ctrl+Shift+drag (wire) | Grasshopper | **Move all wires to another grip** | Board | ✅ exists | Grab every connector on a grip and re-anchor them in one journaled step. |
| Double-click (empty board) | Grasshopper | **Canvas palette** — search-and-place popup at the click point | Board | ✅ exists | Fuzzy search over registered commands + creatables; Enter executes/places at that canvas point. Atlas keeps double-click = zoom-to-point (✅ shipped). |
| Middle mouse click | Grasshopper | **Radial menu** — context commands around the cursor | Board | 🟡 adopt-P2 | Middle *click* (no drag) only; middle-drag stays pan. Fed by the registry; contents context-sensitive (selection vs empty canvas). |

---

## The non-obvious implications (why this is more than a keymap)

1. **Repeat/History/Palette/Radial ⇒ a real command registry.** Four
   requested features are impossible with static key handlers: Space/Enter
   repeat-last, F2 history, the double-click palette, and the radial menu
   all require commands as *values* — IDs with metadata (name, category,
   repeatability, availability) and a dispatcher. This registry is the
   Phase-4 MCP surface arriving early, per Art. VII. It lives in a new pure
   crate (`atlas-commands`, egui-free per Art. I) with `atlas-shell`
   providing the shared UI front-ends.
2. **Grasshopper wires ⇒ connectors in the scene model.** The wire grammar
   (plain/Shift/Ctrl/Ctrl+Shift drag) implies first-class **connector
   nodes**: endpoints anchored to `(node id, side, fraction)`, geometry
   recomputed when anchored nodes move, arrowheads/labels — all
   SVG-expressible (paths + markers), so within the Art. IV ceiling, and
   journaled as ordinary `SceneCmd`s. On a moodboard, wires are *semantic
   relations made visible* — and become machine-readable relations the
   context beacon can carry to agents (Art. VIII: the canvas is the prompt).
3. **Photoshop's brush keys ⇒ a shared color/brush state.** `B D X I [ ] , .`
   only make sense against a foreground/background color pair and brush
   presets. Presets are declarative user-space assets (Art. VII.3) — the
   first concrete instance of agent-extendable workspace data.
4. **Rhino's Esc ⇒ a formal cancel stack.** "Cancel" is a *stack*, not a
   key: operation → draft → mode → selection → chrome. Making the stack
   explicit in the registry ends the ad-hoc Escape cascades in both apps.
5. **Ortho, snap, grid ⇒ one constraint layer.** F8/F9/G/F7 and Shift-drag
   are one system: persistent toggles + one-shot inversions feeding every
   draw/move gesture, not per-tool special cases.
6. **Groups/lock/hide ⇒ selection-model maturity.** These three flags force
   the selection model to answer "what is selectable, what expands, what is
   skipped" in one place — a prerequisite for agent-driven selection later.

## Rejected / deferred summary

| Binding | Verdict | Reason |
|---------|---------|--------|
| Ctrl+RMB zoom (Rhino) | ⛔ | Conflicts with turbo pan — a shipped signature gesture. `Z` covers it. |
| Ctrl+T Trim | ⛔ | Ctrl+T = new tab; trimming beyond the 10% (Art. III). |
| Ctrl+W Zoom Window | ⛔ | Covered by `Z`-drag; chord reserved for close-tab. |
| F11 Attributes | ⛔ | F11 = fullscreen (shipped); content folds into F3 inspector. |
| F12 DigClick | ⛔ | No digitizer domain (Art. III). |
| Ctrl+P Print | ⛔ defer | Roadmap Phase 5 (print-faithful export) owns this. |
| Ctrl+S (Atlas) | ⛔ | Atlas has no document; the index persists itself. |
| Delete (Atlas) | ⛔ | Atlas never deletes user files — assign/export only. |
| Alt+scroll perspective pan | ⛔ | 3D-only; `.3dm` viewports already speak Rhino inside. |
