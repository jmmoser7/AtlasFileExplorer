# Extending Atlas and Slate

You have an idea. This document tells you, in about ten minutes and without
your having read anything else, whether it is a weekend or a rewrite — and
where in the tree it lands.

The project's governing document is [`CONSTITUTION.md`](CONSTITUTION.md).
Everything below is a practical consequence of it; where the two disagree, the
constitution wins and this file is wrong. Two other files are worth knowing
about before you start:

- [`docs/false-affordances.md`](docs/false-affordances.md) — features refused
  because the architecture would make them lies. Check it first. A "no" there
  is structural, and arguing with it is wasted effort.
- [`docs/audit/deviations.md`](docs/audit/deviations.md) — the places where the
  code does not yet match the constitution, tracked openly. If your idea lands
  on one of those rows, say so; you may be fixing two things at once.

The licence is MIT. Nothing here is a claim on what you may build — it is a
description of what this repository will merge.

---

## 1. Triage in one table

Find the row that matches your idea. The classes are defined in §2, and each
one has a worked example drawn from a request that actually happened.

| Your idea… | Class | Realistic cost | Where the conversation goes |
|---|---|---|---|
| changes a number, a spacing, a colour | **Token** | minutes | just do it |
| adds one more of something the system already handles | **Leaf** | a day or two | issue, then a PR |
| adds a new *kind of thing* to a document | **Variant** | a week or more, two renderers, a file-format migration | issue first; expect design pushback |
| adds a new way of working with material | **Organ** | weeks, and it must name a real recurring use | issue first; [Article III](CONSTITUTION.md#article-iii--the-10-rule) is the bar |
| replaces something the rest of the system stands on | **Transplant** | months; a plan before a line of code | discussion, not a PR |
| changes what the product *is* | **Different Animal** | fork it — see §6 | we will help you cut it cleanly |

Two rules of thumb that predict most of the cost:

1. **If both the on-screen painter and the HTML exporter have to learn
   something new, you are at Variant or above.** The board's egui painter
   (`apps/slate/src/app/board.rs`) and the artifact writer
   (`crates/slate-artifact`) are two interpreters of one model, and
   [Article IV](CONSTITUTION.md#article-iv--honest-models) says a property
   lands in both or in neither.
2. **If your feature needs a new option that nobody has asked for, delete the
   option.** [Article III](CONSTITUTION.md#article-iii--the-10-rule) — every
   capability implements the deliberately chosen fraction of its domain that
   somebody actually reaches for. "Photoshop has it" is not an argument here,
   and saying it will cost you a round trip.

---

## 2. The six flexibility classes

### Token — a value in a table somebody already owns

Nothing learns a new concept. You are changing a number in a file that is read
at startup.

**Worked example (real request): "the tabs in the top bar are too tall and the
glow is too strong."** Both live in `crates/atlas-shell/ui-tokens.toml`:

```toml
[topbar]
height = 31.0
tab_top_inset = 6.0
```

The glow is four more keys in the same table. Run either app with
`--features ui-tuner`, drag the sliders, save; the saved values are embedded on
the next build. `docs/ui-tuning-workflow.md` describes
how to add a new tunable area.

**Cost:** minutes, and no design conversation. **What you should know:** the
chrome is shared, so your change lands in both applications at once — that is
[Article X](CONSTITUTION.md#article-x--no-chrome-divergence) working as
intended, not a bug. Colours are the exception today: the fourteen semantic
palette slots are still hardcoded Rust constructors in
`crates/atlas-shell/src/theme.rs`, so a colour change is a one-line code edit
rather than a token edit. Moving them into the same TOML file is scheduled
work, and until it lands, a colour is a Leaf, not a Token.

### Leaf — one more of something the system already handles

The model does not change. You are adding a new case at the end of a dispatch
that already exists, and everything downstream picks it up for free.

**Worked example (real request): "Rhino `.3dm` files should show a preview even
on machines where Rhino is not installed."** The answer was
`crates/atlas-core/src/threedm.rs`: a small parser that pulls the preview
bitmap Rhino already embeds in the file, slotted into the thumbnail fallback
chain beside `office.rs` (Office Open XML embedded thumbnails) and `pdf.rs`
(pdfium page rendering). No document type changed, no export changed, and the
thumbnails appear anywhere thumbnails appear because everything reads the same
cache.

**Cost:** a day or two. **What you should know:** the fallback chain is where
new formats belong. A new *format* is routine; a new *facet* — a new family of
tools that a class of files unlocks — is not, and is described in
`docs/facet-taxonomy.md` as a constitutional-scale decision.

### Variant — a new kind of thing inside a document

Now the document model changes, which means every interpreter of that model
changes with it, and files written by older builds must still open.

**Worked example (real request): "add a line tool like the one in Rhino."**
What that actually cost: parametric two-point geometry, caps, joins, dash, and
width profiles in `crates/slate-doc/src/scene.rs`; a new interaction in
`apps/slate/src/app/board_line.rs` with its behaviour written down first as a
contract in `docs/keymap/contracts/`; stroke outlining in `crates/vector-ink`
so a variable-width stroke can be a filled outline; `<path d="…">` emission in
`crates/slate-artifact`; commands registered in `apps/slate/src/app/commands.rs`
so the binding shows up in the reference window; and
`migrate_legacy_lines()` in `crates/slate-doc/src/doc.rs`, which converts
bounding-box lines written by earlier builds into the new parametric form on
load.

**Cost:** a week or more, three or four files minimum, and the migration is not
optional. **What you should know:**

- Anything you add to the scene must be expressible in SVG (including CSS).
  That is the styling ceiling in
  [Article IV](CONSTITUTION.md#article-iv--honest-models); a property no web
  standard can express cannot be exported honestly, so it does not go in.
- Every mutation must be a named, invertible, journaled command carrying its
  author — see [Article VI](CONSTITUTION.md#article-vi--journal-only-mutation)
  and `SceneCmd` / `SceneJournal` in `scene.rs`. UI code never mutates a
  document directly.
- Bumping the workbook's `format_version` ships with a test that loads a
  verbatim JSON fixture of the previous version.

### Organ — a new capability

A subsystem with its own crate, its own commands, its own tests. The core does
not change; the application grows a lane.

**Worked example (shipped): "show me the dependency structure of a codebase
inside a workbook."** That is the Lens. `crates/code-lens` extracts a graph
from Cargo manifests and Rust source and computes a semantic-zoom layout, with
no renderer dependency at all; `apps/slate/src/app/lens.rs` paints it on the
shared camera and runs analysis on a background thread; agents contribute
cluster *labels* through the file contract in `docs/lens-agent-contract.md`,
and nothing else.

**Cost:** weeks, and the split is lopsided in a way worth knowing before you
start — the pure crate is the cheap, testable, satisfying half, and the
painting and interaction half is where the time actually goes (`lens.rs` is
about 63 KB of it). **What you should know:** an organ has to name its real,
recurring use before it is built. Capabilities also may not reach into each
other; they compose through the core's contracts — commands, facets, portals,
sources ([Article I](CONSTITUTION.md#article-i--the-minimal-core)). See §5 for
the contract a new capability crate signs.

### Transplant — replacing something the rest of the system stands on

The interfaces survive; the implementation does not.

**Worked example (named in the constitution itself): replacing egui with a GPU
vector renderer such as Vello.** Article I calls this the substrate hedge, and
the hedge is real but narrower than it sounds. What ports unchanged: every pure
crate — `slate-doc`, `circle-pack`, `code-lens`, `rhino-mesh`, `vector-ink`,
`atlas-commands` — because none of them names a renderer type. What gets
rewritten: all of `crates/atlas-shell` and everything under `apps/*/src/app/`.
That is the honest number, and it is large: `apps/slate/src/app/board.rs` alone
is roughly 200 KB of egui painting and gesture code, and the `Camera` type
lives in the app with an `egui::Vec2` inside it.

**Cost:** months, and the correct first artifact is a written plan, not a
branch. **What you should know:** the hedge protects the models and the
contracts, not the paint code. Anything that keeps logic out of the paint layer
makes the eventual transplant cheaper, which is why Article I is enforced so
literally on new crates.

### Different Animal — a different product wearing this one's clothes

The request is coherent, and it is not this program. The right answer is a
fork, and §6 names the seams to cut along.

**Worked example (real request): "run this on a Quest — I want to walk around
the board in VR."** Everything about that is reasonable, and none of it is a
change to a 2D canvas application: the geometry, the camera, hit-testing,
snapping, the artifact writer, and the entire chrome layer assume a plane.

**Cost:** a fork, which under MIT costs you nothing in permission and quite a
lot in maintenance. **What you should know:** we would rather help you cut at a
seam we keep clean than watch a good idea die in an issue thread.

---

## 3. The open/closed enum rule

> **Close an enum where an agent must enumerate it. Open an enum where a human
> must express themselves through it.**

A closed enum is a contract: an agent, an exporter, or a remote peer can
enumerate every case and know it has handled all of them, and exhaustive
`match` arms make the compiler enforce that. An open set is a vocabulary:
adding a member must not break anything, because humans will keep inventing
members forever.

| Closed — adding a member is a Variant | Open — adding a member is a Token or a Leaf |
|---|---|
| `NodeKind` — what a scene node can be | `FontChoice` |
| `EdgeRole` — what a typed edge means | `Dash`, stroke caps, joins, corner profiles, width profiles |
| `Facet` — what a file can do | `EdgeRouting` |
| `CommandId` — the command surface | theme names |
| `SourceKind` — where material comes from | |

Which of these exist today, so you know what you are reading:

- **`NodeKind`** (`crates/slate-doc/src/scene.rs`) — five variants: frame,
  image, shape, text, connector.
- **`CommandId`** (`crates/atlas-commands/src/spec.rs`) — a newtype over a
  static string. It is closed in practice rather than in the type system: each
  application declares one static `SPECS` table and every consumer (keyboard
  dispatch, palette, menus, the reference window, the future agent surface)
  reads that one registry, so none of them can disagree about what exists.
- **`Dash`, `FontChoice`, `StrokeCap`, `StrokeJoin`, `WidthProfile`** — all in
  `scene.rs` today, all small, all intended to grow.
- **`EdgeRole`, `Facet`, `SourceKind`, `EdgeRouting`** — **not written yet.**
  They are named here because the rule that will govern them is already
  decided. Today, connectors are a `NodeKind` variant rather than edges,
  `MediaKind` (`crates/slate-doc/src/media.rs`) is the closed classification
  that both renderers agree on, `docs/facet-taxonomy.md` is the plan that
  replaces it, and links are plain absolute paths.

One pattern worth copying when you close an enum that is written to disk:
`ViewKind` (`crates/slate-doc/src/view.rs`) carries a `#[serde(other)]`
`Unknown` variant, so a view kind an older build has never heard of falls back
to Grid instead of failing the load. Closed for logic, tolerant at the file
boundary. (Note that this only softens *unknown values*; a whole workbook whose
`format_version` is newer than the build is still rejected outright.)

---

## 4. The extension ladder — MCP is the plugin API

There is a ladder of ways to extend the tool without touching this repository,
and the project's position on each rung is settled:

| Rung | What it is | Status |
|---|---|---|
| **Declarative assets** | themes, keymaps, brushes, palettes, board templates, portal definitions — data interpreted by the core | intended first tier; a package format and install path are decided but **not built**. Today the data-shaped surfaces that exist are `ui-tokens.toml` and the per-app settings files |
| **MCP servers** | out-of-process, language-agnostic, OS-sandboxed extensions driving the same command surface a human drives | intended first tier alongside assets; the command registry (`crates/atlas-commands`) exists, the MCP adapter **does not exist yet** |
| **In-process sandboxed code** (WASM) | a third-party tool with its own interaction, running inside the process | deferred, and it requires an amendment to [Article VII](CONSTITUTION.md#article-vii--command-parity-agent-native), which names this path explicitly. Until that amendment is ratified, agents proposing script execution are refused |
| **Native in-process binaries** | a `.dll` we load | **never.** Process corruption, ABI churn, platform lock-in |

The reframing worth internalising: **MCP is already the plugin API.** A
third-party "tool" for this program is an MCP server plus a bundle of
declarative assets. That is not a consolation prize — Article VII requires that
every human-performable action be a registered command and that the agent
surface expose those same commands, so an out-of-process extension is not
working around the application, it is using the same front door.

Agents extend a workspace with **data, not code**. That boundary is the reason
an agent cannot corrupt the core, and it is why the ladder stops where it does.

---

## 5. The capability-crate contract

If your idea is an Organ, this is the shape it takes.

**A new capability may:**

- own a crate under `crates/`, holding its model, its geometry, and its
  algorithms;
- register commands in its application's `commands.rs` `SPECS` table — that is
  how a binding reaches the palette, the keyboard dispatcher, and the reference
  window at once;
- read the document through the public API of `slate-doc`, and mutate it only
  through journaled commands;
- present itself as a view over the shared camera, following the pattern
  `apps/slate/src/app/ARCHITECTURE.md` describes: pure geometry in a crate, a
  layout builder producing placements, painting and hit-testing on the shared
  camera.

**A new capability may not:**

- **depend on `egui`, `eframe`, or any renderer** if it holds document, model,
  geometry, or capability logic (Article I). This is the rule most likely to
  bounce a PR. Apps are thin interpreters; the crate is where the durable
  thinking goes;
- **add node kinds or fields to the core scene model** — that makes you a
  Variant, and it needs its own conversation;
- **paint chrome, define chrome colours, or lay out sidebars and tabs.** All of
  that lives in `crates/atlas-shell` so that both applications inherit it
  identically (Article X). If you need a new chrome capability, extend
  `atlas-shell` in a dedicated change;
- **reach into another capability.** Compose through the core's contracts;
- **mutate a document outside a journaled path**, or allocate and tessellate
  per frame inside a paint path
  ([Article II](CONSTITUTION.md#article-ii--performance-is-a-feature) — heavy
  work is asynchronous and generation-tagged, geometry is tessellated once and
  cached).

**A note on portals**, because the word appears in that contract and is easy to
get wrong. [Article V](CONSTITUTION.md#article-v--one-universe-the-board-with-portals-for-views)
describes generated views as *portal* nodes: the frame is ordinary journaled
data, and the contents come from a source and a query. Three classes are
distinguished, and they answer two questions differently — which journal owns
mutations inside them, and what they serialise to on export:

| Class | Mutations belong to | Exports as |
|---|---|---|
| **Generated** (Lens, Grid, Venn) | nobody — contents are regenerated deterministically | the regenerated contents |
| **Document** (a nested workbook, a linked model) | the *child* document's own journal | the child, rendered |
| **Host** (a web surface) | the foreign application, entirely outside this program | a poster plus a pointer, and it says so |

Determinism is a requirement of the **generated** class, because an analysis
view may not invent anything (Article IV.2). It is not a property of portals in
general, and a host portal cannot have it. Note also that this taxonomy is a
design contract rather than shipped code: views today are still tab-level
(`ViewKind` in `crates/slate-doc/src/view.rs`), which Article V calls a legacy
form.

**Practical checklist before you open the PR:** the crate compiles and tests on
Linux even though Windows is the reference platform; platform-specific code
sits behind `#[cfg(windows)]` with a working stub; every user-facing binding is
in `SPECS`; every mutation is invertible and carries an author; and if the
capability exports anything, it serialises the same model the on-screen painter
reads. `AGENTS.md` has the day-to-day working rules and the build commands.

---

## 6. Published fork seams

Three recurring requests whose right answer is a fork. In each case the cut is
named, and so is what you get to keep. MIT means this is generosity, not
rejection — and if you fork at one of these seams, tell us, because a seam that
somebody is actually standing on is a seam we will keep clean.

**A VR or 3D canvas.** The cut is `WorldRect` and the camera. `WorldRect`
(`crates/slate-doc/src/scene.rs`) is four `f32`s on a plane, and the `Camera`
in `apps/slate/src/app/mod.rs` is a 2D offset plus a zoom; layout, snapping,
hit-testing, presentation, and the HTML artifact all assume that plane, so
there is no incremental path from here to a headset. **Reusable:** the tag and
item model, the journal with its authorship, `atlas-commands`, `vector-ink`,
`circle-pack`, `code-lens`, `rhino-mesh`, and the whole idea of an artifact
that is a serialisation rather than a screenshot. **Not reusable:** every
geometry type that hardcodes two dimensions, `atlas-shell`, the board painter,
and the artifact writer's coordinate emission.

**A build that embeds live OS application windows** — a real Excel or Rhino
instance parented over the canvas. This is refused in this repository, and the
reasons are not squeamishness: a reparented foreign window cannot rotate, does
not zoom smoothly, occludes incorrectly, cannot be exported, and is invisible
to anyone joining a shared session. Each of those breaks a promise made
elsewhere. **The cut is `NodeKind`** — one new variant whose payload is a
foreign window handle, one painter arm, one exporter arm that has to admit it
cannot serialise anything — and the result is Windows-only forever.
**Reusable:** everything up to that enum; the seam is narrow enough that a fork
can track upstream almost indefinitely. **Not reusable:** honest export and any
multi-participant story. If what you actually want is tool-to-tool automation,
the intended path is an MCP server for that application plus a document portal
on its file, and that costs you nothing in fidelity.

**A hosted two-hundred-person whiteboard service.** The cut is the session
layer. Live collaboration here is designed around a self-hosted relay binary
that a firm runs on its own hardware, with server-authoritative ordering and
no accounts; a multi-tenant hosted service needs identity, authorisation,
quotas, durable storage, and a store of record — and
[Article IX](CONSTITUTION.md#article-ix--slate-is-a-linker-never-a-database)
forbids this program from quietly becoming a database. **Reusable:** the
document model, the journal, the command stream as a wire format once
commands are property-scoped, and the artifact writer as a perfectly good
read-only share-link renderer. **Not reusable:** the desktop assumption that
the `.slate` file on disk is the document, and the file-path-based link
resolution that goes with it.

---

## 7. How to propose something

Open an issue that says four things:

1. **Which class you think it is** (§1), and why.
2. **The real, recurring use.** Not the general capability — the specific thing
   you did last week and had to leave the tool to do. Article III is scored on
   this sentence.
3. **Which articles it touches**, if you know. Being wrong here costs nothing;
   not looking costs a round trip.
4. **What you are willing to build.**

Expect to be pushed back on with an article number rather than a matter of
taste. That is deliberate: it is written into
[Article XI](CONSTITUTION.md#article-xi--agent-conduct-and-amendment) that a
conflict with the constitution is named and explained rather than quietly
absorbed, and it applies to humans and agents alike. The constitution changes
only by explicit edit from the project owner — nobody else ratifies an
amendment, including on their own PR.
