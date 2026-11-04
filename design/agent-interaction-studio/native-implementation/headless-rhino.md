# Headless Rhino — validated proof and production handoff

Date: 30 October 2026 (America/New_York). Scope: headless kernel feasibility only; no production adapter is claimed implemented.

## Verified proof

The local Rhino 8 installation successfully generated a real `.3dm` file without requesting a Rhino document window.

- Kernel: `C:/Program Files/Rhino 8/System/RhinoCommon.dll` and its installed native Rhino core.
- Host: x64 .NET Framework console probe, compiled with the installed Microsoft C# compiler; no new NuGet/package dependency was installed.
- Important initialization: `[STAThread]` on Main, then `new Rhino.Runtime.InProcess.RhinoCore()` using its documented headless default, and `Rhino.RhinoDoc.CreateHeadless(null)`.
- Work: create a 12 × 8 × 3 box as a Rhino Brep, add it to an isolated headless document, mesh it with Rhino `Mesh.CreateFromBrep`, and save the six generated mesh objects to a version-8 `.3dm` in meters.
- Output: `target/studio-native-baseline/20261030-204228/headless-box-sta.3dm`.
- Output size: 14,035 bytes.
- SHA-256: `58009d2ad7f2fd37f5fea718e58de479fb06f3d9ff2d3084a529e97b57b1b4fe`.
- Result: process exit code 0; log reports six objects and successful output path.
- Evidence: `RhinoProbeSTA.cs`, `RhinoProbeSTA.exe`, and `rhino-probe-sta.log` in that same ignored baseline directory. Initial root probe and log are retained separately.
- Startup was approximately 35 seconds on this machine. During initialization, the STA probe's MainWindowHandle was 0. No visible Rhino document was requested or observed. This is not a full native UI screenshot/gesture certification.
- The initial non-STA probe was stopped only after validating its exact executable path. Both probe processes are now exited. No user's Rhino application was stopped.

The earlier apparent startup hang was not enough evidence of a license failure. The successful run establishes that the installed runtime/license can execute this headless geometry case. It does not establish arbitrary plug-in compatibility, parallel worker licensing, future license availability, or a completed Slate integration.

## Supported implementation direction

Slate remains the Rust UI/application. A small isolated C# Rhino host is the SDK bridge, because RhinoCommon is a managed SDK. Keep it out of the Slate paint thread and process. Do not add a separate viewport, mesh parser, document model, agent composer, or wire protocol.

Use the existing `slate.application` protocol and its `SurfaceMode::Headless`, command descriptors, admission, isolated-draft authority, cancellation and artifact transfer. The current interactive worker requires `NativeFullUi`; preserve it as the optional existing path rather than relabeling it headless.

### Proposed owned files for the next worker

1. `tools/headless-rhino/HeadlessRhino.cs`: STA kernel host, closed validated geometry operations, isolated document lifetime, output generation and observable public operation log.
2. `tools/headless-rhino/build.ps1`: locate installed Rhino and compiler, compile into the repository target directory, never bundle commercial Rhino DLLs or alter licensing/security configuration.
3. `tools/headless-rhino/README.md`: prerequisites, build/run, supported operations, limits and explicit headless behavior.
4. `crates/atlas-connector-runtime/src/headless_rhino.rs` with a child test module: bounded typed geometry-plan validation, host worker/child lifetime and result verification. Add a connector executable only through the existing runtime transport/session APIs, not a new JSON/HTTP service.

Root retains integration ownership of Cargo manifests/lockfile, module declarations, application connector worker/action glue, pack registration and native commands. These changes have not been made by this proof task.

### Proposed command/API contract

- Advertise a closed command such as `org.slate.rhino.headless.generate` with `CommandEffect::DraftMutation`. It is available to an agent only after the existing v0.2 isolated-draft grant admits it.
- Parameters are a finite structured geometry plan: units, bounded operation list, stable operation/object IDs and validated numeric dimensions/transforms/references. No arbitrary Python/C# eval, shell text, plug-in loading or caller-selected output filesystem path.
- For the first end-to-end implementation, boxes, spheres, cylinders and polyline/extrusion operations are enough to prove real generation. More supported Rhino operations may be added explicitly; do not claim the full Rhino API is exposed when it is not.
- Generate into a new owned output workspace/document. Imported source geometry, if supported, remains a read-only reference; original-file write-back is a separate existing authority and command.
- `InvokeCommand` returns bounded counts/units/bounds/provenance and a new revision token after actual completion. Publication uses the existing authenticated `ArtifactTransfer`/`PublicationResult` path and checksum/receipt verification, not an arbitrary file path in a wire reply.
- A completed artifact is ingested through the existing `model-preview` / `rhino-mesh` / Slate model3d invalidation path. Preserve native camera, units, display modes, picking, capture and shared thumbnail caches.
- Preserve the original NURBS/Brep model where possible and include derived render meshes for Slate preview. The feasibility probe saved only the six preview meshes; it is not the final artifact schema.
- Public progress must distinguish starting Rhino, creating geometry, meshing, writing and verifying. Show actual generated operations/script output, never private reasoning or invented completed stages.
- Use one serialized Rhino STA worker initially. Treat cancellation honestly: stop queued work; cooperatively cancel between bounded operations, and retain an uncertain result if an uninterruptible SDK call may have completed. Do not kill or mutate a user's external Rhino document.

## Required integration tests

- Real headless generation -> verified transfer -> native Slate model preview, without a Rhino document window.
- Finite/bounded geometry validation, unsupported operation refusal, degenerate/NaN/infinite dimensions and reference cycles rejected.
- Missing installation/license, startup timeout, cancellation, meshing/save failure and stale generation retain prior accepted output.
- Agent request without the matching isolated-draft grant is rejected; wrong source/revision and source write-back remain denied.
- Actual `.3dm` units/bounds/mesh content, multiple options, independent history branches and cold/warm startup feedback.
- Existing interactive Rhino connector and native model renderer regressions remain covered by their current tests.

## Primary references

- [RhinoCore API](https://developer.rhino3d.com/api/rhinocommon/type/rhino.runtime.inprocess.rhinocore/) documents the default headless constructor.
- [RhinoDoc.CreateHeadless](https://developer.rhino3d.com/api/rhinocommon/rhino.rhinodoc/createheadless) documents isolated headless documents and lifetime ownership.
- [McNeel Rhino.Inside workshop sample](https://github.com/mcneel/Rhino.Inside-Workshop/blob/master/samples/Sample-1/Program.cs) uses STA and NoWindow.
- [Rhino code-driven file IO](https://developer.rhino3d.com/guides/rhinocommon/code-driven-file-io/) documents headless document import/export.
- [xFigura Visione announcement](https://xfigura.ai/news/inside-london-creates-a-first-look-at-visione-xfigura-s-new-design-agent/) is design inspiration, not public evidence of its headless implementation internals.
