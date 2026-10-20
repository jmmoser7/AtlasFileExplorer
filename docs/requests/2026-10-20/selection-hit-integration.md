# Approved selection regions — native integration

User approval: “lock this in and apply to the full project then build and push
to github”, 20 October 2026. The final sandbox establishes the design; it does
not establish native behavior. The agreed contract is
`../../keymap/contracts/selection-hit-regions.md`.

Baseline: `2087cc176473f752a8124f37be431dcf41600ef5` (preview.175).
Branch: `codex/selection-hit-integration-20261020`.
The normal checkout and its Git index remain untouched. Existing four dirty
suggestion-box acceptance records and native evidence remain outside this commit.

## Work ledger

- [x] Approval captured; shared pattern, D01–D17 contract and decisions locked.
- [x] DRY ownership gate approved shared shell hit policy and existing adapters.
- [x] Native adapters integrated; scoped formatting, source ceilings and JSON checks pass.
- [x] Independent intent/regression review and scoped Quick health review complete.
- [ ] Standard Windows release build: attempted, exit 101 (dependency execution blocked).
- [ ] Exact-build native interactive evaluation: Blocked; no release executable produced.
- [x] Commit and push the exact owned paths to GitHub; remote ref verified.

## Acceptance boundary

The earlier local build encountered OS error 5 in generated build helpers.
Keep toolchain, flags, profile, dependencies and normal output directory stable;
retain diagnostics under ignored `target/review/selection-hit-20261020`.
An unexecuted native case remains Untested or Blocked. Do not close original
bug reports or mark this interaction contract shipped from sandbox tests alone.

Native cases: four-size zoom hierarchy; tiny move envelope; continuous edge and
rotation regions; unchanged top-object move/scale/rotation zones over rear
objects; interior-only overlapping top wire hits; adjacent corner geometry ties
in both orders and rotated poses; compact port recovery at high zoom; selected
buried frame/move label; undo/redo/cancel and native curve/portal exceptions.

## Final implementation and retained review

The shared shell policy owns projected-size smoothstep prominence, dormant fine
regions, cursor expansion, continuous edges/rotation arcs and control frames.
Existing native adapters keep authored transform anchors, snapping, modifiers and
journal ownership. Compact controls capture the difference between displayed
and actual anchors before resizing. Fillet capture retains its displayed press
position without jumping. Fully buried unselected controls are suppressed;
explicit selection can recover a coherent outside frame and clear move header.

Native overlap and adjacent-corner arbitration use one cached document-aware
geometry owner. It retains rotated/rounded/trimmed silhouettes and linked text-card
corner defaults; scene generation, source-path revision, item count and tolerance
bucket invalidate derived data. Broad-phase queries reuse the scene spatial index.
Hidden nodes and slide frames do not turn a host into a stack. Ordinary and
revealed raster output rails now agree even on compact hosts. Port paint/cutouts
remain bounded editing overlays at extreme zoom; normal rail datums and durable
wire slot identities remain authoritative in both interpreters.

The independent intent reviewer found no remaining blocker in the final scoped
source review. Review fixes have retained tests: 24 native selection-region cases,
14 geometry cases, and focused shared-field, adapter, port-presentation and compact
rail cases. Their final-source execution results are **pending**. Removed obsolete host-only outward48
and inverse-zoom assumptions were replaced by approved continuous-field cases;
selected-connector endpoint editing retains its separate existing contract.
No report has been closed or archived by this integration.

## Validation evidence

- Scoped `rustfmt --check --edition 2021 --config skip_children=true`: exit 0.
- Changed Rust physical-line ceilings: pass; no allowlist expansion.
- Decisions JSON parses; approved D01–D17 and existing entries retained.
- Approved browser sandbox: 58 Node tests, 58 pass, 0 fail. This is design evidence.
- Acceptance checklist browser QA: real IAB input locked run identity, saved all
  39 expectations as Blocked, exported JSON and rendered the checklist. Initial
  catalog is entirely Untested; this separate run records the build blockage.
- Standard build: `. ./scripts/_msvc-env.ps1` then
  `cargo build --locked --release -p slate -p native-file-atlas`: **exit 101**.
  Quote and serde generated helpers could not execute: Windows OS error 5.
  Cargo never reached Slate compilation. Later source-review fixes therefore
  remain uncompiled too; the blocked attempt is not an exact-final-source pass.
