# Grasshopper & Whiteboard Connector UX — Stage-2 Research

> **Stage-2 research input** for the Atlas keymap project. Documents observed Grasshopper canvas behavior (Rhino 6–8 era, GH1) and a proposed synthesis for Slate board connectors. Sources are public McNeel docs, the Grasshopper Primer, community forums, and Miro/FigJam help centers. Where behavior varies by Rhino build or is undocumented (snap radius, exact bezier math), that is called out.

---

## Research methodology

Primary sources consulted:

| Source | URL |
|--------|-----|
| Grasshopper Primer — Wiring | https://modelab.gitbooks.io/grasshopper-primer/content/1-foundations/1-2/4_wiring-components.html |
| Grasshopper Primer — UI / radial / search | https://modelab.gitbooks.io/grasshopper-primer/content/1-foundations/1-1/2_the-grasshopper-ui.html |
| David Rutten — canvas mouse/keyboard combos | https://www.grasshopper3d.com/forum/topics/what-hotkeys-and-shortcuts-are-available-in-grasshopper |
| Parametric by Design — canvas search & shortcuts | https://parametricbydesign.com/grasshopper/how-tos/canvas-search/ |
| McNeel Forum — wire display, relays, disconnect edge cases | discourse.mcneel.com |
| FigJam connectors help | https://help.figma.com/hc/en-us/articles/1500004414542 |
| Miro connection lines help | https://help.miro.com/hc/en-us/articles/360017730733 |

---

## 1. Wires (highest priority)

Grasshopper wires are **data channels**, not decorative lines. Interaction is grip-centric: every connection starts and ends on a circular **parameter grip** (input left, output right on standard components). Direction of drag is irrelevant — output→input and input→output both work.

### 1.1 Default wire drawing

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | LMB drag starting near an input or output grip circle. |
| **(b) Immediate feedback** | A wire segment follows the cursor from the source grip. While hovering a **compatible** target grip, the preview wire **snaps** and renders **solid** (committed visually, not logically). |
| **(c) Modifiers** | None — default mode. |
| **(d) Commit / cancel** | **Commit:** release LMB on a valid target grip. **Cancel:** release LMB on empty canvas — no connection is created; preview disappears. Grasshopper does **not** open component search on empty release (Unreal Blueprints do; GH users have requested this). |
| **(e) Polish** | Preview distinguishes “valid snap” (solid) vs “floating” (rubber-band). Wires use smooth **Bezier-style** curves by default (not polylines); path is auto-routed between grip positions. |
| **(f) Slate adaptation** | Use the same rubber-band → snap-solid → release-to-commit loop for board connectors. Anchor snaps should appear on **shape edge midpoints** (Miro/FigJam model), not free-floating points, unless modifier-held. |

**Existing wires on plain drag:** On a **single-source input**, a new default drag **replaces** the existing connection. The old wire is implicitly removed when the new one commits. This is the most common foot-gun for beginners and the reason **Shift-add** exists.

### 1.2 Shift+drag — add without replacing (multi-input)

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | LMB+Shift drag from a grip to a target input that supports **multiple sources** (list/tree inputs). |
| **(b) Immediate feedback** | Cursor shows a **small green arrow with “+”**. Wire preview behaves like default. |
| **(c) Modifiers** | Shift held for entire drag. |
| **(d) Commit / cancel** | Release on valid grip adds a wire; existing wires on that input remain. Cancel on empty canvas as usual. |
| **(e) Polish** | Green “+” cursor is the entire affordance — no dialog. Users learn that Shift means “additive, not destructive.” On inputs that only accept one source, behavior may still replace; multi-input params are the intended target. |
| **(f) Slate adaptation** | **Essential for whiteboards.** One shape may legitimately connect to many targets (annotation fan-out, tag links). Shift-add should never sever existing connectors from the same source grip. Show green “+” cursor parity with GH. |

### 1.3 Ctrl+drag — disconnect by tracing

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | LMB+Ctrl drag from a grip, tracing along an **existing** wire back toward its other end. |
| **(b) Immediate feedback** | Cursor turns **red** with a **“−”** icon. Target wire highlights; on disconnect-menu hover, wire turns red. |
| **(c) Modifiers** | Ctrl held. Must drag **all the way to the source component/grip** — a partial trace with “−” visible is **not** enough (common user error on widgets/sliders). |
| **(d) Commit / cancel** | Release on the source grip (or after full trace) removes **that one** wire. Cancel: release elsewhere without completing trace. **Alternate path:** RMB on grip → Disconnect → pick from list when multiple wires share an input. |
| **(e) Polish** | Disconnect is gesture-based, not a separate “delete wire” tool. Tracing teaches wire topology. Context-menu disconnect lists sources with per-item red hover highlight. |
| **(f) Slate adaptation** | Keep Ctrl+trace-remove for power users. Also offer FigJam-style “drag endpoint off shape → float or delete” for discoverability. Board connectors are visual — accidental disconnect is costly; consider brief undo toast. |

