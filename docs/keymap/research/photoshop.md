# Photoshop UX Research — Stage 2 Keymap Input

> **Purpose:** Stage-2 research input for the Atlas/Slate keymap project. Documents Adobe Photoshop’s actual interaction behavior (plus vector-app eraser comparisons) so we can adapt the *best* parts to a **vector-stroke infinite canvas** — not a raster clone.

**Sources:** Adobe Help (`helpx.adobe.com`), Julieanne Kost’s Photoshop shortcut references, Adobe Community threads, Smashing Magazine / Photoshop Essentials brush guides, and vector-tool behavior from Illustrator, Figma, and Excalidraw issue threads (2024–2026).

---

## How to read each section

Every feature is broken into:

| Field | Meaning |
|-------|---------|
| **(a) Trigger** | What activates the behavior |
| **(b) Immediate feedback** | What the user sees/hears instantly |
| **(c) Modifiers** | Keys that change behavior while held or toggled |
| **(d) Commit / cancel** | How the operation finishes or aborts |
| **(e) Subtle polish** | Edge cases, defaults, and “muscle memory” details |
| **(f) Vector-stroke recommendation** | Adaptation for Slate’s infinite canvas + variable-width vector ink |

---

## 1. Brush tool (B)

### (a) Trigger

- **B** selects the Brush tool (or the last-used tool in the brush *family* if Shift-cycling is enabled).
- Click-drag paints; tablet pressure maps to size and/or opacity when enabled in the Options bar.

### (b) Immediate feedback

- **Cursor preview:** By default, a circle outline shows brush diameter. Preferences → Cursors offers:
  - **Normal Brush Tip** — circle ≈ pixels with **>50%** effect (soft brushes look smaller than full diameter).
  - **Full Size Brush Tip** — circle ≈ **100%** affected area.
  - **Precise** — crosshair only; **Caps Lock** temporarily forces precise crosshair even in brush-tip mode.
  - Optional **crosshair inside the circle** for centering.
- **Options bar** updates live: Size, Hardness (round brushes), Opacity, Flow, Smoothing %.
- **HUD (Heads-Up Display):** Alt+right-click drag (Win) / Ctrl+Option+drag (Mac) shows a red preview ring with live Diameter and Hardness values.

### (c) Modifiers

| Input | Effect |
|-------|--------|
| **[** / **]** | Decrease / increase brush **size** (see stepping table below) |
| **Shift+[** / **Shift+]** | Decrease / increase **hardness** in **25%** steps (round brushes only) |
| **,** / **.** (also **<** / **>**) | Previous / next brush **preset** in the preset list |
| **Shift+,** / **Shift+.** | Jump to **first** / **last** preset |
| **1–9** | Set **opacity** to 10%–90%; **0** = 100% |
| **Two digits quickly** | Exact opacity (e.g. **5** then **4** → 54%) |
| **Shift + digit** | Sets **Flow** instead of opacity |
| **Airbrush on** | Digit keys control Flow by default; **Shift+digit** controls Opacity |
| **Alt** (Win) / **Option** (Mac) | Spring-loaded **Eyedropper** — samples **foreground** color (see §3) |
| **Shift + click** (two clicks) | Paints a **straight line** between click points |
| **Alt+right-drag vertical** (HUD) | Adjust hardness continuously |
| **Alt+right-drag horizontal** (HUD) | Adjust size continuously (1 px granularity) |

**Bracket size stepping (documented tiers — not user-configurable):**

| Current diameter | Step per `[` / `]` press |
|------------------|--------------------------|
| 1–10 px | **1 px** |
| >10–~50 px | **5 px** |
| ~50–100 px | **10 px** |
| ~100–200 px | **25 px** |
| Larger | Coarser jumps (Adobe does not publish a full table; users report increasing steps) |

**Smoothing (Options bar, 0–100%, default ~10%):**

- Reduces hand jitter; higher values add lag to stroke preview.
- Gear menu modes: **Pulled String** (stroke starts only after cursor leaves a radius), **Stroke Catch-up**, **Catch-up on Stroke End**, **Adjust for Zoom**.
- Optional **brush leash** line while smoothing (color configurable in Preferences → Cursors).

