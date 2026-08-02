# Tool kits — declarative tools and toolbars

**Status: proposal, not adopted.** Written in response to the question "can a
tool be a file, the way a workbook is a file?" Nothing here is built. The
constitution is not amended by this document; §11 drafts the one clause that
would need ratifying, for the user to accept or reject.

---

## 1. The short version

A user-defined tool is possible today as **pure declarative data**, with no
amendment to Article VII's script prohibition and no new rung on the extension
ladder — provided one boundary holds:

> A kit tool may not define **how a gesture behaves**. It may only define
> **what an existing gesture produces** and **how the tool presents itself**.

Everything the proposal wants — architect kits, designer kits, shared toolbars,
rapid personal experiments — fits inside that boundary. The things that do not
fit (a constraint solver, a parametric node graph, a genuinely new gesture) do
not fit *because they need an interpreter*, and that is exactly the line
Article VII.4 already draws.

The enabling change is not a plugin system. It is a **refactor that separates
gesture grammar from result recipe**, which the board code is already asking
for on its own merits (§4).

---

## 2. What this builds on

Three pieces of prior work converge here, and two of them are already decided.

| Prior decision | Status | Relation to this proposal |
|---|---|---|
| **D14 — extension model** (`docs/audit/2026-07-25-decisions-flexibility.md`) | decided v1, unbuilt; scheduled Wave 3+ as "the extension package format" | Kits *are* that package format. D14's asset list — themes, keymaps, skills, brushes, palettes, board templates, portal definitions — does not name tools. This proposal argues it should. |
| **Amendment D — control surfaces / `PanelSpec`** (audit §7.5) | proposed, **unratified**; explicitly not planned before ratification | Amendment D makes toolbars, flyouts and tabs one thing: `Slot × Trigger × Content`. Kits supply *content*; they must not pre-empt the slot model. §10 keeps them independent. |
| **Tool contracts** (`docs/keymap/contracts/`, the `tool-contract` skill) | shipped for one tool (`line.md`), 31 dimensions registered | The behavior matrix is already a formal tool description. §9 shows that 8 of its 17 tool-scoped dimensions are exactly the fields a kit file needs, and the other 9 are answered by the grammar or the node kind. |

## 3. The gap in the extension ladder

`EXTENDING.md` §4 lists four rungs. Read them for where a *tool* lands:

| Rung | Example given | Status |
|---|---|---|
| Declarative assets | themes, keymaps, brushes, palettes, board templates, portal definitions | intended v1, unbuilt |
| MCP servers | out-of-process command-surface clients | intended v1, unbuilt |
| In-process WASM | "a real third-party canvas tool **with its own interaction**" | deferred, needs the VII.4 amendment |
| Native binaries | — | never |

Rung 1 stops at brushes. Rung 3 begins at "a tool with its own interaction."
Between them sits an unnamed rung that is where nearly all real tools live:

> **A tool with no interaction of its own.** It borrows a gesture grammar the
> core already implements and tested, and varies only what that gesture
> produces, what it is called, what it looks like, and when it is available.

A north arrow stamp, a redline pen, a 1:100 dimension tool, a door symbol, a
title block, a preset portal — none of these needs a new interaction. They need
a *click* or a *drag* the core already knows how to do. Placing them on rung 3
overprices them by an amendment and a WASM host; leaving them off the ladder
entirely is why every one of them currently costs an enum variant and six edit
sites.

This unnamed rung is rung 1. It requires no new machinery beyond a file format
and a loader, because it is data.

## 4. The refactor: grammar × recipe

`BoardTool` has seventeen variants. It looks like seventeen tools. It is
actually a **product of a small closed set and a large open set**, and the code
says so in the clearest possible way. From `finish_draw` in
`apps/slate/src/app/board.rs`:

