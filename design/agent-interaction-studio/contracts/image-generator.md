# Images interaction contract

Status: **accepted design baseline for native implementation; native acceptance pending**.
Date: 30 October 2026. Family: image artifact / generator.
Source: reviewed Studio checkpoint `a7599065` plus subsequent owner native-build/headless-Rhino direction.
Command: existing native owners; new commands must use the production registry.
Inherits: native patterns where applicable, [Studio shared patterns](PATTERNS.md), and [native acceptance gate](../native-implementation/ACCEPTANCE.md).

## Current authority and native baseline

This current section governs native implementation. Chat Train and Images are the interface precedents where details are ambiguous. The owner authorized Rust implementation; this does not claim a native build has passed review. Historical Studio evidence below is superseded where it conflicts with this section or a later explicit owner instruction. Real native capabilities omitted from a fixture are retained unless explicitly removed.

Native image surfaces cover small, medium, large, blank, connected-input and serial generation. Preserve supported providers, export/clipboard, crop/filter/layer tools, source bindings and native wire colors. Blank images use the same port and blister masks as populated images.

Three dots retain Aspect Ratio (Source is a choice, Custom is last), Mode (Single Image Mode / Live Mode), legitimate native Settings and one Preserve model history checkbox. Model/dots, actual circular results, permanent Count and Generate share one datum. Do not replace real settings with fixture-only Seed choices. Copy/duplicate flattens only the active image; Shift multi-selection and incremented single/batch save naming remain. The normal production context menu executes its configured actions; the Studio's menu editor is not shipped as the default right-click outcome.

## Accepted changes and parity constraints

| ID | Required native outcome |
|---|---|
| N01 | Common behavior has common native owners; modalities supply content/capabilities rather than duplicate controllers. |
| N02 | Provider/Model/Effort/Speed are distinct; actual capability metadata validates options. Dedicated masked Add/Change key chevron replaces expired keys without chat. |
| N03 | Compact stationary submenu headers, shared row/icon/hover/divider language; explanations on hover, not paragraphs or Back rows. |
| N04 | Outside click/Escape dismisses; returning to the node/menu within1500 ms restores prior state; later selection does not. |
| N05 | User access preference persists globally across agents/instances/days and honors existing grants without repetitive prompts; real authority boundaries remain. |
| N06 | Public work/tool/script feedback plus Stop for long operations; no private reasoning, fake stages or spinner-only minutes. |
| N07 | Authored geometry, drafts, stable provenance and outputs survive reload; native journal/transactions and existing capabilities are retained. |
| N08 | One Preserve model history checkbox stays visibly checked/open and controls both in-place dialogue and downstream context. Independent new generators start with preservation off; saved choices and explicitly inherited child context are retained. |
| N09 | Same continuous Images/Chat Train text treatment, plain pinned composer, header spinner/hover folds and transient zero-gutter scroll. |
| N10 | Output drag creates a blank closed child; independent history follows separately when enabled. Terminal-only cuts retain data/output and cannot resurrect cut context. |
| N11 | Actual-output circles when the agent is open; exclusive rounded-square previews otherwise, bridged hover and persistent selection. Preview click does not open body/agent controls. |
| N12 | Applicable inputs are graphable/graftable; longest-list split with shared ungrafted bundles, Count per branch, bounded jobs and four-slot capsule. |

## Current behavior matrix

Registry order follows `docs/keymap/contracts/DIMENSIONS.md`; the original native owners remain authoritative for unchanged capabilities.

