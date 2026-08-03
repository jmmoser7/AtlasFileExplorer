# User-authored tools — the contract loop

**Status: proposal, not adopted.** Nothing here is built. The constitution is
not amended by this document; §13 drafts the one clause that would need
ratifying, for the user to accept or reject.

Written against `5a9ea0b` (dock UX, condensed palette, timeline/heat, portal
groundwork). That commit moved four things this proposal depends on, and §15
lists them; the body already reflects them.

---

## 1. The thesis

The product is not a file format. It is a **loop**:

> Right-click a tool in the dock → the contract opens on the canvas, pre-filled
> from that tool → change what you care about → **Create** → an agent compiles
> it → the tool is in your bar, in every future session.

The file format is what the loop writes down. It matters, but it is substrate.
The loop is the thing that makes tool creation democratic, and every design
decision below is judged by whether it keeps the loop honest and fast.

Two properties make this more than a macro recorder:

- **The entry point pre-constrains the answer.** The dock now carries six
  groups — Frame, Portals, Shapes, Text, Object properties, Document
  settings — and right-clicking one of the first three decides the gesture
  grammar before the form opens. The contract arrives with most rows answered
  and the result is nearly always something the core can actually build. You
  are not filling in a blank form; you are editing a working tool's
  description.
- **Every Create produces something honest.** Either a live tool, or a
  well-formed request for a capability the core does not have — never a broken
  tool and never a lie. §3 is how.

The value is sharpest read against the loop that exists today. `docs/dev-loop.md`
is explicit that the new `bacon` workflow is "auto rebuild + relaunch, not
in-process hot-patching," that each save "kills the previous run" so board state
is lost, and that "board-tool feel constants still require a rebuild." Changing
a tool today costs a compile and your place on the canvas. Changing a kit tool
costs a file write.

---

## 2. The loop, end to end

| Step | What happens | Where it lands |
|---|---|---|
| 1 | Right-click a dock icon or a flyout entry | new shared-chrome capability (§10) |
| 2 | Choose *New tool from this…* or *Edit this tool…* | — |
| 3 | The **contract** opens, pre-filled from the source tool | a form over 17 typed rows (§4) |
| 4 | User accepts, alters, or rejects rows; the panel says continuously whether this is buildable (§3) | — |
| 5 | **Create** | writes `<tools>/contracts/<id>.md` + a decisions record |
| 6 | An agent compiles the contract | writes `<tools>/<id>.slatekit` |
| 7 | The tool appears in the bar, **staged** (Art. VII.6) — try it, accept or discard | `data_dir()/tools/` (§7) |

Step 6 is where the constitution does real work, and step 4 is where the
interface earns its keep.

## 3. The routing rule

This is the heart of the design and the thing that makes "anyone can make a
tool" a truthful claim rather than a marketing one.

A filled contract is either **expressible** or it is not:

- **Expressible** — every answer fits the kit vocabulary: an existing gesture
  grammar, an existing node kind, an existing style property. The agent emits
  a `.slatekit` file. The tool is live immediately, with no rebuild, because it
  is data (Art. VII.3).
- **Not expressible** — the contract asks for a gesture the core does not
  implement, a node kind that does not exist, or a style property outside the
  SVG ceiling. No amount of data will produce it. The agent writes the contract
  document and a proposal for the maintainer, and says plainly that this needs
  core work.

**The panel must show which branch you are on while you fill it in, not after
you press Create.** If row D03 is set outside the nine known grammars, the
Create button changes to *Propose*, and the panel says why. A user who wants a
constraint-solving tool should learn that in the form, from the form, in the
same breath as asking for it.

This is Article IV applied to an authoring interface: the tool builder does not
pretend to a capability it lacks. It is also what protects the loop's
reputation — the first time Create yields a tool that silently does nothing,
nobody uses it again.

## 4. What the contract actually edits

The contract interface edits **17 typed rows**, not free text.