- Toolchain: rustc 1.97.1 (8bab26f4f, 2026-07-14), cargo 1.97.1
  (c980f4866, 2026-06-30). No toolchain/profile/flags/dependency/output change,
  cache cleaning, elevation, security change or denial retry was used.
- CodeIntegrity, AppLocker and Defender readable logs had no matching events in
  the attempt window. OS error 5 does not identify the enforcing control.
- Release tests, canonical Cargo contract/kit/size gates, shortcut installation
  and native UI launch remain unperformed behind this build boundary. No release
  `slate.exe` exists in this worktree. Do not substitute an older executable.

Retained logs, denied artifact hashes, exported blocked run and browser screenshot:
[selection-regions/evidence](selection-regions/evidence/).
User checklist: [selection-regions/index.html](selection-regions/index.html).
After execution approval changes, run the unchanged release build, required
release workspace suite and contract/kit/size checks, then the nine native paths
on the exact binary. Record native and user observations separately before marking
this contract shipped or resolving any original report.

## Git handoff

Implementation commit: `35a0ab93a41071379bd55d070fc09a3878e6cc40`.
Pushed branch: `codex/selection-hit-integration-20261020` on
`https://github.com/jmmoser7/AtlasFileExplorer`.
A documentation-only follow-up records push completion and removes a duplicate
copy of the build log. Source remains the reviewed implementation above.
The shared primary checkout index hash remained
`1A24C4CBE899D976D71D38A6401FD2C7F260EEBEF111B1400952BAE7A9E2C575`.
Its dirty files and the preexisting four acceptance records/native evidence in
this worktree were neither staged nor committed.

## Hosted validation follow-up

Draft stacked PR65: https://github.com/jmmoser7/AtlasFileExplorer/pull/65.
CI198 and Windows installer176 checked out synthetic merge
`7ba59b196e0f2cc3cfc6ebb4f3b64be8246438bb` (head fd31f2a1, base2087cc17).
CI198 reached the real compiler and found private-module API paths for vector
booleans, an ambiguous test type, and two explicit-f32 requirements on the newer
hosted Rust1.99.1. The follow-up uses the existing public vector-ink exports and
explicit types. The older fillet-priority regression now probes the approved
compact frame/continuous bands rather than legacy authored-edge slop. No expected
native outcome was weakened. CI199–200 additionally exposed extracted-helper
visibility, cache-stamp type complexity and newer Clippy diagnostics; these were
fixed in the owning modules. CI201 for `1e1466861306c55b60a7dedc7c9316993a54cc41`
passed the Windows workspace check and Linux all-target Clippy; the full Linux
suite was still running when the next coherent source snapshot was prepared.
These results do not cover the following camera corrections.

## Actual camera reachability correction

The first native point-scale regression forced `cam.z = 128`, bypassing Slate's
real 3.5 maximum. That could not establish the user-visible recovery promised by
the approved studio. Board now consumes `atlas-core::display::SLATE_BOARD`
(1/128–128, exponential wheel coefficient .0015) through wheel, camera commands,
fit and workbook restore; compatibility profiles retain their own limits.
Authored objects and groups already smaller than the creation minimum retain
their size on the first resize press. Creation still has its original minimum.

Supporting the approved overview range also required correcting old .05/.01
zoom guards in existing screen/world conversions. Curve and connector picks,
direct/trim picks, object and guide snaps, live-wire target reach, render AA,
paint culling, sheet/canvas-strip conversions and portal paint now consume the
Board minimum. Geometry-flattening tolerances, authored dash rules, softness,
portal input ownership and capability gates retain their existing semantics.
No new input interpreter or portal host was introduced.

The eight additional native regressions exercise real wheel and camera paths,
cursor anchoring, save/reopen at point-detail zoom, stationary tiny resize and
one-step undo, low-zoom stroke/connector picks, wire snap bands, live-stroke
feedback, and snap distances. Independent source review found no remaining
blocker. Scoped formatting and changed-source size ceilings pass; all final
runtime and Windows packaging results remain pending hosted execution. Native
computer-use acceptance remains Blocked by the recorded local execution denial.