| ID | Dimension | Native requirement |
|---|---|---|
| D01 | Initiation & arming | Use existing native placement/open commands and node-local Agent controls. |
| D02 | Stickiness & repeat | Only explicit Send/Generate starts work; settings, result selection and unsent child creation do not. |
| D03 | Gesture grammar | Idle/selected → explicit request → running → completed/stopped/failed; retain last valid output and independent branch identity. |
| D04 | Click vs drag rule | 4 screen-pixel drag threshold; stationary selection precedes text editing;1500 ms return is a separate dismissal policy. |
| D05 | Modifiers | Enter sends; Shift+Enter newline; native undo/redo. Slate pan/zoom is retained. Open generator dialogue owns ordinary wheel; plain unfocused prompt does not. Chat single-window scrolling keeps its reviewed ownership. |
| D06 | Constraints & snapping | Preserve native snapping and constraints; do not invent a new global policy. |
| D07 | Direction / value locks | Preserve authored width through fold/reopen/reload; derive text bounds from width. Chat Full conversation removes its normal height cap. |
| D08 | Numeric / manual entry | Use native direct fields for supported settings; credentials never pass through the composer. |
| D09 | Preview & readouts | One compact datum and truthful running feedback; no redundant Complete row or fixed activity application pane. |
| D10 | Cursor | Unselected body drags; selected text selects; ports, links, controls and edges keep distinct hit/cursor behavior. |
| D11 | Commit | Use existing native journal/transaction; preserve prior results and undo/recovery. Do not bypass scope/consent to remove a UI step. |
| D12 | Cancel | Native layered Escape and Stop retain drafts, prior geometry and completed output; cancel does not silently commit partial work. |
| D13 | Selected presentation | Chat selection enables subsequent text editing; generator preview selection retains its strip without opening controls; body selection owns controls. |
| D14 | Post-edit | Retain authored geometry, output identity and model state; source-specific native tools remain available. |
| D15 | Non-goals | Do not ship Studio authoring chrome, fixture providers/replies or simulated execution as native functionality. |
| D16 | Create-style inheritance | Preserve native theme/font/palette and style inheritance; non-agent property semantics remain unchanged. |
| D17 | Hit-testing & pick | Separate body/control/text/link/preview/resize/wire regions; a glyph cannot intercept its neighboring fold action. |
| D18 | Portal class & authority | Retain the proper native artifact/portal class and journal/consent authority. |
| D19 | Source binding | Stable source, artifact, turn/result and connection identities; no hidden source rebinding or per-frame identity loss. |
| D20 | Query & parameters | Validate actual provider/model/effort/speed and supported input types; snapshot source revisions and history once per submitted job. |
| D21 | Regeneration & staleness | Explicit requests only; changed inputs/reconnections affect future work, not historical provenance. |
| D22 | Contents interaction | Apply the tool-specific current contract above; preserve native interactions absent from the fixture. |
| D23 | Level of detail | World geometry scales without camera-induced rewrap; menus remain screen-sized; use native LOD/cache owners. |
| D24 | Export serialization | Preserve native export/source fidelity; omit machine secrets and transient UI; do not claim unsupported live behavior in exports. |
| D25 | Bake | Retain applicable native bake/flatten behavior; add no competing workflow merely for the prototype. |
| D26 | Collaboration & per-peer | Respect native authored versus per-peer state; credentials and access are user/machine scoped, not document secrets. |
| D27 | Agent surface | Registered commands and actual scope only; retain real capabilities omitted from the Studio unless explicitly removed. |
| D28 | Determinism & provenance | Capture real request/result/source identities and model settings; thumbnails represent their actual saved output. |
| D29 | Performance envelope | No UI-thread network/model work; bounded jobs; cached previews; streaming preserves resize/reading state. |
| D30 | Failure & honesty states | Handle missing/expired credentials, unsupported inputs/models, unavailable host, partial failure and Stop honestly. |
| D31 | View-state ownership | Persist authored size, sources, drafts, outputs/history and relevant links. Hover, fade and pointer state stay transient. |
| D32 | Trust, sandbox & consent | Global durable preference honors granted scope; new material authority remains explicit. Use the secure native credential owner. |
| D33 | Portal chrome | Use the tool-specific native surface and shared generator chrome above; no invented application wrapper. |
| D34 | Portal maximize | Preserve native maximize/restore/Escape; generator UI stays local to its artifact. |
| D35 | Portal-local UI | Source-specific controls stay on the node, not Document Settings or unrelated board panels. |
| D36 | Viewport screenshot | No change to the existing native media/3D contract; preserve applicable interoperability. |
| D37 | View metadata | No change to the existing native media/3D contract; preserve applicable interoperability. |
| D38 | View drop-back restore | No change to the existing native media/3D contract; preserve applicable interoperability. |
| D39 | Viewport photo filters | No change to the existing native media/3D contract; preserve applicable interoperability. |
| D40 | Wired saved views | No change to the existing native media/3D contract; preserve applicable interoperability. |

## Feel constants

All geometry is logical/world units at zoom1 unless explicitly screen-sized. Native colors and palette ownership are unchanged.

