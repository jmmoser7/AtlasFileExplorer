# Shape property editing — rectangle baseline

Status: approved revision 3 implemented (2026-09-17). Native light/dark visuals captured; regression execution is currently blocked by Windows access-denied errors.
Authority: the user's rectangle-palette refinement and confirmed percentage endpoints. This supersedes the first review's Fill / Stroke / Opacity / Geometry / More toolbar. Contract rows remain in `contracts/shape-selection-toolbar.md` and each geometry contract. The paragraphs marked **Proposal** are recommendations, not additional user approvals.

## Palette and outward editors

**Visual authority:** [Dynamic object-property panels](../../../crates/atlas-shell/DYNAMIC_PANELS.md)
owns the approved revision-3 composition and subsequent capsule, wire, and
short-stringer refinements. Read it for shared light/dark styling, canvas-space
placement, RGB treatment, and transient rail metrics; this specification owns
geometry and editing semantics rather than a second visual standard.

Rectangle selection exposes Fill, Stroke and Corners. Capability gating exposes
only common supported properties for a mixed selection. Controls remain
inspectable for locked/read-only selections, with mutation disabled. RGB and
alpha edits preserve one another. Shape dash/cap/join controls remain in the
existing inspector/commands; selected-wire controls follow the connector spec.

One palette opens at a time. Moving through the icon-to-panel gap preserves it. Palette edits preview until outside click or icon change commits one journal group; Esc cancels. Recent dots hold up to six document-local committed RGB colors, newest first and deduplicated. Reuse promotes an existing color. Preview/cancel and geometry-only changes do not add colors. History persists in ViewState as usage metadata, outside authored scene undo/export. Legacy documents seed it once from existing styles.

## Corner amount

**Confirmed.** Let `m = min(width, height)` in the rectangle's local board-space axes. The limit is `m / 2`.

- Percent `p` uses `amount = (p / 100) × m / 2`, for `0 ≤ p ≤ 100`.
- Absolute amount `a` is measured in board units and renders as `min(max(a, 0), m / 2)`.
- Fillet uses this amount as the radius of circular corner arcs.
- Chamfer uses it as the equal cut distance along the two incident sides, not the length of the diagonal chamfer edge.
- A 1 × 1 square: 0% gives sharp corners; 50% gives radius/cut 0.25; 100% gives radius/cut 0.5, hence a circle for Fillet or diamond for Chamfer.
- A non-square rectangle at 100% becomes a capsule with circular fillets. Chamfer clips every corner as far as the shorter side permits. Per-axis elliptical corner rounding would be a different feature; it is not silently substituted for a circular fillet.

**Proposal.** Percentage is persistent relative intent: resizing the host keeps the percentage and recomputes its radius/cut. Absolute mode keeps the authored board-space amount; smaller hosts clamp its effective value without destroying the authored value, so enlarging restores it. Changing mode converts each selected object's current effective amount without a visual jump. A batch percentage edit assigns the same percentage to each host, not the same radius. A batch absolute edit assigns the same authored distance, with per-host limits. Mixed modes/values remain Mixed. Both mode and value are authored, undoable scene data; a percent-only UI that silently stores an absolute radius would violate this proposal on later resize.

The scene `Corner` now serializes absolute and percentage variants compatibly. Both board and artifact resolve their effective amount through the same model method.

## Dimension stringers

**Stated.** Rectangle dimensions appear as editable stringers outside the object: XY-aligned when unrotated, aligned with the rectangle's local axes when rotated. Clicking the dimension value starts numeric editing. Changing it scales around the rectangle's centroid. Line stringers measure actual endpoint-to-endpoint length, not its bounding box. Polyline stringers may measure its bounding box.

**Proposal.** A stringer is a dimension line with witness lines/ticks and a numeric value. Prefer width below and height beside an unrotated rectangle, reserving the region above for the palette. Rotate the dimension geometry with the object's local axes; keep text readable by flipping its baseline when needed. Collision avoidance may move a stringer to the opposite exterior side but must never swap what W and H mean. Keep its lane stable during an edit. Show board units (`u`); do not imply millimetres or inches without an established document conversion.