```rust
BoardTool::RectShape => NodeKind::Shape(ShapeNode {
    shape: ShapeKind::Rect,
    fill: Some(Rgba([accent.0[0], accent.0[1], accent.0[2], 60])),
    stroke: Stroke { width: 2.0, color: accent, dash: Dash::Solid, /* … */ },
    corner: Corner::Square, flip: false, path: None,
}),
BoardTool::Ellipse => NodeKind::Shape(ShapeNode {
    shape: ShapeKind::Ellipse,
    fill: Some(Rgba([accent.0[0], accent.0[1], accent.0[2], 60])),
    stroke: Stroke { width: 2.0, color: accent, dash: Dash::Solid, /* … */ },
    corner: Corner::Square, flip: false, path: None,
}),
```

Two arms, identical but for one enum field and a pile of hardcoded style
constants. `Frame` is a third arm of the same drag. The gesture — press, drag a
rectangle, constrain with Shift, release — is written once and shared. What
differs between "rect tool" and "ellipse tool" is *a literal struct*.

That struct is the recipe, and it is already data in everything but storage.

So:

```
tool = grammar (code, closed, ~8 members)
     × recipe (data, open, unbounded)
     × presentation (data: name, icon, key, aliases)
     × availability (data: view / facet / selection predicate)
```

The closed grammar set covers all seventeen of today's tools:

| Grammar | Gesture | Today's tools |
|---|---|---|
| `Select` | pick, marquee, handles, grips | Select |
| `DirectSelect` | anchor / handle editing | DirectSelect |
| `DragRect` | press-drag-release bounding box | Frame, Rect, Ellipse |
| `TwoPoint` | click-move-click *or* drag; direction lock, typed length (`P2.RhinoDraft`) | Line |
| `MultiPoint` | repeated clicks, Enter or double-click to finish | Polyline, Arc, BezierSpan |
| `Freehand` | sampled stroke, fitted or variable-width | Pen, Brush |
| `PlacePoint` | single click places a thing | Text, Sticky |
| `Sweep` | continuous hit-test along a drag | Eraser |
| `Sample` | read a property from what is under the cursor | Eyedropper |

Nine grammars, and `Pan` is a camera mode rather than a creation grammar.

The refactor is: `begin_gesture`/`end_gesture` match on **grammar**, not on
tool; the active tool carries its recipe alongside. Seventeen arms become nine,
in a 4,900-line file where the tool match is roughly 270 lines. This is worth
doing whether or not kits ever ship — it is Article III applied to the core's
own tool code, and it is the same "one model, several interpreters" move
Articles IV and V already ratify.

**The 10% rule does not get repealed by this; it gets relocated.** Article III
binds the grammar set absolutely — a tenth grammar must name its real recurring
use like anything else. Kits are governed differently because an unused kit
tool is not loaded, not painted, and costs nothing.

## 5. Recipe kinds

A recipe says what a completed gesture produces. Three kinds cover the space:

- **`shape`** — a scene node of an existing `NodeKind`, with a style block.
  Serves every pen, marker, redline, dimension-line and shape variant.
- **`stamp`** — a saved group of scene nodes, placed by the gesture, optionally
  scaled to the drawn rect. Serves north arrows, scale bars, title blocks, door
  and window symbols, logo lockups, annotation callouts.
- **`portal`** — a portal node with a preset source and query. Serves preset
  dashboards. Blocked on Phase 3; the format should reserve it now.

**`stamp` is the highest-value kind and should ship first.** It needs zero new
grammar (`PlacePoint` and `DragRect` both exist), zero new node kinds, and it
is what actually differentiates one profession's toolbar from another's. A
firm's symbol library is a stamp kit.

Stamps want text substitution (a title block with a sheet number). Bound it
hard: **named field substitution into `Text` node content only, from a fixed
vocabulary plus user-supplied literals — no expressions, no arithmetic, no
conditionals.** `{{title}}` yes; `{{title | upper}}` no. That is the VII.7
boundary, and it is worth stating in the format rather than discovering later.

## 6. The file

One extension, not two. A kit holds tools and, optionally, bar layouts; a kit
with one tool and no bar *is* a single shared tool. Fewer registered
extensions, fewer concepts, and sharing stays "send the file."

Suggested `.slatekit`, TOML — matching the user-theme precedent
(`atlas_shell::theme::user_theme_dir`), which is the closest existing thing to
a hand-droppable user asset, and diffs legibly at import review.