| Area | Required reference |
|---|---|
| Fixture dimensions | Small220×260; medium360×360; large680×300 exercise policy, not fixed native sizes |
| Attachment | natural=tool_count×48+168; attach only if natural<=width−16 and natural×40<=width×height×0.2 |
| Detached | Panel420 centered; image-to-strip gap14; buttons30/gap7; panel y=height+14+30+8 |
| Attached blister | Band40, collapsed relief20, shoulders84; backing extends8 upward beneath image |
| Port centers | Inputs y48/90/132; output y48; history y10, superseding old30/72/114 and y6 prose |
| Port shape | Radius6; center1.427 outward (65% area outside); clearance1.4; smooth shoulder radius40% of indent radius |
| Results | Circles cap18, may shrink to14 before paging; squares24/radius4/gap4; max8 columns; alternate rows offset12 |
| Square position | No body controls +8; selected body +28 attached or +52 detached |
| Graft capsule | Width12; height12+4×(min(max(count,1),4)−1), max24; fifth+ wires overlap fourth slot |
| Common menus | Width232 screen units; row30; native icon/hover/divider language |
| Model status | Standard0 / Fast1 / Ultra2 complete bolts; muted effort; full long identity retained with visual ellipsis |
| Rebound / gestures | <=1500 ms return; 4 screen-pixel drag threshold |
| Canvas | Pointer zoom coefficient0.0015/range1/128–128; right/middle/Space-left pan; Shift-wheel horizontal pan |
| Scroll overlay | Width3, min length24; 650 ms idle then350 ms fade; no gutter |

## Golden paths

1. Compare all sizes and blank white surfaces at several zooms; keep correct masks, fillets, backing and preview offsets.
2. Close the agent, hover through the gap into second-row previews, select/Shift-select and right-click; no vanished strip or unwanted toolbar.
3. Repeatedly generate in place with preservation, then drag Output to a blank closed child and branch to siblings.
4. Cut only the terminal history edge, retaining data/output; explicit reconnect affects a future turn without rewriting prior output.
5. One primary plus two grafted styles and Count3 produces six ordered independent results; save naming increments across single/batch operations.

Also execute the shared picker/key, access, rebound, failure, long-work, reload and visual checks in [ACCEPTANCE.md](../native-implementation/ACCEPTANCE.md). One static screenshot is insufficient.

## Implementation reuse and verification

Existing native image presentation/blister, image-agent, wire-host, provider scheduler, credential and export owners; Studio layout/view/menus/history/graft modules are the reviewed reference.

Use real capability validation and stable native data. The Studio demonstrates interaction/geometry, not native authentication, export, permissions or execution. A submitted native build requires source/build provenance, focused checks and actual Slate visual/interaction review. Capability moderator, DRY reviewer and Design Intent Agent must clear their gates. Record necessary departures with evidence; do not silently redesign.

## Historical Studio record — superseded where inconsistent

The complete previous debrief follows. Its old draft labels, no-Rust restrictions, obsolete numeric values/menus and simulation limits describe past review stages. They do not restore rejected UI or restrict the newly authorized native implementation.

# Images interaction contract

Status: draft v0.9 — visual review remains open, 30 October 2026.
Family: portal / image.
Reference: Slate native image generator, saved right-click Menu Studio recipe, and owner-directed Studio sessions.
Command: existing production commands; none added to native Slate.
Inherits: Studio S01, S02, S06, S07, S08, S09.
Owner: native image-presentation, image-agent, wire-host and shared menu owners.
Scope: interactive HTML/JS/CSS/SVG specification. No Rust edits or builds.

## Purpose and retained baseline

One Images tab contains small (220×260), medium (360×360), large (680×300), image/text input and three-generation history-wire fixtures. Native photograph, SVG tool glyphs, color tokens, Ubuntu text, attached eligibility and native labels provide the baseline. Seven image controls need 504 units of flat edge; eight need 552. Their footprint must fit within 20% of image area. Smaller images use a detached 420-unit editor, 30-unit tool buttons, 7-unit gaps and 14-unit image clearance. Eligible large images join the editor to the bottom edge. Keep the four native image/text/style/output bindings; the additional conversation-history wire remains above the primary image wire.

## Explicit requirements from the review

The requirements below are owner-directed. Their browser implementation is still open to visual refinement; the entire image design has not been declared finished.

