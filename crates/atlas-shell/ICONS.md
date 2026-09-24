# Tool and palette icons

Approved direction: Quiet outline, 2026-09-15. Primary Portals uses filleted outer and inner corners.

## Single owner

`assets/tool-icons.json` owns 24-unit SVG path data. `src/icons.rs` owns identity,
metrics and painting. `vector-ink` parses/triangulates/strokes geometry; it stays
renderer-independent. `DockIcon` and Slate's `ToolIcon` map to this catalog.
The design preview reads the same JSON. Never hand-copy glyphs into an app.

## Drawing rules

- Use a 24 × 24 master and nominal 20 × 20 optical footprint, with 2-unit padding.
- Use 1.5-unit strokes, round caps/joins, and open interiors. Preserve recognizable
  silhouettes. Select is solid; Direct select is hollow.
- Keep internal gaps at least 2 units where possible. Simplify secondary detail
  at small sizes before reducing stroke weight. Judge optical centering and weight.
- Use one metaphor. Frame is a page silhouette with one dog-ear, at the
  proposed size's true aspect; Rectangle is a closed square; Arc is smooth;
  Polyline has corners; Bezier shows control handles.
- Primary icons represent the family or panel. They keep their identity when a
  child tool changes. Actions is a wrench; Document settings is a page with sliders;
  Object properties is a subdivided object; Selection is a bounded object.
- Portals uses nested rounded openings: outer fillet 2 units, inner fillet 1 unit.
- No bitmap tool glyphs, provider logos, emoji, gradients or decorative effects.

## Media family

Media uses stacked sheets as a stable primary family icon. Image uses a landscape
frame for flat/print media, Model uses a cube for the existing model viewer,
Video uses a play frame, and TextDoc is a portrait page with three text lines
for Word, spreadsheets, CSV, and source files. All five share the 24-unit
catalog and 1.5-unit outline; there are no optical exceptions or per-app copies.

## Frame sizes

The Frame family icon is the same page-and-dog-ear sheet as the nested
sizes. Letter (8.5×11 portrait), Tabloid (17×11 landscape), Wide (16:9), and Custom (1:1
with a plus) share one path language at that preset's true aspect. The
dock primary shows the current proposed size. They map from `frame.letter`
/ `frame.tabloid` / `frame.wide` / `frame.custom`. Do not reuse Document
settings (page + sliders) for a size.

## State and sizing

Apps supply state colors through the shared Palette/dock tokens. This rollout
does not change existing hover, active, pinned, focus, or disabled semantics.
Do not hardcode colors into glyphs. Keep geometry fixed between these states.
View/Edit is an explicit semantic exception: Mode shows a lock for View and a
pencil for Edit; the label and existing human-only control remain authoritative.

Glyph bounds are separate from hit targets. Preserve dock and canvas-space
scaling; do not add per-app size clamps. Parsing and tessellation are cached once;
the bounded transformed-mesh cache avoids allocation for unchanged chrome.

## Review and maintenance

Inspect 16/20/24 px glyphs in light and dark themes, adjacent to confusable siblings,
at 100/125/150/200% Windows scaling. Check the current dock state treatments too.
A new icon needs a semantic ID, command mapping, rationale, and any optical exception.
Essential control states should maintain 3:1 contrast with their adjacent colors.
Retain command labels and tooltips for discoverability.

Run `cargo test -p atlas-shell icons::tests --lib` for catalog/geometry validity.
Validity alone is not visual verification. When changing the native stroke path,
export native meshes with `ATLAS_ICON_AUDIT_JSON` and the ignored
`export_native_icon_meshes_for_visual_audit` test, then compare them with the SVG
reference using `design/tool-icons/render_native_audit.py`. The audit takes
`native-audit/before.json` and `after.json` and renders actual triangle coverage.
The regression tests in `vector-ink::mesh` guard closed seams and round-join width:
missing edges and incorrect join tangents can still produce valid triangle indices.
Regenerate the specimen with `python design/tool-icons/generate_proposal.py` after
changing catalog paths. The preview's Bold silhouette comparison is exploratory;
it does not control production rendering.

Implementation deliberately preserves existing dock layouts. Shared docking
behavior remains governed by DOCK.md and TOOLBARS.md. Further metaphor or stroke
changes require actual-size visual review, not just enlarged artwork.

The shape-selection strip uses the catalog Fill bucket, Ellipse stroke ring, Corners outline, Filters funnel, Pages grid, and Display for File Atlas Formatting. These share the same path stroke and squircle chrome as the primary dock; applications do not paint private substitute glyphs. [DYNAMIC_PANELS.md](DYNAMIC_PANELS.md) owns the surrounding panel composition and distinguishes squircle icon buttons from circular color swatches, filter radios, and rail handles.

## Bumper

`Bumper` is two pucks touching at one point, with three short impact marks above
the contact. It maps to the selection-strip squircle for Bumper cars
(`board.shape.edit`, `contracts/bumper-cars.md`), shown only when Preferences →
Configure → Tools → Bumper cars is on. The pucks deliberately touch, so the
2-unit internal gap rule gives way at that one point: contact is the metaphor.
It is not Ellipse (one ring) or Join.

## Pages

`Pages` is a 2×2 grid of sheets: the poster-page album and `board.media.unbundle` (spread a PDF/PowerPoint deck onto the board). It is not Media (stacked family sheets), Image (landscape frame), or Frame (dog-eared page). Command mapping: selection-strip squircle → album browse (`board.media.page`) and Unbundle (`board.media.unbundle`).

## Conversation presentation glyphs

ChatTrain uses two separated linked cards; ChatWindow uses a single tall transcript with alternating lines. ChatBundle encloses miniature cards in one container, matching the supplied bundle reference. ChatUnbundle separates those cards with outward arrows. Bundle/Unbundle map to the agent selection strip; Train/Window accompany the contextual presentation menu. These use the shared 24-unit outline catalog and avoid reusing the unrelated geometric Join/Split tools.

ChatPairs is the ChatWindow card holding one pair: a short message bubble at the upper right and two reply lines below it. Stop is an open rounded square, the outline form of the composer's stop button. The chat card's ellipsis menu gives every row a glyph through `menu::Row::glyph`: ChatWindow, ChatTrain, ChatPairs, TextDoc (full conversation), Agent (choose program), Stop, Fit, with the menu family's Lock and Trash for full access and deletion.

## Agent portal

`Agent` is the agent-portal tool glyph: a robot face with two side antennae,
hollow ear loops, a rounded head, two eyes, and a short mouth. Stroke is 1.25
so those loops and the face stay open at dock size; other glyphs stay at 1.5.
It is not the Portals family mark (nested rounded openings) and not a provider
tile. Command mapping: `board.portal.agent` and the Portals flyout row.

## Agent provider identity (21 September 2026)

The user-approved agent workflow uses vendor marks for Codex (OpenAI blossom),
Cursor and Ollama. This is an exception to the generic tool-glyph rule, confined
to provider selection. The same cached vector catalog owns these paths; original
view boxes are fitted without distortion and no outline is added to filled marks.
Sources: https://cursor.com/brand (2D cube),
https://github.com/ollama/ollama/blob/main/docs/ollama-logo.svg,
https://github.com/simple-icons/simple-icons/blob/14.0.0/icons/openai.svg (CC0
vector of the OpenAI blossom). Trademarks remain with their respective owners.
