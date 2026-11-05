# Videos interaction contract

Status: draft · 4 November 2026. No visual approval claimed.
Family: media canvas specimen
Command: ephemeral Studio playback; no workbook mutations
Inherits: S07, S08; production time and replay policy
Owner: `atlas_core::video`; `atlas_core::display::SLATE_BOARD`; `atlas_shell::theme`, `canvas_scale`, `canvas_text`
Scope: standalone Rust host, three real local video specimens; this does not certify ordinary Slate entry or production board integration.

## Preserved baseline

Slate remains a linker. Sources are linked paths, never rewritten. Cloud placeholders are refused before decode. Actual Windows Media Foundation output supplies every displayed video pixel. The built-in `crates/atlas-core/tests/fixtures/video-motion.mp4` is a real local motion clip.

## Requested journey

Launch Video Studio from phase 03 Videos. Click Play or the video body once to start; click again to pause. Replay restarts at the in-point. Loop is explicit. Scrub uses decoded filmstrip samples. Switching clip stops the old play request and rejects its stale tokens. Open video and file drop are Studio chrome outside the specimen. Playback and camera state are ephemeral, so undo/redo is not applicable.

No changes are locked by this draft. Production Slate's board overlay, authored trim/options, selection/movement, document journal, presentation overrides, audio and export are outside this host's verified scope.

## Behavior matrix

| Trigger | Expected visible result | Owner / evidence |
|---|---|---|
| Play from paused/end | Real frames advance; end replay uses shared origin policy | `VideoPool::play`, `video::playback_origin` |
| Pause / switch clip | Current token stops; last actual frame stays | `VideoPool::stop`; Studio event token adapter |
| Replay | Restart from trim in-point | Production decoder replay policy |
| Loop toggle during play | Restart current play with requested looping | Production decoder loop policy |
| Scrub slider | Nearest retained decoded sample and timecode | `VideoPool::request_strip`, production timecode |
| Missing codec / malformed clip | Visible failure, then another clip may play | Production decoder failure event |
| Cloud placeholder | Refuse before opening bytes | `atlas_core::cloud::is_dehydrated` |
| Pan / zoom | Right/middle/Space-drag and pointer-anchored zoom | `SLATE_BOARD` profile; Studio input adapter |

## Constants and reuse

Production decode edges and 96 filmstrip positions remain owned by `atlas_core::video`. UI event/texture work is bounded to eight events per frame. The shared 40 MiB `STRIP_CACHE_BYTES` budget evicts whole older strips and retains all 96 positions in the current strip. Play and strip request tokens are independent, so production preempt/resume policy remains authoritative. Studio caps linked specimens at eight and diagnostic dimension history at 64 entries plus an overflow count. Queue bytes come from real `VideoPool::health`; texture RGBA bytes are an estimate, excluding driver allocations. Slate's palette and canvas text scaling supply appearance; `atlas_shell::video_controls` owns visible Play/Pause geometry and paint for both hosts. This host does not copy the private Slate `VideoBoard` controller or import the Slate application. File picking uses the production `DialogGate`.

## Golden paths

1. Launch → Play first fixture → observe changed pixels/timecode → Pause → Play → Replay.
2. Toggle Loop → play beyond duration → disable Loop → reach Ended → Play again.
3. Scrub → switch clip → replay first → reject stale events. V10's observed target-sample gates and follow-up native scrub callback passed.
4. Open/drop a local clip → play → malformed source failure → valid clip recovery.
5. Run real decode stress in Studio chrome; inspect decoded frame count and failures. The smoke report is not a codec, leak or long-duration stability guarantee.
6. Clear → ordinary Open video picker → baseline reload → 96 positions → one-click playback. V8 component lifecycle proof passed; its earlier binary receipt remains separate from V10.

## Verification and limits

