# 3D preview maintenance

The board has one 3D viewport. It draws a [`PreviewScene`](../crates/model-preview/src/lib.rs)
and does not know which file produced it. `crates/model-preview` is the
owner of that scene and of the format registry. `slate-doc::media` is the
owner of which extensions place as a 3D model node. Those two lists are the
same set; `every_model_extension_has_one_preview_owner` fails if they drift.

An Enscape standalone is not a mesh. It stays a program Slate can recognize
and open. It is not a row in the extension list.

## What a preview is

Triangles, normals, an optional flat color, and bounds. The viewport orbits
that, freezes a poster, and the HTML export links the poster to the original
file. Materials, lights, animations, and walk controls stay in the
application that authored them.

`notes` on the scene record content that was skipped (OBJ materials, glTF
lines, Draco). The card does not print those notes yet. A later inspector
line should read `notes` rather than invent a second status.

## Registry

| Extension | Reader | Spec this build targets | On a future release |
|-----------|--------|-------------------------|---------------------|
| `.3dm` | `rhino-mesh` | openNURBS 8.x layout, archive version ≥ 50 (Rhino 5+). Cached render meshes, breps, extrusions. Curves are still skipped. | When McNeel ships an archive Rhino 5+ files do not already use, add a fixture under `crates/rhino-mesh/tests/fixtures` and extend that reader. Do not teach the viewport about chunks. |
| `.obj` | `model-preview` | Wavefront `v` and `f` (polygons fanned to triangles). | New statements (`cstype`, free-form) stay ignored until a preview needs them. Record the statement here when you start reading it. |
| `.stl` | `model-preview` | Binary (80-byte header + count + 50 bytes per triangle) and ASCII `facet` / `vertex`. | Color extensions (`COLOR`, `Material`) stay ignored. |
| `.gltf`, `.glb` | `model-preview` via the `gltf` crate **1.4.1** | glTF 2.0 triangles, strips, and fans. Positions, indexes, normals, base color. Sibling `.bin` and data URIs. | Bump the `gltf` pin in the workspace `Cargo.toml` when taking a crate release, and re-run `cargo test -p model-preview`. Draco, sparse accessors, skins, and animations are explicitly skipped. Remote `http` buffers are refused. |
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
- glTF textures, animations, and skins.
- Rhino blocks, SubD, and curves (still a `rhino-mesh` follow-up, not a new viewer).

## Checks

```powershell
cargo test -p model-preview
cargo test -p slate-doc --lib media
cargo test -p slate --lib every_model_extension_has_one_preview_owner
```