### 1.4 Ctrl+Shift+drag — move all wires from one grip

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | LMB+Ctrl+Shift drag from a grip that has **one or more** wires, toward another compatible grip. |
| **(b) Immediate feedback** | Cursor shows a **C-shaped black arrow** (bundle/move metaphor). All wires attached to that grip follow as a group preview. |
| **(c) Modifiers** | Ctrl+Shift together. |
| **(d) Commit / cancel** | Release on valid target grip: **all** wires reattach to the new grip (old grip ends empty). Cancel on empty canvas aborts. **Copy variant (partial):** Ctrl+Shift drag then RMB during drag was designed to copy wires instead of move; reported broken/incomplete in some builds. |
| **(e) Polish** | Primary tool for rewiring dense graphs without one-by-one traces. Forum power-users duplicate a distant component, Ctrl+Shift-rewire locally, delete duplicate. |
| **(f) Slate adaptation** | Map to “retarget all connectors from this anchor point to another anchor on a different shape.” Useful when replacing a sticky cluster with a frame. Do **not** conflate with multi-select move — wires must persist through retarget. |

### 1.5 Wire chaining — LMB drag + RMB (stay in wire mode)

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | While LMB-dragging a wire, click **RMB** before releasing LMB. |
| **(b) Immediate feedback** | First connection commits; wire tool **stays active** from the last target grip. |
| **(c) Modifiers** | RMB during drag; add Shift (add wire) or Ctrl (remove wire) for chained multi-ops. **Also:** “RMB while wiring plugs wire into multiple inputs” (community shortcut list) — batch fan-out gesture. |
| **(d) Commit / cancel** | Each RMB click commits one hop; final LMB release ends session. Esc / release on empty cancels current hop only. |
| **(e) Polish** | Eliminates repeated grip→drag→release→grip cycles when wiring long chains. Described by David Rutten as “Awesome and fast!” |
| **(f) Slate adaptation** | High value for moodboard “link this image to five notes.” Implement as optional: RMB during drag = commit hop, continue from target anchor. |

### 1.6 Wire shape, hit-testing, and routing

**Geometry:** Default wires are **smooth curves** (Bezier-like), automatically routed between grips. Users cannot natively edit control points; routing clutter is managed via **Relay** components (double-click wire to insert, double-click relay to remove, drag relay to reroute). Forum wish-list items (polyline wires, break points) remain unshipped in core GH.

**Hit-testing:** Wires are hittable for Ctrl+trace disconnect and double-click relay insertion. Hidden/faint wires remain **logically connected** and can accidentally receive double-clicks (relay insertion on invisible wires is a known annoyance).

**Fancy wires (View → Draw Fancy Wires):** Line style encodes **data topology** — single item (solid grey), list (double grey), tree (double-dash), empty/error (orange). Orthogonal to display mode.

### 1.7 Wire display modes (Default / Faint / Hidden)

Per **input** parameter: RMB grip → Wire Display → Default | Faint | Hidden.

| Mode | Appearance | When selected |
|------|------------|---------------|
| **Default** | Normal wire (respects fancy wires if enabled) | Always visible |
| **Faint** | Thin, semi-transparent | Always visible but de-emphasized; users use for “long distance” cross-group links |
| **Hidden** | Invisible (“wireless” data transfer) | **Green** wires appear only when **either** connected component is selected; vanish on deselect |

**Relinking:** No separate “relink mode.” Change endpoints via plain drag (replace), Shift-add, Ctrl+Shift bundle move, or drag wire endpoint implicitly by moving components (wires stretch). **Relays** are the routing-level relink tool.

**Bulk display:** No native multi-select wire-display toggle (David Rutten: “Wire display settings were a mistake”). Plugins (MetaHopper) batch this.

| **(f) Slate adaptation** | Offer **Default / Faint / Hidden** per connector or per connector group. Hidden suits presentation frames; faint suits cross-frame semantic links. Fancy wires become **connector labels showing link type** (references, tags, comments) at zoom thresholds — not data-tree encoding. |

