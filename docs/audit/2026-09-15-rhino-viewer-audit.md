# Rhino viewer audit and stabilization plan

Date: 2026-09-15. Initial audit below; the follow-up repair and validation are recorded first.

## Follow-up: `Untitled.3dm` navigation repair

The second supplied file exposed a different, now repaired failure. Its 71 surface objects use Rhino's `TL_Brep` class identifier (`f06fc243-a32a-4608-9dd8-a7d2c4ce2a36`). Slate only recognized `ON_Brep`, so it skipped every object and showed a static embedded preview. McNeel maps this identifier to `ON_Brep` in [ON_ClassId::ClassId](https://github.com/mcneel/opennurbs/blob/8.x/opennurbs_object.cpp). The fix routes this known alias to the existing Brep decoder.

- Native extraction now returns **423 parts, 26,718 vertices, 27,390 triangles**. Counts and mesh bounds match independent McNeel `rhino3dm` results exactly. One native read/parse took **38.205 ms**; this is not a frame-rate benchmark.
- Frozen viewports retain the last displayed texture while rendering/writing their poster. The idle interval starts after the first frame, and loading cannot auto-lock the viewport.
- Explicit reactivation retries a previous parse failure. Byte reads now use the existing cloud guard. Mesh posters have a versioned cache key so older incomplete posters are not reused.
- Save finalizes live camera poses. Opening a new tab freezes the old tab, and closing a tab checks for unsaved changes after freezing.
- Native UI validation on this exact source confirmed orbit, wheel zoom, idle freeze, resume at the same camera, and save while live. A separate validation build/workbook was used; the source model was not edited.
- Automated egui input tests cover orbit, Shift-drag pan, and wheel zoom while asserting that the board camera and model frame stay fixed. Further tests cover retained textures, idle timing, camera save, and close protection.

Evidence: [before](2026-09-15-rhino-evidence/untitled-reader-before.log), [after](2026-09-15-rhino-evidence/untitled-reader-after.log), [independent inventory](2026-09-15-rhino-evidence/untitled-reference-inventory.json), [saved camera](2026-09-15-rhino-evidence/untitled-validated-view.slate).

The earlier curve-only sample remains unsupported. The broader worker scheduling, poster I/O, geometry coverage, and material/layer-color work below remains planned. A dedicated **3D Model** icon is recommended for discovery; this repair extends the existing native viewport and does not add a new portal kind.

## Conclusion

The supplied model exposes a missing geometry type, not a slow parser. The file found at `C:/Users/jmoser/Downloads/Grid_AA_Stacked.3dm` contains **2,989 curves and no surfaces or meshes**. Slate's reader intentionally skips curves and returns `NoMeshes`. Increasing a timeout or saving this curve-only file from a shaded viewport will not make the current reader display it.

Build native curve viewing first, and make the last successfully displayed frame survive every transition between live viewing and an idle still. Then remove expensive work from the UI thread and expand compatibility against a representative model corpus.

## Evidence and limits

- The originally supplied path, `C:/Users/jmoser/Downloads/Grid/_AA/_Stacked.3dm`, was absent. The similarly named file directly in Downloads was used and the substitution was announced in chat.
- Source: 721,780 bytes, Rhino archive 80, millimeters; ordinary local Archive attributes. The source was only read. Each probe gates byte reads through `atlas_core::cloud::is_dehydrated`.
- Slate reader: `NoMeshes`, **1.691 ms** for one read/parse. This is a single diagnostic observation, not a latency benchmark or a GPU measurement. [Raw result](2026-09-15-rhino-evidence/reader-probe.log).
- Independent inventory with McNeel's `rhino3dm 8.35.0` WebAssembly library: **2,555 LineCurve, 305 ArcCurve, 129 PolylineCurve**; zero B-rep faces, standalone meshes, or block definitions; 10 layers, all visible. The Z range is -5,000 to +15,000 mm: the curves occupy 3D space. [Inventory and source SHA-256](2026-09-15-rhino-evidence/reference-inventory.json).
- `cargo test -p rhino-mesh`: **21 unit tests and 8 integration tests passed**. The five bundled files cover tiny synthetic mesh/B-rep/extrusion cases and a line-only `NoMeshes` case. They do not demonstrate real-model viewing coverage.
- The latest Slate session snapshot and log tail show Home-screen stalls with zero scene nodes. They cannot attribute a stall to Rhino. The 3D upkeep call and renderer currently lack dedicated session-log spans.
- Audit baseline: HEAD `6338b1c` plus the existing dirty working tree. This audit inspected that working tree. No live GPU interaction, rendered screenshot, freeze/reopen round trip, or performance benchmark was run. Code-path findings below are distinguished from the reproduced reader failure.

