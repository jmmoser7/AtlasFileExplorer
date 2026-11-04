# Element design studio

The design iteration workflow is one element per tab, one concrete canvas, and a conversation with the user until behavior and presentation are agreed. Chat Train is the first tab and first locked reference. [AGENTS.md](AGENTS.md) records the workflow for future work.

The current per-element contracts record the accepted design baseline for the authorized native implementation. Their **Current authority and native baseline** sections and later owner instructions supersede first-pass proposals and historical amendments. Native acceptance remains a separate [acceptance gate](native-implementation/ACCEPTANCE.md); Studio approval and browser checks do not certify the Rust implementation.

## Open the Studio

Run `python serve.py` in this folder and open [the Studio home](http://127.0.0.1:8765/). Its graphic opens six workspaces; start anywhere, skip phases, or return in any order. The bundled static server revalidates edited files and needs no native renderer or compiled service. Opening a workspace starts no build or inference.

The **Launch Rust Studio** button on Home, the Studio tab and phase 03 Frames opens the latest built `target/debug/examples/frame_canvas.exe --large` in its own native window. It uses the fixed loopback launcher in `serve.py` and displays launch failures. Hover the button for the artifact modification time. Restart this server after changing its Python code. The button does not rebuild the executable.

| Workspace | Open Chat Train | Instructions |
|---|---|---|
| 00 Image Generation | [Concept gallery](http://127.0.0.1:8765/?phase=00&tab=chat-train) | [00 guide](phases/00-images/AGENTS.md) |
| 01 JS | [Existing browser Studio](http://127.0.0.1:8765/?phase=01&tab=chat-train) | [01 guide](phases/01-js/AGENTS.md) |
| 02 Rust Component | [Component workspace](http://127.0.0.1:8765/?phase=02&tab=chat-train) | [02 guide](phases/02-components/AGENTS.md) |
| 03 Rust Canvas | [Canvas workspace](http://127.0.0.1:8765/?phase=03&tab=chat-train) | [03 guide](phases/03-canvas/AGENTS.md) |
| 04 Integration Time Report | [Recorded integration work](http://127.0.0.1:8765/?phase=04&tab=chat-train) | [04 guide](phases/04-integration/AGENTS.md) |
| 05 Release Time Report | [Recorded release work](http://127.0.0.1:8765/?phase=05&tab=chat-train) | [05 guide](phases/05-release/AGENTS.md) |

Every phase uses the same element tabs and IDs: `chat-train`, `image-generator`, `dashboard-builder`, `text-agent`, `rhino-agent`, `frame`, plus named custom tabs. Chat Train stays first and the shared `studio` tab stays last for the workflow image gallery and global time audit. Change `phase=00` through `05` while retaining `tab=<id>`; an element may be empty until work is attached in that phase. Legacy `?tab=chat-train` (and other existing tab links) opens phase 01. The plus button adds one shared element, not a separate phase-specific tab.

The 02/03 pages are currently empty workspace scaffolds, not built native harnesses. Specific authorized Rust work must use the real shared production owners; opening these pages does not authorize native changes or builds. Reports 04/05 read actual recorded work after it happened, retain failures, and mark unknown durations **not recorded**. They cannot start builds or claim stable publication from integration evidence. The Instructions button exposes the current phase guide; Debrief opens the element's contract. Theme and review controls remain outside the element.

## Image baselines and feedback

Phase 00 shows captured current JS specimens alongside earlier concepts and workflow images. The [baseline catalog](js-baselines.js) records each image's identity, source URL/fixture, actual capture date, viewport, DPI/device scale, theme/zoom and relevant source hashes; dated PNGs and their [capture manifest](assets/js-baselines/20261103-162043Z/capture-manifest.json) retain that provenance. These are browser prototype appearance references, not native parity evidence or new contract approval. New captures append variants rather than replace gallery history.

Each image has a feedback composer beneath it, also available in its lightbox. Gallery and lightbox share an autosaved draft for that image on this browser/device; Ctrl/Cmd+Enter invokes the displayed action. Storage failures are shown rather than reported as saved.

When the host exposes `window.openai.sendFollowUpMessage`, **Send to chat** awaits its callback; a failure is shown and the text remains. When that bridge is absent, including an ordinary localhost page without a host bridge, **Copy request** copies the image ID/path, element, available capture context and typed feedback for you to paste into chat. Copying or saving a draft does not deliver it to an agent, start a chat turn or mean it has been processed. Both paths retain the draft; use the displayed result to distinguish submission, copy and failure.

The local Studio verified on 3 November 2026 uses **Copy request** and requires pasting into chat; no direct-send host connection was available in that review.

To have image comments addressed, send the image-linked request in chat and identify the intended image/variant. The agent must inspect that actual baseline and its associated text, preserve other variants, and apply only the authorized request. Local drafts are read only when explicitly requested; they do not silently become received tasks.

## Current references

- [Chat Train contract](contracts/chat-train.md): preserved baseline, 23 accepted changes, 35 in-scope behavior dimensions, feel constants, golden paths, owners and verification limits.
- [Shared Studio patterns](contracts/PATTERNS.md): menus, agent configuration, conversation feedback, geometry, selection, header feedback and review-tab structure.
- [Contract template](contracts/TEMPLATE.md): format for subsequent element debriefs.
- [Approved decisions](contracts/decisions.json): machine-readable decisions and change IDs.
- [Native design intent](native-implementation/design-intent.md) and [acceptance gate](native-implementation/ACCEPTANCE.md): implementation scope, retained capabilities and required native evidence.
- [Native baseline](chat-train/BASELINE.md): original font, token and renderer provenance.

Contracts describe the current agreement. WORKLOG.md records iterations and superseded experiments. The original survey and broad prototype remain preserved but are not the active review surface.

## Chat Train

The canvas supports independent cards, single windows, pairs and individual messages, unsent continuation/forks, three fold states, width-driven fitting, inline agent configuration, transient scroll thumbs, drag-first selection, Stop, undo/redo and local persistence. Reset example restores three long pairs. The picker includes limited/no-capability cases and a long-name stress fixture.

Replies, model availability, access and schedules are simulations. API-key entry saves only sample status; key text is discarded. No provider or native production permission is changed by the Studio.

## Source and checks

Images is a self-contained HTML/CSS/SVG canvas with small, medium, large, connected-input and history-wire examples. Native source measurements, font and icon geometry are translated into `image-generator/layout.js`, `view.js` and `style.css`. `app.js` handles canvas gestures, per-instance state and history connections. The shared agent picker and image-specific capability catalogue are retained. Its [current contract](contracts/image-generator.md) records the accepted native baseline and historical prototype limits. No provider is contacted by the Studio.

- studio.js / studio.css: tab shell and debrief access.
- contract-viewer.html: readable rendering of standalone Markdown contracts.
- chat-train/model.js: canonical conversation state and reversible projections.
- chat-train/agent-catalog.js: capability validation and labels shared by requests and picker.
- chat-train/agent-picker.js: compact agent control.
- chat-train/app.js / style.css: rendering, direct manipulation and sample replies.
- chat-train/model.test.cjs and capabilities.test.cjs: focused state/capability checks.
- chat-train/check-ui.cjs: earlier browser regression harness; claim only results rerun for the current revision.
- generator-ui.js: shared generator dialogue, composer, count and model-label assembly.
- elements/runtime.js / session-model.js: shared specimen runtime and pure session state for Dashboard, Text and Rhino.

The home and all browser phase pages run without a build step or native runtime. The phase 01 specimens are HTML/JavaScript/CSS; real Rust artifacts require separate authorized component/canvas work. Add `reference=1` to the Chat Train phase 01 URL for the comparison transcript, or `sample=1` for its long sample without reading or writing saved conversation state. The image tab uses local static assets only. Reload after editing HTML, JavaScript or CSS.

Shared navigation is defined in `canvas-navigation.js` and contract S08. The Images tab also covers image/tool selection, hover-only result strips and native image context-menu recipes. Mode and Settings live under the compact three-dot menu; custom aspect ratio is explicit.


## Current element behavior

- [Dashboard Builder contract](contracts/dashboard-builder.md): arbitrary native Web artifacts with the shared generator; its three-dot menu contains only Preserve model history. Native transactions, prior versions and undo remain authoritative.
- [Text Agent contract](contracts/text-agent.md): query a selected passage on its original source, follow its recorded answer, and explicitly apply a validated replacement only to that range. Querying or browsing an answer does not replace the source or restore model context.
- [Rhino 3D contract](contracts/rhino-agent.md): native model controls and shared agent dialogue, with headless model creation/editing through the existing authority and journal owners. The browser pavilion, progress and Python remain simulations.
- [Images contract](contracts/image-generator.md): native image tools and capability-aware configuration, shared dialogue, result previews and history/graft behavior.

Images, Dashboard, Text and Rhino share the generator dialogue/composer and capability-aware Provider/Model/Effort/Speed picker. Public progress, tool and script feedback appears in the continuous dialogue, with header working/fold controls and Stop. Output drag creates a blank closed downstream surface; one Preserve model history toggle governs in-place and downstream context, with a check reflecting its saved state. Terminal history cuts retain saved outputs while establishing a new context boundary. Saved-output circles and exclusive square previews follow the shared Images selection/hover rules. These behaviors are specified by [shared patterns](contracts/PATTERNS.md) and the current element contracts.


Browser replies, tools, credentials and access are simulated; persisted specimen state lives in localStorage. The Studio performs no live inference, source-file write or Rhino call, and its projected SVG model is not native geometry validation. The native global access preference must honor actual permission scope, and real provider/credential, headless runtime, export, journal and GPU behavior require separate native evidence. Consult [native validation](native-implementation/VALIDATION.md) for recorded results and remaining limits.

The [Studio debrief](contracts/studio.md), [native chrome audit](research/native-agent-chrome.md), [WORKLOG](WORKLOG.md) and earlier validation captures preserve the iteration history. Their first-pass staging controls and separate activity layouts are superseded where they conflict with the current contracts; they are not additional present-day controls or proof of native acceptance.


## Frames

Open [the Frames JS canvas](http://127.0.0.1:8765/?phase=01&tab=frame). The initial specimen is the native Letter frame, with independent rotation, small-frame, overlap, content and authored-style fixtures. Inspect, notes, exports and specimen controls belong to the Studio shell. Reset datum discards the current specimen experiment.

The [Frame contract](contracts/frame.md), [numeric baseline](frame/baseline.json), [source fingerprints](frame/source-baseline.json) and [acceptance protocol](frame/ACCEPTANCE.md) are the handoff for later Rust work. This is an unapproved source-measured datum. Numeric and browser checks do not certify native pixel parity; paired native captures are a separate required gate. No Rust code is changed or built by the Studio.