### 1.8 Dangling wire on empty release

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | Drag from grip, release on empty canvas. |
| **(b) Feedback** | Wire preview simply disappears. |
| **(c–d)** | No commit; no search popup in GH (contrast UE Blueprints). |
| **(e) Polish** | Some users pull long wires across the canvas to plug into a **nearby empty input** on an already-placed component — intentional spatial workflow. |
| **(f) Slate adaptation** | **Strong recommendation:** adopt Blueprint-style “release on empty → command/search popup” filtered to connectable board actions (new text note, tag, frame, filter). Positions result at release point and auto-connects. This fits whiteboard thought flow better than GH’s silent cancel. |

---

## 2. Grip / port visual design

| Dimension | Grasshopper behavior |
|-----------|---------------------|
| **(a) Trigger** | Hover or click near circular grips on component edges. |
| **(b) Immediate feedback** | Grips are **filled circles** on parameter edges; compatible targets accept snap during wire drag (wire solidifies). Invalid targets do not snap. |
| **(c) Modifiers** | Shift → green + cursor; Ctrl → red − cursor; Ctrl+Shift → black C-bundle cursor (see §1). |
| **(d) Commit / cancel** | N/A for hover; click starts wire drag. |
| **(e) Polish** | **Snap radius is not documented** in official sources — snapping is forgiving (“click and drag **near** the circle”). Grip **type** is color/shape-coded by datatype in GH (geometry vs number vs text). ZUI: zoom **in** on a component to reveal ⊕/⊖ controls for optional inputs (Zoomable User Interface) — separate from canvas zoom-out LOD. |
| **(f) Slate adaptation** | Replace datatype-colored circles with **edge anchor handles**: small dots at shape side midpoints (4–8 per shape depending on type). Hover: enlarge + show side highlight strip. Snap radius ~12–16 px screen space. Image nodes: anchors on bounding box; text: left/right midpoints; frames: prefer outer edge. Show link-type icon on hover (reference, annotation, tag). |

---

## 3. Double-click empty canvas → component search

Also invoked by **Space** or **F4** (Create); distinct from **F3 / RMB canvas Find** which locates **existing** components.

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | Double-click empty canvas (LMB). |
| **(b) Immediate feedback** | Popup search field at cursor; focus in text entry immediately. |
| **(c) Modifiers** | Typing filters results live by **name, nickname, user alias**. Special prefixes instantiate preconfigured components (`"text` panel, `//text`, `123` slider, `1<5<3` ranged slider, `+`, `-`, `/`, etc.). |
| **(d) Commit / cancel** | **Enter** or click result → places component at **double-click point** (or search origin). Esc dismisses. Arrow keys navigate list. |
| **(e) Polish** | **Why beloved:** bypasses 20+ toolbar tabs; rewards nickname muscle memory (`ccx` → Curve Curve). User-defined **Component Aliases** (RMB toolbar icon). Ctrl+Alt+click placed component jumps to toolbar tab. Search scans loaded plugins — duplicate names are ambiguous. Quotation/`//` shortcuts create pre-filled panels — reduces parameter fiddling. |
| **(f) Slate adaptation** | Implement as **board command palette**: double-click → search commands (place text, shape, frame, image placeholder, **start connector**, run agent, tag). Fuzzy match on command id + aliases. Enter places at click point. Wire from Blueprint: if drag-release on empty, open same palette **pre-filtered** to connectable targets. |

---

## 4. Middle-mouse radial menu

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | **MMB click** on canvas or component; also **Spacebar** (older Primer) or **Ctrl+Space** (newer shortcut tables); Mac trackpad: **Option+Space**. **Not** a hold-drag pie — **click** invokes. |
| **(b) Immediate feedback** | Circular radial menu at cursor. **Context-dependent:** more sectors when invoked **on a selected component** vs empty canvas. |
| **(c) Modifiers** | None documented for sector selection — mouse move + click sector. |
| **(d) Commit / cancel** | Click sector executes; click outside dismisses. |
| **(e) Polish** | Documented sectors include frequent actions: **Recompute**, **Zoom** variants (canvas / Rhino preview), **Bake**, preview toggles, file ops — exact set varies by selection. Muscle memory: hand stays on MMB after panning (RMB pan is separate). Known fragility: MMB fails on some mice/drivers; Ctrl+Space fallback. Radial on component includes **Zoom to Rhino preview** (magnifying glass + cylinder icon). |
| **(f) Slate adaptation** | Radial is **secondary** to search popup for Slate; still useful for view ops (zoom selection, fit frame, toggle grid, snap). On **connector selected:** arrowheads, line style, delete, add label. On **shape:** start connector, send to back, group. Keep **click** not hold — matches GH muscle memory. |