```toml
format_version = 1
id     = "arch-redline"
name   = "Architect's redline"
author = "hp"
requires = { grammars = ["place_point", "freehand", "two_point"] }

[[tool]]
id      = "north-arrow"
name    = "North arrow"
grammar = "place_point"
icon    = "compass"
key     = "Alt+N"
aliases = ["north", "compass"]
sticky  = false
recipe  = { kind = "stamp", nodes = "stamps/north-arrow.nodes.json" }

[[tool]]
id      = "redline"
name    = "Redline pen"
grammar = "freehand"
icon    = "pen"
key     = "Alt+R"
sticky  = true
snap    = { grid = false, ortho = false }
[tool.recipe]
kind         = "shape"
node         = "path"
stroke       = { color = "#e8443a", width = 2.0, cap = "round", join = "round", profile = "taper" }
create_style = "pinned"     # D16: ignore last-used style, always redline red

[[bar]]
id    = "redline"
name  = "Redline"
items = ["north-arrow", "redline", "core:text"]
```

Bars reference tools by id, including tools from other kits (`core:text`). A
tool may appear in several bars; a personal "favourites" bar can draw from five
installed kits. Bars are layout; tools are content; keep them separable or you
get duplication immediately.

Asset paths inside a kit are **relative locators** (Art. IX.2), resolved
against the kit file, so a kit folder can be zipped and mailed.

## 7. Where kits live, and what happens when one is missing

Three scopes, all legitimate, and the proposal as stated only named the second:

| Scope | Meaning | Stored in |
|---|---|---|
| **User** | "I always want my kit, everywhere" | `data_dir()/kits/` — same pattern as `themes/` |
| **Workbook** | "this competition board uses these tools" | a `kits: Vec<KitLink>` field on `SlateDoc` |
| **Session** | "let me try this without committing" | nothing on disk |

Workbook kits are **links, not copies** (Art. IX.1), stored relative-first
(IX.2), with tri-state health (IX.3): `Ok` / `Missing` / `Unknown`. `package`
copies kits beside the workbook and records their origin (IX.4).

**The load-bearing rule, and the one that makes the whole scheme safe:**

> A kit tool produces only ordinary scene nodes. A missing kit costs you the
> ability to make more of them; it costs the document nothing.

No custom node kinds, ever. A `.slate` file must never become unopenable, or
partially unrenderable, because a third-party file is absent. A missing kit
greys its bar entries and says so. This is a stronger guarantee than a plugin
system can normally make, and it comes free from the data-not-code boundary.

Version tolerance follows `ViewKind::Unknown` rather than
`SlateDoc::load_from`: an unknown *grammar* marks that one tool `Unsupported`
and the rest of the kit still loads. Whole-file rejection is right for a
document and wrong for a toolbar.

## 8. What a kit may not do

The non-goals are the specification. A kit may not:

1. define a new gesture grammar (needs an interpreter → VII.4);
2. contain control flow, expressions, or arithmetic (VII.7);
3. define a new `NodeKind` or a new style property (Art. IV — a style property
   lands in both interpreters or neither, and a kit cannot teach
   `slate-artifact` anything);
4. mutate a document outside the journal (structurally impossible: its only
   output is a node stamp committed through `SceneCmd`);
5. run at load time, or do anything at all without a human gesture;
6. touch the network, or the filesystem outside its declared relative assets.

A kit is inert until a human arms one of its tools.

**Provenance.** Data-not-code makes the blast radius small but not zero: a
shared kit binds hotkeys and, once portals land, source queries. A portal
recipe pointing at `~/` is a privacy leak, not a code exploit. Import should
show the kit's origin and its bindings, leave portal source bindings for the
user to re-point rather than trusting them, and never fetch anything.

## 9. The contract becomes machine-checkable