### (d) Commit / cancel

- Stroke **commits on pointer release** (each dab is a paint application).
- **Edit → Fade Brush Tool** (immediately after stroke) can reduce opacity/blend — ephemeral, not a mode.
- No explicit “cancel stroke in progress”; Esc does not undo current drag.

### (e) Subtle polish

- Holding `[` / `]` **continuously** repeats stepping.
- **Shift+straight-line** connects *every* new Shift+click to the **previous paint point** while Shift stays down — a famous pain point; breaking the chain requires releasing Shift, placing a new anchor click, then re-engaging Shift.
- Right-click / Ctrl-click in canvas opens **Brush Preset Picker** without leaving the tool.
- **Tilda (~)** toggles paint ↔ erase-with-current-brush (Clear blend mode) — obscure but loved by mask painters.
- Brush preview HUD color is cosmetic (default red) — not the paint color.

### (f) Vector-stroke recommendation

| Photoshop habit | Slate adaptation |
|-----------------|------------------|
| Size circle cursor | Show **world-space** width circle from stroke profile + current zoom; outline the **feathered mesh extent**, not just centerline width. |
| Hardness | Map to **profile softness / pressure curve toe** — vector equivalent of edge falloff, not a raster tip. |
| `[` / `]` stepping | Keep **tiered steps in screen pixels** (convert to world units via zoom) so bracket keys feel identical at any magnification. Offer HUD drag for 1-unit fine control. |
| Smoothing | Port **smoothing % + optional pulled-string** to the stroke fitter (pre-mesh); show a faint **leash** during drag. Default ~10%. |
| Shift+click lines | Provide **polyline mode** (click corners) *and* Photoshop-compatible Shift+connect; add a pref to **disable auto-connect** (requested in PS for years). |
| Opacity / Flow digits | Map to **stroke opacity** and **ink deposition rate** (multi-pass alpha within one stroke for flow-like buildup). |
| Preset cycling `,` `.` | Cycle **stroke presets** (width profile + opacity + blend), not raster brush tips. |

---

## 2. Eraser tool (E) — raster vs vector

### 2A. Photoshop Eraser (E)

#### (a) Trigger

- **E** selects Eraser (Shift+E cycles eraser *family* tools in some versions).
- Click-drag erases pixels under the brush footprint.

#### (b) Immediate feedback

- Same brush cursor preview as Brush tool.
- **Checkerboard** appears where transparency is created (unlocked layers).
- On **locked Background** layer, erased areas fill with **background color**, not transparency.

#### (c) Modifiers

| Mode (Options bar) | Behavior |
|--------------------|----------|
| **Brush** | Soft/hard round eraser; Size, Hardness, Opacity, Flow |
| **Pencil** | Hard-edged eraser (100% hardness) |
| **Block** | Fixed **square** tip; erases a constant **screen-area** chunk regardless of zoom — unique PS quirk |

- **Opacity / Flow** number keys work like Brush.
- **Alt + Eraser** (with history state configured): paints back from a **History** snapshot (“Erase to History”).

#### (d) Commit / cancel

- Each stroke commits on release; history-based erase requires pre-selected history state.

#### (e) Subtle polish

- Block mode feels like a **screen-space stamp**, not world-space — confusing on zoom.
- Eraser respects layer transparency; does not erase vector paths (there are none in PS).

#### (f) Vector-stroke recommendation

Do **not** clone pixel erasure. Prefer **Illustrator-style path splitting** (see 2B) adapted to variable-width meshes:

- **Segment eraser:** Drag across a stroke → **split** at crossings, delete the overlapped segment(s), keep remaining paths as editable strokes.
- **Whole-stroke eraser (Excalidraw mode):** Optional modifier (e.g. **Alt**) or small-stroke heuristic → delete entire stroke object when click target is ambiguous.
- Reuse brush **size cursor** and `[` / `]` for eraser radius (world units).
- **Opacity** → strength of delete (partial erase = opacity mask on mesh, journaled as geometry trim at 100%).
- Avoid Block mode; it has no vector meaning.

