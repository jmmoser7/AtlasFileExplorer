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
- Use one metaphor. Frame uses extending rails; Rectangle is a closed shape;
  Arc is smooth; Polyline has corners; Bezier shows control handles.
- Primary icons represent the family or panel. They keep their identity when a
  child tool changes. Actions is a wrench; Document settings is a page with sliders;
  Object properties is a subdivided object; Selection is a bounded object.
- Portals uses nested rounded openings: outer fillet 2 units, inner fillet 1 unit.
- No bitmap tool glyphs, provider logos, emoji, gradients or decorative effects.

## Media family

Media uses stacked sheets as a stable primary family icon. Image uses a landscape
frame for flat/print media, Model uses a cube for the existing Rhino viewer, and
Video uses a play frame. All four share the 24-unit catalog and 1.5-unit outline;
there are no optical exceptions or per-app copies.

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