The dimension registry (`docs/keymap/contracts/DIMENSIONS.md`) defines D01–D17
as the tool-scoped set — verified by `cargo xtask contracts`, which reports
"17 shared". Each row carries a proposal, a source (`stated` / `precedent` /
`pattern` / `research` / `guess`), a confidence score, and a verdict. That is
already a `ControlSurface` in the audit's §7.5 sense: a named set of typed
parameters with a layout hint.

**The markdown file is the serialization, not the source of truth.** This
matters practically: there is no markdown renderer or parser anywhere in the
dependency tree, and adding a round-trip markdown editor would mean the
contract slowly corrodes as the parser and the writer disagree. The existing
system already solved this — `decisions.json` holds the rows, `<tool>.md` holds
the human document, and `xtask` enforces that they agree. The portal writes
both, exactly as the skill does today. Article IV.1: exports are
serializations.

**Nobody fills in 17 rows.** With inherit-from-existing-tool (§5), nearly all
of them arrive answered at high confidence, and the ones worth a user's
attention are the low-confidence guesses — which the skill already surfaces as
"open questions". The real interaction is *"here are 17 answers, three are
guesses, change what you like"*, and that is the difference between a feature
people use and a form people abandon.

**A useful deletion:** the `tool-contract` skill currently produces a volatile
`.canvas.tsx` matrix, disposable after agreement, with a "send decisions to
agent" button. The contract portal replaces it. One less artifact, one less
handoff, and the skill's step 3 collapses into the app.

## 5. Inheritance is a fork that names its origin

"Right-click an existing tool to inherit its attributes" is the best idea in
the proposal, and it needs one decision: is the derived tool **linked** to its
parent, or a **snapshot**?

Snapshot. Article IX.4 already settles the analogous question for packages —
"a package is a permanent fork, not a synchronised mirror… a package records
where each asset came from, so that a human can always find the original." The
same reasoning applies with more force here: live inheritance chains among
user-authored tools would make a shared kit non-self-contained and would turn
"why did my tool change?" into an unanswerable question.

So a derived tool records `derived_from = "core:line"` and copies the rest.
The contract document records the same in its provenance line, which makes the
lineage of a user's toolbar readable.

## 6. Editing a built-in: shadow, never mutate

"All tools can be edited or overwritten in this way" should include the ones
that ship with the app — and it can, without the app ever writing to its own
install.

If the default toolbar is itself a kit compiled in with `include_str!` (the way
`ui-tokens.toml` already is), then "edit the Line tool" forks the built-in
entry into a user kit that **shadows** it by id. The built-in file is never
touched. Three things follow for free: *revert to default* always works, an app
update can ship a fix to the built-in without clobbering user edits, and the
set of things a user has changed is exactly the list of files in their tools
folder.

This also makes the kit format honest by construction: the defaults are
expressed in the same vocabulary users get, so there is no privileged path a
user tool cannot reach.

## 7. Where the files live

Not the install directory. On Windows that is `Program Files`, which is not
user-writable without elevation, is shared across accounts, and is subject to
being replaced wholesale by an installer. Tools written there would need
elevation to create and would vanish on update.

The repository already has the right convention, with twelve consumers:
`atlas_core::index::data_dir()` → `%LOCALAPPDATA%\NativeFileAtlas\`. Themes are
the exact precedent — `atlas_shell::theme::user_theme_dir()` is
`data_dir()/themes`, and dropping a `.toml` in it is how a user adds a theme
today.

```
%LOCALAPPDATA%\NativeFileAtlas\
    tools\
        contracts\
            my-north-arrow.md          <- the contract, human-readable
            my-north-arrow.json        <- the decision rows, machine-readable
        my-north-arrow.slatekit        <- the compiled tool
        stamps\
            north-arrow.nodes.json     <- assets, relative locators (Art. IX.2)
