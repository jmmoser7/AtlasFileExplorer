# Portal capture — interaction contract

Status: agreed
Family: tool
Reference: User request, 14 October 2026; current 3D viewport screenshot menu and media D36–D40
Command: `portal.capture` menu plus `portal.capture.still.{canvas,file}`, `portal.capture.video.start`, `portal.capture.pause`, `portal.capture.resume`, `portal.capture.save.{canvas,file}` and `portal.capture.cancel` (legacy video/stop aliases retained) · Key: none · Palette: "capture portal" (aliases: screenshot, snapshot, record portal)
Inherits: P0.* (all), P1.node, P1.shape.properties; source portal interaction remains governed by P1.portal and its subtype contract. Host backend reuse follows P2.PortalHost.
Canvas: `portal-capture-tool-contract`

The requested scope is fixed: capture in every portal blister, still images and
video where applicable, File or Canvas destinations, and the existing 3D capture
experience as the starting point. The user accepted the proposed defaults on 14 October 2026: "Use these defaults
and implement capture." All applicable matrix rows are approved. The contract
remains agreed until implementation and golden-path verification are complete.

Capture is an explicit creation of authored media from rendered content
(Art. VI.3). It does not change the source portal's authority or export
serialization. A portal that cannot supply frames explains that capability
state; it does not offer a control that silently does nothing.

## Behavior matrix

Every applicable registry dimension appears below, in registry order.
Portal-only and media-only dimensions are visible with applicability notes in
the review canvas; this tool-family contract answers D01–D17.

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-------------------|--------|------|
| D01 | Initiation & arming | Capture appears in every portal selection blister. Offer still image and video where applicable, with File and Canvas destinations, starting from the 3D capture UX. | stated | 100 |
| D02 | Stickiness & repeat | A capture is one explicit invocation; the menu remains available for another. Deviates P0.4: Space/Enter may reopen the menu but never silently start or stop recording. | guess | 55 |
| D03 | Gesture grammar | Still takes one frame to File or Canvas. Video starts immediately into a private draft with a persistent transport: Starting → Recording → Pause → Save to File or Place on Board. Resume returns to recording and excludes paused time. Only one recording runs at a time. | stated | 100 |
| D04 | Click vs drag rule | Click the Capture icon and its controls. P1.shape.properties owns dismissal and navigation: right/middle drag and camera pan/zoom retain the editor; a completed secondary/middle click without a drag keeps the existing outside-click behavior. No capture-region or press-and-hold gesture. | stated | 100 |
| D05 | Modifiers | No new keyboard shortcut or modifier behavior. Palette and blister dispatch the same registered capture commands; normal page keys retain their meaning during recording. | guess | 55 |
| D06 | Constraints & snapping | Capture reads the source's current view and aspect; grid and object snaps do not alter its pixels. Canvas results are placed immediately to the right of the source with the existing 3D screenshot gap and asset placement behavior. | precedent | 90 |
| D07 | Direction / value locks | n/a — capture has no directional gesture or Tab lock. Portal camera and page controls keep their own contracts. | pattern | 88 |
| D08 | Numeric / manual entry | No dimensions field in the first version. Video fits the source aspect inside 1920×1080 at a maximum 30 frames/second; do not stretch. One active recording bounds encoding and readback work. | guess | 45 |
| D09 | Preview & readouts | A persistent timer and Pause control sit outside the source frame in the existing attached blister. Host, panel, typography, icons, spacing, strokes and hit regions share the camera transform (P0.9); no fixed-screen minimum, cached screen anchor, viewport clamp or automatic board-camera refit. Starting is distinct until a frame is accepted. Paused freezes time and offers Save File, Place Board, Resume and Discard. Saving retains feedback; errors name the failure. Selection changes do not end the source-bound transport; its visibility follows the host and ordinary clipping/LOD. | stated | 100 |
| D10 | Cursor | No capture cursor. The portal's existing focus/cursor contract applies while capturing; menu targets use the normal arrow. | precedent | 90 |
| D11 | Commit | Stills offer File or Canvas before capture. Video chooses Save to File or Place on Board after Pause; destination finalization is asynchronous. Cancelling the file picker keeps the paused draft available. Canvas output uses one journaled linked-media add beside its source. | stated | 100 |
| D12 | Cancel | Esc closes an open capture menu before peeling portal focus and never hides active recording feedback. Discard cancels the private draft without a node or finished recording. Tab switch, source deletion/rebind or source-tab close cancels and names why. Save-picker cancellation retains the paused draft; only explicit destination choice commits. | stated | 100 |
| D13 | Selected presentation | A Canvas still becomes an ordinary linked image; a Canvas recording becomes ordinary linked video, each selected after insertion. P1.node governs handles and picks. Preserve the source portal and journal the add as one undo. | precedent | 92 |
| D14 | Post-edit | Use ordinary image/video editing after capture. Saved workbooks use their existing assets location; unsaved workbooks write to the durable per-user outputs location. Save/link/place completion retains its initiating tab and source identity; errors never insert into the current unrelated tab. | guess | 55 |
| D15 | Non-goals | Capture the portal content only, excluding Slate chrome and pointer. Video is silent MP4 when built-in encoding is available; unavailable encoding is explained, never simulated. No audio, arbitrary region, desktop capture, montage, concurrent recordings, or edits to the source. Still defaults to PNG; File picker also offers JPEG/WebP as current 3D capture does. | guess | 45 |
| D16 | Create-style inheritance | Capture preserves the source's displayed view, background, aspect, and applicable display filters. It never applies BoardLastStyle, selection cast, handles, or the portal blister to captured pixels. The 3D path retains its current view metadata behavior. | precedent | 90 |
| D17 | Hit-testing & pick | P1.node governs resulting media. P1.shape.properties and the shared rectangular blister owners govern Capture presentation and input, including host-body retention, exact clipped hit regions and navigation through controls. Capture actions preserve contents focus; the source keeps its normal input outside the attached controls. | pattern | 90 |

