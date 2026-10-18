# 3D preview maintenance

The board has one 3D viewport. It draws a [`PreviewScene`](../crates/model-preview/src/lib.rs)
and does not know which file produced it. `crates/model-preview` is the
owner of that scene and of the format registry. `slate-doc::media` is the
owner of which extensions place as a 3D model node. Those two lists are the
same set; `every_model_extension_has_one_preview_owner` fails if they drift.

An Enscape standalone is not a mesh. It stays a program Slate can recognize
and open. It is not a row in the extension list.

## What a preview is

Triangles, normals, UV0, standard material factors, optional base-color
textures, cached mesh feature edges, and bounds. The viewport orbits that,
freezes a poster, and the HTML export links the poster to the original file.
Imported material data is renderer-neutral; lights, animations, and walk
controls are not imported.

`notes` on the scene record content that was skipped (unsupported material
features, glTF lines, Draco). The card does not print those notes yet. A later inspector
line should read `notes` rather than invent a second status.

## Registry

| Extension | Reader | Spec this build targets | On a future release |
|-----------|--------|-------------------------|---------------------|
| `.3dm` | `rhino-mesh` | Rhino 5+ cached mesh/B-rep/extrusion geometry, UVs, standard material diffuse/opacity/emission/gloss fallback, object/layer material assignment and display colors. | PBR/RDK and plugin user data remain unsupported. Curves/blocks/SubD still need geometry work. |
| `.obj` | `model-preview` | Wavefront polygon geometry, normals, UVs and standard MTL scalar/base-map subset. | Linked MTL reads go through the caller's cloud-safe dependency reader. Procedural/free-form geometry remains unsupported. |
| `.stl` | `model-preview` | Binary (80-byte header + count + 50 bytes per triangle) and ASCII `facet` / `vertex`. | Color extensions (`COLOR`, `Material`) stay ignored. |
| `.gltf`, `.glb` | `model-preview` via the `gltf` crate **1.4.1** | glTF 2.0 triangles, strips, fans, normals, UV0, PBR scalar factors and base-color image locators/embedded images. | Dependency reads are caller-owned. Draco, sparse accessors, skins, animations, non-base texture maps and remote buffers remain unsupported. |
| `.blend` | none | — | Same card: "No preview for Blender files yet." A reader lands only when it can emit a `PreviewScene` without embedding Blender. |
| `.dwg`, `.dxf` | none | — | Same card, DWG/DXF copy. A DXF reader, if it ever exists, is still only preview geometry. |
| `.skp` | none | — | Same card, SketchUp copy. |
| `.fbx` | none | — | Same card, FBX copy. |
| Enscape `.exe` | sniff only | ASCII or UTF-16LE `EnscapeStandalone` or `EnscapeClient.exe` in the first 16 MiB or the last 1 MiB. | When a Chaos release moves that marker, update `enscape.rs` and the byte-sample test. Do not commit a real executable. Do not parse the scene out of the package. |

## Adding a format

1. Add a reader module under `crates/model-preview/src` that returns a `PreviewScene`, or a gap string if the honest answer is "not yet".
2. Register the extension in `FORMATS` in `lib.rs`.
3. Add the same extension to `MODEL_EXTENSIONS` in `crates/slate-doc/src/media.rs`.
4. Add a fixture test in `crates/model-preview/tests/readers.rs`. Prefer a few dozen bytes written in the test over a binary blob.
5. If the new reader changes geometry for files the old reader already opened, bump `CACHE_KEY_VERSION` in `apps/slate/src/app/model3d.rs` so frozen posters are not reused.
6. Update the table above, including the spec version you actually tested.

Do not add a second viewport, a `board_<format>.rs`, or a portal kind for a file that can become a `PreviewScene`.

## White-card and diagram rendering (October 2026)

The existing `model3d/gpu` interpreter serves live views, frozen posters,
screenshots and generator captures. It uses retained 4x MSAA targets; live
frames stay on the GPU. The renderer revision and saved camera/material state
invalidate posters. Static geometry, feature-edge buffers and the light-space
shadow map are uploaded once per model, not once per frame.

- **Arctic:** neutral white surfaces and background, Z-up hemisphere fill,
  a fixed directional key, model-fitted soft shadow mapping, and an automatic
  ground receiver slightly darker than the model. A bounded horizon-style
  depth AO pass is filtered against depth discontinuities. AO uses full
  viewport resolution through 1.2 million pixels and filtered half resolution
  above that threshold. AO attenuates the ambient contribution and includes
  a restrained white-surface bounce allowance. This is real-time raster
  rendering; off-screen occlusion and physically traced indirect light are
  not represented.