```

The intent behind "root directory" is right and worth keeping: **one well-known
folder the user can open, read, back up, and mail.** That deserves an *Open
tools folder* command so it is discoverable rather than folklore.

## 8. Precedent becomes two-tiered

A consequence worth designing for rather than discovering.

`decisions.json` today accumulates the maintainer's approved decisions, and the
skill seeds new tools from them at 85–95 confidence. In this loop, **every end
user starts accumulating their own precedent** — and their fifth tool is much
faster to author than their first, because their own accepted rows pre-fill it.
That compounding is one of the strongest arguments for the whole design.

It only works if the two tiers stay separate. Repo precedent is shipped and
reviewed; local precedent is yours. Local rows seed local proposals at high
confidence and must never silently merge upward into the repository's shared
database. The skill's source vocabulary already distinguishes `precedent` from
`guess`, so this is a scoping rule rather than new machinery.

## 9. The substrate the loop needs

The loop cannot pre-fill a contract from an existing tool, or compile one into
data, unless tools are made of parts that can be copied and written down. Today
they are not: `BoardTool` is an **eighteen**-variant enum in the app crate, and
the defaults are hardcoded inline.

The Repository Lens portal, added in `5a9ea0b`, is the cleanest possible
demonstration — and it is dated evidence rather than an argument. It is a
genuinely new node kind, and it reused an existing gesture **unchanged**:

- Its contract declares `Inherits: … P2.DragShape` — the same archetype as
  Frame, Rect, and Ellipse.
- In `end_gesture` it falls through to the `BoardDrag::Draw` catch-all, then
  takes a click-to-place default exactly the way Frame does:
  `else if tool == BoardTool::RepoLens && !moved { self.place_repo_lens_at(…) }`.
- Its only real novelty is *what the completed gesture produces*
  (`PortalNode::unbound_repo_lens`) and its presentation.

And it cost **eighteen edit sites across five files** — `board.rs`,
`board_portal.rs`, `board_icons.rs`, `dispatch.rs`, `ui/tools.rs` — plus an
enum variant and arms in `label()`, `tool_icon()`, and `hotkey()`. A tool whose
grammar already existed still paid the full structural tax.

The same fusion shows in `finish_draw`, where the Rect and Ellipse arms are
identical but for one enum field and a block of style constants. There is
nothing here to inherit *from*: "inherit the Rect tool's attributes" would mean
transcribing a match arm by hand, eighteen times over.

So the enabling refactor is to split what is currently fused:

```
tool = grammar (code, closed, 9 members)
     × recipe (data: what the gesture produces)
     × presentation (data: name, icon, key, aliases)
     × availability (data: view / facet / selection)