### 2B. Vector apps — how they erase stroked paths

| App | Behavior | Split vs delete |
|-----|----------|-----------------|
| **Illustrator — Eraser** | Drag across filled paths/shapes; object divided along eraser path | **Splits** into new paths; Shift constrains eraser to straight cuts |
| **Illustrator — Path Eraser** | Drag along **open path** | Removes **segment** between crossings; leaves remaining path(s) |
| **Figma** | No freehand eraser on canvas; **Cut tool (X)** in vector edit mode splits paths | Explicit **split**; boolean subtract for shapes |
| **Excalidraw** | Eraser removes **entire connected stroke** (one object) | **Delete whole stroke** — widely reported UX frustration; partial erase requested/open PR |

**Edge cases (Illustrator):**

- Eraser on compound paths / Live Paint can produce unexpected topology.
- Path Eraser sensitivity changed in recent versions (community reports “eraser eats too much”).
- Shift+eraser: straight-line cuts (45°/90° constraints reported by users).

**Recommendation for Slate:** Default to **Illustrator-like segment split** on variable-width strokes (split centerline + regenerate mesh). Offer **whole-stroke delete** via **Alt+eraser** or tap without drag. Journal splits as invertible path commands, not silent mesh hacks.

---

## 3. Eyedropper (I)

### (a) Trigger

- **I** selects Eyedropper.
- **Alt** (from Brush, Pencil, Gradient, Bucket, etc.) → temporary Eyedropper (**spring-loaded**).

### (b) Immediate feedback

- **Sampling ring** (optional, Options bar “Show Sampling Ring”): annulus shows **new sample color** vs **color being replaced** (fg or bg).
- Toolbar fg/bg chips update immediately.
- Info panel can show live values under cursor.

### (c) Modifiers

| Input | Effect |
|-------|--------|
| **Click** | Sample → **foreground** |
| **Alt+click** (Eyedropper active) | Sample → **background** |
| **Alt** (from Brush) | Sample → **foreground** only (not bg) |
| **Sample size** (Options bar) | Point, 3×3, 5×5, 11×11, 31×31, 51×51, 101×101 average |
| **Sample All Layers** | Composite vs active layer only |

### (d) Commit / cancel

- Single click commits sample; releasing Alt returns to previous tool (spring-loaded).

### (e) Subtle polish

- After clicking inside a document, cursor can **drag outside Photoshop windows** to sample anywhere on screen.
- Wrong chip active in **Color panel** (bg selected) causes “Alt picks bg while painting” confusion — Adobe documents this explicitly.
- **Shift+click** with Eyedropper adds a **Color Sampler** point (persistent readout), not a paint color.

### (f) Vector-stroke recommendation

- Sample from **rendered canvas composite** (respect layer visibility); optional **selected objects only**.
- Spring-loaded **Alt** from ink tools is **mandatory** for painting UX.
- Sampling ring maps well to **fg chip preview** on infinite canvas — show ring at cursor in world space.
- For variable-width strokes, default sample size **3×3 or 5×5 screen px** to avoid picking only a feather edge.
- Store samples as **document color swatches** + fg/bg model (§4).

---

## 4. Default colors — D and swap X

### (a) Trigger

- **D** — reset colors.
- **X** — exchange fg ↔ bg.

### (b) Immediate feedback

- Toolbar chips flip instantly.
- **D:** fg = **black**, bg = **white** (default).
- **D** with **layer mask** active: fg = **white**, bg = **black** (mask painting convention).

### (c) Modifiers

- None on D/X themselves.
- Related: **Alt+Backspace** fill with fg; **Ctrl+Backspace** fill with bg (Windows).

### (d) Commit / cancel

- Immediate; no dialog.

### (e) Subtle polish

- **Painting tools** use **foreground** by default.
- **Background** used by: gradient secondary stop, some fills, eraser-on-locked-layer, filter backgrounds.
- **X** is the fastest color toggle during line art (ink ↔ paper).

### (f) Vector-stroke recommendation