| ID | Requirement | Current specification |
|---|---|---|
| I01 | Slate canvas navigation | Right/middle/Space drag pans; wheel zooms at pointer; Shift-wheel pans horizontally. A stationary right-click opens a context menu; dragging suppresses it. S08 owns the shared rules. |
| I02 | Focused editing owns wheel | Hovering a text box still zooms. Focused prompt consumes ordinary scrolling; Ctrl-wheel explicitly zooms. |
| I03 | Popup dismissal | Outside click and Escape close menus. The outside click dismisses an open popup before deselecting the card. |
| I04 | Aspect property naming | Aspect Ratio is a submenu in the three-dot menu; Source remains one choice, Custom is last and takes width:height values. |
| I05 | Descriptive mode choices | Mode contains Single Image Mode and Live Mode. |
| I06 | Shared overflow menu | Aspect Ratio, Mode and Settings use stationary expanding headers and the Chat Train row/icon/divider language. |
| I07 | Detached surface | Freestanding editor has four rounded corners; attached editor retains its joined top. |
| I08 | Compact header | Model and adjacent three dots at left; remaining space holds circular history, persistent count and Generate on one datum. |
| I09 | Mutually exclusive results | Generator open: circle history. Image panels closed: square previews appear on hover and remain while selected. Selecting another image or clicking canvas clears them. Never show both. |
| I10 | Hover continuity | Image, gap and all grid rows are one hover region; 180 ms departure grace when unselected. Selection pins the previews regardless of pointer position. Context menus also retain the target. |
| I11 | Separate context targets | Image body and result thumbnail have independently configurable menus. Actions target the clicked result. |
| I12 | Parity across fixtures | Small, medium, large, input and series consume the same view, geometry, menu and navigation modules. |
| I13 | Wire-handle experiment | See the measured experiment below; it is not a final native port contract. |
| I14 | Borderless result imagery | Zero border, zero internal padding and no resting selection ring on circle and square images. The photograph fills the shape. |
| I15 | Count and packing | Count always visible with descriptive hover; show all available circle results when they fit by adapting their diameter. |
| I16 | Menu customization | Right-click directly opens the editor for the clicked element, with target-specific checkboxes, labels, ordering, divider insertion/removal, preview, notes and JSON export. |
| I17 | Non-agent exclusions | Native fill, stroke, corner/crop, filter, brush and layer menus remain unchanged and outside this specification. |

## Wire-handle experiment — pending visual selection

Handle radius remains 6 world units. Old clearance 4 is reduced by 65% to 1.4. Each center moves 1.427 units outward, giving approximately 65% of the circular area outside the image and 35% inside. Tangent cubic shoulders meet the image edge; a circular inner arc surrounds the handle. Input/output wire endpoints use the moved centers. Input vertical datums remain 30/72/114, output 30; history is at 6. These are review values, not an approved native geometry migration.

## Compact-card and result constants

Model width follows measured text plus chevron space, clamped to 56–120. The model hit target fits text, 4-unit gap, chevron and minimal right padding. The 14-unit three-dot target follows after 1 unit; circular history starts 12 units beyond it. Header begins at y=2 and is 24 units high; text and history centers are y=14. Prompt begins y=28, has 8-unit side margins, 6-unit inner padding, 32–144 height, and 8-unit bottom padding. Font remains 11-unit Ubuntu with 16-unit prompt line height. Detached radius is 8. Count remains 1–8 and is always visible.

Circle history adapts its diameter between 14 and 26 to fit available header width, then pages for larger lists. Small has 12 sample results to exercise the packing. Square previews use 24-unit cells, 4-unit gaps and 4-unit corner fillets. They wrap after eight columns in a running bond: alternate rows move 12 units (half an image) to the right. The hover bridge includes every row. Image fixtures reuse the local photograph; no image generation occurs.

## Embedded right-click Menu Studio

Recovered source: design/right-click-menu-studio/drafts/2026-10-12-pass-03.json; labels/actions reconciled with apps/slate/assets/context-menus.json. The original standalone editor HTML was not found. The embedded editor preserves the saved image ordering and uses the common menu chrome.

Image body and Result thumbnail are independent targets. Each has enable checkboxes, editable labels, row order, Add action, Insert divider, removal controls, a live preview, implementation notes, persistent local configuration and JSON export. Select a row to insert a divider after it. Model picker, prompt, wire, count, options, toolbar, connected text and canvas targets are also independently editable; unconfigured targets start empty. Draft changes take effect only on Apply to Studio. Cancel discards edits. Menu customization is Studio chrome, not a production context-menu action. In this authoring iteration, the right-click gesture opens the editor directly rather than rendering a production context menu first. Body defaults follow the saved recipe. Thumbnail defaults are Use this result, Save image as, Duplicate and Remove result; these choices remain configurable review defaults, not locked production availability.