```

Nine grammars cover all eighteen of today's tools: `Select`, `DirectSelect`,
`DragRect` (Frame, Rect, Ellipse, **RepoLens**), `TwoPoint` (Line —
click-move-click or drag, direction lock, typed length), `MultiPoint`
(Polyline, Arc, Bezier), `Freehand` (Pen, Brush), `PlacePoint` (Text, Sticky),
`Sweep` (Eraser), and `Sample` (Eyedropper). `Pan` is a camera mode, not a
creation grammar. That an eighteenth tool arrived needing no tenth grammar is
the load-bearing evidence for the split.

Recipes come in three kinds:

- **`shape`** — a node of an existing kind with a style block.
- **`stamp`** — a saved group of nodes placed by the gesture: north arrows,
  scale bars, title blocks, door symbols. The highest-value kind and the one to
  ship first.
- **`portal`** — **no longer speculative.** `NodeKind::Portal` landed in
  `5a9ea0b` with `PortalNode { class, kind, title, source, query, fill }`, and
  the authored half is exactly the shape a recipe wants: a `SourceUri` and a
  `RepoPortalQuery { include_remotes, max_commits, axis }`, with extracted
  contents derived and never stored. A portal recipe presets `source` and
  `query`; five preset lenses on one repository become five kit entries.

Two limits the new code makes precise. `PortalKind` is a **closed enum in
`slate-doc`** (one variant, `RepoLens`), so a kit may preset an existing portal
kind and never introduce a new one — a new kind is core work, in both
interpreters. And `PortalClass` currently has only `Generated`, which matters
for §11.

This refactor also collapses the tool match in `begin_gesture` from eighteen
arms to nine, in a 4,900-line file. It is worth doing on its own merits.

**Article III is relocated, not repealed.** The 10% rule binds the grammar set
absolutely — a tenth grammar must name its real recurring use like anything
else. Kits are governed differently because an unused kit tool is not loaded,
not painted, and costs nothing.

### What a kit may never do

The non-goals are the specification, and they are what let an agent compile a
stranger's contract without risk. A kit may not: define a new gesture grammar;
contain control flow, expressions, or arithmetic (Art. VII.7); define a new
`NodeKind` or style property (Art. IV — a kit cannot teach `slate-artifact`
anything); mutate a document outside the journal; run at load time or do
anything without a human gesture; or touch the network.

The load-bearing consequence: **a kit tool produces only ordinary scene nodes.**
A missing kit costs you the ability to make more of them and costs the document
nothing. A `.slate` file must never become unrenderable because a third-party
file is absent.

## 10. Staging, authorship, and the dock

**Staging.** Article VII.6 already requires that agent mutations enter a staging
layer and await human acceptance. A compiled tool should therefore arrive
*staged*: visible in the bar, marked as proposed, tryable, acceptable or
discardable as a unit. That is exactly the right UX for step 7, and it is
already the law rather than a new invention. `crates/atlas-stage` is the
planned home (workplan T2.3).

**Authorship.** Two distinct authors must not be confused. The *kit file* is
authored by an agent from the user's decisions, and records that. The *scene
commits the tool later makes* are authored by the human wielding it, not by the
agent that wrote the tool. Article VI's author field is per-commit, so this
falls out correctly as long as nobody is tempted to attribute node creation to
the tool.

**The dock, and why right-click is the only gesture left.** `TOOLBARS.md` now
spends the icon gesture budget precisely: hover shows a title chip, linger
expands it to the description, **single click opens a volatile body**, **double
click pins**, and the minimize glyph dismisses or unpins. Every primary gesture
is spoken for.

Secondary click is not. `floating_dock` still returns `Option<&'static str>`
and has no `PointerButton::Secondary` path, so right-click is both free and the
*only* thing free — which turns it from one option among several into the
obvious one. Adding it is a shared-chrome change (Art. X: a dedicated task, not
mixed into app work) and needs a line in `TOOLBARS.md`.

The six groups are hardcoded arrays in `apps/slate/src/app/ui/tools.rs`, and
they become data under the kit format anyway. Worth noting that
`.cursor/rules/dock-chrome.mdc` still says a Tool or Dashboard "body preview
opens on-icon while hovering," which contradicts `TOOLBARS.md`'s "Hover →
title chip. No body." One of the two is stale; the rule file and the contract
should agree before either is used to specify right-click behaviour.

## 11. Sequencing, and one risk worth naming

The proposal puts the contract on the canvas as a portal. That is coherent under
Article V and it is genuinely the right long-term home. The dependency is now
much narrower than it was a week ago, and worth stating exactly.

`NodeKind::Portal` **exists** as of `5a9ea0b`, with placement, journaled
frame/source/query, async extraction, and focus/bake/refresh commands. So
"portal" is no longer a Phase-3 abstraction. But the contract interface would be
a **document** portal — the child file owns mutations (Art. V.3) — and
`PortalClass` today has exactly one variant, `Generated`. A generated portal
"owns no mutations" and regenerates deterministically, which is precisely what
an editable form is not. So what the contract-as-portal actually needs is
`PortalClass::Document` plus an editing path, not portals in general.

The second dependency is unchanged: the control-surface model is **Amendment D,
unratified and explicitly unplanned before ratification**. Making the contract
interface a portal means the authoring loop waits on both.

The loop does not need either. Recommended split:

