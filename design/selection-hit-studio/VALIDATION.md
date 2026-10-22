# Studio validation — 2026-10-20

Scope: the disposable browser studio. These results do not resolve native Slate
reports, approve a changed interaction contract, or establish native parity.

## Current pass: foreground priority and adjacent corner ties

- **58 focused checks pass**, zero failed, including eight new layering checks.
  Foreground field geometry is compared with the same object alone, and more
  than 1,000 winning probes verify its move/resize/rotate/fillet regions over a
  rear body. Other checks cover the rear move core, strict interior wire hits,
  compact top-object ports, contact versus positive-area overlap, rounded
  silhouette distance, and matching fast paint/input ownership.
- Coincident adjacent corners are checked on either side of the seam with both
  draw orders. Rotated adjacency is checked at 50%, 100%, 200%, and 400% zoom;
  these fixtures assert that the corners really remain coincident before testing
  ownership. Exact equal distances fall back to front order.
- Actual browser input: on Adjacent corner tie, a left-side press resized A
  180×120→175×138; a right-side press resized B to 165×138. On Overlap order,
  B's NW rotation region over A's body rotated B to 14.38°, and its outside NW
  scale region resized B 230×175→240×190. One Undo restored each gesture.
- At B's input port over A, an outside press chose B resize W, while an inside
  press chose B wire in. The interior wire drag completed and one Undo removed
  it. The buried-object move label now also avoids foreground transform regions.
- All browser JavaScript syntax checks pass. All 18 native owners still match
  2087cc17 / preview.175. No native source or global interaction contract changed.

Evidence: `preview-layer-priority.jpg` and `preview-adjacent-corners.jpg`.

## Previous pass: adaptive regions and overlap

The user's overlap feedback supersedes the previous protected-patch proposal.
Passing its centre probes did not establish usable overlapping-object behavior.
The new proposal retains semantic zoom while replacing reserve dots with full
edge bands, corner neighborhoods, rotation arcs, outward wire regions, and
compact-object frames. Cursor proximity expands reach; the same resolver
partitions competing regions for both input and the local colored preview.

- **50 focused checks pass**, zero failed: geometry, gesture, semantic
  comparison, and adaptive checks. The 13 new checks cover actual exposed-edge
  drags away from midpoint handles, foreground interiors over rear controls,
  continuous rotation arcs, buried selection and movement, nearby silhouette
  avoidance for its move label, cursor expansion, rotated edges/fillet, wire
  connection, and honest audit reporting. Old protected-point checks remain
  comparison evidence only. All browser JS passes syntax checking; all 18
  native owner hashes still match 2087cc17 / preview.175.
- Actual browser input in an isolated QA session: rear A's exposed south edge
  resized from a non-midpoint position, changing height 140→174.18; one Undo
  restored it. A drag over A's covered SE corner moved foreground B. Explicitly
  selecting coincident rear E revealed an outside frame; dragging its clear-side
  move label moved E and one Undo restored it. A→B wire drag completed once;
  one Undo removed it. Wheel input increased zoom from 100% to 128.27%.
- The mixed overlap scene's audit reported 75/79 targets with sampled winning
  area at 87.78%, naming four covered targets. This is an observation, **not a
  pass verdict**. Partially covered rear corners follow silhouette order;
  selecting a completely buried object exposes its outside frame. Sampling
  below 4px cannot establish exact reachability for very small regions.
- Browser JavaScript errors: none captured during these interactions. User
  tuning and authored scene are retained on migration to Adaptive regions.
  Native acceptance and global contract approval remain pending user iteration.

Evidence: `preview-adaptive-overlap.jpg`. Earlier screenshots and validation
sections below are retained as historical evidence, not acceptance of this pass.

## Previous pass: semantic zoom and reachable targets

- User tuning was read from the browser: corner hit radius **7**. The new
  policy retains that value, with the previous profiles available for comparison.
- **37 focused checks pass**: the prior 24 plus 13 semantic checks. These cover
  continuous ordered prominence, equivalent projected-size radii, wire recovery
  on 1u objects, protected patches over 13 zoom samples, overlap, thin rotated
  geometry, viewport edges, dormant picking, held ownership, cursor anchoring,
  displaced-port connections, panned free wires, and popup dismissal.
  The fixed-grid fallback also keeps all 480 possible node patches separate,
  inside the minimum supported canvas and outside the property rail.
- Reachability checks probe nine points inside every active patch. A separate
  30-object fully overlapping scene at a 330×500 viewport had 487 active targets
  with zero blocked probes. These are model checks, not native UI acceptance.
- **Actual browser input:** the fitted four-size stack reported zero blocked
  probes. Wheel-up zoomed from 81.72% to about 238% without changing page scroll.
  At 12800%, the centered 1×1 sample regained both wire targets; an exact browser
  pointer drag created its output wire and one Undo removed it. At 0.78%, only
  four move targets remained, with zero blocked probes and 60 fine targets dormant.
  Fully offscreen objects stop producing target clutter; partially visible ones
  retain protected patches.