---

## 5. Ctrl+B — send to back

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | Select object(s); **Ctrl+B**. |
| **(b) Immediate feedback** | Object moves **behind** other canvas objects in z-order. |
| **(c) Modifiers** | **Ctrl+Shift+B** — move back one step; **Ctrl+F** front; **Ctrl+Shift+F** forward one step. Also **Edit → Arrange**. |
| **(d) Commit / cancel** | Immediate; undoable. |
| **(e) Polish** | **Critical GH nuance:** draw order **also affects calculation order** — back objects may solve first. Groups participate in same z-stack; nested groups caused historical “can’t reach subgroup” issues. |
| **(f) Slate adaptation** | Map to board z-order for overlapping images/shapes. **Do not** tie to calculation order (no dataflow). Connectors typically render **above** shapes or **below** labels — pick one global rule. Ctrl+B on a frame should send frame chrome back, not necessarily its children. |

---

## 6. Ctrl+Shift+P — preferences

| Dimension | Behavior |
|-----------|----------|
| **(a) Trigger** | **Ctrl+Shift+P** (Windows); Mac builds may use **⌘,** per platform tables. |
| **(b) Immediate feedback** | Grasshopper Preferences dialog opens. |
| **(c) Modifiers** | N/A |
| **(d) Commit / cancel** | OK / Cancel / Apply. |
| **(e) Polish** | Key path: **File → Preferences → Interface → Shortcuts** — full remappable menu shortcuts (click item → green checkmark recording mode → key chord → confirm). Mouse combos (wire gestures) are **not** listed there — they are hard-coded canvas behaviors per David Rutten’s forum post. Other tabs: display, preview colors, etc. |
| **(f) Slate adaptation** | Slate Advanced window already hosts command reference — mirror GH’s split: **remappable commands** in prefs; **gesture grammar** (Shift/Ctrl wire modifiers) documented as fixed unless explicitly made tunable. Ctrl+Shift+P → Advanced/settings is reasonable alias. |

---

## 7. General canvas feel

### Navigation

| Input | Action |
|-------|--------|
| RMB drag | Pan |
| RMB + Ctrl drag | Zoom |
| Scroll wheel | Zoom |
| LMB drag (empty) | Marquee select |
| LMB + Shift drag | Add to selection marquee |
| LMB + Ctrl drag | Subtract from selection marquee |
| LMB + Shift drag (on selection) | Move with 45° angle snaps |
| Alt + LMB drag (on selection) | Duplicate while dragging (historically; broken when Rhino swallows Alt in some versions) |

### Semantic zoom (LOD)

When canvas zoom falls below **~60% of default zoom** (hard-coded; scales with Windows display scaling), component **icons and text fade/hide** to reduce clutter. Separate from **ZUI** (zoom **in** on one component to expose extra parameters). David Rutten indicated the 60% threshold may eventually become configurable.

### Marquee and selection

Window selection is LMB-drag rectangle. Shift/Ctrl variants union/subtract. **Groups** complicate window select (drag inside group moves group vs selects children — long-standing UX tension). Ctrl+arrow navigates wire graph (upstream/downstream); Ctrl+Shift+arrow grows selection along graph.

| **(f) Slate adaptation** | Adopt pan/zoom/marquee parity for muscle-memory transfer. Semantic zoom: at low zoom, show shape thumbnails/icons only; hide connector labels and anchor dots until zoom threshold. At high zoom, show anchor handles and label text. ZUI equivalent: zoom into a frame to edit its internal layout grid. |

---

## 8. Miro / FigJam connector UX (whiteboard baseline)

### FigJam

- **Types:** straight, bent (orthogonal-ish), curved — toolbar or **L** / **Shift+L** / **X**.
- **Creation:** drag between objects; **snaps to shape sides**; moving shape moves connectors.
- **Endpoints:** drag blue dot to reattach to another side/object; **Cmd/Ctrl** while dragging allows **free point** on shape interior (no edge snap).
- **Routing:** drag path handles to reroute around obstacles; bent connectors add segments.
- **Style:** color, dashed/solid, thick/thin, arrow/triangle/diamond/none endpoints, **text labels** on path.
- **Dangling:** endpoints may remain **unattached** intentionally (“let it vibe there”).
- **Quick create:** adding shapes can auto-insert connectors.

### Miro