## Existing design worth keeping

The user-facing 3D portal is currently an `ImageNode` with a journaled `ModelCamera`, rather than a `PortalNode` hosting Rhino. Its native implementation is `apps/slate/src/app/model3d.rs`; the parser/model owner is `crates/rhino-mesh`. HTML export already prefers a per-node camera poster and links to the source asset.

Parsing happens off-thread. Models share CPU/GPU geometry by cache key. A live viewport only renders when its camera or quantized size changes. Three viewports can be live, and the current idle timeout is 30 seconds. The board already has a thumbnail fallback, lock control, and basic camera command registrations. Extend these owners; do not add another board painter or a foreign-application host to fix loading.

## Findings

### R1 — P1: valid curve-only Rhino files are invisible (reproduced)

`crates/rhino-mesh/src/lib.rs:220` filters object records to meshes, B-reps and extrusions. Curves, SubD, points and instance references are skipped. `NoMeshes` is returned when no triangle parts survive (`lib.rs:172`). The independent inventory proves that the user's sample is a valid curve-only model and explains this particular failure.

**Fix:** add renderer-independent 3D line, arc and polyline representations to the existing model owner, decode these types, and draw cached line geometry in the existing renderer. Mixed curves/meshes must share camera bounds and appear together. Add a coverage report listing supported, unsupported and malformed object counts; partial output must be labeled.

### R2 — P1: the freeze handoff depends on another render and a disk write (code confirmed)

`model3d.rs:861` removes the live viewport, including its existing texture, before making a fresh 1,600-pixel poster. `save_poster` (`:1228`) performs PNG encoding and file writes synchronously and discards errors. The following paint opens and decodes that PNG synchronously (`:995`). There is no transfer of the last live texture to a frozen slot. A failed render/write can therefore lose the displayed angle and fall back to the generic file thumbnail; the handoff itself can stall the UI.

**Fix:** retain the last good texture immediately, tagged with the camera that produced it. Freeze presentation and resource eviction are separate actions. Encode/write asynchronously, publish the cache entry atomically after success, and retain the previous good entry on failure. If input has changed the camera since the last render, queue that final render without blanking the old frame or labeling it as the new pose prematurely.

### R3 — P1: save and workbook transitions do not consistently finalize live poses (code confirmed)

`save_doc` and `save_doc_to` (`mod.rs:1018`, `:1129`) serialize the document without finalizing the live camera. `close_tab` checks `dirty` before calling `lock_all_models` (`:967`), so a clean document whose live camera changed can become dirty after the close guard and then be removed. `new_tab` (`:943`) changes the active document without freezing models; opening another nonblank workbook uses this path. Live state is keyed only by per-document `NodeId`, so matching IDs in a new document can also refer to old live state. `on_exit` (`:1927`) only releases leases.

**Fix:** one existing document-transition owner finalizes camera intent before save/close decisions and document identity changes. Scope runtime viewport and pending-work identity to workbook + node + source revision. Preserve normal unsaved-change behavior; shutdown must not silently auto-save a workbook. Save while live, Save As, close, new/open/switch workbook, Home, presentation and export all need explicit coverage. No additional hand-written focus helper.

### R4 — P1: passive poster generation can download cloud-only models (code confirmed)

Painting a locked node without a poster queues `request_model_poster` (`board.rs:1663`, `model3d.rs:1026`). That reaches `read_counted` (`:484`), which opens and reads the whole source without the cloud guard. A worker thread does not prevent hydration. Several visible models can therefore start source downloads just from displaying the board.

