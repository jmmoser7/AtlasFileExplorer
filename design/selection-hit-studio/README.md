# Selection hit-zone studio

A disposable HTML/JavaScript studio for refining selection behavior before one
native implementation pass. No build, dependency install, providers, or workbook
is required. Open `index.html` in a normal browser, or serve this directory only:

```powershell
python -m http.server 5297 --bind 127.0.0.1 --directory design/selection-hit-studio
```

Open <http://127.0.0.1:5297/>. The Codex browser requires HTTP. That server binds
only to this computer and serves only this prototype directory.

## Try it

1. Choose Overlap torture (partial overlap, touching tiny objects, coincident
   objects), Scale ladder (normal, small, extra small, point), Single object,
   Overlap order, Adjacent corner tie, Tiny targets, Crop handles, Wire crossing,
   or Crowded canvas. Reset scene restores that arrangement while retaining tuning
   and recorded checks. Add objects and reorder them to construct conflicts.
2. Move the pointer over a target. The inspector shows the winner and all
   overlapping candidates, including targets hidden behind a foreground body.
   Green is body, blue is transform, amber is wire, purple is controls.
3. Drag from the body, an edge/corner, the top-edge corner-radius grip, an
   outside-corner rotation zone, or a wire port. Rotation snaps to multiples of
   45 degrees within 4 degrees; Alt bypasses snapping. The original press owns
   the whole drag. Shift-click toggles membership;
   Shift-drag preserves the selected group. Undo/redo operates once per gesture.
   Escape cancels. Crop body drags pan the test picture inside its viewport.
4. Use the mouse wheel over the canvas for smooth zoom around the pointer.
   The logarithmic slider spans 0.78%–12800%; Center selection brings a sample
   back into view at the current zoom. Toggle hit zones to judge the paint alone.
   Change zoom and target sizes.
   In Adaptive regions mode, the local colored overlay shows the current winning
   regions, including clipping and competition. Scan hit zones samples their
   actual winning area; it lists covered targets without declaring them resolved.
   The previous protected-point and expanded-control profiles remain available.
   These checks never declare native acceptance.
5. Record what happened and what should win. Choose Works as expected or Needs
   a change. Export checks downloads JSON and also displays it for copying.
   Import JSON restores a proposal. Work is saved in this browser when local
   storage is available; export provides a portable copy.

The eight tuning controls are proposals. Use source values restores the captured
configuration. Changing the scene, selection, configuration, or view clears the
last gesture so a new check cannot silently reuse it. Completed gesture records
also carry their tested configuration, source revision, zoom, and before/after
geometry. Export includes source hashes, proposed deltas, observations, and a
short implementation brief. It does not approve or modify native Slate.

## Shared zoom hierarchy

The **Scale ladder · four sizes** scene stacks Normal (240×112), Small (96×48),
Extra small (24×18), and Point (1×1). The prominence bars show which interaction
domains are relevant as the same object changes size on screen. The rule uses
the projected shortest side, rather than authored dimensions or zoom alone:

- Wires fade over 36–88 screen pixels.
- Edge/corner resize and fillet fade over 12–48 screen pixels.
- Rotation fades over 3–16 screen pixels.
- Move remains available for visible objects, with an 18-pixel minimum body target.

Each range uses a smoothstep curve; zooming out removes relevance in the order
wire → resize/fillet → rotation → move. Fine radii shrink with that prominence.
A fine target becomes dormant when its radius falls below 2 screen pixels; it
then neither wins input nor reserves a patch. A 1u object regains wires at high
zoom: there is no authored 44u-height cutoff in this proposal. Semantic-mode
base radii are screen-space values, so identical projected sizes get identical
hit radii across different world scales. The corner-hit adjustment is retained.

## Previous proposal: protected points

