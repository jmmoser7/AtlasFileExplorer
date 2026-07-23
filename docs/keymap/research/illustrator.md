# Illustrator UX Research — Path Editing & Related Workflows

> **Stage-2 research input** for the Atlas/Slate keymap project.  
> Sources: Adobe Help (helpx.adobe.com), Adobe default shortcut tables, and practitioner documentation (Astute Graphics, Envato Tuts+, Laura Coyle, community forums). Researched July 2026; Illustrator desktop behavior as documented for recent CC versions.

This document captures **interaction semantics** Illustrator users muscle-memorize — not a feature parity checklist. Each section uses the same schema:

| Field | Meaning |
|-------|---------|
| **(a) Trigger** | How the action starts |
| **(b) Immediate feedback** | What changes on screen before commit |
| **(c) Modifiers** | Keys that alter behavior while active |
| **(d) Commit / cancel** | How the edit lands or aborts |
| **(e) Polish** | Subtle details pros rely on |
| **(f) Minimal Slate recommendation** | The 10% worth implementing excellently |

---

## 1. Direct Selection tool (A) — highest priority

### Role vs Selection tool (V)

| Tool | Shortcut | Selects | Typical edit |
|------|----------|---------|--------------|
| **Selection** (black arrow) | `V` | Whole objects, groups, compound paths | Move, scale, rotate entire shape |
| **Direct Selection** (white arrow) | `A` | Anchor points, path segments, handles | Reshape geometry |

With **Selection tool**: one click on a grouped object selects the **group**. Anchor points are hidden by default (optional preference can show them).

With **Direct Selection tool**: clicking a path shows **all anchors on that path**; clicking an anchor selects it. Clicking empty canvas deselects.

### Anchor point visuals (hollow vs filled)

On a selected path under Direct Selection:

- **Selected anchor(s)** → **filled** square (solid).
- **Unselected anchors on the same path** → **hollow** square (outline only).
- **Direction handles** (Bezier control points) appear only for **selected** anchor points on **curved** segments — not for corner points without curves.
- Hover labels (Smart Guides): cursor over anchor shows **“anchor”**, over segment **“path”**, over handle **“handle”** (when Smart Guides enabled).

Visibility prerequisites:

- **View → Show Edges** (`Ctrl+H` toggles hide/show edges). Without edges, anchor widgets may not paint even though geometry still moves with arrow keys.
- **Preferences → Selection & Anchor Display**: anchor/handle size (Default/Max), “Highlight Anchors on Mouseover”, “Show Handles When Multiple Anchors Are Selected”.

### Selecting anchors

| Action | Behavior |
|--------|----------|
| Click anchor | Select that anchor; deselects others on path unless Shift held |
| **Shift+click** anchor | Add/remove anchor from selection (same as subtract if already selected) |
| **Marquee drag** (empty area → drag box) | Select all anchors whose hit targets fall inside marquee |
| **Shift+marquee** | Add marquee hits to current anchor selection |
| **Lasso tool** (`Q`) | Freeform marquee for anchors; Shift adds |
| Click path segment (not anchor) | Selects segment; adjacent anchors may highlight depending on segment type |

Clicking a **segment** vs an **anchor** is intentional: segment click is the entry point for segment reshaping (below).

### Dragging anchors

| Action | Behavior |
|--------|----------|
| Drag selected anchor | Moves anchor; connected segments reshape live |
| **Shift+drag** anchor | Constrain movement to **45°** increments |
| Arrow keys | Nudge selected anchor(s) by document nudge preference |
| Drag with multiple anchors selected | All selected anchors move together |

Dragging an endpoint of an **open path** moves only that end; interior anchors move their local neighborhood.

### Dragging segments (segment reshape)

With Direct Selection, **click then drag a path segment** (straight or curved):

- **Straight segment**: entire segment translates (both endpoints move together).
- **Curved segment**: default reshape adjusts the curve; behavior depends on **Preferences → Selection & Anchor Display → “Constrain Path Dragging on Segment Reshape”** (on by default in long-standing Illustrator versions):
  - **ON**: direction handles keep their **angle** while segment moves — pros use this to reposition a curve without “spinning” handles.
  - **OFF**: dragging can introduce/rotate handles more freely (useful when converting straight → curved by dragging).