- **Creation:** **L** hotkey, toolbar, or drag from **blue dots** on shape perimeter.
- **Anchor model:** **Side blue dots** → line may **cross through** shape when objects move; **center dot** → line stays **outside** (no overlap). Major diagram-readability distinction.
- **Routing:** straight, orthogonal, curved; white/blue control points; **double-click** control point resets segment.
- **Labels:** double-click line or +T; drag along path.
- **Modifiers:** **Shift** while drawing constrains angle; **Ctrl/Cmd** disables snap to other objects.
- **Line jumps** at intersections (orthogonal/straight only).
- **No hide** — delete only.

---

## 9. Synthesis — Grasshopper gestures on whiteboard connectors

Slate connectors are **semantic/visual** (references, tags, comments, flow annotations), not typed dataflow. The synthesis keeps GH **modifier grammar** and Miro/FigJam **spatial anchoring**.

### Proposed connector model

```
┌─────────────┐         ┌─────────────┐
│   Shape A   ●─────────●   Shape B   │
│  [anchors]  │  wire   │  [anchors]  │
└─────────────┘         └─────────────┘
     ↑ side midpoint anchors (default snap)
```

| GH gesture | Slate connector behavior |
|------------|-------------------------|
| LMB drag from anchor | Start connector; rubber-band preview; snap to compatible target anchor |
| Plain drag to occupied single-target anchor | **Replace** existing connector on that anchor (if one-to-one policy) |
| Shift+drag | **Add** connector without removing others from same source anchor |
| Ctrl+drag trace | **Remove** connector by tracing along path (power user) |
| Ctrl+Shift+drag | **Move all** connectors from anchor A to anchor B |
| LMB drag + RMB | Chain-connect multiple targets without mode exit |
| Release on empty canvas | Open **command/search popup** at point (Blueprint pattern — exceed GH) |
| Double-click wire | Insert **routing waypoint** (relay analog) |
| Wire display faint/hidden | De-emphasize or hide connector clutter; show on selection |

### Anchor policy (from Miro/FigJam)

| Rule | Recommendation |
|------|------------------|
| Default snap | **Edge midpoints** (4 sides minimum); prefer nearest side to cursor |
| Center anchor | Optional **“route outside shapes”** mode (Miro center-dot semantics) |
| Cmd/Ctrl + drag endpoint | Free attachment point on shape border path (FigJam) |
| Object move | Connectors **stick** and reroute (orthogonal or curved/bent) |
| Labels | Double-click or +T; draggable along path; optional tag text from link type |
| Arrowheads | Per-connector toolbar + radial; default none for moodboard, arrow for flow diagrams |
| Hit target | Wires pickable at 8 px screen stroke + 12 px hover halo |

### Command popup (GH search + Blueprint drop)

Double-click empty canvas **or** connector release on empty → fuzzy palette:

- Place: text, sticky, shape, frame, image slot
- Connect: “link to existing…” picker
- Filter by source context when opened from dangling connector drag

Place at cursor; if opened from wire drag, auto-connect first compatible anchor.

### What not to port literally

| GH feature | Why skip or adapt |
|------------|-------------------|
| Fancy wire data types | No typed dataflow on board |
| Calculation order = z-order | Irrelevant |
| Hidden wireless data | Connectors are intentionally visible semantics |
| Per-input wire display on GH components | Simpler: per-connector or layer-level visibility |
| Relay double-click on hidden wires | Gate relay creation: only when wire visible or connector selected |

---

## 10. Open questions for Stage-3 keymap drafting

1. **Confirm modifier chords** with Windows baseline: Ctrl vs Cmd on Mac for Slate (GH Mac remaps differ).
2. **Single vs multi connector per anchor pair** — moodboards often allow many; enforce policy per link type.
3. **Replace vs add default** — GH replaces on single inputs; FigJam/Miro always add. Recommendation: **add default, replace only when target anchor accepts max 1** (e.g. “primary parent”).
4. **Dangling connector** — keep FigJam “vibe” vs auto-delete on deselect.
5. **Radial vs search overlap** — Space key assignment must avoid conflict (GH history: Space = search *or* radial depending on version).

---

## References (abbreviated)

- McNeel — *What hotkeys and shortcuts are available in Grasshopper?* (David Rutten, authoritative mouse combos)
- Mode Lab — *Grasshopper Primer* §1.1 UI, §1.2.4 Wiring
- Parametric by Design — Canvas search, Keyboard shortcuts
- McNeel Forum — wire display, relays, disconnect trace, canvas zoom LOD, Blueprint-style drop-wire search wish
- Figma Learn — FigJam connectors
- Miro Help — Connection lines

*Document version: Stage-2 initial research — 2026-07-22.*