## Scope exclusions

**Fill color, stroke width, corner fillets/crop, filters, brush and layers menus are explicitly excluded. Their existing native Slate design and behavior must remain unchanged.** The provisional panels from v0.4 have been removed. Their native toolbar glyphs remain as visual context and clicking them reports this boundary.

Generation, provider authentication, external file operations, native menu modification and production history storage are simulated or unconnected. Key-entry UI saves sample status only and discards key text. The fourth wire carries sample upstream prompts separately from image content; disconnecting starts fresh downstream conversation history. It is a requested Studio addition, not an existing native InputSlot.

## Behavior matrix

| ID | Dimension | Review behavior |
|---|---|---|
| D01 | Initiation | Images tab, five scenario groups. |
| D02 | Repeat | Independent per-instance prompt/options, retained locally. |
| D03 | Gestures | Native robot entry; shared Slate navigation. |
| D04 | Click / drag | Body selects/moves; controls retain their actions; right drag pans. |
| D05 | Modifiers | Space drag, Shift-wheel, Ctrl-wheel, Escape. |
| D06 | Constraints | Native attachment predicate; centered detached fixture. |
| D07 | Value locks | Width retained through fold/reopen. |
| D08 | Manual entry | Visible prompt, custom aspect and bounded count. |
| D09 | Preview | Editable DOM/SVG, no native renderer dependency. |
| D10 | Cursor | Body, control, resize and pan cursors remain distinct. |
| D11 | Commit | Settings do not submit model work. Menu drafts apply explicitly. |
| D12 | Cancel | Escape/outside click; menu-editor Cancel discards draft. |
| D13 | Selection | Result identity retained without resting image borders. |
| D14 | Post-edit | Each example can be revisited and refined. |
| D15 | Non-goals | Real generation and non-agent menu redesign. |
| D16 | Style | Native references plus explicit review changes above. |
| D17 | Hit testing | DOM controls; growing hover bridge; actual wire centers. |
| D18 | Authority | Local Studio only; no production grants. |
| D19 | Sources | Standalone, wired image/text and explicit history fixtures. |
| D20 | Parameters | Shared provider/model/effort/speed picker; unsupported options disabled. |
| D21 | Regeneration | Single Image Mode / Live Mode, simulated requests. |
| D22 | Contents | Compact circle history and hover-only square grid. |
| D23 | Detail | World geometry scales with browser camera. |
| D24 | Export | Debrief and menu-spec JSON export. |
| D25 | Bake | Not applicable. |
| D26 | Collaboration | Local fixture persistence; no remote collaboration. |
| D27 | Agent surface | Browser implementation only. |
| D28 | Provenance | Native source/capture references and saved menu recipe. |
| D29 | Performance | Synchronous local layout, no inference during paint. |
| D30 | Failures | No native-service dependency; explicit simulated actions. |
| D31 | View state | Independent from Chat Train and its conversations. |
| D32 | Trust | No real key or provider request. |
| D33 | Chrome | Studio editor clearly distinct from target image chrome. |
| D34 | Maximize | Shared infinite-canvas navigation. |
| D35 | Local UI | Review controls stay outside the target surface. |

## Golden paths

1. Review Small, Medium and Large; robot toggles the appropriate detached/joined editor; width survives a collapse cycle.
2. Read the model name, open its picker, inspect explicit API key action, close by clicking canvas.
3. Open three dots, then Aspect Ratio/Mode/Settings; header stays stationary; select Custom ratio or cancel it.
4. Close the generator and deselect; hover image → cross the gap → select a second-row square → right-click it → menu persists.
5. Right-click body → editor opens on Image body → disable an action or insert a divider → Apply. Right-click a thumbnail opens its independent target.
6. Inspect Inputs/Series; fourth wire is above primary; disconnect and restore history without changing image wires.
7. Hover an unfocused prompt and wheel to zoom; focus it and wheel to scroll; right drag pans without opening a menu.

## Implementation reuse and verification

Native reference owners: board.rs, board_properties/image_palette.rs, selection_tools image/agent/blister/layout modules, slate-doc wire_host, atlas-core display and shared menu contract. Browser owners: canvas-navigation.js; image-generator/layout.js, view.js, menus.js, menu-studio.js, context-recipe.js and style.css; shared Chat Train picker and image capability catalog.

