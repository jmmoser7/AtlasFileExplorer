# Headless Rhino SDK bridge

This isolated x64 STA console host uses the locally installed and licensed Rhino
8 kernel. `RhinoCore()` and `RhinoDoc.CreateHeadless(null)` create no Rhino
document window. Slate continues to use its existing native model viewport and
`slate.application` admission, revision and artifact contracts. The helper's
private job files are an SDK implementation detail, not another public protocol.

## Installed runtime requirements

Stable and preview installers place `HeadlessRhino.exe` beside `slate.exe`.
End users do not need this repository or a compiler. Generation requires a
separately installed, licensed Rhino 8 at `%ProgramFiles%\Rhino 8\System` and
.NET Framework 4.8. The helper resolves Rhino's managed and native dependencies
from that installation; neither Rhino DLLs nor a Rhino license ship with Slate.
The helper is a .NET Framework executable, not a modern .NET application, so it
does not need a `.runtimeconfig.json` or `.deps.json`. The release publisher's
.NET 9 installation runs packaging tooling only. Reading an existing `.3dm`
file's cached preview geometry does not require this helper or a Rhino license.

Claude Code or Cursor subscription use additionally requires the current user's
native Windows vendor CLI and subscription login, configured through Slate's
existing connection instructions. Those CLIs and credentials are not bundled;
the private Node runtime for the separate Cursor sidecar does not install them.

Packaging recompiles this helper. Record the final packaged binary's SHA-256
and verify an actual licensed SDK job from the installed location, outside the
checkout, rather than carrying forward a development binary's acceptance.

## Build