- **Hidden line:** neutral or material-colored diagram fills with cached
  welded boundaries/creases and camera-dependent silhouettes. Depth-tested
  antialiased triangle strokes keep approximately constant pixel width and
  suppress coplanar tessellation diagonals. Slope-aware polygon offset on
  filled surfaces keeps the visible feature strokes continuous. These are
  mesh feature lines, not Rhino Make2D/NURBS seam/intersection extraction.
- **Materials:** an independently journaled on/off control in Shaded and
  Hidden line. Arctic always keeps its white-card override. Material mask
  remains segmentation; Z-buffer remains inverse depth. Imported base maps
  are decoded on the parse worker, capped at 2048 pixels per edge and a
  shared 64 MiB decoded-image budget. Missing/cloud-only maps fall back to
  scalar color without downloading placeholders. Opacity is retained for
  future glazing support; this renderer still draws opaque surfaces. Normal,
  roughness, metallic texture maps, advanced Rhino PBR and Enscape shaders
  are not interpreted.

External MTL/texture edits are not independently watched or fingerprinted in
the model's source cache identity yet. Reparse the model after changing those
dependencies. First GPU admission still packs geometry, uploads buffers and
textures, generates mipmaps, and builds the shadow map synchronously on the
UI thread. It has no geometry-size, per-frame time, or upload-chunk budget.
The texture memory/decode limits do not bound this first-use frame cost.