See validation/image-generator-browser.md and saved review screenshots. Tests cover native attachment boundaries, the 65% circular-area offset, header non-overlap, grid wrapping, navigation and existing Chat Train model/capabilities. Visual approval remains open. Browser font rasterization/shadows are not certified native pixel matches. Viewport-edge relocation and native low-zoom suppression are not implemented.

## Revision control

v0.1 schematic rejected; v0.2 native runtime rejected; v0.3 static browser rebuild; v0.4 compact controls/shared navigation/hover continuity; v0.5 wire experiment, borderless grid, packed header, permanent count, separate menu configuration and explicit exclusion of provisional non-agent panels. Do not change Rust until requested. Navigation S08 is locked; the overall image presentation is still in review.

## v0.6 — Reselection and direct authoring gesture

The last opened image-agent panel is remembered per image through deselection and reload. Clicking the canvas hides it; clicking the image again restores it. Explicitly closing it with the robot clears that remembered-open state. Prompt, count and model settings survive. This restores the native interaction intent; it is not a new production design.

Right-click directly opens one Right-click Menu Studio for the clicked element. The canvas still pans on right-drag without opening the editor. The page cancels contextmenu default/bubbling in capture phase, including model popups and editable prompts, to prevent the overlapping browser menu observed by the owner. No browser/application-wide setting is changed. The editor itself does not recursively reopen when right-clicked.

## v0.7 — Running bond, selected previews and direct menu ordering

The owner clarified that rows are a brick running bond, not an aligned grid. Odd-numbered rows have a half-image (12-unit) horizontal offset. Preview images retain rounded corners without borders. Selected images retain their square previews after pointer departure while their generator is closed; circle/square exclusivity remains. Selecting another image or the canvas clears the prior selection and its retained grid. Thumbnail selection selects its image without reopening the generator.

Menu preview names and dividers can be selected and dragged directly. A placement marker indicates before/after insertion; moving updates the editable list. Cancel discards the draft order; Apply and JSON export retain it. Arrow buttons were removed; Alt+Up/Down remain as a keyboard alternative. Native menus are still outside this browser-only implementation phase.


## Image action refinement — 30 October 2026

The native return grace is 1500 ms (board_properties/selection.rs). Reopening after longer absence no longer applies. Cutout shoulder fillet is 40% of the indent radius. Shift-click selects a contiguous result range in either circular history or square previews; selected results share the thumbnail action set with an added Save selected images action. Active-result copy and duplicate flatten only the active image, never its history. Duplicate creates an independent plain image on the canvas. Saves use a persistent image-0001.png, image-0002.png sequence across single and batch operations. Shared result-menu actions follow primary-image ordering while retaining user labels and action selections. Because right-click is still assigned to authoring, Try selected action in the Menu Studio executes the selected action for testing. Browser verified four selected results and the first sequential download; external image clipboard falls back to the Studio copy buffer if unsupported. Native Rust remains unchanged.

Batch save downloads a single ZIP containing individually numbered PNGs because the embedded browser blocks multiple automatic downloads. Verified images-0005-0008.zip contains image-0005.png through image-0008.png, with ZIP CRC validation passing. Single saves remain PNG downloads.

## v0.9 — Shared presentation corrections and explicit history modes

All sizes use circle result images capped at 18 world units, with the same card padding. Selecting a result selects only the result/image context; it does not arm the primary body's toolbar. Clicking the primary image body arms its toolbar. Square results under a selected detached strip start 52 units below the image; under an attached blister they start 28 units below. Hover-only or thumbnail-only selection keeps the 8-unit offset. Shared selection and Shift-range behavior remain.

The attached generator apron extends eight units behind the image's lower fillet, with the image painted above it. No separate horizontal seam or filter fringe is part of the intended native presentation. The prototype's added white icon-filter fringe has been removed. These are retained native-quality requirements, not permission to regress the existing production renderer. Image/text wires use the native default width of 4 world units (base 2 × connector scale 2); history wires use the Chat Train precedent of 4.5. Invisible hit paths must remain transparent.

### History initiation and handle geometry

The three-dot menu has one history action: **Preserve model history**. It enables both in-place conversation and inherited downstream generation; there is no separate conversation-mode or new-downstream-image action in this menu. Enabling it does not immediately create an image or wire. The next generation exposes the gray history handle. Sending again in the same image appends a user/agent pair. Dragging the primary output handle onto the canvas creates a downstream image; when preservation is enabled, the history connection accompanies the image connection automatically. The outgoing gray handle also supports this drag.