| | Ships as | Depends on |
|---|---|---|
| **The loop** | a floating window over the canvas, using existing chrome | the §9 refactor only |
| **The presentation** | promoted to a canvas portal | `PortalClass::Document`, Amendment D |

Same rows, same file, same agent, same result. Promote the surface when the
substrate exists. This costs nothing later because the rows are the durable
model and the window is one interpreter of them — which is the same
one-model-several-interpreters move Articles IV and V already ratify.

### Order of work

1. **Grammar/recipe refactor** (§9) — smaller tool match; recipes become
   inspectable, copyable structs. Prerequisite for everything.
2. **`crates/atlas-kit`** — pure model, loader, resolver, `cargo xtask kits`.
3. **Built-in toolbar as an embedded kit** — proves the format; no privileged
   path.
4. **Dynamic command tier + dock ownership changes** (§12) — kit tools reach
   the palette, keyboard, and reference window.
5. **Contract window + Create** — the loop, minus the agent.
6. **Agent compilation + the routing rule** (§3) — the loop, complete.
7. **Right-click entry points and inheritance** (§5) — the loop, fast.
8. **`stamp` recipes** — symbol libraries, the profession-shaped payload.
9. **Contract as a canvas portal** — after Phase 3.

Steps 1–3 commit to nothing: abandoned there, the core is simpler than it
started.

## 12. Honest cost

1. **The command registry must become dynamic.** `Registry` holds
   `&'static [CommandSpec]`, and `CommandSpec` is `Copy` with `&'static str`
   fields throughout. Kit tools must be registered commands (Art. VII.1), so
   the registry needs an owned runtime tier beside the static table. This is
   the single largest change — and Wave 2's `atlas-mcp` needs it regardless.
2. **`DockItem.id` is `&'static str`, and `5a9ea0b` drove that deeper.**
   `DockState` now interns ids in `pinned: Vec<&'static str>`, `body_preview`,
   `label_hover`, and `panel_open: HashMap<&'static str, f32>`, so the
   `'static` assumption is threaded through the dock's persistent state rather
   than sitting only in the item list. `DockIcon::Custom` still takes a Rust
   `fn` pointer. Both need owned, data-driven forms; this cost went **up**.
   Icons: start with a named built-in glyph library, then SVG path data through
   `vector-ink`, which costs no new dependency since the SVG ceiling is already
   law.
3. **Hotkey collision is much smaller than it was**, because `5a9ea0b` shipped
   type-to-command: an unbound letter opens command entry pre-seeded, a bare
   letter chord waits out `BARE_LETTER_HOLD` in case a second character
   promotes it, and `Chord::is_bare_letter` exists to arbitrate. The new
   `RepoLens` tool ships with `hotkey()` returning `""` and is reached by name.

   So **a user tool's name is its primary invocation channel and a key is
   optional.** `aliases` becomes the field that matters, the palette becomes
   the discovery surface for user tools rather than the dock, and kit tools
   should default to *no* bare letter — bare letters are now a scarce resource
   that costs every user a hold window. Namespace kit commands as
   `kit.<kit-id>.<tool-id>`, and reserve conflict reporting for keys a user
   explicitly asks for.
4. **Dock right-click** (§10), as a dedicated shared-chrome task.
5. **The agent compiler** needs a schema strict enough to validate against and
   a refusal path (§3). This is prompt-and-schema work, not model work.

## 13. Constitutional position

Nothing in the loop is interpreted, so nothing here needs the Article VII.4
script amendment. One clarification is worth ratifying, because VII.3's list of
permitted declarative assets is the clause the whole design stands on and it
does not name tools:

> **Draft amendment — Article VII.3.** Agents extend the user's workspace with
> **data, not code**: brushes, palettes, dashboards, templates, **and tools**
> are declarative user-space assets interpreted by the core — they cannot
> corrupt it. **A declarative tool binds a core-provided gesture grammar to a
> result recipe and a presentation; it may not define gesture grammars, node
> kinds, or style properties.**