- **Shift+drag** segment or anchor: constrain to **45°**.
- **Shift** while reshaping a curve (handle editing mode): constrains handles **perpendicular** to the segment and can enforce **equal handle length** — used to form semicircular arcs.

Advanced: **Reshape tool** (under Scale tool) adds a **focal point** model — click a focal anchor/segment, Shift+click additional focal points, then drag; unselected points move proportionally by distance from focal point. Rare in daily work but important for organic path editing.

Pen-tool overlap: while drawing with Pen (`P`), **Ctrl** (see §2) temporarily gives Direct Selection to tweak the last placed point without leaving Pen mode.

### Handle (direction line) editing

| Action | Behavior |
|--------|----------|
| Select curved anchor | Direction lines + square handle endpoints appear |
| Drag handle endpoint | Adjusts curvature on that side of the anchor |
| **Alt+drag** handle (`Option` on Mac) | **Break symmetry**: move one handle independently (corner-like control on one side) |
| **Shift+drag** handle | Constrain handle angle to 45°; in some reshape contexts, constrain perpendicular/equal length |

Handles are **not shown** for multiple selected anchors unless preference “Show Handles When Multiple Anchors Are Selected” is checked — a common source of “where did my handles go?” confusion.

### Groups and nested content

Direct Selection on a **group**:

- First click often selects **one object inside** the group (deepest clickable path under cursor) rather than the group bounding box.
- To edit a path inside a group without isolation: click through with Direct Selection, or use **Group Selection tool** (§2), or enter **Isolation mode** (§7).

Double-click with **Selection tool** enters isolation (§7) — not Direct Selection’s primary pattern, though Direct Selection offers **“Isolate Selected Object”** in the Control panel when a nested path is selected.

### Schema summary

| | |
|---|---|
| **(a) Trigger** | `A` → click/marquee anchors or segments |
| **(b) Feedback** | Hollow/filled squares; handles on selected curved anchors; live path preview while dragging |
| **(c) Modifiers** | Shift add/subtract selection & 45° constrain; Alt independent handle drag; Lasso/Q for freeform anchor pick |
| **(d) Commit / cancel** | Mouse up commits; `Esc` does not undo drag (standard undo only); click empty space deselects |
| **(e) Polish** | Smart Guide labels; constrain-segment-drag preference; show-handles-for-multi-select preference; edge visibility |
| **(f) Minimal Slate recommendation** | **Ship this first and ship it completely.** Required: A tool, hollow/filled anchor states, segment drag with constrained handle directions (default ON), Shift multi-select + 45° constrain, Alt break handle symmetry, marquee anchor select, visible handles on single-anchor selection. Optional v2: Lasso, Reshape focal mode, preference toggles. Match SVG-ceiling stroke/fill preview while editing. |

---

## 2. Selection vs Direct Selection interplay

### Dedicated shortcuts

| Shortcut | Action |
|----------|--------|
| `V` | Selection tool |
| `A` | Direct Selection tool |
| `Ctrl+\`` (`Cmd+\`` Mac) | **Switch to last-used selection tool** among Selection, Direct Selection, Group Selection (explicit toggle — stays on that tool after release) |
| **Alt** held (`Option`) | While Direct or Group Selection active: **toggle between Direct Selection ↔ Group Selection** |
| **Ctrl** held (`Cmd`) | **Temporary selection overlay** while another tool is active (see below) |

### Ctrl-hold temporary selection (critical muscle memory)

Documented Illustrator/InDesign family behavior (practitioner sources; Adobe shortcut table lists related `Ctrl+\``):