This is the part that pays off most for the stated motivation ("create tools
quickly, see if they work, iterate").

The tool-contract skill already makes you answer 17 dimensions before building.
Sorted by who can answer them:

| Answered by | Dimensions |
|---|---|
| **Kit data** | D01 initiation, D02 stickiness, D05 modifiers (from a fixed effect vocabulary), D06 snapping defaults, D08 which parameters accept typed entry, D09 readout fields, D10 cursor, D16 create-style |
| **The grammar** | D03 gesture grammar, D04 click-vs-drag, D07 direction locks, D11 commit, D12 cancel |
| **The node kind** | D13 selected presentation, D14 post-edit grips, D17 hit-test and pick |
| **Prose only** | D15 non-goals |

So the kit file *is* the behavior matrix, in machine form. Two consequences:

- `cargo xtask contracts` can validate kits the way it validates markdown
  today — every referenced grammar and icon exists, every kit tool has a
  contract row, no duplicate hotkeys.
- The skill's step 7 ("implement") collapses to "write the kit" whenever an
  existing grammar fits, and escalates to Rust only when it does not. That is
  the difference between a tool idea costing a compile cycle and costing a
  sentence.

Golden-path testing gets cheap too: the grammar is tested once, so a kit tool's
test is a snapshot of the nodes it produces at a given input.

## 10. Why this is the right axis for different professions

The proposal's premise is that architects, engineers and designers need
different tools. True, but the useful version is sharper. All three want lines,
rectangles, curves and text. What differs is **where precision comes from**:

| | Precision comes from | Consequences for the tool |
|---|---|---|
| **Architect** (Rhino, AutoCAD, Revit) | **typed magnitude** — the mouse sets direction, the keyboard sets distance | sticky modal commands, Enter/Space repeats, ortho and object snaps as persistent *world* state rather than per-tool options, real-world units and drawing scale, layers as discipline codes, annotation-heavy output |
| **Engineer, mechanical** (SolidWorks, Grasshopper) | **relationships that survive editing** — constraints drive geometry, dimensions are inputs not readouts | under-constrained is a first-class UI state; the *definition* is the artifact and geometry is its output; history-based edit |
| **Engineer, software** | **extraction** — the source is text or a graph, the layout is computed | deterministic, diffable, regenerated rather than drawn |
| **Designer** (Illustrator, Figma) | **the trained eye**, assisted | nudge and align rather than type; named shared styles and components as reusable objects; pathfinder booleans, masks, blend modes; artboards as pages |

Now read that against what a kit can carry:

- Typed magnitude is a **grammar** (`TwoPoint` already implements it — the Line
  tool's typed length and Tab direction lock are the architect's grammar,
  already shipped and already contracted). Kits reuse it freely. ✅
- Units, scale, snap defaults, readout vocabulary, style sources, symbol
  libraries: **all data**. ✅
- Extraction is **portals** — already the plan, already Phase 3. ✅
- Constraints are **core work, not data**. A solver cannot be a declarative
  asset. `docs/keymap/specs/constraints.md` already speaks to this. ❌

So kits serve the architect and the designer almost completely, serve the
software engineer through portals, and serve the mechanical engineer only
partially. Worth saying plainly rather than promising everything: **the
constraint intuition is the one profession-shaped hole kits cannot fill.**

The resolution of the stated anxiety follows directly: you no longer need any
*tool* to be universal. You need the **grammars** to be universal — nine of
them, each one a real interaction contract worth getting right — and every tool
above that line is allowed to be parochial, personal, or wrong.

## 11. Constitutional position

Nothing here needs the VII.4 script amendment, because nothing here is
interpreted. One clarifying amendment is worth ratifying, because VII.3's list
of permitted declarative assets is the clause the whole scheme stands on and it
does not currently name tools:

> **Draft amendment — Article VII.3.** Agents extend the user's workspace with
> **data, not code**: brushes, palettes, dashboards, templates, **and tools**
> are declarative user-space assets interpreted by the core — they cannot
> corrupt it. **A declarative tool binds a core-provided gesture grammar to a
> result recipe and a presentation; it may not define gesture grammars, node
> kinds, or style properties.**

The second sentence is the load-bearing one: it writes §4 and §8 into law, so
that the next agent asked for "just a small script hook in a kit" is refused by
an article rather than by taste.

Other articles: **III** relocates rather than relaxes (§4). **IV** is protected
by non-goal 3. **VI** holds because a kit's only output path is `SceneCmd`.
**IX** governs kit links and packaging (§7). **X** is satisfied by putting the
kit model in a pure crate and the bar rendering in `atlas-shell`, so File Atlas
inherits the capability and simply ships no kits today — see §12.

## 12. Honest cost

Not free. The invasive parts, in descending order:

1. **The command registry must become dynamic.** `Registry` holds
   `&'static [CommandSpec]`, and `CommandSpec` is `Copy` with `&'static str`
   fields throughout. Kit tools must be registered commands (Art. VII.1), so
   the registry needs an owned runtime tier alongside the static table. This is
   the largest single change — and it is needed for Wave 2's `atlas-mcp` and
   for agent-authored assets regardless, so kits are not paying for it alone.
2. **`DockItem.id` is `&'static str`** and `DockIcon::Custom` takes a Rust
   `fn` pointer. Both must accept owned/data-driven forms. Icons: start with a
   named built-in glyph library; SVG path data rendered through `vector-ink` is
   the natural second step and costs no new dependency, since the SVG ceiling
   is already law.
3. **Hotkey and id collision.** Two kits binding `W` needs a deterministic
   resolution order and a visible conflict state in the Advanced window. Kit
   command ids need namespacing: `kit.<kit-id>.<tool-id>`.
4. **The grammar/recipe refactor** in `board.rs` (§4).
5. **`SlateDoc` gains `kits`**, which is a `format_version` bump and a fixture
   test. Note it inherits the existing gap that doc-level (non-scene) state has
   no journal; kit attachment should become journaled when that lands.

Two further recommendations that cost little and derisk a lot:

- **Ship the built-in toolbar as a kit.** Compile the default kit in with
  `include_str!`, the way `ui-tokens.toml` is embedded. It guarantees the
  format is expressive enough by construction, guarantees no privileged path
  that user kits cannot reach, and gives the loader its first real test.
- **Add capture-as-tool before any kit editor UI.** "Save this brush as a
  tool" and "save selection as a stamp" write a kit entry from live state. Two
  commands, no editor, and they deliver most of the authoring value — you get
  a tool by making the thing once and naming it.

## 13. Sequencing

| Step | Depends on | Delivers |
|---|---|---|
| 1. Grammar/recipe refactor in `board.rs` | nothing | a smaller tool match; recipes become inspectable structs |
| 2. `crates/atlas-kit` — pure model, loader, resolver | 1 | a kit can be parsed and validated; `cargo xtask kits` |
| 3. Built-in toolbar as an embedded kit | 2 | format proven; no privileged path |
| 4. Dynamic command tier + dock ownership changes | 2 | kit tools reach the palette, keyboard, and reference window |
| 5. User-scope kits from `data_dir()/kits/`, hot-reloaded | 4 | the fast personal-experiment loop |
| 6. Capture-as-tool commands | 5 | authoring without an editor |
| 7. Workbook-scope kit links + packaging | 5 | sharing; drag-and-drop onto a document |
| 8. `stamp` recipes | 5 | symbol libraries — the profession-shaped payload |
| 9. `portal` recipes | Phase 3 | preset dashboards |

Steps 1–3 are useful on their own and commit to nothing: if kits are abandoned
after step 3, the core is left simpler than it started.

## 14. Open questions for the user

1. **Scope precedence** — when a user kit and a workbook kit bind the same key,
   which wins? (Recommendation: workbook wins, because it is the more specific
   context, and the conflict is shown rather than silent.)
2. **Does a kit dropped on a document attach to the workbook, or install for
   the user?** The proposal says workbook; a modifier or a prompt could offer
   both. Needs deciding before the drop handler is written.
3. **One extension or two** (`.slatekit` holding both, versus separate tool and
   bar files). Recommendation: one.
4. **Does this wait for Amendment D?** Recommendation: no. Kits supply content;
   Amendment D governs slots. Keeping bar layout deliberately thin — an ordered
   list of tool ids with flyout grouping — means ratifying Amendment D later
   subsumes it cleanly instead of colliding with it.
5. **Should Atlas host kits?** Not on day one, but the model belongs in a pure
   crate and the rendering in `atlas-shell` so that it can, without divergence.