Early native slice built in 5m03s after a 90-second check. Root observed one-click Play with advancing real fixture frames and an active Loop checkbox. An immediate early AX snapshot initially suggested a Pause failure. That inference was not conclusive: native AX may lag an action by one frame, and settled V3 screenshot plus subsequent AX confirmed Paused at 0.625. The initial observation remains in capture metadata/history; it does not establish a proven production Pause defect. The source token-invalidation safeguard remains justified by review independently of that inference. The early phase 00 capture keeps its original `video` image identity and source/binary receipt. The color-bar motion fixture is the known test-pattern reference, not an observed stripe defect.

V3 component UI passed settled Play/Pause and verified helper crash → different clip → original-clip retry; its captures and binary/crash receipts remain preserved. V8 passed Clear to zero clips/cache/textures/worker, ordinary picker reload with 96 positions, and one click producing eight real frames then Ended. Root visually verified those four historical Video images and composers. V10's new pass capture and source/binary/runtime receipts are under `assets/video-studio/20261104-v10-pass/` and registered under the same Video tab. This contract records component evidence, not ordinary Slate integration, release or design approval.

`video_canvas.exe [path ...]` accepts local clips; `--stress` starts the bounded exercise. Use the explicit five-path set below for the supported-corpus gate. Actions wait for observed frame/time progress, actual target samples and failure-before-recovery evidence, with an unchanged ten-second bound. Required decode checks need every Play stage to advance; adaptive playback must show both dimensions. Only the first specimen prefetches a full strip; other strips are demand-driven. Receipts archive prior attempts as well as updating `target/video-studio/stress-latest.json`. Optional WebM failure is distinct from required fixture failure. Scripted scrub does not certify pointer hit testing; resize remains an interactive case. UI byte counters exclude codec/driver allocations and full-process memory.

### Current decoder evidence and remaining gates

**V10 passed: 25 core + 12 real decoder + 3 harness gate tests.** The adaptive 96-position benchmark completed in **2.806 seconds**, with **one seek and 116 reads**, below the unchanged ten-second gate. Coverage includes adaptive pixels, lossless bounded strip delivery, all 96 positions, interruption, trim/replay, context cancellation and helper fault recovery. Actual receipts remain under `target/video-studio/v10-*`, with final source/binary/test metadata also copied beside the V10 gallery capture.

**V10 native stress passed 92/92 observed actions in 97.744 seconds:** 162 actual playback frames across five supported clips including WebM, one expected malformed-input failure and successful advancing-frame recovery. Peak retained RGBA was **41,889,600 ≤ 41,943,040 bytes**; peak queue **1,536,000 ≤ 8 MiB**. Follow-up one-click Play advanced 162→384 frames; settled Pause stayed at 384/Idle. Native slider interaction changed 0.4→0.49 with decoded sample/image time 0.50; AX setValue also invoked the real callback. Earlier hardcoded coordinates missed the small track and did not establish a slider defect. The stress capture precedes these follow-up input observations, which have separate metadata.

**V10 standalone native entry passed:** the visible phase 03 Video button called the fixed POST launch route and opened a new native window with the same V10 binary SHA-256. The primary poster loaded 96 positions; second and third controls each played once through Ended with actual pixels. The clean three-specimen capture is now phase 03's latest preview, while the full stress capture remains in phase 00. `dashboard-launch.json` and `native-ui-proof.json` beside those assets preserve launch identity and follow-up input facts. Studio was left open for review. Packaged Slate integration, full CI and publication are separate gates; this is not a release approval.

Historical V9 stopped at adaptive Scrub with 32 positions after ten seconds; V7's fixed-delay and V8's eager-backlog failures also remain retained. V10 fixed strip throughput with a bounded forward cursor while preserving 96 indices, 0.08-second tolerance, 120-frame seek scan and the ten-second action deadline. The original V9 Action-10 JSON was recovered byte-exact from its automatic archive (SHA-256 `eab0803a5ddf27317e8d341415e161a086564173ed73de4d97ef06e390354261`) rather than retaining a mangled pretty-copy. No previous failure receipt is promoted to a pass.