- While using **most tools** (Scale, Rotate, Pen-adjacent workflows, etc.), **holding Ctrl** temporarily activates the **last-used selection tool** (Selection or Direct Selection).
- **Pen, Pencil, Scissors** are special: Ctrl-hold defaults to **Direct Selection** (not last-used black arrow) — pros nudge points mid-draw without switching tools.
- While **already on Selection or Direct Selection**, holding **Ctrl** temporarily switches to the **other** arrow (black ↔ white) for the duration of the hold (Illustrator; not InDesign).
- Release Ctrl → revert to previous tool and prior selection state (selection changes made while held typically persist).

**Caveat:** Toggling black ↔ white arrow **during Pen + Ctrl-hold** via `Ctrl+Tab` / `Cmd+Option+Tab` has regressed in some CC builds (Adobe community reports). Do not depend on a single OS-level chord across platforms.

### Group Selection tool (hidden under Direct Selection)

| Click pattern | Result |
|---------------|--------|
| 1st click on object in group | Select that **child object** |
| 2nd click same location | Select **parent group** containing it |
| 3rd click | Select **next group up** the hierarchy |
| … | Walks up nested groups |

Alternative to isolation for “select this country, now this continent” map-style hierarchies.

### Group vs path selection semantics

| Intent | Tool |
|--------|------|
| Move whole logo group | `V` |
| Move one path inside group without isolating | Group Selection or Direct Selection + careful clicking |
| Edit anchors | `A` (often after isolate or deep click) |
| Enter group editing context | Double-click group with `V` (§7) |

### Schema summary

| | |
|---|---|
| **(a) Trigger** | `V`/`A` permanent; Ctrl-hold overlay; Alt-hold Direct↔Group; Group Selection multi-click |
| **(b) Feedback** | Cursor changes to arrow; bounding box vs anchor view; group selection adds objects to selection set with each click |
| **(c) Modifiers** | Ctrl, Alt, Shift for selection ops; `Ctrl+\`` explicit last-tool switch |
| **(d) Commit / cancel** | Release modifier restores prior tool; selection changes stick |
| **(e) Polish** | Pen→Direct default on Ctrl; last-used tool memory; Group Selection avoids isolation round-trip |
| **(f) Minimal Slate recommendation** | Implement **Ctrl-hold temporary Direct Selection** from Pen/path tools as P0. Implement **Ctrl-hold toggle V↔A** when a selection tool is already active. Group Selection can wait if **isolation + Outliner** exist; expose `Ctrl+\`` as explicit “cycle selection mode”. Document Pen special-case in keymap reference. |

---

## 3. Scale tool (S)

### Interaction model

1. Select artwork (whole object, group, or **partial path selection** — Scale tool works on partial selections; bounding box alone does not).
2. Press `S` (Scale tool).
3. **Click once** on artboard → sets **transformation origin** (reference point marker appears at click).
4. **Click-drag anywhere** (not necessarily on the object) → scale relative to that origin. Distance from origin to cursor drives scale factor.
5. Repeat: each origin click re-anchors future drags.

**Alt+click** (`Option+click`): sets origin **and** opens **Scale dialog** (numeric uniform/non-uniform %, preview, copy checkbox). Dialog remembers last uniform setting; focus in first field for keyboard entry.

**Double-click Scale tool** in toolbar: opens Scale dialog with origin at **center of selection** (no click-to-set-origin step).

### Modifiers during drag