Clicking the number replaces it in place with a focused field. Enter commits one invertible command; Esc restores the original and keeps selection. Clicking elsewhere applies a valid value; invalid/nonpositive/nonfinite input remains editable with a local explanation. Preserve precision beyond the displayed rounded value. The existing aspect constraint, when explicitly enabled, scales both dimensions; otherwise only the edited axis changes. Do not alter stroke width as a side effect of a dimension edit. Centerline/geometric dimensions exclude stroke expansion, caps, and editor chrome.

For rectangle center `c`, rotation `R`, local dimensions `w,h`, and requested width `w'`, transform its geometry by `c + R × diag(w'/w, 1) × R⁻¹ × (x-c)`. A height edit uses `diag(1,h'/h)`. This states the centroid and orientation invariants; implementation must call the shared transform owner rather than introduce per-tool copies.

Stringers are selection view state: they do not become scene annotations, persist in the workbook, or appear in exports. Their edits do persist. Bounds/length measurements are cached on geometry changes, not recalculated across the scene each frame. Arbitrary path bounds use the actual curve extrema, not the possibly larger control-point normalization rectangle. A zero-width or zero-height path exposes that axis as a readout until an explicit geometry edit supplies an extent; scaling must not divide by a fabricated epsilon.

## Geometry recommendations and alternatives

The rectangle and line requirements below are stated. Other defaults and alternatives are proposals. Alternatives are documented for comparison, not extra simultaneous controls.

| Geometry | Recommended stringers | Numeric edit and invariant | Considered alternative |
|---|---|---|---|
| Rectangle / rounded rectangle / chamfered rectangle | Local W and H outside the edges | Change one local dimension about the geometric center; keep rotation; recompute effective corner amount by its mode | A linked W/H edit scales both; a diagonal length adds little useful precision and is omitted |
| Circle | One diameter, marked Ø | Uniform scale about center; preserve circularity | Radius R gives the same control at half the value; choose one presentation, not both competing fields |
| Ellipse | Two principal-axis diameters, D1 and D2, in the stored local orientation | Change one semiaxis about center, keeping orientation; do not swap axis identities when one becomes longer | Radii R1/R2; diameters are recommended because they match the object's visible span |
| Straight line | One dimension parallel to the line, actual length L | Keep midpoint and direction fixed; move both endpoints equally | An explicitly chosen endpoint anchor for drafting workflows; do not default to bounding-box W/H |
| Open polyline | W/H of its geometry in its stable local frame; initially board XY | Scale vertices about box center on the edited axis; preserve vertex count and open topology | Direct Selection can reveal segment lengths; total path length L can uniformly scale the whole chain, but is not a second default sizing scheme |
| Closed polyline / polygonal path | Same W/H as open polyline | Scale about bounds center; preserve closure and winding | Per-edge dimensions in Direct Selection for fabrication-like work. Area/perimeter are useful readouts but ambiguous edit targets |
| True circular arc | Radius R and included angle θ; optional arc-length readout L | Radius edit keeps circle center and angular interval; sweep edit keeps radius, circle center and angular bisector, moving endpoints symmetrically | One arc-length L stringer can uniformly scale around the circle center while holding sweep. Radius/angle edits require durable arc parameters |
| Existing arc stored only as cubic PathData | Tight local W/H (implemented) | Scale about tight-bounds center while preserving curve topology | Actual curve length L could provide uniform scaling later; a fitted cubic must never be labelled an exact-radius circular arc |
| Bezier path | Tight local W/H | Affine scale anchors and handles together about tight-bounds center; maintain their relationships | Actual curve length L with uniform scaling is useful for fitting a curve to a target length; endpoint chord length would misrepresent it |
| Freehand pen | Tight local W/H | Scale path about tight-bounds center; keep stroke width; never re-run sketch fitting just to change size | A single L stringer for uniform scaling of a mark; generally less predictable than visible extents |
| Brush stroke | Centerline W/H; label any optional ink-envelope readout separately | Scale centerline about its bounds center; preserve authored stroke width/taper settings | An explicitly linked Scale ink option could also scale width, but is a separate future decision |
| Mixed or multiple shape selection | One overall XY union W/H by default | Scale the arrangement and member geometry uniformly about union center; object-relative corner percentages recompute per host | Per-object stringers would overlap and are better offered only after isolating an object; do not silently treat a group value as per-item size |

There is no reliable provenance tag distinguishing every pen/brush/arc/Bezier output today. Capability gating must follow actual geometry and authored parameters. All geometry above remains editable as a generic path where more specific parameters do not survive.

## Other board objects