Run `./tools/headless-rhino/build.ps1` from the repository. Optional
`-RhinoSystem 'C:\Program Files\Rhino 8\System'` selects the installed SDK.
The script uses the installed .NET Framework x64 compiler and framework
`System.Web.Extensions`. When the local SDK is absent (including on release CI),
it downloads the official `RhinoCommon` NuGet package version
`8.18.25100.11001`, verifies SHA-256
`65b997c1dc357a5e8d1a1ef42e420b817e947899deb8551d7cf24f5b29c5cc62`,
and extracts only `lib/net48/RhinoCommon.dll` as a compile reference under
`target/headless-rhino-reference/`. See McNeel's
[NuGet build guidance](https://developer.rhino3d.com/en/guides/rhinocommon/using-nuget/).
It installs no Rhino runtime and changes no Cargo settings.
Output is `target/headless-rhino/HeadlessRhino.exe`. Rhino DLLs remain in their
installed directory and must never be redistributed with the helper. The cached
NuGet reference is not copied to the helper output or release package either.
At runtime the existing assembly resolver loads RhinoCommon and dependencies
from the explicitly configured installed Rhino 8 System directory; a licensed
Rhino installation remains required. The compile reference grants no runtime
license and does not install or activate Rhino.

## Supported declarative operations

`GeometryPlan` has `units` (`meters`, `millimeters` or `source`) and `operations`
(0..64; no operations creates/preserves an empty/existing model).
IDs contain 1..64 ASCII letters, digits, underscores or hyphens. Dimensions are
finite, positive and bounded; positions are finite and bounded. References must
name earlier operations; unknown operations/fields and forward/cyclic references
are rejected. Operations are:

- `box`: `id`, `origin` [x,y,z], `size` [x,y,z].
- `reference`: `id`, source geometry `object_id` GUID, as reported by
  native inspect with units, object names/kinds, layer and material metadata.
- `sphere`: `id`, `origin`, `radius`.
- `cylinder`: `id`, `origin`, `radius`, `height`; upright on world XY.
- `extrude`: `id`, `points` (3..128 XY pairs), `height`; closed planar profile.
- `mesh`: `id`, `vertices` (3..4096 XYZ points), `triangles` (1..8192
  triples of zero-based vertex indices), optional `closed`. Coordinates must be
  finite and bounded; indices must be valid integers and faces nondegenerate.
  Native Rhino validates the mesh. `closed: true` additionally requires
  `Mesh.IsSolid` (closed, oriented manifold); omitted/false permits open panels.
  Vertex indices and winding are authored data: no repair, welding or Boolean
  conversion is implicit.
- `appearance`: `id`, `target`, `diffuse` (three RGB bytes 0..255),
  `gloss` and `transparency` (each 0..1). Duplicates and consumes its earlier
  target like a transform, assigning an actual standard Rhino material through
  `ObjectMaterialSource.MaterialFromObject`. Diffuse becomes `DiffuseColor`,
  gloss becomes `Shine = gloss * Material.MaxShine`, and transparency becomes
  `Material.Transparency`. Plans without appearance retain the previous defaults.
- `move`: `id`, `target`, `vector` [x,y,z].
- `rotate`: `id`, `target`, `origin`, nonzero `axis`, `angle` in radians.
- `scale`: `id`, `target`, `origin`, positive uniform `factor`.
- `union`, `difference`: `id`, `left`, `right`; real Rhino Booleans, with
  tolerance 0.000001 model units. Operands must be Breps or extrusions; meshes
  are rejected explicitly. A failed Boolean is an error, never a proxy.

Transforms and Booleans consume their inputs; only unconsumed terminal geometry
is exported. Other operations can reuse earlier IDs to create explicit branches.
Output geometry carries operation IDs in names (or retained source names) and
`slate.operation` user strings. Generic GeometryBase duplication preserves source
geometry and assigned attributes across references/transforms.
Derived render meshes are cached on Brep faces, so exports do not contain
duplicate visible mesh objects. Indexed mesh results are real mesh objects;
existing `rhino-mesh` reads them and the cached Brep meshes. New material RGB,
shine, transparency and assignments are verified after the saved 3dm is reopened.
The native preview already shows standard diffuse and gloss. Transparency is
retained in the real file and preview data, but surfaces currently render opaque.
This change does not add transparent rendering, PBR or renderer plug-in support.

The shared connector's scalar parameter dialect admits
`{"plan":"<GeometryPlan JSON, at most 24576 UTF-8 bytes>"}` for
`org.slate.rhino.headless.generate` (`DraftMutation`). `parse_parameters` then
strictly deserializes and validates the closed plan before process launch.
Only this descriptor opts into the shared `application/json` string annotation;
ordinary strings retain their 4096-character limit and exact serialized parameter
objects remain bounded to 65536 bytes, including escaping and UTF-8.
The trusted source owner supplies a bounded isolated `.3dm` snapshot and origin.
Unrelated geometry, layers, materials and source units are retained. Relative
material textures are rebased through FileReference metadata without file reads.
There is no script evaluation, shell command, plugin loading or caller-selected
output path. Extend the enum, validation and SDK switch together
to add another supported operation; do not expose Rhino's arbitrary script API.

## Runtime integration

`HeadlessRhino::generate` is blocking worker code. Configure trusted absolute
helper/installation/workspace paths and a 1..300 second deadline (180 seconds
recommended; observed cold startup was about 35 seconds). One serialized job
runs at a time. Each gets a newly created owned directory. The helper validates
and saves a version-8 `model.3dm`, reopens it, writes bounded counts/units/bounds
and public operation progress, and exits. Rust bounds the report and artifact,
checks provenance and returns bytes only after success.

`HeadlessDraft` wraps the existing v0.2 `Session` with an isolated binding,
closed invocation admission and explicit host-granted agent draft authority.
Linked-source writes and interactive surface commands are unavailable. Completed
models enter the existing `ArtifactStore`; publication uses
`PublicationResult`, `ArtifactSender` and `receive_artifact` with checksum and
receiver receipt verification. Native callers still journal acceptance and check
their current request/generation before changing a model source.

Stop uses the shared `Cancellation`. Before launch it refuses the job; while
running it writes an owned stop marker and waits for an SDK boundary. After five
seconds it can terminate only its own helper process. An uninterruptible call is
reported as uncertain, with no publication. Previous accepted models remain the
caller's responsibility and must survive any failure, cancellation or stale job.

## Retained local evidence

`target/headless-rhino/proof` records a real box minus sphere, moved by [1,2,0].
The compiled helper exited 0, reopened its output and reported one Brep, seven
cached face meshes, 1187 vertices, 1092 faces, meters and bounds
[1,2,0]..[13,10,3]. This verifies SDK generation, not native UI acceptance.
The earlier STA feasibility probe remains under
`target/studio-native-baseline/20261030-204228`.

`target/headless-rhino/proof-all` exercises all nine operations in millimeters,
including actual union/difference, all three transforms and planar extrusion.
The reopened model retains three terminal Breps and 16 cached face meshes
(1458 vertices, 1272 faces). Its owned helper had `MainWindowHandle = 0` during
startup. The output passed the helper's saved-Brep/cached-mesh re-read checks.

## Native Slate entry points

`board.model.headless.new` creates a real empty model using the existing media
link/journal owner and binds the ordinary preferred text agent. Regular release
build/package scripts place the helper beside Slate. Stable package verification
requires it and its runtime notes. Local development builds may use
`-SkipIfUnavailable` when neither a reference download nor the compiler is
available; checksum failures always stop the build.

With a placed 3D model selected, `board.model.headless.activate` explicitly opens
an isolated headless capability binding. It leaves the native Slate viewport in
place. `board.model.headless.generate` takes command detail
`{"node":123,"parameters":{"plan":"..."}}`; `board.model.headless.stop` revokes
pending native leases. All three are registered native commands. The existing
interactive connector controls remain available separately.

After explicit headless activation, scoped agents use the existing
`board.model.connector.inspect` / native invoke tools to discover the generate
descriptor and submit a closed plan. They still need their existing source scope,
workspace autonomy and matching isolated-draft permit. Agent send never performs
activation or creates a permit. Completion returns a typed model result only
after verified artifact transfer and parsing through `model-preview`.

The app validates the current source/key and caller, wins the existing lease's
publication step, then changes only the model node's linked item through the
normal scene journal. Its authored frame, camera and styling survive; Undo
restores the prior linked item. A stale, cancelled or failed job cannot replace
the model. No source file is overwritten. Native UI acceptance and matching
screenshots remain separate review gates.

Focused real integration test (requires the built helper and a licensed install):
`cargo test --locked --release -p slate real_headless_generation_transfers_and_loads_native_preview -- --ignored --nocapture`.
