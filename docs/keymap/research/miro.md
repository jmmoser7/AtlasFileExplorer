# Miro Canvas UX Research — Stage 2 Keymap Input

> **Purpose:** Stage-2 research input for the Atlas/Slate keymap project. Documents Miro’s *current* (2024–2026) canvas interaction behavior as observed from official help docs, Miro engineering posts, and community reports. Use this to inform keyboard-first infinite-canvas design for Slate—not as a verbatim copy spec.

**Sources (primary):** [Miro Shortcuts & hotkeys](https://help.miro.com/hc/en-us/articles/360017731033-Shortcuts-and-hotkeys), [Keyboard navigation](https://help.miro.com/hc/en-us/articles/11997028019858-Keyboard-navigation-while-working-on-boards), [Frames](https://help.miro.com/hc/en-us/articles/360018261813-Frames), [Sticky notes](https://help.miro.com/hc/en-us/articles/360017572054-Sticky-notes), [Shapes](https://help.miro.com/hc/en-us/articles/360017730713-Shapes), [Pen](https://help.miro.com/hc/en-us/articles/360017730573-Pen), [Text](https://help.miro.com/hc/en-us/articles/360017572094-Text), [Search](https://help.miro.com/hc/en-us/articles/360018109534-Searching-text-on-the-board), [Mouse/trackpad](https://help.miro.com/hc/en-us/articles/360017731053-Using-Miro-with-a-mouse-trackpad-or-touchscreen), [Toolbars/View/Grid](https://help.miro.com/hc/en-us/articles/360017730553-Toolbars), [Miro Engineering — keyboard navigation](https://medium.com/miro-engineering/miro-accessibility-introducing-keyboard-navigation-for-board-objects-dba41d0c4903).

**Confidence legend:** ✅ documented by Miro · ⚠️ inferred from community / secondary sources · ❓ not publicly documented (exact pixels/offsets).

---

## Cross-cutting patterns worth stealing

| Pattern | Miro behavior | Why it feels good |
|--------|---------------|-------------------|
| **Temporary pan** | `Space` + drag pans without switching tools | Keeps flow during creation/editing |
| **One-shot vs sticky tools** | Most creation tools revert to Select after one placement; Pen/Eraser/Lasso/Comment stay active | Reduces accidental multi-placement; power users double-click toolbar to lock |
| **Dual navigation model** | `Tab` = reading-order traversal; `Ctrl/Cmd+Arrow` = spatial “nearest neighbor” | Matches both document-like and canvas-like mental models |
| **Container hierarchy** | `Ctrl/Cmd+Shift+Up/Down` enter/exit frames | Tames infinite boards with structure |
| **Discoverability** | `?` in bottom-right opens shortcut sheet; `Ctrl/Cmd+K` command palette | Keyboard users can self-serve |
| **Modifier escape hatches** | `Shift` = marquee select; `Ctrl/Cmd` while drag = disable snap (2024+) | Precision without permanent settings churn |

---

## 1. Frame tool (`F`)

### (a) Trigger
- **`F`** activates the frame tool (single-character shortcut; can be disabled in Accessibility → Single-character shortcuts).
- Creation toolbar → Frame icon opens the **frames panel** with presets/templates (“View templates” for presentation/slide layouts).
- **Wrap selection:** Select objects → context menu `…` → **Create frame**.

### (b) Immediate feedback
- Cursor enters frame-placement mode; frames panel shows draggable presets and template gallery.
- New frames default to **white** background; title bar appears at top (editable).
- With **Object dimensions** enabled (View menu), live width/height readout while dragging/resizing.

### (c) Click vs drag & modifiers
| Gesture | Result |
|---------|--------|
| **Click** on canvas | Inserts a frame at the **currently selected preset size** (from panel) at click point ✅ |
| **Click + drag** on canvas | Draws a frame to arbitrary size ✅ |
| **Drag preset** from frames panel | Drops preset-sized frame ✅ |
| **Select tool + drag frame border/title** | Moves entire frame; contents move with it ✅ |
| **Select tool + drag frame edge handles** | Resizes; aspect ratio presets available via `…` context menu ✅ |
| **`Shift`** (multi-select frames) | Select several frames for bulk hide/export/delete ✅ |

**Object capture / parenting:**
- Objects visually inside a frame generally **move with the frame** and are associated for export/presentation ordering ✅.
- **Edge case (well-documented pain point):** An **invisible inner-edge drag band** (~1 cm at typical zoom, scales with zoom) near the frame interior border intercepts drags and **moves the frame** instead of drawing a selection marquee ⚠️. Workarounds: zoom in, start selection outside frame, use **`Shift+drag`** for marquee (community still reports conflicts near frame edges).
- Objects placed *under* an existing frame may not auto-attach; nudging them slightly often attaches ✅.

### (d) End / cancel
- **`Esc`** exits edit modes and returns toward Select (general Miro behavior) ✅.
- Frame deletion: select frame → `…` → Delete, or focus title → `Backspace`/`Delete` ✅.
- Tool reverts to **Select after one frame placement** (one-shot tool behavior) ⚠️ unless toolbar tool is double-clicked to lock.

### (e) Polished details
- **Frames panel** sidebar: thumbnail/list views, drag-reorder = **presentation slide order** ✅.
- **Organize frames** auto-sorts panel order by on-board position (left→right, top→bottom) ✅.
- **Presentation mode** (`Present` button): fullscreen, chrome hidden, frame titles hidden, arrow keys / UI arrows advance slides ✅.
- **Hidden frames** (facilitator feature): audience sees placeholder with closed-eye icon ✅.
- **Duplicate frame (`Ctrl/Cmd+D`):** smart date increment in frame titles (day/year roll forward) ✅.
- **Copy/paste between frames** preserves relative coordinates from source frame’s origin ✅.
- Per-frame **share link** (`…` → Copy link) ✅.

### (f) Recommendation for Slate
- Treat frames as **first-class slides**: ordered list, presentation mode, export per frame.
- Support **click = preset, drag = custom** — fastest path for facilitators.
- **Avoid** invisible frame-edge grab zones that compete with marquee select; if a frame border is draggable, show a visible hover affordance and give **`Shift+drag` unconditional marquee priority** inside frames.
- **`F` then number keys** (or last-used preset) would exceed Miro for keyboard-first users.
- Auto-create frame from selection is essential for retrofitting structure.

---

## 2. Sticky notes (`N`)

### (a) Trigger
- **`N`** opens sticky-note tool / color-size picker on creation toolbar ✅.
- Toolbar drag-drop, click, or **click-drag** on board ✅.
- **Bulk mode:** Tools → Bulk mode (or sticky submenu); **`Enter`** adds next sticky in bulk list; **`Esc`** exits bulk mode ✅.
- **Paste from spreadsheet:** `Ctrl/Cmd+V` → “Paste as sticky notes” (up to 5,000 cells, max 50×100) ✅.

### (b) Immediate feedback
- Default placement uses **yellow** sticky (community consensus) ⚠️; **16 fixed palette colors** (no custom colors) ✅.
- Three nominal sizes **S / M / L**; at 100% zoom, **S feels “Post-it sized”** ⚠️ ([Facilitator School](https://www.facilitator.school/blog/miro-sticky-notes-tricks)).
- Context menu on selection: color, shape (square ↔ rectangle), tags, emoji, author label, **Auto font size** ✅.
- Optional **Sticky Stack** places a deck of blank stickies to peel off ✅.

### (c) Creation flows & modifiers
| Flow | Behavior |
|------|----------|
| Click empty board | Places default sticky; can type immediately if tool stays active |
| Click-drag | Creates sticky sized to drag rect ✅ |
| **Bulk mode typing** | Each **`Enter`** = next sticky in the batch (vertical list input, then commits to board) ✅ |
| **Tab while editing sticky** | Creates **new sticky to the right**, same size/color, focus moves to it — primary “rapid sticky” flow ✅ ([Keyboard navigation help](https://help.miro.com/hc/en-us/articles/11997028019858-Keyboard-navigation-while-working-on-boards), [Facilitator School](https://www.facilitator.school/blog/miro-sticky-notes-tricks)) |
| **Blue connector dots** (on selected sticky/shape/card) | Hover shows ghost placement; click spawns connected object ✅ |
| **`Shift`/multi-select** | Batch color/size/tag changes ✅ |

**Color cycling:** ❌ No keyboard color-cycle shortcut documented; color changes via context menu only.

**Autosizing text:** Optional **Auto font size** shrinks/grows text to fit; manual font size also available ✅. ~3,000 character limit ✅.

### (d) End / cancel
- **`Esc`** exits bulk mode or stops editing ✅.
- After single placement, tool typically **returns to Select** (one-shot) unless toolbar locked ⚠️.
- **`Backspace`** deletes selected sticky ✅.

### (e) Polished details
- Square ↔ wide rectangle conversion without recreating object ✅.
- Tags (up to 8) + board search integration ✅.
- Emoji reactions for voting ✅.
- Paste-from-Excel preserves cell colors ⚠️ (video/community).
- **`Enter`** on selected sticky enters edit mode ✅.

### (f) Recommendation for Slate
- Implement **`Tab`-while-editing → spawn adjacent sticky** as a first-class brainstorming primitive (rightward default; **`Shift+Tab`** or arrow keys for other directions would improve on Miro).
- Expose **S/M/L presets** + remember last color/size per session.
- Add **keyboard color cycle** (`Ctrl/Cmd+.` or 1–9 palette slots) — easy win over Miro.
- Keep **Bulk mode** as a separate modal (`Enter`/`Esc` semantics) for structured note entry.
- Auto font size should be toggleable per sticky and per user default.

---

## 3. Minimap (`M`)

### (a) Trigger
- **`M`** toggles minimap open/closed (pin/unpin) ✅.
- Bottom-right **navigation toolbar** → map icon, or click **zoom percentage** → “Pin map” ✅.

### (b) Immediate feedback
- Minimap appears **bottom-right**, above/with navigation cluster (zoom, fit, fullscreen, help) ✅.
- Shows **board thumbnail** with **viewport rectangle** overlay ✅.
- State **persists** per user session/preference ✅.

### (c) Interaction
| Action | Behavior |
|--------|----------|
| **Drag viewport rectangle** | Pans main canvas to match ⚠️ (community; not detailed in help) |
| Click on minimap | Likely jumps viewport center to click point ⚠️ (standard pattern; not explicitly documented) |
| Zoom controls adjacent | Independent of minimap; `%` display clickable ✅ |

**Auto-hide:** ❌ No auto-hide documented — minimap stays until user toggles off ✅.

### (d) End / cancel
- **`M`** or map icon unpins/hides ✅.

### (e) Polished details
- Co-located with **Alt+1 zoom-to-fit**, **Alt+2 zoom-to-selection**, `Ctrl/Cmd+0` → 100% ✅.
- Minimap is optional — board usable without it for focused work ✅.

### (f) Recommendation for Slate
- **`M` toggle** + pinned preference is sufficient; optional **auto-show when zoomed out beyond threshold** would help orientation without Miro’s permanent clutter.
- Viewport rectangle drag is mandatory; **click-to-center** improves keyboard-mouse hybrid use.
- Consider **frame outlines** on minimap for slide-oriented boards (Slate differentiator).

---

## 4. Toggle grid (`G`)

### (a) Trigger
- **`G`** toggles **background grid visibility** on/off ✅.
- **View → Grid** submenu: grid type (**None / Line grid / Dot grid**) + **snap-to-grid** enable ✅.

### (b) Immediate feedback
- Grid renders on canvas background (lines or dots per View setting) ✅.
- **`G`** only toggles visibility — does **not** toggle snap ✅ ([community confirmation](https://community.miro.com/ask-the-community-45/turning-the-grid-on-and-off-14521)).

### (c) Snap behavior
- **Snap to grid** shipped mid-2024 as separate View setting ✅ ([community delivery post](https://community.miro.com/ideas/snap-to-grid-205)).
- While dragging objects, hold **`Ctrl` (Win) / `Cmd` (Mac)** to **temporarily disable snap** ⚠️ ([community reports post-2024 update](https://community.miro.com/ask-the-community-45/everything-is-snapping-to-the-grid-16470)).
- Snap increment may feel finer than visible grid (community reports **~0.25 cell** snapping) ⚠️.
- Additional **smart guides** align to other objects (object-to-object snapping) — separate from grid ✅/⚠️.

### (d) End / cancel
- **`G`** again hides grid ✅.
- Snap preference persists via View menu ✅.

### (e) Polished details
- Grid type (dots vs lines) suits different board aesthetics (diagramming vs brainstorming) ✅.
- Grid visibility independent of snap allows clean exports with alignment intact ✅.

### (f) Recommendation for Slate
- Split **`G` = visibility** and **`Ctrl/Cmd+G` or View toggle = snap`** explicitly in docs/UI (Miro conflates in user mental model).
- Always provide **hold-modifier to disable snap** during drag.
- Match grid spacing to **sticky/frame preset sizes** where possible.
- Show subtle **snap flash** on snap events for tactile feedback (native apps can exceed web).

---

## 5. Hand mode (`H`) and Select tool (`V`)

### (a) Trigger
- **`V`** → Select tool ✅.
- **`H`** → Hand (pan) tool ✅.
- **`Space` + hold** → temporary hand **without changing active tool** ✅ ([Mouse/trackpad help](https://help.miro.com/hc/en-us/articles/360017731053-Using-Miro-with-a-mouse-trackpad-or-touchscreen)).
- **Right-click drag** pans in mouse navigation mode ✅.

### (b) Immediate feedback
- **Select (`V`):** arrow cursor; marquee on empty-canvas drag; object hit-targets highlight on hover ✅.
- **Hand (`H`):** hand/grab cursor; canvas pans on drag ✅.
- **Space held:** cursor switches to grab/hand while space down ✅ (community/tooltip idea: “Hand [H] Temporary hand [space]”).

### (c) Modal behavior & modifiers
| Mode | Left-drag on empty canvas | Left-drag on object |
|------|---------------------------|---------------------|
| **Select** | Marquee selection | Move object |
| **Hand** | Pan canvas | Pan canvas (object not selected) ✅ |
| **Space held (any tool)** | Pan canvas | Pan canvas ✅ |

- **`Shift+drag` (Select):** marquee / additive selection behaviors ✅.
- Navigation mode setting (**Mouse vs Trackpad**) changes wheel = zoom vs pan — per-user, per-browser ✅.

### (d) End / cancel
- Release **`Space`** → revert to previous tool instantly ✅.
- **`V`** / **`Esc`** → return to Select / deselect ✅.

### (e) Polished details
- Spacebar pan is the **most praised** navigation pattern — works during pen, text, connector placement ✅.
- Hand on toolbar reduces accidental object moves for new users (community feature request) ⚠️.

### (f) Recommendation for Slate
- **`Space` temporary pan is non-negotiable** — implement at canvas input layer below tools.
- **`V`/`H` modal tools** + visible cursor swap.
- Never steal **`Space`** for other actions while a pointer device is primary.
- Optional: **`Middle-mouse drag`** pan (Miro supports on some mice) for desktop power users.

---

## 6. `Tab` / `Shift+Tab` — move through objects

### (a) Trigger
- **`Tab`** → next object; **`Shift+Tab`** → previous object ✅.
- Requires **object selection context** (not while editing text, except sticky special-case) ✅.
- **Note:** When focus is on **toolbars/panels**, `Tab` cycles chrome — press **`B`** to focus board first ✅.

### (b) Immediate feedback
- **Selection ring** moves to next/previous object ✅ ([Miro Engineering blog](https://medium.com/miro-engineering/miro-accessibility-introducing-keyboard-navigation-for-board-objects-dba41d0c4903)).
- **Context menu** appears above selected object ✅.
- Screen reader announcements (accessibility) ✅.

### (c) Traversal order
- **Reading order:** top → bottom, then left → right within rows ✅.
- Rows defined by **vertical overlap** (not strict Y equality) — stable sort algorithm ✅.
- Scope = **current nesting level** (inside frame vs board root) ✅.
- **`Ctrl/Cmd+Arrow`:** jump to **nearest object in direction** (direction-biased distance metric) ✅.
- **`Ctrl/Cmd+Shift+Down/Up`:** enter/exit container (frame, group) ✅.

### (d) With `Enter` / typing
- **`Enter`** on selected object → **edit text** (sticky, shape, text box) ✅.
- **`Esc`** → stop editing; **`Esc`** again → return focus toward menus ✅.
- While editing: **`Tab`** on sticky → spawn adjacent sticky (see §2), **not** traverse objects ✅.

### (e) Polished details
- Dual models (linear + spatial) backed by usability testing with sighted and non-sighted users ✅.
- **`Alt+Shift+Home/End`** (Win) / **`Option+Shift+Home/End`** (Mac) → first/last object in container ✅.

### (f) Recommendation for Slate
- Implement **reading-order Tab** + **spatial Ctrl/Cmd+Arrow** exactly — this is Miro’s best keyboard UX.
- Visual: high-contrast focus ring + optional **brief pan** to keep selection in view (Miro doesn’t always auto-pan — opportunity).
- **`Enter` to edit, Esc to exit** must be universal across stickies, text, and board labels.
- Resolve **`Tab` ambiguity** (chrome vs canvas) with explicit **`B` focus board** or focus ring on canvas.

---

## 7. Arrow keys / `Shift+Arrow`

### (a) Trigger
- **`Arrow keys`** with object(s) selected → **nudge** position ✅.
- **`Shift+Arrow`** → **larger nudge** ✅ ([Keyboard navigation help](https://help.miro.com/hc/en-us/articles/11997028019858-Keyboard-navigation-while-working-on-boards)).
- **`Ctrl/Cmd+Arrow`** → select **nearest object in direction** (not nudge) ✅.
- **No selection + board focused (`B`)** → **pan canvas** ✅.

### (b) Immediate feedback
- Selected objects jump by small increment; **`Shift`** increases step ❓ (exact px not documented).
- Multi-select nudge moves group together ✅ (past bug fixed 2023–2024).

### (c) Modifiers summary
| Keys | Selected object | Board focused, no selection |
|------|-----------------|-----------------------------|
| Arrow | Nudge small | Pan viewport |
| Shift+Arrow | Nudge large | Pan faster ❓ |
| Ctrl/Cmd+Arrow | Select neighbor | — |
| Alt+Arrow | **Duplicate** in direction ✅ | — |

### (d) Cancel
- Nudge commits immediately (undo with **`Ctrl/Cmd+Z`**) ✅.

### (e) Polished details
- Separate **navigation** vs ** manipulation** bindings reduce accidental moves when browsing objects with Ctrl/Cmd+Arrow ✅.
- **`Alt+Arrow` duplicate** enables quick linear layouts without mouse ✅.

### (f) Recommendation for Slate
- Document and tune nudge steps: suggest **1 canvas unit / 10 canvas units** with Shift (or 1px/10px at current zoom).
- When nothing selected, **arrow pan** should match accessibility spec — essential for keyboard-only preview.
- **`Alt+Arrow` duplicate** + optional **`Ctrl+D` repeat last offset** (Miro lacks repeat — easy Slate win).

---

## 8. `Ctrl+D` duplicate and `Alt+drag` duplicate

### (a) Trigger
- **`Ctrl/Cmd+D`** duplicates selection ✅.
- **`Alt/Option+drag`** duplicates while dragging ✅.
- **`Alt/Option+Arrow`** duplicates with offset in cardinal direction ✅.

### (b) Immediate feedback
- Duplicate appears offset from original; original remains ✅.
- Alt-drag shows **live ghost** following cursor ✅.

### (c) Offsets & patterns
| Method | Offset behavior |
|--------|-----------------|
| **Ctrl+D** | Fixed default offset ❓; community: feels **semi-random**, **does not repeat** last Alt-drag vector ⚠️ |
| **Alt+drag** | Duplicate placed exactly where released |
| **Alt+Arrow** | One step in arrow direction ❓ (exact distance undocumented) |

**Alt+drag quirk:** Must keep **Alt held until mouse button releases** or operation becomes **move**, not duplicate ⚠️ ([community bug report](https://community.miro.com/ideas/alt-drag-duplicate-bug-10265)).

### (d) End / cancel
- Release mouse → duplicate committed ✅.
- **`Esc`** during drag may cancel depending on tool ⚠️.

### (e) Polished details
- Frame duplicate with **smart date title bump** ✅.
- **`Alt+drag` on connector/shape** is standard across Adobe/Figma — muscle memory ⚠️.

### (f) Recommendation for Slate
- **`Ctrl+D` should repeat last duplicate delta** (Adobe/Figma pattern) — Miro gap.
- Allow **Alt release before mouse up** (industry standard).
- Show **duplicate preview outline** during Alt-drag.
- **`Ctrl+D` then typed text** on sticky/shape (community request) for rapid form-filling workflows.

---

## 9. `Ctrl+F` search

### (a) Trigger
- **`Ctrl/Cmd+F`** opens Find bar ✅.
- Alternative: board menu `…` → Edit → Find ✅.

### (b) Immediate feedback
- Search field (typically top of board) ✅.
- Matching widgets **highlighted**; non-matches **dimmed** ✅.
- Results list under search field ✅.

### (c) Navigation & filters
- **Click result** → **pans and zooms** to object ✅.
- **Filter dropdown:** All content → stickies, tags, text, etc. ✅.
- **`Alt+2` / `Option+2`** zoom-to-selection complements search jump ✅.

### (d) End / cancel
- **`Esc`** closes find ⚠️ (standard pattern).
- No find-and-replace ✅.

### (e) Polished details
- Tag-aware search integrates with sticky workflow ✅.
- Dimming non-results reduces visual noise on dense boards ✅.

### (f) Recommendation for Slate
- Match **dim + highlight + results list + click-to-fly**.
- Add **`Enter`/`Shift+Enter`** to cycle results keyboard-only (Miro under-documents keyboard result cycling — verify/improve).
- Search **frame titles**, **board node text**, **tags**, and **placed file names** (Slate scope).
- **`F3` or `Ctrl+G`** for “find next” once query established.

---

## 10. Page Up / Page Down — z-order

### (a) Trigger ([Shortcuts table](https://help.miro.com/hc/en-us/articles/360017731033-Shortcuts-and-hotkeys))
| Shortcut (Windows) | Action |
|--------------------|--------|
| **PgUp** | Bring to **front** |
| **Shift+PgUp** | Bring **forward** one step |
| **PgDn** | Send to **back** |
| **Shift+PgDn** | Send **backward** one step |

Mac: **`fn+↑/↓`** or **`PgUp/PgDn`**; Shift variants for one-step.

### (b) Immediate feedback
- Object jumps in stack; visual overlap updates immediately ✅.
- Context menu `…` also exposes Bring to front / Send to back ✅.

### (c) Semantics
- **PgUp/PgDn alone** = extremal (full front/back) ✅.
- **Shift+PgUp/PgDn** = **single layer** step (added ~April 2024) ✅ ([community delivery](https://community.miro.com/ideas/add-push-backwards-bring-forwards-option-to-objects-selections-2464)).
- Applies to shapes, stickies, images, etc. ✅.

### (d) Cancel
- **`Ctrl/Cmd+Z`** undoes reorder ✅.

### (e) Polished details
- One-step forward/back critical for dense diagrams — was long-requested ✅.

### (f) Recommendation for Slate
- Adopt **identical four-level model** (front/forward/back/backward).
- Show **transient z-order indicator** (e.g., “3 → 4”) for learning.
- Map Mac **`fn+arrow`** equivalents in keymap doc.

---

## 11. Text (`T`), Oval (`O`), Rectangle (`R`), Pen (`P`)

### Text — `T`
| Aspect | Behavior |
|--------|----------|
| Trigger | **`T`** or toolbar Text |
| Create | **Click** to place; text box **auto-expands** as you type ✅ |
| Drag-create | ❌ Not supported (unlike shapes) — click only ✅ |
| Defaults | Standard text styling; formatting via context menu |
| After placement | **Reverts to Select** (one-shot tool) ⚠️ — must press **`T`** again for next box ([community](https://community.miro.com/ask-the-community-45/can-i-keep-text-box-on-1879)) |
| Edit | Double-click or **`Enter`** when selected ✅ |

### Rectangle — `R` / Oval — `O`
| Aspect | Behavior |
|--------|----------|
| Trigger | **`R`** / **`O`** or **`S`** opens shapes panel |
| Create | **Click** = default size shape at point ✅; **Click-drag** = custom size ✅ |
| Perfect circle | Click circle tool after **`O`** / reload if custom size cached ⚠️ |
| Modifiers while drawing/resizing | **`Shift`** = lock aspect ratio ✅; **`Alt/Option`** = resize from center ✅ |
| Text | Select shape → type to add label (~6k chars) ✅ |
| After placement | **One-shot → Select** ⚠️ |
| Style | No persistent default style across boards (community pain) ⚠️ |

### Pen — `P`
| Aspect | Behavior |
|--------|----------|
| Trigger | **`P`** or toolbar Pen |
| Create | Freehand stroke on pointer down/move ✅ |
| Presets | Up to **3 presets** each for pen, highlighter, smart drawing (color + thickness) ✅ |
| Smart drawing | Converts strokes to shapes/stickies/lines ✅ |
| Eraser | **`E`** or sub-tool; swipe to delete pen strokes ✅ |
| **Stays active** | Pen, Eraser, Lasso, Comment are **multi-use** — do **not** auto-revert ✅ |
| Exit | **`V`** or another tool ✅ |

### Tool locking (all creation tools)
- **Double-click toolbar icon** locks tool for repeated use (community/Miro support) ⚠️.

### (f) Recommendation for Slate
- **One-shot vs sticky** split is worth copying: creation tools → Select; pen → stays.
- Offer user pref: **“Stay in tool after create”** for facilitators.
- **`R`/`O` click vs drag** should match shapes; **`T` click-only** with immediate caret focus.
- **`P` presets on 1/2/3** keys while pen active would exceed Miro for keyboard users.
- Auto-switch to Select after create should **still allow immediate drag** of just-created object (Miro sometimes feels like extra click).

---

## 12. Spacebar + drag canvas move

### (a) Trigger
- **`Space` (hold) + left-click drag** ✅.
- Works from **any tool** including Pen, Text, connectors ✅.

### (b) Immediate feedback
- Cursor becomes **hand/grab** while Space held ✅.
- Canvas pans in drag direction; objects do not move ✅.

### (c) Variants
- **`H` + drag** = same pan but **modal** (tool switch) ✅.
- **Right-click drag** = pan in mouse mode without Space ✅.
- **Trackpad:** two-finger slide pans (mode-dependent) ✅.

### (d) End
- Release **Space** → prior tool and cursor restored **immediately** ✅.
- Release mouse → stop pan ✅.

### (e) Polished details
- Does not affect undo stack ✅.
- Community proposal: **Space during object creation** to reposition anchor (Illustrator-style) — **not in Miro** ⚠️ (feature request).

### (f) Recommendation for Slate
- Implement Space-pan at **input dispatcher** priority over tool handlers.
- Optional Illustrator-style **Space-reposition during create-drag** is high-value for precision layouts.
- Show **grab cursor** only while Space held — subtle but critical feedback.

---

## Known Miro gaps (Slate opportunities)

1. **`Ctrl+D` repeat-last-offset** after Alt-drag ❌  
2. **Keyboard color cycle** for stickies ❌  
3. **Alt release before mouse-up** on duplicate drag ❌  
4. **Frame inner-edge vs marquee** conflict ⚠️ chronic pain  
5. **Exact nudge/snap distances** undocumented ❓  
6. **Text tool persistence** ❌ (always one-shot)  
7. **Tab/Arrow adjacent sticky spawn** only while editing, only to the right ⚠️ limited  
8. **Find-next keyboard cycling** under-documented ❓  

---

## Research metadata

| Field | Value |
|-------|-------|
| Date | 2026-07-22 |
| Researcher role | UX research subagent (Stage 2 keymap) |
| Method | Web search + official Miro Help Center + Miro Engineering blog + community forums |
| Codebase | Intentionally not consulted (per brief) |