The in-place conversation follows Chat Train single-window typography, user indentation, spacing and one continuous composer at the end. The composer has no separate boxed field or dividing card.

History handles follow Chat Train's small 7-unit gray dot at y=10 and x=6 / width−6, embedded inside the edge. Space is reserved even before enabling history: Primary/Text/Style datums are now y=48/90/132 and Output y=48. Other handles do not move when history is enabled.

### Snapshot and reconnection rules

A branch inherits a deep copy of upstream context at creation. New upstream turns mark a connected child stale; they never rewrite the child's earlier turns. Refresh/reconnect queues the current upstream snapshot for the child's next turn. Cutting is allowed only into a terminal image. It retains existing replies and images as records but exits history mode for that image and any downstream scope; later standalone requests do not inherit that context. The UI distinguishes detached context, upstream changes and pending refresh. Own turns in one branch do not mutate sibling branches. The existing fourth-wire requirement remains explicit; native document semantics will be reconciled only when Rust implementation is requested.

### Validation

Pure conversation tests cover independent branches, frozen snapshots, detach retention, refresh queued until submit, and preserved child turns. Browser checks cover a new child inheriting six messages, an independent child reply, disconnect/reconnect, and thumbnail-only selection rendering zero toolbar buttons. The Series fixture includes sample dialogues for exploration. Real inference remains disconnected.

## v1.0 draft refinement — one history control and direct wire gestures

Click a gray history wire to select it; Delete or Backspace cuts it only when its receiving image is terminal, without deleting the image wire, images or prior conversation. Blank-canvas selection and Escape clear wire selection. Detaching stops future upstream synchronization; already used history remains part of previously generated turns. Reconnecting queues the updated upstream snapshot for the next turn.

Wire colors in this browser preview do **not** amend Slate's production wire palette. Native colors and non-history wire semantics remain governed by their existing production owners. No Rust edits or builds are part of this iteration.

Browser verification covers output drag creating a child with inherited messages, one history menu toggle, in-place generation and selected-wire deletion/reconnection. The image presentation remains draft pending owner review.

## v1.1 draft — dialogue, terminal history cuts and input grafting

- Dialogue matches Chat Train single-window type: 14/20 text, user indent 28, subtle ink/weight distinction, 20-unit separation and plain pinned composer. Transcript is bounded to 220 units, composer to 100; both scroll without a reserved gutter. User scroll position survives repaints. No redundant simulation paragraph consumes the text datum.
- A 3-unit overlay thumb appears only during user scrolling and fades after 650 ms idle. This rule includes scrollable menus. An explicitly opened image dialogue/composer captures wheel input on hover; a plain unfocused image prompt retains canvas zoom. Ctrl+wheel zooms explicitly.
- Preserve model history changes its check mark without dismissing the three-dot menu. New images created by output drag start blank, unselected, with all palettes closed and no result thumbnails. Click the image and its robot to open its editor normally. Inherited context is retained internally without automatically displaying a palette.
- History handles obey ordinary image interaction visibility. No gray dots remain floating when the image is not selected or hovered. Existing lines retain their endpoints and production colors remain unchanged.
- Only the history edge entering a terminal image can be cut. A node with downstream image/history connections cannot have its incoming history cut. Cutting exits history mode, hides history handles, clears the preservation check and prevents future inheritance throughout the affected downstream scope. Historical text is archived out of the active context and images remain records. Re-enabling preservation starts fresh; explicit reconnection supplies a new upstream snapshot. Reconnection explicitly opts back into preservation; it is not implicit.
- Graft inputs is available for Primary image, Text and Style image targets, including their connected source objects. The Studio's direct right-click authoring workflow is retained. Input capsules grow with four-unit wire pitch and stop at four visible slots; subsequent wires overlap the last slot. Two grafted styles with one primary produce two outputs in the same panel.

Research and future native obligations: [Image input grafting](../research/image-input-grafting.md). This includes verified Grasshopper sources, data matching, job planning, provenance and bounded provider concurrency. Browser-only simulation; no Rust edits/builds.


### Blank-surface parity correction

Blank white images and populated images use the same surface mask, including input/output notches and the collapsed attached blister cutout. Blank state changes the surface content only; it does not bypass geometry. Verified blank-card robot opens and closes the joined editor.
