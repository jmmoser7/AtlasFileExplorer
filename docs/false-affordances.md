# The false-affordance register

Features this project refuses **because the architecture would make them
lies**.

A false affordance is a feature that works in the demo and fails the person who
trusts it. A hidden region whose content still sits in the exported file. A
redaction rectangle with selectable text underneath it. An expiring link over
bytes the recipient already has on disk. Each of these has an implementation
that looks convincing in a pull request, and each of them ships a promise the
software cannot keep.

This register exists so that a good-faith request with a convincing-looking
implementation gets a real answer instead of a debate. The answers here are
blunt on purpose. Softening them would destroy the only thing the register is
for.

**What this register is not.** It is not a list of everything the project has
declined. Things refused for scope reasons — because nobody actually needs them
weekly — are governed by
[Article III](../CONSTITUTION.md#article-iii--the-10-rule) and recorded in
`docs/audit/2026-07-25-decisions.md` and
`docs/audit/2026-07-25-decisions-flexibility.md`. A row belongs here only when
the feature would *misinform its user*.

**The test for a new row.** Would a competent person, using the feature exactly
as labelled, end up believing something false about their own data? If yes, it
belongs here, with its reason. Rows change only when a binding decision changes
them — the scope of the write-back row below is set by decision D9, not by
preference.

**A refusal here is not a refusal of the need.** Every row names what the tool
does instead, or names honestly that it does nothing and where the real answer
lives.

---

| # | Refused | One-line reason |
|---|---|---|
| 1 | Permission-hidden or encrypted board regions | The content is in the file. Hiding it at paint time is theatre. |
| 2 | Legal redaction | A rectangle over text leaves the text selectable; a crop ships the whole original. |
| 3 | DRM, view-limited or expiring artifacts | The bytes are on the recipient's disk before any script of ours runs. |
| 4 | Hallucinated analysis graphs | An analysis view that invents an edge destroys the only reason to look at it. |
| 5 | Silent or agent-initiated write-back to source files | Overwriting an original is the one action the journal cannot undo. |

---

## 1. Permission-hidden or encrypted board regions

**The request.** "This frame is only visible to partners." "Encrypt this region
so the contractor can open the workbook but not see the fee schedule."

**The convincing implementation.** A `visible_to` field on a node, checked by
the painter and by the exporter. It demos perfectly.

**Why it is a lie.** A `.slate` workbook is pretty-printed JSON, written by
`SlateDoc::save_to` and read by anyone who holds the file. Whoever can open the
workbook has the fee schedule, whatever the painter chose to draw. Encrypting a
region inside a document the recipient can decrypt is the same statement with
more steps: the key has to travel with the file, or the region cannot be shown
to the people it *is* meant for. The planned collaboration model makes this
worse rather than better: a relay distributes deltas to every connected peer.

The board's existing `hidden` flag is not a counter-example and must not be
sold as one. It is an authoring visibility toggle. The exporter honours it
(`hidden_nodes_are_excluded_from_export` in `crates/slate-artifact`), which is
an honesty property of *export*, not an access control: anybody with the file
can flip the flag back.

**What we do instead.** Access control belongs to the thing that stores the
bytes: separate workbooks for separate audiences, and filesystem or share
permissions on the files themselves. That is a boring answer that is actually
true.

## 2. Legal redaction

**The request.** "Draw a black box over the client's name, export, and send it
to the consultant."

**The convincing implementation.** A filled shape node on top, or a crop that
pushes the sensitive part out of frame. Both look right on the canvas and in
presentation mode.

**Why it is a lie.** The exported artifact is HTML, and the rectangle is a
`<div>` stacked over content that is still there:

- Text nodes export as live text, and file snippet cards export the excerpt
  inside a `<pre>` block. Under your black box, the text is selectable,
  copyable, searchable, and in the page source.
- A crop is CSS positioning, not a pixel operation. `crop_style` in
  `crates/slate-artifact/src/render.rs` computes something like
  `width:200%; left:-50%` and the *whole original file* is copied into the
  artifact's `assets/` folder (or inlined as a base64 data URI). Cropping
  something out of frame ships it anyway.
- Image adjustments are CSS filters. A blur that hides a face is a rendering
  instruction sitting next to the unmodified original.

None of this is a bug to be fixed. It falls straight out of
[Article IV](../CONSTITUTION.md#article-iv--honest-models): an export is a
serialisation of the model, not a re-photograph of the screen. That property is
worth far more than a redaction feature, and the two cannot both be true.

**What we do instead.** Nothing, and we say so. Redaction is destructive by
definition: the honest workflow is to produce a redacted file in a tool that
rewrites the bytes, and place *that* file in the workbook. If a
"flatten to raster on export" mode is ever built for other reasons, it will not
be labelled a redaction feature, because a rasterised page is a defence against
copy-paste and not against anyone determined.

## 3. DRM, view-limited or expiring artifacts

**The request.** "Recipients should be able to view the deck but not download
the images." "The link should stop working on Friday."

**The convincing implementation.** The exported HTML already carries a small
JavaScript slide runtime. Add a date check, disable right-click, block the
save shortcut, strip the download attribute.

**Why it is a lie.** The artifact is a folder: an HTML file plus copies of the
linked originals (`build_assets` in `crates/slate-artifact` performs a plain
file copy of every placed asset, or embeds it as a data URI). By the time any
script of ours executes, the recipient's disk already holds every byte. Every
client-side restriction in a file the client possesses is a request, not an
enforcement, and labelling it "protected" tells the sender something false
about a document they have already given away.

**What we do instead.** Do not send the artifact. Enforcement requires a server
that holds the content and streams it under an authorisation check — a
different product with a different threat model, and one of the published fork
seams in [`EXTENDING.md`](../EXTENDING.md). What the export *does* promise is
the opposite of DRM: what you send is what you have, in a form that will still
open in ten years without this program.

## 4. Hallucinated analysis graphs

**The request.** "The Lens should show the modules that *ought* to exist."
"Let the model fill in the dependencies the parser could not resolve." "Have
the agent add the missing edges."

**The convincing implementation.** The graph is already a data structure and an
agent is already connected to the workspace. Appending a few inferred nodes is
a dozen lines.

**Why it is a lie.** Those dozen lines destroy the only property that makes an
analysis view worth opening. A reader cannot tell an extracted edge from a
plausible one by looking at it, so a single inferred edge devalues every real
edge on the canvas. This is written into the constitution as
[Article IV.2](../CONSTITUTION.md#article-iv--honest-models): analysis views
present only what deterministic extraction found. `crates/code-lens` extracts
the graph from Cargo manifests and Rust source, and agents may contribute
*labels* for clusters through `docs/lens-agent-contract.md` — deliberately the
only contribution they can make, and it is matched against nodes that
extraction already produced.

**A distinction that matters, so nobody cites this row wrongly.** A generated
image that a human placed on a board is authored content, no different from a
photograph, and is permitted (decision D16); the intent is that such an image
carries its provenance. The prohibition is on *analysis* inventing structure,
not on generated material existing.

**What we do instead.** Show the gap. What extraction could not resolve is a
fact about the codebase and belongs on screen as a gap, not filled in with a
guess. If the graph is too sparse to be useful, the fix is a better extractor.

## 5. Silent or agent-initiated write-back to source files

Read this row carefully, because the line is narrower than it first looks.

**Write-back to an original is permitted** when a human explicitly asks for it,
per action, in a command whose confirmation names the file it will overwrite,
with provenance recorded in the journal. That is decision D9. The safe path —
writing a derived copy beside the original — is the default the command offers;
true overwrite is the opt-in inside it. The default behaviour of the tool is
unchanged and stays unchanged forever: crops, filters, and adjustments live in
the workbook and never touch the material they point at.

**What this register forbids is every other shape of the same operation:**

- **Silent write-back.** A background setting, a "keep originals in sync"
  checkbox, a save path that quietly rewrites the sources a workbook links to.
  Anything that makes it possible for a user to modify their own originals
  without, at that moment, being told which file is about to change.
- **Agent-initiated write-back.** An agent may not invoke it at all. Not with
  confirmation, not with a permission flag, not as part of a larger batch it
  proposed.

**Why.** Everything else in this system is reversible, and that reversibility is
structural: mutations are invertible journaled commands
([Article VI](../CONSTITUTION.md#article-vi--journal-only-mutation)), and an
agent's work is inspectable and undoable through the same journal
([Article VII](../CONSTITUTION.md#article-vii--command-parity-agent-native)).
A journal can invert a document command. It cannot restore bytes that were
overwritten outside the document. A model misreading an instruction and
overwriting a client's original is the single action available here that has no
undo, and no confirmation dialog placed in front of an automated caller changes
that.

**Where the code stands today.** No write-back of any kind exists. File Atlas's
export engine only ever copies into a destination, and its undo removes only
the copies it made (`crates/atlas-core/src/export.rs`). Slate writes its own
workbook and its exported artifacts, and nothing else — never the material it
links to. When write-back is built, it lands as a named command
under the constraints above; the choice between per-action confirmation and a
per-workbook permission is not yet settled, and nothing may implement the
feature before it is.

---

## Adding a row

Open an issue with the request as it was actually phrased, the implementation
that looks like it would work, and the sentence a user would end up believing
that is not true. That third part is the whole test. If it is missing, the
refusal probably belongs in a scope discussion instead, which is what
[`EXTENDING.md`](../EXTENDING.md) and the decision records are for.