## Blister correction, 14 October 2026

The user explicitly reaffirmed host attachment, camera-scaled presentation and retention during right-button pan. These requirements already belong to P0.9 and P1.shape.properties. Earlier implementation notes claiming a screen-space capture exception were erroneous and are superseded. The maximized source-plus-apron allocation is documented separately as a proposed clarification in the feedback specification; no new general exception is approved here.

## Feel constants

These names and defaults live in the existing token owner or a named capture
constants block. Attached layout and surface geometry retain their shared property-panel owners.

| Token | Meaning | Initial value |
|-------|---------|------------------------|
| `portal.capture.video.max_width_px` | Recording width cap, preserving source aspect | 1920 px |
| `portal.capture.video.max_height_px` | Recording height cap, preserving source aspect | 1080 px |
| `portal.capture.video.max_fps` | Maximum capture cadence | 30 frames/second |
| `portal.capture.video.max_active` | Concurrent recording jobs | 1 |
| `portal.capture.canvas.gap` | Placement gap shared with the 3D screenshot output | Current 3D gap, 24 world units |

No new screenshot sizing, blister surface, or view metadata constants are
owned here; reuse their current owners.

## Golden paths

These input scripts are the required headless acceptance tests.

1. **GP1 — still to Canvas.** Select a rendered portal → Capture → Still
   image → Canvas → one durable image asset and one linked image appear to
   the portal's right with source aspect. Source remains. Ctrl+Z removes
   the image in one undo; the source remains.
2. **GP2 — still to File.** Select a rendered portal → Capture → Still
   image → File → choose PNG, JPEG, or WebP → one encoded file is saved,
   completion names its path, and no board node is added. Cancelling the
   picker writes nothing.
3. **GP3 — video to Canvas.** Select an applicable moving portal → Capture
   → Start video → interact with the content → Pause → Place on Board →
   bounded asynchronous encoding creates one linked video node and one
   durable recording asset. Ctrl+Z removes that node in one undo.
4. **GP4 — video to File.** Select an applicable moving portal → Capture
   → Start video → Pause → Save to File → choose destination → one
   finalized MP4 is saved and no board node is added. Completion does not
   change the active tab.
5. **GP5 — cancel draft.** Start recording → Cancel → no authored node or
   finished recording is created. Capture menu remains available.
6. **GP6 — ownership and invalidation.** Start recording → switch tab,
   close source tab, delete or rebind source → apply the agreed lifecycle
   answer. A late frame or encoder completion cannot place media into a
   different tab or attach to a replacement source.