This comparison profile was rejected for overlapping-object feel. It is no
longer the default. In this profile every active target has a protected,
visibly outlined patch inside the canvas.
Placement avoids other patches and property controls; leaders show displacement
and carry no input. The protected glyphs paint above object contents and ordinary
hit regions yield to those patches. This keeps fully overlapped objects and
partially clipped controls reachable. A fixed-grid fallback has capacity for all
480 possible node targets at the supported minimum canvas size if local packing
runs out of room. Fully offscreen objects are dormant and
can be brought back with Center selection. Scan hit zones probes nine locations
inside each active patch and reports blocked probes. This is a sandbox guarantee
over its supported scenes and view size, not evidence of native acceptance.

Held gestures keep their press owner and patch until release even when crossing
a relevance threshold. Property controls use a compact screen-space rail; the
source/expanded profiles retain the earlier chrome. Fillet shares the resize
domain; the precise ranges and packing are proposals awaiting user iteration.
Exports include the policy, camera, and each gesture's tested policy. Source-only
gap/slop controls are disabled in semantic mode to avoid ineffective tuning.

The viewport grid changes density at low zoom, keeping drawing work bounded.
Browser QA uses `?qa=1`, a separate storage namespace, so tests do not overwrite
the user's adjustments.

## Previous proposal: expanded tiny controls

The Hit rules switch also offers **Expanded controls · previous** and the
**Slate source profile** for comparison. Adaptive regions is initially enabled.
At a shortest side of 24 screen pixels or less, the selected object gets an
external control cage, at least 72 screen pixels across with 24 pixels of
clearance. The cage grows when hit radii need more separation. It does not enlarge
the object. An at-least-18-pixel body target keeps moving available; eight resize
targets, four rotation targets, and an external **R** radius control stay apart.
Detached targets yield to a neighboring object's actual body. The R control uses
48 screen pixels of travel for the full available radius, so a 1u object remains
adjustable. Crop uses the same cage with full-side edge hits. Resize clamps at 1u;
the held cage stops with that limit. The resting cage can settle after release.

The tiny scene contains 44×44, 24×18, 8×6, 4×3, and 1×1 samples; use the inspector
to try intermediate sizes. Rotation and radius readouts update during gestures.
The proposal and its confidence (55, pending user feedback) travel with exported
checks. Source values switches back to source handling as well as source numbers.

## Source and fidelity

The current source snapshot is **2087cc17 / 1.0.0-preview.175**, tree
`d1a03ab9fb37121e1121f4b22644020d2deb248f`. On 2026-10-20, the other active Slate
chat confirmed this as the latest validated native source. All 18 relevant
working-tree owner files matched that snapshot. The immutable capture and
owner hashes are in `source-baseline.json`; `source-baseline.js` mirrors it for
file-URL use without a fetch. Recheck source drift with:

```powershell
node design/selection-hit-studio/verify-baseline.cjs
node --test design/selection-hit-studio/geometry.test.cjs design/selection-hit-studio/semantic.test.cjs design/selection-hit-studio/adaptive.test.cjs design/selection-hit-studio/layering.test.cjs
```

The capture was amended on 2026-10-20 to include previously omitted rotation
and fillet owners/constants. The previous sandbox's inset fillet position was
incorrect: Slate places it on the rotated **top edge**, at travel
`max(radius, min(10, shortest-side/2))`. Its hit is a screen-aligned square.
Source-derived values include 12u corner reach, 12u rotation reach with an 18u
radial corner offset, 6u edge band, an 8 screen-point
hit minimum, 6 screen-point drag threshold, 14u property-strip gap, and 68u bottom
dimension gap. Crop paint and reach shrink together on tiny objects. The model
includes foreground-body occlusion, tiny-body priority, wire inner/extended
targets, and press ownership. The wide-image strip uses the source's 504u
footprint and 20% area ceiling. Confidence and owner names accompany the capture.