These objects share the geometry selection strip when the scene exposes the matching style field: Fill for frames, portals, and text sticky-note backgrounds; Stroke, Corners and photo filters for images; stringers for local W/H. Text ink stays a typography property. Portal source/consent UI stays on the portal (D35). Frame deck order, tags, add-images and present reuse the same squircle strip instead of a forked popup. Images and video/PDF/model viewports use local W/H about their frame center; changing dimensions resizes their presentation, not source files. Slide frames use local W/H about center, but contents retain current frame-transform semantics rather than a new automatic scaling rule. Portals use their permitted frame axes; changing their frame does not mutate source contents or camera state. Text and sticky notes use box W/H for layout/reflow while font size remains its own text property. Connectors expose routed length as a readout while attached; a length edit must not detach or move connected objects silently. Embedded dock strips preserve the existing contain-scale behavior.

## Acceptance additions

- Rectangle 4 × 2 at 30°: type width 6 on its width stringer -> center and rotation unchanged, height 2, local width 6; one undo restores width 4.
- Line length 5: type 9 -> midpoint and direction unchanged, endpoints move 2 each; no bounding-box dimensions.
- Rotate a selected rectangle through 90° and 180° -> stringers remain attached to the same local axes and labels remain readable; no dimension meanings swap.
- Fill RGB edit or desktop sample -> stroke color/alpha unchanged; fill-opacity edit -> fill RGB unchanged.
- Square 1 × 1: percentage 0/50/100 -> amounts 0/0.25/0.5; Fillet/Chamfer at 100 -> circle/diamond; no self-intersection or duplicate zero-length path segments in export.
- Rectangle 4 × 2 at 50%: effective radius 0.5; increase height to 4 -> radius 1. Absolute 0.5 stays 0.5 for the same resize.
- Toggle percentage/absolute -> appearance unchanged, only authored amount semantics change; undo restores both mode and value.
- Batch 2 × 2 and 4 × 4 at 50% -> radii 0.5 and 1; setting absolute 0.25 -> both 0.25. Fill RGB differences survive an alpha-only batch edit.
- Open an editor and cross the icon-to-editor gap -> selection and editor persist; clicking a field never starts a canvas drag.

Implemented defaults: persistent percentage intent; circle diameter; midpoint line length; tight local path bounds; uniform group scaling. Palette previews commit on icon change/outside click and cancel on Esc. Optional alternatives above (arc parameters, curve-length sizing, collision-lane switching and linked sizing) remain future work. Automated checks are described in the contracts; native cross-monitor sampler validation remains a separate acceptance check.


## Prior implementation verification — before revision 3

- Eight selection-property tests pass, including rotated group centroid sizing, tool-switch cancellation, and real pointer strip activation. Five ordered-drawing regressions pass for moving vertices, line endpoint latching, snapped click classification, draft closure, and pen event/release preservation.
- Shared suites pass: atlas-shell 113, slate-doc 138, slate-artifact 37, vector-ink 57 (345 total; two opt-in tests ignored). Percentage persistence/backward compatibility and exact CSS amounts are covered.
- The broader Slate run reports two unrelated failures: `maximized_layout_has_no_fillet` expects the old portal header height; `never_repeat_set_matches_spec` references the removed `canvas.fit` command. Those concurrently edited areas were preserved.
- The native contracts checker passes: contracts, dimension registry and decisions agree. Nine geometry/toolbar matrices and all 153 corresponding decision rows also passed an independent exact-content and D01–D17 completeness check.
- Windows Computer Use timed out during app discovery, so native overlay/multi-monitor input is not claimed as verified. A release build succeeded; replacement of the final executable is waiting for the running Slate process to close.

## Revision 3 verification — 2026-09-17

- `cargo check -p slate --tests` succeeds, including the new headless input tests. Regression coverage includes rotated in-place dimension typing across camera changes, exterior lanes, committed-color history, inline RGB entry, transient scrub metrics and host-transform covariance.
- The native `atlas-shell` shape_palettes fixture rendered both shared themes successfully. Production-widget captures: [light](../../../design/shape-palettes-2026-09-17/native/palette-light.png), [dark](../../../design/shape-palettes-2026-09-17/native/palette-dark.png). These verify widget appearance, not a full Slate interaction session.
- Windows denied launching the compiled Slate and atlas-shell regression executables and the contracts checker (`os error 5`). The tests are not reported as passed. An independent consistency check verifies all 153 approved behavior rows, sources, confidence scores and D01–D17 coverage across the nine shape contracts.
- No screen-coordinate cache or viewport-clamping fallback remains in shape property placement. Shared `Palette` slots style the panel, rail handles, text, borders and selected states for both themes.