**Fix:** use `atlas_core::cloud` on the worker before any byte read, and distinguish cloud-only, unavailable, missing and ordinary local sources. Keep a cached still or thumbnail visible. Any hydration is an explicit human action for that source using the existing owner. This is a general loading defect; the supplied Downloads file is local and did not exercise it.

### R5 — P1: loading is asynchronous but unbounded, and recovery is sticky (code confirmed)

`request_model` (`model3d.rs:595`) starts one thread per new key and uses unbounded channels. It reads the complete source into memory. The three-live-view limit does not cap passive poster loads. Failed entries are not evicted by the ready-only CPU cache policy and prevent another request for the same key. Results carry a key but no request generation/cancellation token. Auto-lock counts from unlock, even while loading. Board fallback text turns any parse failure into a missing-mesh suggestion (`board.rs:1713`).

**Fix:** a bounded, prioritized load queue, shared per-source work, cancellation/generation checks, byte-based memory limits, typed failures and explicit Retry. Treat loading separately from idle interaction; a long load must not silently revoke the user's activation. Record read, decode, upload and first-frame stages. A known-good still remains visible during refresh or failure.

### R6 — P1: substantial render work still runs on the UI thread (code confirmed; not timed)

`ModelEngine::upload` (`model3d.rs:1399`) sorts and repacks the whole model before one large GPU upload. Each changed view (`:1503`) allocates MSAA and resolve resources, draws, synchronously reads pixels back, flips/converts buffers, deletes resources and uploads the result as an egui texture. The poster queue (`:937`) processes every requested node without a frame budget. Measurement hover raycasts every triangle (`:276`, `:1090`).

**Fix:** prepare immutable geometry off-thread; retain render targets by resolution bucket; budget uploads and poster jobs. Prefer keeping the live image on the GPU, with a scoped integration check for egui clipping, rotation, opacity and texture lifetime; perform readback only when capture/export requires it. Use a spatial index for large-model picking. Keep immediate viewport interaction prioritized over background poster refinement.

### R7 — P2: cached stills are hard to recover cheaply (code confirmed)

An auto-fit camera cannot look up its poster until in-memory bounds exist (`model3d.rs:1000`). After restarting, an otherwise cached initial view may require another source parse. Poster textures are counted, not budgeted in bytes; crossing 64 clears the entire map (`:747`). Disk cache filenames have source key, camera hash and aspect, but no renderer/parser version. Ready CPU entries use a count cap; failed entries and bounds have no equivalent bound. GPU eviction occurs on upkeep, so the linger deadline also needs a scheduled wake when no other work remains.

**Fix:** a versioned poster manifest that can resolve an authored camera without loading geometry; byte-based LRU retention with the visible last-good still pinned; atomic replacement and bounded cleanup. Keep source revision and rendered-camera identity with every still. Old-source stills remain available with a visible stale indicator until replacement succeeds. Persisted images are derived cache data, not source-model copies inside `.slate`.

## Proposed interaction lifecycle

The user explicitly requested an active view that becomes a preserved still when unused. The exact inactivity trigger remains a recommendation, not an approved contract change.

1. **Frozen:** display the saved angle. No 3D render, geometry decode or source read just to maintain an unchanged still.
2. **Activate:** retain that image while the necessary source work runs; display a specific loading state or actionable failure.
3. **Interactive:** resume the saved camera. Render only changes; load curves and meshes through the same model/camera lifecycle.
4. **Leave the viewport:** recommended trigger is loss of contents focus, not the pointer briefly crossing its edge. Immediately retain the last displayed frame. A short idle policy while still focused can be tuned separately; do not apply the existing 30-second timer to a pending load.
5. **Persist:** journal changed camera intent and write the derived still asynchronously. Both identify the same source revision and pose. GPU geometry can linger briefly for quick re-entry, then leave the byte-budgeted cache. The still remains.
6. **Resume / refresh:** show the previous still until a replacement is ready. If the source disappeared, was changed or is cloud-only, preserve the still and explain its status. No automatic jump back to a default angle.

