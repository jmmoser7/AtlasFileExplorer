# Interaction pattern hierarchy

The shared vocabulary for tool contracts. A contract
(`docs/keymap/contracts/<tool>.md`) **never restates** a pattern from this
file — it declares what it inherits and lists only deviations and additions.

Levels (specificity increases downward):

- **L0** — universal: every tool, both apps.
- **L1** — object-class: every tool producing/editing that class of object.
- **L2** — tool-family archetypes: a shared gesture grammar several tools arm.
- **L3** — tool-specific: lives only inside the tool's contract file.

**Promotion rule (anti-bloat):** when the same rule appears in two contracts,
it moves UP to the lowest level that covers both, gets a pattern ID here, and
both contracts replace their copy with a reference. Never copy a pattern
downward. When an L2 rule turns out to apply to a whole object class, promote
it to L1, etc.

**Deviation rule:** a contract may override an inherited pattern, but the
override must be written as `deviates P2.x: <what changes>` so the exception
is searchable.

---

## L0 — Universal (both apps, every tool)

- **P0.1 Cancel stack.** Esc peels exactly one layer per press:
  ActiveOperation → Draft → Mode (armed tool → Select) → Selection → Chrome.
  (`atlas-commands::CancelLayer`.)
- **P0.2 One gesture = one undo.** Everything a single user gesture produced
  reverts with a single Ctrl+Z (journal command grouping).
- **P0.3 Journal-only mutation.** Commits go through the journal with an
  author; tools never mutate the document directly (Constitution Art. VI).
- **P0.4 Repeat-last.** Space (tap, not held) or Enter while idle re-runs the
  last repeatable command — so any tool is re-armed by Space/Enter if it was
  the most recent command.
- **P0.5 Camera never blocked.** Scroll-zoom and Space-drag pan work during
  any draft or drag without dropping it; a Space *tap* is still repeat-last
  (P0.4) — only Space+movement pans.
- **P0.6 Feel constants are tokens.** Every tolerance, radius, threshold,
  alpha, and step curve is a named constant (`ui-tokens.toml` or a `mod
  consts` block) referenced by the contract — never an inline magic number.
- **P0.7 Arming routes through the registry.** Hotkey, palette entry
  (typed name + aliases), and tools-rail icon all dispatch the same
  `CommandId`; the armed tool is visible in the rail and via the cursor.
- **P0.8 Availability gating.** Commands declare availability
  (board tab, selection present, …) and are inert — not error-prone —
  when gated.

## L1 — Object-class

### P1.node — every board node

- **P1.node.flags** lock / hide / group semantics (Ctrl+L/H/G family);
  locked nodes still feed smart guides.
- **P1.node.select** click select, Shift+click add/toggle, marquee;
  group click selects the group, Ctrl+Shift+click a member.
- **P1.node.move** drag with smart guides; ortho (F8, Shift inverts) and
  grid snap (F9) apply; arrows nudge.
- **P1.node.zorder / clipboard** PageUp/PageDown/Ctrl+B; Ctrl+C/X/V,
  Ctrl+Shift+V in place.

### P1.shape — closed shapes (rect, ellipse, frame)

- **P1.shape.style** fill + stroke; new shapes consume the current style
  defaults; color applies via fg/bg state and inspector.
- **P1.shape.aspect** Shift during creation locks aspect (square/circle).

### P1.curve — open curves (line, arc, polyline, bezier span, pen, brush ink)

- **P1.curve.style** stroke only, no fill; stroke width/cap/dash editable
  after the fact; Ctrl+J joins endpoints.
- **P1.curve.create-style** the last **single-node** edit (inspector patch,
  grip edit, or prior create) seeds stroke + opacity on the next compatible
  create (draft curves: Line, arc, polyline, …). When nothing was edited yet,
  draft curves use `default_curve_stroke` at the current fg color — **Square**
  end caps, Miter joins (distinct from expressive ink's round caps). Brush/Pen
  ink keeps its own round defaults (`P2.StickyInk`). Implementation:
  `board_style::BoardLastStyle`, updated from `patch_nodes` (single target) and
  grip commits.
- **P1.curve.grips** selected open curves expose their defining points as
  gripable handles (endpoints, on-curve anchors) — **not** a resize bbox.
  Applies to **every** selected simple line in the selection, not only when
  one line is selected; homogeneous multi-line selections skip group bbox
  handles. Direct Selection (A) additionally exposes tangent handles and
  segments.
- **P1.curve.pick** click and marquee selection hit the **stroke** (via
  `vector_ink::hit_stroke` + `pick.slop` ≈ 4 screen px), never the node's
  axis-aligned rect alone. Legacy `ShapeKind::Line` included. Marquee: the
  stroke centerline intersects the marquee, or a stroke hit at the marquee
  center. Implementation: `board_path::hit_shape_stroke`,
  `board_path::marquee_hits_node`.

### P1.text / P1.image

Exist (edit-in-place, crop/adjust); patterns promote here when a second
text- or image-producing tool appears.

## L2 — Tool-family archetypes

### P2.RhinoDraft — precision draft tools (line, arc, polyline, bezier span)

State machine: `Armed → Placing(point k) → … → Commit`.

- **P2.RhinoDraft.gesture** both grammars commit identically:
  click-move-click **and** press-drag-release. Disambiguation: movement
  beyond `draft.drag_threshold` px before release = drag grammar; release
  within threshold = click grammar.
- **P2.RhinoDraft.rubber** live rubber-band preview from the last placed
  point to the (constraint-resolved) cursor.
- **P2.RhinoDraft.ortho** held Shift inverts the F8 ortho state for the
  pending segment (45° steps, board convention).
- **P2.RhinoDraft.tab** Tab locks the pending segment's *direction* at its
  current angle; movement then only changes length; Tab again unlocks.
- **P2.RhinoDraft.numeric** after the first point, typed digits build a
  length readout; Enter (or the committing click) places the next point at
  that distance along the current direction. Backspace edits; Esc clears the
  entry before it cancels anything else.
- **P2.RhinoDraft.esc** Esc backs out one placed point per press; with no
  points placed it disarms to Select (P0.1 layering).
- **P2.RhinoDraft.oneshot** commit returns to Select; Space/Enter re-arms
  (P0.4).

### P2.DragShape — area tools (rect, ellipse, frame)

- press-drag-release only; Shift = aspect (P1.shape.aspect); releases under
  `MIN_DRAW` discard; commit returns to Select.

### P2.StickyInk — expressive stroke tools (brush, eraser)

- sticky (stays armed until Esc/tool change); every stroke commits its own
  undo step; `[`/`]` step width (Photoshop tiers); width-circle cursor;
  brush spring-loads eyedropper on Alt.

### P2.PlaceOnce — click-to-place (text, sticky note)

- click places and enters edit-in-place; Esc/blur commits text; sticky note
  chains the next placement on Tab.

## L3 — Tool-specific

Only in `contracts/<tool>.md`. If you're about to write the same L3 rule in
a second contract — stop and promote it.