### Selection-preview refinement — 2026-09-18

Opening an adjustment icon now fades selection decoration out through the shared
shell painter; editor switches keep it hidden and close/cancel restores it.
Authored node paint, property controls and stringers use the unfaded painter.
The selection contract's GP14 records this behavior. Its regression examines
the actual board paint output for shapes and wires in both themes, and checks
selection/document preservation. The test target compiled successfully, but
Windows denied launching it (`os error 5`); execution and live visual review
are not claimed.

The release with `ui-tuner` compiled to `target/release/deps/slate.exe` on
2026-09-18 at 10:20 local time (SHA256
`1AC423E4E00CDDE37DD9D406A7B4D1CC2745731F96E2133B6F1CD166F0D835F8`).
Installing it at `target/release/slate.exe` is pending: the running 10:08
build locks that file. Save/close Slate before normal replacement; no live
executable swap was attempted.

### Follow-up refinements

- Trim now consumes authored corner geometry for both rectangles instead of square bounding rectangles. Rounded/chamfered percentage modes and rotation survive the operation; the untouched corners remain intact. Existing incorrect trim results are already baked paths and must be undone/recreated.
- Curve quality: dash runs retain intermediate spline samples; round caps and joins use noncrossing boundary rails; tapered strokes keep adaptive samples. Fills, strokes, clips and selection curves refine with zoom. Native production-mesh captures were inspected in both themes: [light](../../../design/shape-palettes-2026-09-17/native/curve-quality-light.png), [dark](../../../design/shape-palettes-2026-09-17/native/curve-quality-dark.png).
- Affected regression targets compile. Windows still refuses to launch headless test executables (`os error 5`); native screenshot review is not a claim that those tests ran or that the entire live Slate interaction flow was exercised.

- Short dimension labels move beyond the second stringer tick along its local axis, with a short continuation of the baseline. The displayed label remains the inline input and follows rotation/zoom.
- Wire selection uses the same property owner and native shell controls: Stroke color/opacity plus a shallow Bezier/Square, weight, Solid/Dashed, None/Arrows strip. Mixed wire/shape selection offers shared stroke controls; wire-containing selections omit box-dimension resizing. See [connector model and interactions](connectors.md).
- Routing choices are journaled and saved per connector. New wires capture the creation default; legacy records without a routing field retain the session fallback. Board painting, picking, snapping, derived bounds and SVG export read the effective route. Shift/Ctrl clicks and crossing marquee select wire batches.
- Native production-widget captures for the wire capsule and short-stringer fallback: [light](../../../design/shape-palettes-2026-09-17/native/wire-properties-light.png), [dark](../../../design/shape-palettes-2026-09-17/native/wire-properties-dark.png). All four affected crate test targets compile. The new slate-doc routing persistence test compiled but Windows denied launching it (`os error 5`); execution is not claimed.

- The corner strip is 17 board units high, with a separate outlined slider capsule and pill thumb. Production-widget captures verify both themes: [light](../../../design/shape-palettes-2026-09-17/native/refined-light.png), [dark](../../../design/shape-palettes-2026-09-17/native/refined-dark.png).
- The same fixture verifies the shared stroke renderer on a boolean-trimmed rectangular notch. Miter joins now align their cross-section with the angle bisector; bevels preserve both adjacent edge normals. The scene geometry and stored stroke settings are unchanged.
- `cargo check -p slate -p vector-ink -p atlas-shell --tests` and compilation of the vector-ink test executable succeed. Windows still denies launching that executable (`os error 5`), so the new geometry and GP7 trim regressions are compiled but not reported as passed.
- Latest release (trim geometry and curve-quality follow-up) compiled to `target/release/deps/slate.exe` on 2026-09-17 at 18:45 local time. SHA256: `FC123C8ED5DAF8D43094E2BD9FB4C41AE15B6F3F5E10352AE69E4AAF97B94AF3`. Windows blocked replacement of `target/release/slate.exe` because Slate is running; the launcher still points to the 18:17 build. Save/close Slate, then replace the executable normally. No live executable swap was attempted.