The second sentence is load-bearing: it writes the routing rule of §3 into law,
so that the next request for "just a small script hook in a kit" is refused by
an article rather than by taste.

Elsewhere: **III** relocates rather than relaxes (§9). **IV** is protected by
the non-goals and by §4's serialization rule. **VI** holds because a kit's only
output path is `SceneCmd`, and §10 keeps authorship straight. **VII.6** is
satisfied by staging. **IX.4**'s fork-and-name-your-origin rule is reused
verbatim for inheritance (§5). **X** is satisfied by putting the kit model in a
pure crate and all rendering in `atlas-shell`, so File Atlas inherits the
capability and simply ships no kits today.

## 14. Open questions

1. **What does Create do when the contract is not expressible?** §3 proposes
   the button becomes *Propose* and writes a maintainer-facing document. The
   alternative — refusing to let the user answer outside the vocabulary at
   all — is simpler but teaches nothing and makes the registry of unmet needs
   invisible.
2. **Is a user's accepted contract row precedent for their next tool only, or
   does it ever flow upstream?** §8 says local-only, silently never upward.
   Anything else needs a review path.
3. **Can a user edit a grammar's feel constants** (ortho angle, click-vs-drag
   threshold) per tool, or are those global? Per-tool is more powerful and
   makes two tools with the same grammar behave differently, which may be
   confusing rather than flexible.
4. **Does a shared tool bring its contract with it?** Recommendation: yes, both
   files travel — the contract is the tool's documentation and its diff.
5. **Should the contract window be modal?** It is a form that produces a file;
   non-modal lets a user try the parent tool while editing the child's
   contract, which is a genuinely useful thing to do.
6. **Does a kit tool get a bare letter at all?** §12 argues no by default, now
   that typing a name reaches everything and bare letters cost every user a
   hold window. If user tools may claim them, the resolution order against
   `SPECS` needs deciding.

## 15. What `5a9ea0b` changed for this proposal

Recorded so a later reader can tell which parts of the argument are dated
evidence and which are still projection.

| Landed | Effect here |
|---|---|
| `NodeKind::Portal` + `PortalNode` / `PortalClass::Generated` / `PortalKind::RepoLens` / `RepoPortalQuery` | `portal` recipes stop being speculative (§9). Two new hard limits: `PortalKind` is closed, so kits preset kinds and never add them; and the contract-as-portal needs `PortalClass::Document`, which does not exist (§11). |
| `BoardTool::RepoLens` — 18th variant, 18 edit sites, 5 files, reusing `P2.DragShape` unchanged | The grammar/recipe thesis stops being an inference about Rect and Ellipse and becomes a dated observation about the newest tool in the tree (§9). |
| Type-to-command: `Key::as_letter`, `Chord::is_bare_letter`, `BARE_LETTER_HOLD`, pre-seeded command entry — and `RepoLens` shipping with no hotkey | The largest reduction in the proposal's cost. A tool's *name* is its invocation channel, hotkeys become optional, and the collision problem shrinks to the keys a user explicitly requests (§12). |
| Dock condensed to six groups incl. **Portals**; `TOOLBARS.md` spends hover / linger / click / double-click / minimize | The right-click entry point survives and is now the only free gesture (§10). "Custom instance of *X* group" has a concrete six-item target list, one of which is Portals (§2). |
| `DockState` interning `&'static str` in `pinned` / `body_preview` / `label_hover` / `panel_open` | Cost item 2 went up: the `'static` id assumption is now in the dock's persistent state, not just its item list (§12). |
| `bacon` dev loop — "auto rebuild + relaunch, not in-process hot-patching"; board state lost per save; feel constants still need a rebuild | Sharpens the motivation: a tool tweak costs a compile and your place on the canvas; a kit tweak costs a file write (§1). |
| Portal contract now **agreed**, 31/31 rows approved; registry unchanged at 31 dimensions (17 tool-scoped) | §4's row count is re-verified against `cargo xtask contracts`, and the contract system is proven through a second, larger contract. |