Save/export must capture the intended pose deterministically. Export waits asynchronously for the required capture or reports a specific failure; it must not silently substitute a generic thumbnail for an already authored view. Ordinary board painting remains independent of this wait.

## Delivery sequence and acceptance gates

### 1. Make the supplied model viewable and errors truthful

- Extend `rhino-mesh` with LineCurve, ArcCurve and PolylineCurve decoding and stable bounds; cache curve tessellation rather than rebuilding it on paint. Keep analytic input and approximation tolerances explicit.
- Add model coverage/status data and truthful UI messages. Restore layer visibility and object/layer color semantics for the supported geometry.
- Create small shareable fixtures for each curve type, a mixed curve/mesh file and a partial-support file. Use the supplied file as an opt-in local regression input, without committing its source bytes.
- Gate: all **2,989** sample curves accounted for, visible at appropriate zoom, correct units/bounds, and the existing 29 reader tests still pass. Compare rendered appearance against a reference view before declaring visual parity.

### 2. Make freeze, save and reopen reliable

- Transfer the last good display texture into frozen state; store posters off-thread and atomically; use a versioned manifest and workbook-scoped identities.
- Route save and document transitions through the existing ownership points; distinguish live camera intent, the last rendered camera and the persisted still.
- Gate: orbit → leave → reactivate, Save while live → reopen, new/open/switch/close workbook, presentation/export, source offline, failed poster write and deleted node during load. No blank-frame transition; saved pose and exported still agree; undo restores the prior authored view.

### 3. Bound loading and rendering cost

- Cloud guard, bounded jobs and memory, prioritization, cancellation, Retry and dedicated activity spans. Reuse render targets and reduce live readback; budget GPU uploads/posters. Preserve aspect ratio when capping resolution and fit both horizontal and vertical bounds.
- Gate: multiple visible models and repeated open/close stay inside declared CPU/GPU byte budgets; cancelled results never replace current content; cloud-only sources are not hydrated by passive viewing.
- Proposed performance goals, to measure on this machine: warmed interaction p95 at or below **16.7 ms** on the sample and agreed representative models; first interactive view of this local sample within **1 second**; a resident frozen frame survives the transition on the next paint. An idle unchanged viewport performs **zero additional 3D draws/uploads/source reads** after queued persistence work finishes. These are targets, not achieved measurements.

### 4. Extend coverage against evidence

- Test Rhino 5/6/7/8, curve-only and mixed geometry, cached and uncached B-reps/extrusions, nested/transformed blocks, SubD, hidden layers, large coordinates, corrupt/truncated inputs, network and cloud sources, and multiple views of one model.
- Next geometry support is chosen from actual missing-object reports. Blocks require definition lookup and transforms; SubD and uncached surfaces require separate coverage decisions.
- McNeel confirms that Save Small omits cached render meshes. Its public openNURBS toolkit does **not** include surface tessellation. A switch to that reader alone is therefore not a general missing-mesh fix. Evaluate an optional local Rhino-assisted mesh preparation path only for actual surface cases, with explicit user initiation and no source write-back. Curve-only files like this sample must work natively without that step. See [Save Small](https://wiki.mcneel.com/rhino/savesmall), [openNURBS limitations](https://developer.rhino3d.com/guides/opennurbs/what-is-opennurbs/) and [reading stored render meshes](https://developer.rhino3d.com/en/guides/opennurbs/reading-render-meshes/).

## Architecture and scope

Keep geometry and decode/coverage contracts in the pure crate; runtime loading, GPU work and transient state in the current 3D capability; authored camera in `slate-doc`; independent export interpretation in `slate-artifact`. Extend the command registry for user-facing Retry/Refresh or camera actions. Source-specific controls stay on the viewport/selection inspector, per P1.portal.local-ui. Changes to gesture semantics should be captured through the tool-contract workflow before implementation.

This follows Constitution I (pure capability boundaries), II (bounded asynchronous work), IV (truthful geometry and export), VI (journaled intent), IX (local source handling) and XII (one owner). No new `board_*.rs` module or portal host is needed for the initial work. Any later plan introducing one must pass the required DRY review. This task delivers the audit and game plan requested, not a production implementation.
