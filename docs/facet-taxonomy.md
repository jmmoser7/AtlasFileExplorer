# Facet taxonomy — how file types are classified

Companion to `CONSTITUTION.md` (Articles III and VII) and `ROADMAP.md`
(Phase 5). This document is the classification scheme for every kind of
material the tool handles, present and future.

## The principle: facets, not a hierarchy

Files are not sorted into a tree of types. A file type is described by the
**set of capabilities (facets) it exhibits**, and tools bind to facets, not
formats:

- The crop tool appears for anything `Raster`.
- Text search appears for anything `Text`.
- The page navigator appears for anything `Paged`.
- The outliner appears for anything `Composition`.

This is what keeps tooling "common and consistent" across formats (the
user's core ethos) while keeping the 10% rule honest: a facet's tools are
implemented **once, well**, instead of being re-implemented per format.
Adding a format becomes declarative — declare its facets, plug in a decoder,
and the UI accommodates automatically.

## The facets

| Facet | A file has it when… | Example tools it unlocks |
|-------|---------------------|--------------------------|
| `Raster` | it contains pixel imagery | crop, filters/adjustments, zoom-to-pixel |
| `Vector` | it contains resolution-independent geometry | path editing, stroke/fill inspection |
| `Text` | it contains extractable text | search, excerpting, snippet cards |
| `Paged` | it has discrete pages or sheets — **including print output** (drawing sets, competition boards) | page navigation, sheet layout, print-faithful export |
| `Spatial3D` | it contains 3D geometry | orbit viewport, section cuts |
| `Timeline` | it plays over time | scrub, trim, poster frame |
| `Structured` | it has machine-readable internal structure (code, data, tables) | outline, graph extraction, queries |
| `Composition` | it references other documents as parts | outliner, dependency view, link health |

Facets are deliberately few. A new facet is a constitutional-scale decision
(it implies a family of tools); a new *format* is routine.

## Starter matrix

| Format | Raster | Vector | Text | Paged | Spatial3D | Timeline | Structured | Composition |
|--------|:------:|:------:|:----:|:-----:|:---------:|:--------:|:----------:|:-----------:|
| JPEG / PNG / WebP | x | | | | | | | |
| GIF (animated) | x | | | | | x | | |
| SVG | | x | x | | | | | |
| PDF | x | x | x | x | | | | |
| DOCX | | | x | x | | | | |
| XLSX / CSV | | | x | | | | x | |
| Markdown / TXT | | | x | | | | | |
| Source code (.rs, .py, …) | | | x | | | | x | |
| Cargo workspace / repo | | | x | | | | x | x |
| MP4 / WebM | x | | | | | x | | |
| MP3 / WAV | | | | | | x | | |
| Rhino .3dm | | | | | x | | | |
| glTF / GLB | | | | | x | | | |
| IFC | | | x | | x | | x | x |
| Point cloud (E57, LAS) | | | | | x | | | |
| USD / USDZ | | | x | | x | | x | x |
| PSD | x | | | | | | x | |
| AI / EPS | x | x | x | x | | | | |
| JSON / TOML / YAML | | | x | | | | x | |
| `.slate` workbook | — | — | — | — | — | — | — | — |

`.slate` files are constitutionally special: they **never become items** —
every add/drop path opens them as tabs instead (see `AGENTS.md`), which is
what makes workbook recursion impossible. They have no facet row on purpose.

Notes on the matrix:

- PDF is the canonical multi-facet case: raster + vector + text + paged, so
  it picks up tools from all four families.
- `Paged` covering print is deliberate — an architect's output lives in
  sheets, and print-faithful export (Roadmap Phase 5) is a `Paged` tool, not
  a format feature.
- USD/IFC carrying `Composition` reflects their reference/assembly nature;
  the outliner they unlock is the same one a repo gets.

## Migration path from `MediaKind`

Today `crates/slate-doc/src/media.rs` classifies by extension into a closed
`MediaKind` enum (`Image`, `Video`, `Model`, `Pdf`, `Text`, `Doc`,
`Workbook`, `Other`), and both the board painter and `slate-artifact` match
on it. That enum is the seed of this taxonomy, and it evolves in Phase 5:

1. `media_kind(path)` grows into `facets(path) -> FacetSet` (extension-based
   at first; sniffing later where extensions lie).
2. Kind-specific decode paths become **decoder providers** registered per
   facet (thumbnail, excerpt, mesh, page raster).
3. Tool menus and inspectors query the facet set instead of matching the
   enum; board and artifact stay two interpreters of one classification
   (Constitution Art. IV).

Until that refactor lands, `MediaKind` remains the single taxonomy both
renderers must agree on — do not fork it per-app in the interim.