- Keep **fg/bg** model for board ink, fills, and eraser-to-paper-color on locked backgrounds.
- **D** resets to theme-aware defaults (e.g. black/white in light mode, white/black optional in dark).
- **X** swaps stroke color vs canvas background for quick inversion workflows.
- Show chips in **shared chrome** (atlas-shell), not per-app.

---

## 5. Crop tool (C)

### (a) Trigger

- **C** selects Crop.
- Drag on canvas creates crop marquee (or auto-matches active selection bounds).

### (b) Immediate feedback

- **Shaded mask** outside crop box; **Rule of Thirds** grid inside (default overlay).
- Handles at corners and edges; Options bar shows W×H, resolution, “Delete Cropped Pixels”.

### (c) Modifiers

| Input | Effect |
|-------|--------|
| **O** | Cycle **overlay** (Rule of Thirds, Grid, Golden Ratio, etc.) |
| **Shift+O** | Cycle overlay **orientation** |
| **H** or **/** | Toggle visibility of **shaded area outside** crop |
| **Drag outside corner** | **Rotate** image/crop box (not just resize) |
| **Ctrl/Cmd** (while crop active) | Access **Straighten** tool (draw horizon line) |
| **Shift+drag handle** | Constrain aspect (when unlocked) |
| **Alt+drag** | Resize from center (standard transform convention) |

### (d) Commit / cancel

| Action | Result |
|--------|--------|
| **Enter / Return** | Apply crop |
| **Double-click** inside marquee | Apply |
| **Esc** | Cancel (revert marquee) |
| **Checkmark / Cancel** in Options bar | Apply / cancel |
| Click **another tool** | Applies crop (default) |

### (e) Subtle polish

- Active selection → crop marquee **matches selection**; Esc resets to full canvas.
- “Delete Cropped Pixels” is **destructive**; Adobe recommends leaving unchecked for reversible crops.
- Right-click inside marquee → context menu (Reset Crop, aspect ratios, etc.).

### (f) Vector-stroke recommendation

On an infinite canvas, “crop” maps best to **export frame** or **presentation slide bounds**, not destroying content:

- **C** defines a **Frame** (portal/slide) with handles + rule-of-thirds overlay.
- **Enter** commits frame geometry; **Esc** cancels.
- **Drag outside** rotates **frame** (camera rotation), not necessarily rotating all board content — clarify in UI.
- Never delete outside content by default (honest model / journal-only mutation).

---

## 6. Zoom tool (Z)

### (a) Trigger

- **Z** selects Zoom.
- **Ctrl+scroll wheel** zooms without switching tools (universal).

### (b) Immediate feedback

- Click: zoom **in** or **out** one step (based on Options bar +/− mode).
- Animated zoom (if enabled) smoothly scales view.
- Options bar: **Scrubby Zoom** checkbox changes drag behavior.

### (c) Modifiers

| Input | Effect |
|-------|--------|
| **Click** | Step zoom toward clicked point (often centers on click if pref enabled) |
| **Alt+click** | Opposite zoom direction |
| **Click+hold** | Continuous zoom until release |
| **Drag (Scrubby Zoom ON)** | Drag **right** = zoom in, **left** = zoom out |
| **Drag marquee (Scrubby OFF)** | Draw box → zoom to fit box |
| **Ctrl+0** | **Fit on screen** |
| **Ctrl+1** | **100%** (1 image px = 1 screen px) |
| **Ctrl++ / Ctrl+−** | Step zoom (any tool) |

### (d) Commit / cancel

- Instant; no modal. Marquee zoom commits on mouse release.

### (e) Subtle polish

- Last used +/− mode persists on tool.
- Scrubby Zoom can be **grayed out** if GPU/OpenCL prefs block it — common support thread topic.
- **Animated Zoom** pref affects feel separately from Scrubby.

### (f) Vector-stroke recommendation

- **Z+click** zoom to cursor; **Alt+click** zoom out — keep.
- **Marquee zoom** essential for infinite boards; default **marquee** for desktop, **scrubby** optional on pen.
- **Ctrl+0** = **fit selection** if something selected, else **fit all content in view**.
- Zoom is **camera** command, journaled only if tied to named view presets.

---

## 7. Ctrl+I invert · Ctrl+U Hue/Saturation

### 7A. Ctrl+I — Invert

#### (a–d)

- **Ctrl+I** (Win) / **Cmd+I** (Mac): Inverts **selection** or active layer colors (RGB complement).
- Immediate; no dialog. Menu: Image → Adjustments → Invert.

#### (e) Subtle polish

- On mask, inverts mask values.
- Destructive on pixel layer unless applied as adjustment layer workflow.

#### (f) Vector-stroke recommendation

- **Ctrl+I** on **selected strokes** → invert **stroke/fill color** (SVG-ceiling styles), not rasterize-then-invert.
- Scope: selection only; no selection → prompt or active layer objects.

### 7B. Ctrl+U — Hue/Saturation

#### (a) Trigger

- **Ctrl+U** / **Cmd+U** opens **Hue/Saturation** adjustment (legacy dialog) or creates adjustment layer (modern workflow).

#### (b) Immediate feedback

- Live preview on canvas; Properties panel sliders update image.

#### (c) Controls & typical ranges

| Control | Range | Effect |
|---------|-------|--------|
| **Hue** | **−180 … +180** | Rotates hue on color wheel |
| **Saturation** | **−100 … +100** | −100 → grayscale; +100 → hyper-saturated |
| **Lightness** | **−100 … +100** | Darken / lighten |
| **Colorize** | checkbox | Monochrome tint via hue slider |
| **Channel dropdown** | Master, Reds, Yellows, … | Target hue band |
| **Range sliders** | Inner = full effect; outer triangles = feather | Per-channel targeting |

#### (d) Commit / cancel

- Dialog: OK / Cancel. Adjustment layer: non-destructive until merged.

#### (e) Subtle polish

- Recent PS adds **prominent-color swatches** and **Invert Hue Range** button for targeted edits.
- Drag Saturation to −100 to diagnose which pixels fall in a channel band.

#### (f) Vector-stroke recommendation

- Map to **style adjustment** on selected objects with SVG-filter-equivalent limits (hue-rotate, saturate, brightness in CSS/SVG terms).
- Keep **same slider ranges** for muscle memory.
- **Colorize** → single-hue monochrome stroke style for diagrams.
- Apply via journaled **style commands**, not bitmap filters.

---

## 8. Ctrl+R rulers · guides

### (a) Trigger

- **Ctrl+R** / **Cmd+R** toggles **rulers** (top + left).
- Drag from ruler into canvas → **guide**.

### (b) Immediate feedback

- Rulers show tick marks; cursor position highlighted on rulers while moving.
- Guides appear as **movable lines** (cyan by default; color editable).

### (c) Modifiers

| Input | Effect |
|-------|--------|
| **Right-click ruler** | Change **units** (px, in, cm, mm, pt, pica, %) |
| **Double-click ruler** | Open **Units & Rulers** preferences |
| **Drag ruler origin box** (top-left intersection) | Move **0,0** origin |
| **Double-click origin box** | Reset origin to **top-left of document** |
| **Shift+drag** new guide | Snap guide to **ruler tick marks** |
| **Alt+drag** from ruler | Toggle **horizontal ↔ vertical** guide |
| **View → Snap To** | Snap objects to guides, grid, bounds |

### (d) Commit / cancel

- Guides persist until dragged off canvas or deleted; rulers are view-only toggle.

### (e) Subtle polish

- **View → New Guide** accepts explicit position with units (`50%`, `100px`, etc.).
- Info panel units sync with ruler units when configured.

### (f) Vector-stroke recommendation

- Infinite canvas needs **world-coordinate rulers** anchored to **camera origin**, not a fixed artboard corner.
- **Ctrl+R** toggles rulers; drag guides in **world space** (persist per document/board).
- Snap ink and frames to guides; show distance HUD when dragging objects (Smart Guides equivalent).

---

## 9. Shift+drag conventions

### (a) Trigger

- **Shift** held during drag or click modifies constraints (context-dependent).

### (b–c) Major Photoshop conventions

| Context | Shift behavior |
|---------|----------------|
| **Marquee / shape drag** | **Square / circle** from corner |
| **Move selection** | **Constrain axis** (horizontal or vertical) |
| **Free Transform rotate** | **Snap to 15°** increments (45° in some tools) |
| **Free Transform scale** | **CC 2019+:** proportional scale is **default**; **Shift** *disables* proportion lock (legacy pref reverses this) |
| **Selection tools (marquee, lasso, wand)** | **Shift+drag** or **Shift+click** → **add** to selection (+ icon on cursor) |
| **Alt/Option** | **Subtract** from selection |
| **Shift+Alt** | **Intersect** (marquee/lasso; not Quick Selection) |
| **Brush Shift+click** | Straight segment between points (§1) |
| **Eraser (Illustrator)** | Straight eraser stroke |

### (d) Commit / cancel

- Constraint applies for duration of modifier + gesture.

### (e) Subtle polish

- Selection add/subtract also exposed as persistent Options bar icons — Shift is the fast path.
- Quick Selection defaults to **Add** mode on subsequent drags without Shift.

### (f) Vector-stroke recommendation

| Convention | Adopt? |
|------------|--------|
| Shift+drag axis lock (move) | **Yes** — world axis, not screen axis unless rotated view |
| Shift+rotate 15° | **Yes** for frames and objects |
| Shift+scale proportional | **Yes** — prefer **modern default** (proportional without Shift) with legacy pref |
| Shift+click add selection | **Yes** for multi-select on board |
| Shift+straight ink | **Yes**, with optional break-chain pref |

---

## 10. Shift+letter tool cycling

### (a) Trigger

- **Shift + tool shortcut letter** cycles among tools sharing that letter in the **same group**.

### (b) Immediate feedback

- Tool icon in toolbar updates; Options bar changes to new tool.
- Example: **Shift+B** → Brush → Pencil → Color Replacement → Mixer Brush (order = toolbar stack).

### (c) Modifiers

- Requires preference **Use Shift Key for Tool Switch** (enabled by default).
- Pressing **B** again without Shift may also cycle (reported as confusing when pref conflicts — community threads).

### (d) Commit / cancel

- Instant switch; no commit.

### (e) Subtle polish

- Cycling only includes tools with **same shortcut letter** assigned in Keyboard Shortcuts → Tools.
- Removing a tool’s letter removes it from cycle.
- **Pen tool group** historically only cycles subset unless shortcuts manually aligned.

### (f) Vector-stroke recommendation

| Letter | Suggested Slate cycle |
|--------|----------------------|
| **B** | Brush / Ink → Marker → Highlighter (variable-width family) |
| **E** | Segment Eraser → Whole-stroke Eraser |
| **V** | Select → Direct-select (if added) |

Document cycles in **Commands & shortcuts** reference (command parity).

---

## 11. Ctrl+V paste · Ctrl+Shift+V paste in place

### (a) Trigger

- **Ctrl+C** copy → **Ctrl+V** paste.
- **Ctrl+Shift+V** / **Cmd+Shift+V** → **Paste in Place**.
- Menu: Edit → Paste Special → Paste in Place.

### (b) Immediate feedback

- **Ctrl+V:** New layer with content at **center of document/view** (not original coordinates).
- **Ctrl+Shift+V:** New layer at **same x,y** as source in the **same document**; across documents, preserves **relative document coordinates**.

### (c) Modifiers

- **Shift+drag layer** between documents: alternative placement workflow.

### (d) Commit / cancel

- Immediate paste; undo via Ctrl+Z.

### (e) Subtle polish

- Paste in Place is critical for **animation frames** and multi-doc comp work.
- Localized PS builds (e.g. Norwegian) have broken Paste in Place shortcuts — language/keymap conflicts reported.

### (f) Vector-stroke recommendation

- **Ctrl+V:** paste at **view center** or **mouse** (pick one; Photoshop center is default).
- **Ctrl+Shift+V:** paste at **source world coordinates** — essential for board duplicates and cross-tab paste.
- Paste strokes as **journaled add-nodes**, preserving style and width profile.

---

## 12. F6 — Color panel

### (a) Trigger

- **F6** toggles **Window → Color** panel (Fn+F6 on Mac laptops).

### (b) Immediate feedback

Panel shows:

- **Foreground** and **background** color swatches (large chips).
- **Sliders + numeric fields** for active chip (mode from flyout):
  - Grayscale, **RGB**, **HSB**, **CMYK**, **Lab**, Web Color.
- **Color ramp** field for picking saturation/brightness at current hue.
- **Double-click** chip → full **Color Picker** dialog.

### (c) Modifiers

- Click chip to choose which color **Eyedropper edits** (fg vs bg) — critical (§3).
- Flyout: **Copy Color as HTML**, **Copy Color’s Hex Code**, **Make Ramp Web Safe**.

### (d) Commit / cancel

- Slider changes apply live to active chip; used on next paint/fill.

### (e) Subtle polish

- Panel is resizable (drag bottom edge) for larger ramp.
- Related: **F5** = Brushes, **F7** = Layers (adjacent function-key muscle memory).
- **Shift+F6** = Feather Selection dialog (different command — don’t steal).

### (f) Vector-stroke recommendation

- **F6** opens shared **Color** panel in atlas-shell (identical in File Atlas + Slate).
- Show fg/bg, HSB + hex, recent colors strip.
- Active chip indicator must be **obvious** (prevent fg/bg eyedropper confusion).
- Wire panel to **stroke style** color tokens on the board scene graph.

---

## Cross-cutting themes for Slate

1. **Spring-loaded modifiers beat mode switches** — Alt+eyedropper, Alt+HUD, Space+pan, Z+marquee.
2. **Tiered bracket stepping + HUD fine control** — bracket keys for speed, drag for precision.
3. **Digit keys for opacity/flow** — zero learning curve for Photoshop migrants.
4. **Shift means “constraint” or “add to selection”** — consistent semantics across tools.
5. **Eraser is the hardest raster→vector mapping** — prefer path split (Illustrator) over whole-stroke delete (Excalidraw default).
6. **Infinite canvas changes crop/rulers/paste** — frames, world guides, paste-in-place in world coords.
7. **Document edge cases** — Shift+line auto-connect, fg/bg chip state, CC2019 transform defaults: either match PS or deliberately diverge with keymap doc notes.

---

## References (web)

- Julieanne Kost — [Brush shortcuts](https://jkost.com/blog/2017/05/brushes-and-painting-tool-shortcuts-in-photoshop-cc.html), [Color tips](https://jkost.com/blog/2017/05/tips-for-working-with-color-in-photoshop-cc.html), [Crop tips](https://jkost.com/blog/2017/05/essential-tips-for-cropping-in-photoshop-cc.html), [Rulers/guides](https://jkost.com/blog/2017/05/grid-guides-and-ruler-shortcuts-in-photoshop-cc.html), [Selections](https://jkost.com/blog/2021/07/25-shortcuts-and-tips-for-creating-better-selections-in-photoshop.html)
- Adobe Help — [Eyedropper](http://www.photoshopforphotographers.com/CC_2013/Help_guide/tp/Eyedropper_tool.html), [Eraser](https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/erase-parts-of-an-image-with-the-eraser-tool.html), [Hue/Saturation](https://helpx.adobe.com/photoshop/desktop/adjust-color/color-corrections/apply-a-hue-or-saturation-adjustment.html), [Rulers](https://helpx.adobe.com/photoshop/using/rulers.html), [Stroke smoothing](https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/create-smoother-more-polished-brush-strokes-with-stroke-smoothing.html), [Illustrator Eraser](https://helpx.adobe.com/illustrator/using/tool-techniques/eraser-tool.html)
- Excalidraw — [Issue #10725](https://github.com/excalidraw/excalidraw/issues/10725) (vector eraser semantics)
- GraphicDesign.SE — [Brush size stepping](https://graphicdesign.stackexchange.com/questions/166578/change-brush-size-increment-with-short-cuts-finer-control-of-brush-size-sho)
