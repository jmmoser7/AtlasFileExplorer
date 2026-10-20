# Witnessed drawing and portal access

User instruction, 18 October 2026: make the collaborator visibly use Slate's
drawing tools, split complex work among temporary visible avatars, and inspect
Web/File Atlas portals through the interface. The existing approved face, hats,
chat chrome and canvas scale remain the visual authority.

The implementation extends registered targeted commands. Human gestures and
agents share result builders, join preparation, eraser math, WebHost and the
File Atlas session/folder-map painter. Temporary drawing avatars execute a
single multi-stroke command; they do not create model sessions. Authoritative
scene changes occur once at completion, preserving ordinary Undo/proposals.
Stamped prefixes use the human brush's viewport-sized live canvas, including
its variable tips and final raster handoff, rather than queuing a new bitmap
for every moving endpoint. The cache is bounded and is discarded on cancel.

## Acceptance paths

The 19 October native retest resumes the saved and recovered Workflow workbook.
It preserves the original deck and recovered work, completes the requested
eight new ordered 1600×900 research slides, retains readable Research/Drafts
evidence, and reuses the existing generated image and generator record without
starting a paid generation. The retained Atlas source-fixtures case reads the
exact returned `research-brief.txt`, `presentation-review.txt`, and
`drawing-probe.txt` locators; it does not edit those files.

- Draw pen, stamped brush and polyline components. Before commit, the scene and
  journal remain unchanged while visible path prefixes grow at the actual
  drawing contacts. Visit corners and dwell at polyline vertices. No pen-up ink.
- Draw an Arc using the human start/end/through picks, and trace rectangle,
  ellipse and polygon outlines from their shared geometry. Commit the exact
  editable parametric kit shapes after tracing, with no early filled result.
- Smooth vector/brush strokes through the shared bounded smoother. The swept
  prefix changes only reached work, repeated frames add no extra smoothing,
  and one Undo restores the original geometry and blur.
- For a complex batch, retain the parent and recruit only the bounded number
  of execution avatars. Helpers travel continuously from the parent to their
  assigned strokes. A completed batch is one attributed Undo/Redo operation.
- Pause or cancel mid-stroke. Stop advancing visible work immediately; cancel
  removes only ephemeral previews and retains completed unrelated work.
- Concurrent human target edits, locks, grant revocation or tab changes fence
  the operation before it can commit stale geometry.
- Colliding pending creations receive a recoverable wait/retry result before
  a second avatar can preview the same object IDs. A human creation that
  collides during a preview cancels that preview without ending the provider
  conversation or changing the human's object.
- Erase stamped strokes and vector paths using the human eraser result; show
  only the swept prefix. Deferred empty-stamp removal retains the same author
  and undo group, and never amends a later unrelated edit.
- Join eligible open paths/closed shapes through the ordinary join owner.
- Create an Atlas portal, await its existing enumeration, list bounded entries,
  then highlight and read one exact granted file. Keep human selection/camera.
  Refuse stale generations, traversal, out-of-root links and cloud placeholders.
- Create/open a consented Web portal and await its existing live host without
  repositioning the board. Read/capture/navigate/type/click/scroll/key through
  the portal. Human takeover releases the agent's focus; terminal requests and
  revocation remove only request-owned focus/highlights.
- Review actual production-render drawing frames at 50%, 100%, 200% zoom.
  Avatar/hat/prop scale with the canvas and use the approved face and hat fits.
- Resume the eight-slide research deck from the recovered workbook. Compare the
  original slides and evidence before and after; verify exact slide dimensions,
  styling, order, preview pixels, and saved workbook contents.
- Deny a Web portal origin, confirm the agent cannot read it, then use the
  human Allow site control for that exact origin and verify an actual text read.
- Resume inspection of the existing image generator and output. Verify status,
  output count, actual dimensions, and aspect from the retained result without
  starting another paid generation.
- Sweep the eraser through empty board space and verify the route remains
  responsive and crash-free, creates no empty ink, and leaves Undo behavior
  correct for the surrounding completed work.
- Exercise a roughly 2,953-byte composer message. The composer stays at or below
  96 px and scrolls internally; transcript Ctrl+A and native Copy select/copy
  transcript text without selecting board content, while ordinary composer
  shortcuts continue to work.

## Evidence boundaries

The [interactive evaluation document](acceptance.html) records each case as
Pass, Fail, Blocked or Untested, with notes and portable result export. Every
new build starts Untested; automated results never prefill native verdicts.

PR 63 records the implementation and automated checks. The retained 19 October
research workflow evidence is under `target/review/research-workflow-20261019`,
including the original workbook, recovery-preservation record, source fixtures,
and build/test logs. Software captures use the production board painter with
synthetic fixture content; they are not native pointer/GPU or human acceptance.
The latest available native executable is `target/release/slate.exe`, SHA256
`0E9764DB21121B206047E254DED6F25BCB748391809DD8A8B330853209C87983`,
50,848,256 bytes, built at `2026-10-20T01:06:07.0514501Z` from combined
source `344988a4`. Its actual research continuation and separate suggestion-box
fixture run were saved and closed normally. Earlier access violations were not
reproduced; no crash cause or fix is established.

The latest research Save preserves 284 nodes, all 74 original nodes and both
original linked items/tags exactly unchanged. Eight slide frames retain
1600×900 geometry and orders 11–18. All eight final slide previews were reviewed
on earlier source `722dc082`, with readable content and complete, zero-omission
metadata. Their evidence retains that build identity. Atlas analyzed-file
highlighting, exact file reads, tags, Smooth, Join and Erase were exercised in
the actual app. The failed count-two image request and its one recovered
1672×941 output remain visible; no additional paid generation was started.

On `344988a4`, actual Google pixels showed the typed query before Enter and
matching results afterward. A fresh read-only helper's full 1,199-byte public
result was paged under one generation, repairing the previous worker-scope
failure. Editable drawing variants, the parent avatar/hat and temporary helper
departures were observed. Timed captures show finished paths, not intermediate
stroke-prefix growth; production-animation tests provide separate logic evidence.
Compact chat, live public activity and board interaction were observed; the saved
workbook retains the conversation.
Ordinary and whole-transcript copy round trips are earlier-build evidence; the
latest clipboard probe was inconclusive.

Final production source `135a2b81` is pushed and awaits native delivery. Its
per-call immutable Web capture filenames, live URL replies, transcript ordering,
first-drag/empty-generator and image-save picker repairs require bounded native
rechecks. At 02:11 UTC, CI 193 passed Windows compilation and Linux Clippy;
Linux runtime and installer 171 were running. Earlier `f8507720` passed 3,928
Linux workspace tests with zero failures and 93 ignored. The local final test
did not run: Windows denied the new `rfd` build helper with `os error 5`.
Development approval is pending, and the existing build setup is unchanged.
PRs 63/64 are draft, unmerged; no final release has been published.

Image inspection uses bounded displayed-image PNGs rather than returning an
unchanged image's original file as provider input. Codex attaches those pixels
through its installed dynamic-tool response schema. The transport adds dependency
edges to the workspace's existing PNG decoder and already-locked base64 version;
it changes no package versions, features, toolchain, build profile or output path.
Tests of this adapter are protocol/transport evidence, separate from a live model
vision turn.

Google and EPA were individually allowed by the human before the explicit
global opt-in. Desktop availability is checked before native input; a mouse
jiggler is user-managed and does not establish that the desktop is unlocked.
The evaluation cases remain Untested until their recorded native steps are
completed for the identified build. Automated checks and partial native runs do
not prefill human verdicts or establish complete workflow acceptance.