Slate renders its supported standard-material subset independently, without
invoking Enscape or requiring its runtime. Technical support and runtime
licensing are separate from content rights: use user-owned or independently
licensed maps and verify permission for Chaos library assets. A texture
reference saved in Rhino does not grant reuse rights. The
[Chaos EULA](https://www.chaos.com/legal/eula) includes materials/bitmaps in
Content, limits their use to designated products, and restricts separate use
of licensed materials and product components. Viewing an already exported
Enscape standalone is a distinct workflow; Chaos says its recipient needs
no Enscape license. [Official standalone guidance](https://blog.chaos.com/enscape-for-sketchup-get-started)

### Research and implementation decisions

The supplied architectural references call for broad soft shadows, subtle
contact/cavity darkening, near-white surfaces and legible feature lines.
The implementation order is import/state foundations, one retained GPU
pipeline, soft shadows and AO, hidden lines, then real-context visual and
cache/performance validation. This keeps the existing portal authority,
journal, capture and export owners.

Primary references:

- [Rhino Arctic](https://docs.mcneel.com/rhino/8/help/en-us/options/view_displaymodes_arctic.htm)
  establishes white surfaces, soft shadows and ambient occlusion.
- [XeGTAO](https://github.com/GameTechDev/XeGTAO) informs bounded AO sampling
  and depth-aware filtering. Its HLSL compute implementation is not embedded.
- [Visibility-bitmask AO](https://arxiv.org/abs/2301.11376) addresses finite
  occluder thickness; it is a future improvement for thin railings/foliage,
  not a claim about this initial horizon-based pass.
- [Filament material behavior](https://google.github.io/filament/main/materials.html)
  motivates applying AO to indirect light rather than dirtying direct light.
- [NVIDIA soft shadows](https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-8-summed-area-variance-shadow-maps)
  informs blocker-dependent penumbrae; the current bounded implementation
  uses depth comparisons and a fixed sample budget.
- [Rhino Technical](https://docs.mcneel.com/rhino/8/help/en-us/options/view_displaymodes_technical.htm)
  distinguishes silhouettes, boundaries and creases from raw mesh wireframe.
- [McNeel rendering assets](https://developer.rhino3d.com/en/guides/opennurbs/accessing-rendering-assets/)
  describes the richer openNURBS/rhino3dm import route without running Rhino.
  The current reader implements the standard fallback subset, not that full API.

`model_render_quality_real_gl` is an ignored real-context check. It writes
rendered images and device/timing evidence under `target/render-quality`,
checks shader/FBO health, opacity, known white roof points, silhouette
coverage and live/poster parity, and verifies retained live rendering
creates no GL objects and performs no readbacks. The import tests
include Rhino 5/8 material fixtures plus OBJ/glTF and mesh-edge regression cases.

Validation on the reference host, 2026-10-15: `cargo check --locked -p slate
-p model-preview -p rhino-mesh -p slate-doc --all-targets` passed. Import-test
binaries initially could not execute. After approval retries, all 34 executed
import/geometry tests passed (10 unit/metadata, 6 material, 8 reader, 1 Rhino
material and 9 Rhino fixture tests). The Rhino library test executable
`target/debug/deps/rhino_mesh-4c7e2d6154f0319c.exe` still returned `Access is
denied (os error 5)`. The native Slate model suite reran successfully on the
same executable: 118 passed and 2 ignored. The separate
`primary_click_outside_a_focused_page_releases_focus` test also passed.
`cargo fmt --all -- --check` passed.

The final real-GPU check passed on Intel Iris Xe / OpenGL 3.3 and produced exact
live/poster pixels in five display/material cases and at 1920×1080 Arctic,
with opaque output, zero steady live GL-object creation/readbacks, and 1.0
vertical silhouette coverage on the fixture. At 900×650, GPU-finished median
times were 9.190 ms for Arctic and 2.276 ms for Hidden line. Full-HD Arctic
measured 11.653 ms median; the slowest of 12 samples was 16.169 ms. Cold
upload measured 66.976 ms submission and 127.685 ms through GPU completion,
excluding shader construction. These small-fixture measurements establish
neither sustained 60 fps nor representative large-model performance; the
synchronous first-admission limitation above remains and can cause a first-load hitch.

The check samples known white roof points independently of the ground tone.
Retained evidence is in
`target/arctic-final-check.log`, `target/arctic-import-retry.log`,
`target/arctic-slate-refined-tests.log`, `target/arctic-focus-tests.log`,
`target/arctic-gpu-refined-tests.log` and `target/arctic-gpu-final-tests.log`.
The current `xtask` helper compiled successfully, but Windows also denied
`target/debug/xtask.exe`; current-source size and contract checks could not
execute. Their logs are `target/arctic-size-current.log` and
`target/arctic-contract-current.log`. The three document camera-state tests
also compiled, but `target/debug/deps/slate_doc-c0d54de1c3d11686.exe` was
denied before execution; see `target/arctic-camera-tests.log`.
The subsequent `cargo build --locked --release -p slate --bin slate` passed,
and the release shortcut script completed without errors. The actual
`target/release/slate.exe` opened Home from the repository working directory;
its window remained responsive, and its matching session-log PID recorded
`first_frame` and 32 completed frames. Evidence is retained in
`target/arctic-release-build.log`, `target/arctic-release-smoke.json`,
`target/arctic-release-smoke-first.json` and
`target/arctic-release-smoke-final.json`. This is a release Home-window
smoke check; viewport pixel validation used the real-GPU harness above.
The full release workspace test suite was not rerun. The three previously
denied test/check executables still require approval before retrying.
Follow `windows-builds.md` for the remaining executable approvals. To
rerun the real-GPU check using the retained build profile:

```powershell
cargo test --locked -p slate --lib model_render_quality_real_gl -- --ignored --nocapture
```

The existing real-window `bench_model3d` also includes Hidden line in its
live/screenshot parity cases. Set `BENCH_MODEL_DISPLAY=arctic` or
`BENCH_MODEL_DISPLAY=hidden_line` to run its orbit/idle/frame-cost scenarios
with that display mode, using the same build profile retained for approval.

## Enscape standalones

Recognition is a bounded read, off the UI thread, and only after
`atlas_core::cloud::is_dehydrated` says the file is local. Reading any byte
of a dehydrated OneDrive file would download the whole executable, so a
cloud-only `.exe` stays an ordinary file card for that session.

A confirmed file uses the model card. Double-click parents the standalone's window to that card. Clicking away
parks it: the process stays alive for this Slate session and the card shows
the last frame, so the next double-click does not launch again. Only one
Enscape process is kept. The first launch happens off the UI thread.

The parked frame is what a wired ComfyUI generator receives. Enscape does not
expose a depth buffer, so the generator restyles that picture instead of
running the mesh depth pass used for Rhino. A new frame (after you click
away) is a new live render. Nothing in document load starts the process.

If this computer cannot run the file, the card and a toast say why:
not Windows, the file is missing or online-only, the card is rotated, or
the executable was built for a different Windows system (`ERROR_BAD_EXE_FORMAT`
/ `ERROR_EXE_MACHINE_TYPE_MISMATCH`). Those sentences live in
`apps/slate/src/app/enscape_host.rs`.

HTML export stays a poster (the captured frame, when one exists) plus a link.
The page cannot run the executable.

## What we do not keep current

- Enscape's renderer version, materials, or lighting. The standalone the user exported already contains that engine.
- Full Blender, AutoCAD, SketchUp, or FBX imports.
- Advanced material graphs, non-base texture channels, animations and skins.
- Rhino blocks, SubD, and curves (still a `rhino-mesh` follow-up, not a new viewer).

## Checks

```powershell
cargo test -p model-preview
cargo test -p slate-doc --lib media
cargo test -p slate --lib every_model_extension_has_one_preview_owner
```