7. **GP7 — capability honesty.** Select a still-only portal, or a portal
   whose video encoder/backend is unavailable → Still remains available
   where a frame can be supplied; Video states why it is unavailable.
8. **GP8 — no capture overlays.** Capture a selected, focused, and maximized
   source → output contains its displayed content and applicable filters,
   without Slate selection outline, blister, handles, or chrome.
9. **GP9 — unsaved workbook.** Capture to Canvas before saving the workbook
   → asset is durable in the per-user outputs location, and save/reopen
   preserves the linked media.
10. **GP10 — bounded interaction.** During recording, pointer, keyboard, and
    wheel input still follow source focus rules; no encoder or filesystem
    work blocks the frame loop. Frame backlog stays bounded and stale
    source-generation results are discarded.

11. **GP11 — source-bound feedback.** Start recording → dismiss launch controls with Esc or change selection → exterior Starting/Recording timer and Pause remain attached to the original source. Normal host clipping and LOD apply; input outside the transport stays with content.
12. **GP12 — pause and resume.** Record → Pause → timer freezes and destination choices appear → wait → Resume → timer advances from the frozen duration; the MP4 timeline excludes the pause.
13. **GP13 — deferred destination.** Pause → Save to File → cancel picker → same paused draft remains → Place on Board → exactly one video node and one undo. A stale picker cannot finalize a newer capture.

14. **GP14 — camera and host attachment.** At host widths 220/360/680 board units and zoom 0.5/1/2, normalized apron, text, icon and hit geometry agree. Pan translates source and controls together. Moving to a viewport edge never resizes controls or refits the board camera.
15. **GP15 — navigation and dismissal.** Secondary/middle press then drag pans while retaining Capture and its source selection; a no-drag secondary/middle click keeps canonical click-away behavior. Primary outside click dismisses launch controls; host-body interaction retains them. Starting a pan on the controls remains navigable.

## Approved defaults

Accepted by the user on 14 October 2026: still PNG/JPEG/WebP; silent,
content-only MP4 up to 1080p/30 fps; one recording with explicit Start/Pause and deferred destination choice;
File or Canvas output; cancel when leaving the source tab. The subsequent user refinement explicitly requires an exterior timer, Pause leading to destination choices, and the existing blister graphic style. These changes are pre-accepted stated behavior. No questions remain.

## Implementation reuse

This is a shared action across existing portal types, not a new portal host.
The source subtype owns rendering and supplies a captured frame/capability
description. One capture owner should own destination selection, encoding
lifecycle, bounded queues, durable asset writing, tab/source generation
validation, and the journaled linked-media add.

Existing owners to extend or extract from: `model_screenshot` for current
3D capture and file-picker behavior; **P2.PortalHost.bake** for the required
shared PNG + provenance + placement owner; existing
workbook assets/linked-media creation for durable media; `model-preview::view_meta`
for 3D metadata; `atlas-shell::selection_tools` for blister/popup geometry.

DRY review approved the boundary on 14 October 2026, conditional on extracting
the existing web/Atlas bake output into `board_portal` before adding consumers.
Generalize the current 3D popup and common placement/output steps; retain the
model-specific pixel renderer and metadata encoder.

The proposed native snapshot seam samples actual egui paint output through one
cached GL callback after portal content and before Slate chrome. It must also
run for maximized agent portals, which skip the ordinary finish hook. Use
bounded preallocated resolve/PBO/CPU buffers, zero-timeout fence probes,
nonblocking handoff, and session/tab/size generations. Skip frames under queue
pressure and release GL resources on their owning context. Restore framebuffer,
pixel-pack, and scissor state. Slate uses 4× MSAA: resolve at identical source
dimensions before downscaling ([OpenGL §18.3.1](https://registry.khronos.org/OpenGL/specs/gl/glspec46.core.pdf)).
This seam supplies visible, clipped content; it cannot capture a full offscreen
portal. Web and 3D stills can use their existing pixel providers. Recording must
be measured against the 60 fps canvas requirement; asynchronous APIs alone are
not performance evidence.

Forbidden forks: per-portal screenshot menus, file writers, save-picker
receivers, output-path builders, tab routing, or copied web/3D input loops.