**This is an approximation for iteration, not a native acceptance environment.**
Rectangle/image/generator proxies are supported, including rotation for images
and shapes. Generators represent non-rotatable portal hosts. Rounded body
silhouettes, toolbar artwork, popup contents, and their placement are
representative. Toolbar/popup controls have a simplified top UI layer. The
Controls Corners popup adjusts radius; other popups demonstrate input capture.
Crop changes a simulated viewport and image offset, without native crop/export
semantics. No general object snapping, text editing, exact native icons/fade, providers,
file operations, or hosted portal behavior is reproduced. Wire routing is visual.
The browser and model checks cannot close original native bug reports.

## Ownership and later integration

`geometry.js` owns this sandbox's hit model; `engine.js` owns its gestures;
`semantic.js` owns shared prominence and camera math, plus the previous
protected-patch comparison; `adaptive.js` owns continuous feature fields,
cursor expansion, silhouette clipping, and winning-area sampling;
`render.js` paints; `studio.js` owns UI, history,
wheel animation, feedback, and transfer. This
deliberately separate prototype must never be imported into the Rust application.
DRY review approved this disposable boundary. No native sources or global
contracts were changed. Existing native owner locations are in the source capture.

After user iteration: reconcile observations and proposed deltas, agree the
interaction contract, implement the approved differences in the existing Rust
owners, then run native UI evaluation against the resulting build. Relevant
contract dimensions are D03/D04/D09–D13/D17 (gestures, selection, cancellation,
grips, and undo); existing select-sweep, image-crop, and shape-selection-toolbar
contracts remain authoritative until that later review.

## Current proposal: adaptive regions — 20 October follow-up

This iteration responds to the user's rejection of scattered protected points.
It retains the screen-size hierarchy (wire → resize/fillet → rotation → move)
and blends the previous expanded frame into compact-object controls. Edges have
continuous bands; rotation uses corner arcs; wire regions extend outward from
ports. The cursor expands nearby reach by up to 65%, then normalized feature
distance partitions competing regions. The minimum move core remains available.
The local colored preview calls the same resolver as input.

When hosts overlap with positive area, the foreground move, resize, rotation,
and fillet regions retain their isolated geometry and take priority over lower
hosts. A lower body does not clip those foreground regions. Foreground wire
regions require the cursor strictly inside the visible host; their outside
reach yields to scaling/movement. Compact foreground wire anchors stay on the
actual host edge so their interior hit area survives the expanded frame.

Touching edges/corners count as adjacency. Coincident corner regions compare
cursor distance to each actual host silhouette, then use front order at exact
equality. Distance accounts for rotation and rounded corners rather than host
centres or bounding boxes. Overlap classification uses convex outlines (eight
segments per rounded quarter); unrounded rectangle contact is exact.

Fully covered, unfocused objects suppress their controls. Selecting one in
Objects reveals a single outside frame; its move label chooses a position clear
of both bodies and higher-priority foreground controls. No target relocates into
a reserve-point grid. Rear corners can remain covered according to layer order;
the scan reports them. The profile choices and user tuning remain reversible.

Evaluate these user flows: drag a long edge away from its midpoint; move a front
object where it covers a rear corner; resize an exposed rear edge; select and
move the coincident rear object; zoom the touching tiny pair until only moves
remain; zoom back in until fine controls return. Undo should reverse each drag
once. In Adjacent corner tie, press just left/right of the shared upper corner:
the corresponding host should resize regardless of draw order. In Overlap order,
rotate the top object's NW corner over the rear body, then try its input wire
just outside and just inside the left edge. Compare profiles for feel.

## Approved native integration (20 October 2026)

The user approved the final Adaptive regions / foreground-priority iteration.
Its native contract is [selection-hit-regions](../../docs/keymap/contracts/selection-hit-regions.md).
The Rust policy now lives in `atlas-shell::selection_tools::hit_regions`; this
JavaScript remains the retained design reference, not a second native runtime.
`source-baseline.json` records the earlier captured source revision. Native edits
intentionally supersede those hashes; do not rewrite historical evidence or treat
prototype checks as native acceptance. The integration ledger records build and
UI status in `docs/requests/2026-10-20/selection-hit-integration.md`.
