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

## Acceptance paths

- Draw pen, stamped brush and polyline components. Before commit, the scene and
  journal remain unchanged while visible path prefixes grow at the actual
  drawing contacts. Visit corners and dwell at polyline vertices. No pen-up ink.
- For a complex batch, retain the parent and recruit only the bounded number
  of execution avatars. Helpers travel continuously from the parent to their
  assigned strokes. A completed batch is one attributed Undo/Redo operation.
- Pause or cancel mid-stroke. Stop advancing visible work immediately; cancel
  removes only ephemeral previews and retains completed unrelated work.
- Concurrent human target edits, locks, grant revocation or tab changes fence
  the operation before it can commit stale geometry.
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

## Evidence boundaries

Unit and headless integration results will be recorded after execution.
Software captures use the production board painter with synthetic fixture
content; they are not native pointer/GPU or human acceptance. The user's open
Slate session is protected and must be safely closed before its executable can
be replaced. No source-write or security-policy exception is introduced.

Image inspection uses bounded displayed-image PNGs rather than returning an
unchanged image's original file as provider input. Codex attaches those pixels
through its installed dynamic-tool response schema. The transport adds dependency
edges to the workspace's existing PNG decoder and already-locked base64 version;
it changes no package versions, features, toolchain, build profile or output path.
Tests of this adapter are protocol/transport evidence, separate from a live model
vision turn.
