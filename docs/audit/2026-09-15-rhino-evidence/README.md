# Rhino audit evidence

These probes read one explicit source file; they do not edit it. `reader-probe.rs` calls the existing `atlas_core::cloud::is_dehydrated` guard before reading bytes. `reference-inventory.cjs` runs that guarded probe before its own source read. A nonzero guard result aborts the reference inventory.

## Results

- `reader-probe.log`: current `rhino-mesh` returns `NoMeshes` for `Grid_AA_Stacked.3dm`.
- `reference-inventory.json`: independent McNeel `rhino3dm 8.35.0` WASM inventory, including source SHA-256, geometry counts, units and bounds. The file contains only curves.
- Parser tests run during the audit: `cargo test -p rhino-mesh`; 21 unit and 8 integration tests passed.

The logged timings are individual observations with different scopes, not a benchmark comparison. The native time measures read/parse; the reference time includes read/parse and object inventory, excluding WASM initialization.

## Reproduction

From the repository root in PowerShell, build the libraries first if their artifacts are absent. For the recorded run, the native probe linked `librhino_mesh-f4b2eada79accdb1.rlib` and `libatlas_core-2af6d47c7ee4294f.rlib` from `target/debug/deps`. The former was built by the parser test command; the latter provided the existing cloud guard. HEAD was `6338b1c` with the pre-existing working-tree edits. No production code was changed by the audit.

Compile `reader-probe.rs` with `rustc --edition=2021`, the two `--extern` library arguments, `-L dependency=target/debug/deps`, and the native `windows_x86_64_msvc-0.52.6/lib` and `windows_x86_64_msvc-0.53.1/lib` directories from the local Cargo registry. Dot-source `scripts/_msvc-env.ps1` on this machine first. The output used was `target/debug/rhino-audit-probe.exe`.

Run that executable with the source path as its only argument. Run the independent inventory as:

```powershell
node docs/audit/2026-09-15-rhino-evidence/reference-inventory.cjs MODEL_PATH GUARDED_PROBE_EXE RHINO3DM_PACKAGE_DIRECTORY
```

The reference package was installed outside the repository at `%TEMP%/atlas-rhino-reference-js-20260915/node_modules/rhino3dm`, using `npm install --prefix <temporary-directory> --ignore-scripts --no-audit --no-fund rhino3dm@8.35.0`. Model bytes never leave this machine. A Python wheel was also tried in a separate temporary directory, but its DLL import failed; all reference results delivered here come from the successful WASM run.

Only aggregates and probe source are stored here; the user's `.3dm` is not copied into the repository. The app's live renderer, freeze transitions and exported appearance still need runtime validation during implementation.

## Follow-up repair: `Untitled.3dm`

The user subsequently supplied `C:/Users/jmoser/Downloads/Untitled.3dm` and reported nonworking orbit, pan and zoom. The 2,752,208-byte local file has SHA-256 `0fbed6ec3bd3743fb2908492382d5b86da905212fe2bbef54bc7b7297cf53e7e`. All 71 Breps use Rhino's TL_Brep alias, which the initial reader did not dispatch to the existing Brep decoder. Its nested cached meshes were valid.

- `untitled-reader-before.log`: native `NoMeshes`.
- `untitled-reader-after.log`: **423 parts, 26,718 vertices, 27,390 triangles**, read/parse **38.205 ms** for one run.
- `untitled-reference-inventory.json`: independent McNeel reader matches all render mesh counts and bounds. The Brep's analytical bounding box is slightly larger than its saved mesh bounds; these are different measurements.
- `untitled-validated-view.slate`: a separate validation workbook linking the original source, with the camera saved through Slate's UI. It contains no model geometry.
- Regression: changing the existing synthetic Brep fixture's class UUID to TL_Brep reproduced `NoMeshes` before the fix and now retains all six cached face meshes. An arbitrary unknown UUID still fails safely.
- `cargo test -p rhino-mesh`: 30 passed. `cargo test -p slate model3d --lib`: 18 passed, including full egui orbit/pan/wheel routing, retained frozen texture, load/idle timing, save, and close protection.

### Native UI verification

The debug build was copied to `target/debug/slate-rhino-validation.exe` and opened with `target/rhino-validation.slate`. The user's original Slate process and unsaved workbook remained open.

1. The initial poster showed the full canopy, generated from the repaired reader.
2. Double-click activated the viewport (cyan border and tool strip).
3. A left drag from (720,500) to (880,540) changed the view angle while board zoom stayed 100%.
4. A wheel input over the model enlarged the canopy while the board frame stayed fixed.
5. After 30 seconds without interaction, the live toolbar disappeared and the same view remained visible. A versioned PNG poster was written.
6. Double-click resumed that saved perspective. A second drag changed the camera again.
7. Ctrl+S while live froze the view and persisted target `[0,0,6070]`, yaw `-0.14539814`, pitch `0.74`, distance `86100.66` in the separate workbook.

Shift-drag pan is covered by the egui event test; the native automation tool does not expose modifier-held drags. This pass does not claim Rhino material fidelity, export validation, large-model performance, or support for the earlier curve-only sample. The source SHA-256 was unchanged across the before/after inventories.