V4 traced raw presentation origin **0.25 seconds** and a stale **400 × 300**, pitch-1600 layout against **133,120 bytes** at the adaptive change. Clock/queue changes alone were insufficient. V7 seeks in presentation seconds without adding the composition offset, normalizes decoded timestamps against the immutable first non-preroll origin, and follows current Media Foundation flags/types. Video time zero is that first non-preroll frame; leading gaps are normalized. There is no audio-sync claim. Integer display apertures are supported; fractional apertures fail gracefully. Unsupported codecs must fail cleanly and allow retry/another clip. The backend gates above now pass; native/app/release gates retain their separate scope. Studio displays actual frame timestamps; its stress clock only schedules actions.

Independent PyAV **16.0.1** reference decoding proved the adaptive fixture contains a genuine encoded near-black frame at **0.983 seconds**: RGB histograms **25,368 × (0,0,0), 1,708 × (1,1,1), 2,924 × (2,2,2)**, matching Media Foundation. All other frames satisfy the strict red/green checks. Receipt: `target/video-studio/adaptive-pyav-reference.json`; source SHA-256 `7fa7d6c9754d3314811911e553b8dc75e0d4d50cc3a4731497e493be44554636`. The V7 pixel test permits only that exact known histogram in the first second; no production suppression was added for this frame. PyAV exists only in ignored `target/python-tools` for verification, with no product dependency or global installation.

An earlier corpus attempt rejected the real `2x2-green.mp4`. Microsoft's [H.264 decoder format constraints](https://learn.microsoft.com/en-us/windows/win32/medfound/h-264-video-decoder#format-constraints) specify a **48 × 48 pixel minimum** plus profile/chroma constraints. The 2 × 2 case is a separate unsupported-boundary/recovery case, not a supported decode pass. Its rejection and the later supported adaptive-clip failure are distinct; neither excuses the other. MP4 extension alone does not establish compatibility.

The reported affected PC has Intel Iris Xe, 16 GB RAM and a captured 97% memory load with roughly 439 MB available; the local test GPU is Iris Plus. The bounded strip cache and frame queue exclude native codec allocations, driver/GPU residency and other process memory. No intentional machine OOM or physical multi-GPU certification is claimed. The observed adaptive layout/buffer mismatch does not by itself prove the affected PC's visible stripe defect.

| Explicit Studio stress fixture | Required observation |
|---|---|
| `crates/atlas-core/tests/fixtures/video-motion.mp4` | Actual advancing decoded frames; known color-bar motion reference |
| `target/video-studio/corpus/400x300-red-resize-200x150-green.mp4` | Actual dimension change and red/green output, including only the independently verified encoded near-black boundary frame |
| `target/video-studio/corpus/test-1s.mp4` | Actual decoded playback frames |
| `target/video-studio/corpus/test.mp4` | Actual decoded playback frames |
| `target/video-studio/corpus/test-1s.webm` | Decode if the codec is available; otherwise clean failure followed by supported MP4 recovery |

The local WPT corpus is verification input, not shipped content; its manifest preserves source URLs/digests. Tiny H.264 remains a separate unsupported-boundary/recovery case. V10's supported-corpus component stress is green; V3 recovery and V8 lifecycle retain their earlier binary scopes. Ordinary app integration, resize coverage, release validation and physical Iris Xe verification remain separate. The encoded black frame, codec limits and colored baseline do not establish the affected machine's reported stripe defect.

No synthetic frames are shown as decoded evidence. Layout-only synthetic stride fixtures belong to backend tests and must be labeled separately from real decoding. Current process isolation, helper packaging, app integration and release proof belong to their respective owners and are not established by the early capture.

## Revision control

Draft v0.1. No decisions.json approval entries added. Subsequent receipts must retain failed attempts and identify source/binary/fixture identity.