| Modifier | Effect |
|----------|--------|
| **Shift+drag** | **Uniform** scaling (lock aspect ratio) |
| **Alt+drag** | Scale from origin **and duplicate** (transform copy) |
| **`\`+drag** (backtick) | Transform **pattern fill** independently of object (Illustrator-specific) |

Transform panel / Control bar can also scale, but **cannot** arbitrarily set an off-canvas origin the way click-to-set-origin can.

### Scale tool vs bounding box (Selection tool)

| Capability | Bounding box | Scale tool (`S`) |
|------------|--------------|------------------|
| Uniform scale | Shift+drag corner | Shift+drag |
| Scale from center | Alt+drag corner | Click center as origin, then drag |
| **Arbitrary origin** | Opposite corner or center only | **Any click point** |
| Start drag anywhere | Must grab handle | Yes — drag from any screen location |
| Partial path / open paths | No bbox | **Yes** |
| Numeric scale from custom origin | Awkward | **Alt+click origin → dialog** |

Pros reach for Scale tool when:

- Matching sizes between **two arbitrary points** on different objects (measure-scale workflow).
- Scaling from a **corner that is not the bbox corner** (e.g., align two objects at a shared pivot).
- Scaling **selected anchors only** without affecting the rest of the path.
- **Isometric/perspective** construction where origin must sit on a construction point off-object.
- Repeatable **percent scaling** from a fixed origin (`Alt+click`, type %, Enter, then `Ctrl+D` repeat).

For everyday “make this icon bigger”, bbox handles on `V` suffice — Scale tool is the precision instrument.

### Schema summary

| | |
|---|---|
| **(a) Trigger** | `S` → click origin → drag; or Alt+click for dialog; double-click tool icon for centered dialog |
| **(b) Feedback** | Origin crosshair/marker; live scaled preview; dialog with % fields |
| **(c) Modifiers** | Shift uniform; Alt duplicate; `\` pattern-only |
| **(d) Commit / cancel** | Mouse up or Enter in dialog commits; Esc cancels dialog |
| **(e) Polish** | Scale strokes/effects toggles (Transform panel effect options); origin survives until moved; works off-screen after zoom |
| **(f) Minimal Slate recommendation** | **Defer dedicated Scale tool v1** if bbox + Transform inspector support % scale and center/corner origins. **Do implement** click-to-set-origin scaling before calling parity “good enough” — it is the main reason pros open `S`. Minimal: `S` → click origin → drag with Shift uniform; Alt+click numeric dialog. Skip pattern-only `\` drag until patterns exist. |

---

## 4. Graphic Styles panel (Shift+F5)

### What a graphic style captures

Graphic styles store **appearance attributes only** — not geometry:

- Fills, strokes (color, weight, dash, cap/join where applicable)
- Effects stack (drop shadow, blur, etc. — Illustrator-specific)
- Transparency / blend mode
- **Not** path shape, size, position, or live shape parameters

Styles can be applied to objects, **groups**, or **layers** (children inherit appearance).

### Workflow

| Action | Method |
|--------|--------|
| Open panel | **Window → Graphic Styles** or **Shift+F5** |
| Create style from selection | **New Graphic Style** button, panel menu, or drag object into panel |
| Apply style | Select object → click style swatch |
| Apply without removing existing appearance | **Alt+click** style (additive — Illustrator-specific) |
| Break link | **Break Link to Graphic Style** — edits no longer update the style definition |
| Update all instances | Edit one linked instance → **Redefine Graphic Style** (Appearance panel menu) or Option-drag appearance onto style swatch |
| Libraries | **Open Graphic Style Library** / save custom libraries; shipped presets (Text Styles, etc.) |

Deleting a style: applied objects **keep** current look but lose the link.

### Schema summary

| | |
|---|---|
| **(a) Trigger** | Shift+F5 panel; click swatch to apply; New from selection |
| **(b) Feedback** | Thumbnail swatches; Appearance panel title shows linked style name |
| **(c) Modifiers** | Alt+click additive apply |
| **(d) Commit / cancel** | Click applies immediately; undo reverses |
| **(e) Polish** | Redefine updates all linked instances; merge styles; preview modes (square vs text) |
| **(f) Minimal Slate recommendation** | Given SVG-ceiling + board scene model, implement **named appearance presets** (stroke/fill/opacity stack per node style) rather than a full Graphic Styles panel. Minimal: save current appearance → name → apply to selection; optional **weak link** (“update preset from selection”). Skip layer-level style inheritance and effect stacks until effects exist. Map to board `SceneCmd` style patches for journal parity. |

---

## 5. Attributes panel (F11 / Ctrl+F11)

Adobe’s current default shortcut: **Ctrl+F11** (Windows) / **Cmd+F11** (Mac). Older references list **F11** alone.

### Contents (what matters for a properties inspector merge)

| Control | Purpose | Slate relevance |
|---------|---------|-------------------|
| **Overprint Fill / Overprint Stroke** | Print separations: ink prints atop without knockout | Low (print export) unless targeting print PDF |
| **Overprint Black** (related dialog) | Bulk overprint rules for blacks | Low |
| **Nonprinting** (when shown) | Object omitted from print output | Medium for guide layers |
| **Note** | Arbitrary metadata string on object | Medium — useful for agent/board annotations |
| **Image Map** (Show All) | Rectangle/polygon hotspot + **URL** for PDF/export hyperlinks | Low for canvas app; medium if HTML artifact export needs links |
| **Central display** | Shows mixed state (**dash** in checkbox) when selection disagrees or fill/stroke absent | UX pattern worth copying |

Attributes panel does **not** host stroke alignment, width, cap/join — those live in **Stroke panel** / **Properties panel**.

### Schema summary

| | |
|---|---|
| **(a) Trigger** | Ctrl+F11; object selected |
| **(b) Feedback** | Checkboxes with checked/unchecked/mixed dash states |
| **(c) Modifiers** | — |
| **(d) Commit / cancel** | Toggle applies immediately |
| **(e) Polish** | Mixed-state dashes for ambiguous fill/stroke-less objects; panel menu “Show All” reveals URL/image map |
| **(f) Minimal Slate recommendation** | Fold into **Properties inspector**: stroke/fill/opacity (from SVG ceiling), **nonprinting/guide flag**, optional **note/URL metadata** on nodes. Skip overprint until print pipeline exists. Show mixed-state UI when multi-select disagrees. |

---

## 6. Join (Ctrl+J) and path closure

### Join (`Ctrl+J` / `Cmd+J`)

**Requires:** exactly **two** endpoints selected (Direct Selection) **or** two paths with endpoints eligible for connection.

| Selection | Result |
|-----------|--------|
| **Two endpoints** (same or different paths) | Creates connecting segment; if coincident → **one merged anchor** |
| **Two endpoints, not coincident** | **Straight segment** drawn between them (paths remain separate anchors at endpoints until averaged) |
| **One open path, both endpoints selected** | **Closes** the path |
| **Two open paths selected** (object-level) | Join connects **nearest** endpoints; second Join may close path |

Join creates **corner** points by default; smooth joins need extra steps (below).

**No user-facing tolerance preference** for Join — endpoints must be selected explicitly. “Tolerance” in practice = use **Average** first.

### Average (`Alt+Ctrl+J` / `Option+Cmd+J`)

Moves selected anchors to their **mutual average** position:

- Dialog: **Horizontal**, **Vertical**, or **Both** axes.
- Used before Join to merge two endpoints into one physical point.

### Average + Join with corner/smooth choice

**Shift+Alt+Ctrl+J** (`Shift+Option+Cmd+J`): runs Average (both axes) then Join, then prompts **Corner** vs **Smooth** point — the production shortcut for technical drawing cleanup.

Plain **Ctrl+J** after manual Average: join without dialog (corner).

### Closing paths

| Method | Behavior |
|--------|----------|
| Select both endpoints of one open path → Join | Closes path |
| Pen click first point | Closes with Pen semantics |
| Join on two-path selection twice | May close after first bridge segment |

After join, convert to smooth with **Anchor Point tool** (`Shift+C`) or Properties/Control bar convert buttons if handles needed.

### Context menu

Right-click with two endpoints selected: **Join** and **Average** appear (platform-dependent).

### Schema summary

| | |
|---|---|
| **(a) Trigger** | Select 2 endpoints → `Ctrl+J`; Average `Alt+Ctrl+J`; combined `Shift+Alt+Ctrl+J` |
| **(b) Feedback** | New segment preview implicit on commit; dialog for corner/smooth on combined shortcut |
| **(c) Modifiers** | — |
| **(d) Commit / cancel** | Immediate; undo restores; corner/smooth dialog Enter/Esc |
| **(e) Polish** | Join order matters on multi-path selections; join is corner-first; Average Both before join prevents double-stacked points |
| **(f) Minimal Slate recommendation** | P0: **Join** (`Ctrl+J`) for two selected open endpoints + close single open path. P1: **Average** (`Alt+Ctrl+J`) with Both/Horizontal/Vertical. P1: **Average+Join** chord with corner/smooth prompt. Enforce journaled `SceneCmd`. No tolerance slider needed — snap-to-point (1–8 px preference) covers “close enough”. |

---

## 7. Isolation mode and sub-object selection

### Entering isolation

| Method | Entry |
|--------|-------|
| **Double-click** group/object with **Selection tool (`V`)** | Primary muscle-memory entry |
| **Direct Selection** → **Isolate Selected Object** (Control panel) | When nested path already selected |
| Control panel **Isolate** button | Same |
| Right-click → **Isolate Selected Group** | Context menu |
| Layers panel → **Enter Isolation Mode** | Layer/sublayer isolation |

Each double-click on a nested group **descends one level**. Breadcrumb bar appears at top of document window showing hierarchy (e.g., `Layer 1 ← Group ← Path`).

### Isolation environment

- Isolated content: **full color**, fully interactive.
- Everything else: **dimmed** and **locked** (cannot select/edit).
- New objects created in isolation are scoped to the isolated container and align to original coordinate space on exit.

### Exiting isolation

| Method | Exit depth |
|--------|------------|
| **Esc** | One level up |
| **Double-click empty artboard** | Fully out (preferred by many pros) |
| Breadcrumb **back arrow** | One level per click |
| Control panel / Layers **Exit Isolation Mode** | One level |

Deep nesting: multiple Esc or breadcrumb clicks required.

### Alternatives in other apps (for keymap awareness)

| App | Pattern |
|-----|---------|
| Illustrator | Double-click isolate; Group Selection multi-click; Ctrl-hold selection overlay |
| Figma | **Ctrl+click** (Cmd+click) deep select; Enter isolation on frames |
| Affinity Designer | **Ctrl+click** select inside group; no classic isolation bar |
| Inkscape | **Ctrl+click** descend groups; **Shift+Ctrl+click** raise selection |

Slate should pick **one primary** deep-edit metaphor and document alternates in Advanced → Commands.

### Schema summary

| | |
|---|---|
| **(a) Trigger** | Double-click with V; or Isolate button |
| **(b) Feedback** | Dimmed backdrop; breadcrumb bar; isolated layer highlight in layer list |
| **(c) Modifiers** | — |
| **(d) Commit / cancel** | Esc/back arrow exits; edits persist |
| **(e) Polish** | Edits stay aligned to pre-isolation coordinates; symbol edit mode uses same isolation chrome |
| **(f) Minimal Slate recommendation** | Implement **isolation mode** for board groups/frames: double-click enter, Esc exit, breadcrumb trail, dim non-isolated ink. Pair with **Outliner/layer list** click-to-isolate. Optional: Group Selection click-through as power-user alternative. Do not require ungrouping. |

---

## 8. Anchor point conventions (corner vs smooth)

### Point types

| Type | Handles | Segment behavior |
|------|---------|------------------|
| **Corner** | None, or **independent** directions/lengths per side | Sharp angle; straight segments meet at angle |
| **Smooth** | **Collinear** handles (180°); lengths may differ | Continuous tangent across anchor |

Illustrator also uses **smooth points with unequal handle lengths** (still tangent-aligned).

### Converting

| Tool | Shortcut | Action |
|------|----------|--------|
| **Convert Anchor Point** | **Shift+C** | Click corner → add symmetric handles; drag corner → smooth; click smooth without drag → **remove handles** (corner) |
| **Direct Selection** | `A` | Alt+drag one handle → break symmetry |
| Control bar / Properties | Buttons | Convert selected anchors to corner / smooth |

**Convert smooth → corner (retain independent handles):** drag one handle independently (or Alt+drag).

**Convert corner → smooth:** Convert tool drag from anchor to expose handles, then adjust.

### Alt / Option modifier (handle break)

**Alt+drag handle** with Direct Selection or Convert tool: only the dragged handle moves; opposite handle stays fixed — essential for drawing asymmetric curves without converting entire point to cusp through menu.

**Alt+click** anchor (Convert tool): can remove handles / convert to corner depending on context.

Pen tool note: Alt temporarily invokes Convert Anchor behavior on **existing** points mid-path; starting-point behavior differs (Pen continuation semantics).

### Schema summary

| | |
|---|---|
| **(a) Trigger** | Shift+C convert; A + drag handles; Alt+drag to break |
| **(b) Feedback** | Handles appear/vanish; live curve preview |
| **(c) Modifiers** | Alt independent handle; Shift 45° on drag |
| **(d) Commit / cancel** | Mouse up commits |
| **(e) Polish** | Join → corner first; smooth join via Shift+Alt+Ctrl+J; convert buttons for multi-select |
| **(f) Minimal Slate recommendation** | P0: corner vs smooth in path model; **Alt+drag** break symmetry; **Shift+C** or inspector toggle convert. P1: click-to-remove-handles. Journal all as invertible path edits. Match SVG cubic/quadratic handle semantics. |

---

## Cross-cutting preferences (path UX)

From **Preferences → Selection & Anchor Display** — worth exposing in Slate Advanced settings:

| Preference | Default (Illustrator tradition) | Impact |
|------------|----------------------------------|--------|
| Constrain Path Dragging on Segment Reshape | On | Segment drag keeps handle angles |
| Show Handles When Multiple Anchors Are Selected | Off | Multi-anchor edit hides handles unless enabled |
| Snap to Point | On (1–8 px radius) | Anchor snapping during drag |
| Object Selection by Path Only | Off | Click fill does not select |
| Highlight Anchors on Mouseover | Varies | Hover affordance |
| Show Anchor Points in Selection Tool | Off | V tool shows anchors when enabled |

---

## Priority matrix for Slate (keyboard-first vector canvas)

| Priority | Feature cluster | Rationale |
|----------|-----------------|------------|
| **P0** | Direct Selection complete semantics | Core board authoring |
| **P0** | Ctrl-hold temporary selection from Pen/path tools | Pen workflow throughput |
| **P0** | Join + Average + close path | Path cleanup |
| **P0** | Corner/smooth + Alt break handles | Curve authoring |
| **P0** | Isolation mode for groups | Nested board scenes |
| **P1** | Click-to-set-origin scale (`S`) | Precision layout (when bbox insufficient) |
| **P1** | Named appearance presets (graphic-style subset) | Reuse stroke/fill across nodes |
| **P1** | Properties inspector (Attributes merge) | notes, guide/nonprinting |
| **P2** | Group Selection click-through | Power users |
| **P2** | Reshape focal tool | Organic illustration |
| **Defer** | Overprint, image maps, effect stacks, pattern-only scale | Outside SVG/board 10% |

---

## Source index

- [Adobe — Select anchor points](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/select-anchor-points-in-paths.html)
- [Adobe — Convert anchor points](https://helpx.adobe.com/illustrator/desktop/draw-shapes-and-paths/modify-paths/convert-anchor-points-on-a-path.html)
- [Adobe — Adjust path segments](https://helpx.adobe.com/illustrator/using/adjust-path-segments.html)
- [Adobe — Isolate objects](https://helpx.adobe.com/illustrator/desktop/manage-objects/select-objects/isolate-objects.html)
- [Adobe — Graphic Styles panel](https://helpx.adobe.com/illustrator/desktop/special-effects-styles/apply-graphic-styles/graphic-styles-panel-overview.html)
- [Adobe — Default keyboard shortcuts](https://helpx.adobe.com/illustrator/using/default-keyboard-shortcuts.html)
- [Adobe — Overprinting / Attributes](https://helpx.adobe.com/illustrator/using/overprinting.html)
- [Adobe — Slices and image maps (URL fields)](https://helpx.adobe.com/illustrator/using/slices-image-maps.html)
- Practitioner: Astute Graphics selection guide; Envato Tuts+ Pen/appearance guides; Laura Coyle join paths; Graphic Design Stack Exchange scale-tool vs bbox threads
