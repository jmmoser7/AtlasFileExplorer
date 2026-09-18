# Slate shapes: selection properties and drawing reliability

Research date: 2026-09-17. Scope: pre-implementation proposal. Method: official product documentation and static inspection of the current checkout. No live Figma/Miro session or Slate pointer replay was performed. Application source is unchanged.

> Revision 2: [rectangle palette, dimension stringers and corner modes](../specs/shape-property-editing.md) supersedes the initial toolbar composition and dimensional controls below. [Desktop color sampling](../specs/desktop-color-sampling.md) captures the project-wide eyedropper requirement. The original research and code findings remain applicable.

## Reference findings

Miro puts shape fill, border styling, opacity and rectangle rounding in the selected object's context menu. Its radius control supports a slider and numeric value for squares/rectangles. This supports keeping frequent edits adjacent to the selected geometry. Exact toolbar offsets and collision behavior are Slate proposals, not documented Miro measurements. [Miro Shapes](https://help.miro.com/hc/en-us/articles/360017730713-Shapes)

Figma Design normally exposes these properties in the right Design sidebar. Rectangle corner radius can be uniform or independent; smoothing is separate. Lines do not receive rectangle corner controls. Borrow its property applicability, without claiming Figma Design uses the same floating toolbar being proposed here. [Figma corner radius and smoothing](https://help.figma.com/hc/en-us/articles/360050986854-Adjust-corner-radius-and-smoothing)

Figma's stroke controls cover paint, position, weight, width profile and endpoints; its documentation explicitly notes that SVG only supports centered strokes, so inside/outside stroke export needs simplification. Treat those alignments as geometry/model work rather than adding a selector that changes nothing. [Figma stroke properties](https://help.figma.com/hc/en-us/articles/360049283914-Apply-and-adjust-stroke-properties)

Figma also exposes selection colors for mixed fills. For Slate, the proposal is a Mixed state per differing property, explicit replacement of the edited property only, and one undo for a batch adjustment. That is an adaptation, not a claim that both products implement identical mixed-selection rules. [Figma mixed selection colors](https://help.figma.com/hc/en-us/articles/360042553434-View-and-adjust-colors-in-a-mixed-selection)

Miro's connection-line context menu offers line type, arrowheads, thickness and color. Connection lines are distinct from Slate's authored line/path shapes; their routing and text-label behavior should not be copied implicitly. [Miro Connection lines](https://help.miro.com/hc/en-us/articles/360017730733-Connection-lines)

Miro Pen documents color/width presets and separate smart drawing. It does not document sample spacing or curve-fitting tolerances; no algorithm recommendation here is attributed to Miro. [Miro Pen](https://help.miro.com/hc/en-us/articles/360017730573-Pen)

## Proposed editing inventory

The primary row should stay compact: applicable Fill, Stroke, Opacity, Geometry and More. This is Slate's proposed composition.

- **Color:** solid fill or no fill, stroke color, separate fill/stroke alpha, swatches/recent colors, numeric color entry and eyedropper. Gradients, image/pattern fills and multiple paint layers are later candidates.
- **Stroke:** width, solid/dashed/dotted, appropriate caps and joins, taper. Later candidates: custom dash/gap/offset, start/end arrowheads, individual rectangle-side strokes and inside/outside alignment.
- **Corners:** rectangle square/rounded/chamfer mode and uniform radius/cut first; independent corners and corner smoothing later. True polyline fillets require path geometry, not merely a round stroke join.
- **Geometry:** W/H/aspect and rotation for area shapes; Rx/Ry for ellipses; length/angle/endpoints for lines; vertex/handle editing for paths. Parametric arc radius/sweep and polygon/star parameters need corresponding durable model data.
- **Freehand:** ink width/color/opacity, taper and path editing. A reversible post-edit smoothing slider requires original samples or an explicit reversible path-replacement command; the current path alone does not preserve the original sketch.
- **Composition:** style presets/copy-paste, ordering, grouping, lock, alignment/distribution, and explicit conversion in secondary surfaces. These are candidates, not a commitment to build every feature from another product (Article III).

See [shared toolbar contract](../contracts/shape-selection-toolbar.md) for exact capability gates and [rectangle](../contracts/rectangle.md) for the first geometry contract. The remaining draft contracts cover ellipse, line refinement, polyline, arc, Bezier span, pen and brush. Eraser and eyedropper do not create distinct selected geometry.

## Findings in Slate

### Polyline moving-click loss: direct code evidence

`apps/slate/src/app/board.rs:2443` routes point tools through `resp.clicked()`; `board_click` sends Polyline/Arc to `path_tool_click`. Moving enough for a press to become a drag prevents that click route, while `begin_gesture` returns None for Polyline/Arc. This explains a lost point path. It also affects Arc. A future raw-event input test must verify the actual frame/event ordering, not just call `path_tool_click` directly.

Proposal: a point pick records the resolved press-event coordinate exactly once. Movement and release cannot discard or duplicate it. Bezier shares anchor capture but uses motion for its handle. Keep camera navigation and toolbar capture ahead of these handlers.

### Polyline self-snaps and closure: two separate gaps

`board_osnap.rs:85` queries `doc().scene` only. Draft points in `BoardPathDraft` are outside that scene, so earlier vertices/segments are absent. Separately, `board_path.rs:1127` finishes a polyline using `points_to_path_data(..., false)` and `closed=false`. Snapping to the start alone would therefore not create a topologically closed shape.

Proposal: add transient draft candidates to the existing snap owner, using its enabled snap kinds and priority. End targets earlier vertices; Mid/Near/Intersection use completed draft segments when enabled. Exclude the current rubber-band and immediate previous vertex. A Close cue on the first vertex after three distinct non-collinear points commits `closed=true`, without a duplicate start point. Other self-snaps continue the chain; extracting a loop from an earlier segment while retaining/discarding a tail needs an explicit separate decision.

### Line: a distinct path and a confirmed coordinate mismatch

Line already uses raw press/release (`board.rs:2279` onward); it does not share the polyline `clicked()` gate. `board_line.rs:155` measures first-press travel from the **snapped** start. A stationary click within the 8px snap radius but more than 4px from its target can therefore be classified as a drag. It may commit a short/degenerate line before the intended second point. This is a source-level failure mechanism, not a live reproduction of every reported line symptom.

The board also reads aggregate frame pointer state, rather than consuming each button event with its own position. Same-frame movement and hover-gated release deserve regression coverage. Preserve click-click and deliberate press-drag-release, measure the first grammar threshold from raw press travel, and propose latching a second click at its press coordinate. Fast travel during the first press remains inherently ambiguous with the shipped drag grammar; do not pretend a threshold alone can distinguish intention perfectly.

### Pen: sampling, fitting and flattening must be evaluated separately

`board.rs:4033` appends the latest frame pointer only after more than 1.5 **world units** of movement. Thus sample spacing changes on screen with zoom, and intermediate pointer events within a frame are not collected here. `board.rs:4595` finishes with the stored points rather than explicitly appending the release point. These are concrete quality risks.

`board_path.rs:1254` uses fitting tolerance `1 / zoom`; `vector-ink/src/fit.rs` first applies Ramer–Douglas–Peucker simplification and then Catmull–Rom-to-cubic conversion. Committed strokes already contain cubics: simply replacing lines with Beziers is not the fix. The RDP tolerance does not bound error of the final interpolated cubic; overshoot and intentional corners need independent checks. The live preview is also a sampled polyline, so compare it separately from the final fitted path and zoom-bucket tessellation.

Proposal: collect available ordered motion samples plus endpoints, use a named screen-space sampling target (initial comparison: 0.5px), measure final-curve error (initial comparison: 1px), preserve intentional corners, and use the existing vector-ink/cached rendering owners. Do not solve sampling loss by globally increasing tessellation. Brush shares the sampler and fitter and must retain its expressive defaults if those owners change.

## Review and implementation boundary

Every new matrix is draft; existing approved line history remains intact. The floating toolbar and geometry-appropriate properties are explicitly requested and pre-accepted in the review artifact. Four open choices cover mixed-selection scope, self-snap loop semantics, second-point line placement, and pen fidelity. The rest of the rows remain individually reviewable.

P0.9 currently requires node-local controls to scale with zoom. This proposal follows it; a constant-screen-size floating toolbar would need an explicitly named exception in that pattern, not a hidden clamp. All chrome painting stays in atlas-shell, all edits use named journal commands, and any new style property must work in both the board and artifact interpreter (Articles IV, VI, X and XII).

The golden paths in each draft are proposed acceptance scripts, not executed tests. Implement them when the corresponding contract is agreed and implementation is authorized.