- Low-zoom grid density is bounded; zoom uses cursor-anchored animation. Original
  owner hashes remain unchanged, and all browser JavaScript syntax checks pass.
  Evaluation uses an isolated QA session. No native application or global
  interaction contract was changed.
  The user session was refreshed with the four-size stack, corner hit 7 retained
  (along with the other saved settings), and zero blocked probes. The actual
  narrow-pane layout is captured in `preview-semantic-zoom.jpg`.

## Previous pass: rotation, fillet placement, and smaller targets

- **Source capture amended:** 18 owner hashes match 2087cc17 / preview.175,
  including the fillet geometry and rotation gesture/snap owners. The earlier
  omission of rotation and incorrect diagonal fillet inset are corrected.
- **24 model checks pass**, including oriented body/resize picking, opposite
  resize anchor preservation, 45-degree rotation snapping and Alt bypass,
  top-edge fillet placement/projection, and the expanded strategy across
  24×18 through 1×1 objects, thin 1×200/200×1 shapes, four zooms and three angles.
- **Independent review fixes:** expanded crop-bar ends now belong to their
  oriented edge hit bands; the proxy cage uses actual clamped resize deltas;
  alignment compares rotated painted bounds. Dedicated regression checks cover
  all three findings. The held cage follows outward movement and stops at the
  1u minimum when dragged farther inward.
- **Real browser input:** outside NW rotation on the 320×240 image snapped to
  45 degrees, with one Undo restoring 0 degrees. Dragging the corrected top-edge
  fillet produced radius 40, with one Undo restoring 0. On the 1×1 object: body
  drag preserved 1×1 dimensions; east resize produced 21×1, with Undo restoring 1×1;
  outside rotation was reachable; the external R drag produced radius 0.25.
  Switching profiles showed Source baseline versus 1 proposed change. No browser
  errors were captured.
- Browser evaluation used `?qa=1`, a separate local-storage namespace. The user's
  ordinary session was refreshed while preserving its 24×18 selection, Tiny
  targets scene, 200% zoom, and draft note. Exported observations carry the
  strategy and its proposed policy; no native acceptance cases were closed.
  `preview-tiny-controls.jpg` shows the final 1×1 QA scene after reload.
- All browser JavaScript syntax checks pass. Exact native integration remains
  deferred. The old axis-aligned-only limitation is superseded by this pass;
  the remaining limitations in README.md still apply.

## Initial pass (retained evidence)

- **Source capture:** all 14 relevant working-tree owners match the captured
  2087cc17 / preview.175 source. `verify-baseline.cjs` checked normalized hashes
  against both that commit and the current files.
- **Model and gesture checks:** `node --test geometry.test.cjs`: 14 passed,
  zero failed. Covers rear-target occlusion, front corner priority, tiny bodies,
  all eight tiny crop grips across four zooms, wire target precedence, one-event
  first movement, cancellation, group movement, Shift-drag, crop-body panning,
  popup input capture/dismissal, resize minimum, and missed-target recording.
- **Syntax:** all five browser JavaScript files pass `node --check`.
- **Actual browser input:** isolated Codex in-app browser, default desktop
  viewport. Body drag retained ownership while crossing a corner; one Undo
  restored it and one Redo reapplied it. Tiny body drag moved without resizing.
  Crop-body drag panned the test content while its viewport stayed fixed; crop
  SE resize changed dimensions and one Undo restored them. Wire drag created
  one history step removed by one Undo. Empty generator moved on its first
  body press. Foreground B body won over A's rear edge; inspector named both
  rear candidates as occluded. Hit scanning displayed model overlaps without
  assigning a verdict; selection changes cleared stale scan output.
- **Feedback and transfer:** a browser-recorded generator check retained
  cornerHit=12 in its gesture context. Changing the setting to 13 cleared the
  old gesture before the next check. Export showed the 12→13 proposal and
  separate observation contexts. Import with `feedback:[null]` was rejected
  before state mutation. A valid import restored the source baseline and zero
  checks. Agent QA observations were removed from the delivered session.
- **Independent review:** the reviewer found and verified fixes for early
  Shift deselection, stale gesture/config pairing, and malformed feedback.
  No remaining blocker within that review scope.
- **Final page:** reloaded after the last changes, repeated foreground-body
  priority, zero captured browser error logs. `preview.jpg` is an actual browser
  screenshot of that final view. Local session remains at source values.

No responsive viewport overrides, native launch, Rust build, Cargo checks, or
changes to native source/global contracts were part of this task. Exact native
paint, rotation, other document kinds, providers, and native input behavior still
require evaluation in Slate after the user agrees on the sandbox result.

The preview server binds only to 127.0.0.1:5297 and serves this directory. See
README.md to restart it. The implementation remains isolated in
`design/selection-hit-studio/`; other agents' changes were left in place.
