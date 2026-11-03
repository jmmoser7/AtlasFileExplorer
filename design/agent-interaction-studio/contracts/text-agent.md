# Text Agent interaction contract

Status: **accepted design baseline for native implementation; native acceptance pending**.
Date: 30 October 2026. Family: native text artifact / agent.
Source: reviewed Studio checkpoint `a7599065` plus subsequent owner native-build/headless-Rhino direction.
Command: existing native owners; new commands must use the production registry.
Inherits: native patterns where applicable, [Studio shared patterns](PATTERNS.md), and [native acceptance gate](../native-implementation/ACCEPTANCE.md).

## Current authority and native baseline

This current section governs native implementation. Chat Train and Images are the interface precedents where details are ambiguous. The owner authorized Rust implementation; this does not claim a native build has passed review. Historical Studio evidence below is superseded where it conflicts with this section or a later explicit owner instruction. Real native capabilities omitted from a fixture are retained unless explicitly removed.

The text is the native editable content surface; preserve selection, font/style, source behavior, undo/redo and commands. Property strip remains Fill → Stroke → Agent. Do not create a separate document-editor application shell.

Selecting text captures exact Unicode-safe offsets and source revision; the excerpt remains targeted when the composer gains focus. An answer creates a local link to its stable response identity. Hover/focus previews question/answer in place; activation opens and scrolls the dialogue to that response. Unrelated edits rebase anchors; editing the linked range invalidates stale links. Opening an old answer does not restore model context. A proposed replacement is explicit, applies only to the validated range and is undoable. Removing Dashboard Apply Preview does not permit automatic text replacement. Output drag creates the continuation; no extra Create chat train menu row.

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
| Host | Native text/artifact sizing and editing; Fill, Stroke, Agent |
| Dialogue | 14/20, inset16, user indent28; normal220 / expanded480; one plain composer |
| Answer viewer | Compact question/answer/Open response popup constrained to usable viewport; native popup/readability tokens |
| Results | Actual saved-text thumbnails; circles18; squares24/radius4/gap4; max8 columns, alternate-row12 offset |
| Prompt | Describe the text or the next change; selected query identifies the excerpt; preserved dialogue Message… |
| Common menus | Width232 screen units; row30; native icon/hover/divider language |
| Model status | Standard0 / Fast1 / Ultra2 complete bolts; muted effort; full long identity retained with visual ellipsis |
| Rebound / gestures | <=1500 ms return; 4 screen-pixel drag threshold |
| Canvas | Pointer zoom coefficient0.0015/range1/128–128; right/middle/Space-left pan; Shift-wheel horizontal pan |
| Scroll overlay | Width3, min length24; 650 ms idle then350 ms fade; no gutter |

## Golden paths

1. Query a Unicode-containing selection; focus the composer and retain the exact excerpt.
2. Hover/focus its answer link, inspect the answer in place, then activate and reach the exact response.
3. Edit before/after it, undo/redo, save/reopen: anchors rebase. Edit the linked words: stale link/replacement invalidates.
4. Request a rewrite and explicitly replace only the matching range; an outdated revision cannot edit other text.
5. Continue in place or drag Output to a blank child; saved-text thumbnails remain independently selectable without opening controls.

Also execute the shared picker/key, access, rebound, failure, long-work, reload and visual checks in [ACCEPTANCE.md](../native-implementation/ACCEPTANCE.md). One static screenshot is insufficient.

## Implementation reuse and verification

Existing native text note/artifact, selection/range and annotation/link owners, native journal and shared generator/conversation owners.

Use real capability validation and stable native data. The Studio demonstrates interaction/geometry, not native authentication, export, permissions or execution. A submitted native build requires source/build provenance, focused checks and actual Slate visual/interaction review. Capability moderator, DRY reviewer and Design Intent Agent must clear their gates. Record necessary departures with evidence; do not silently redesign.

## Historical Studio record — superseded where inconsistent

The complete previous debrief follows. Its old draft labels, no-Rust restrictions, obsolete numeric values/menus and simulation limits describe past review stages. They do not restore rejected UI or restrict the newly authorized native implementation.

# Text Agent interaction contract

Status: v0.4 working design specification; host layout remains under review.
Date: 30 October 2026. Family: native canvas element / agent.
Scope: plain HTML/CSS/JS Studio only; no Rust edits, builds or live execution.
Inherits: Studio S01–S10 where applicable and the reviewed Images rules.
Native source evidence: [chrome audit](../research/native-agent-chrome.md).

## Preserved host baseline and scoped selection

The content surface is editable text, not an application frame. Native agent-note owners are `board_flow/text_note.rs` and `board_flow.rs`; the property strip retains Fill, Stroke and Agent. Highlight-to-query is the requested addition: an excerpt captures exact text, offsets and source revision and remains visible while the composer has focus. A replacement proposal applies only to that matching range. Changed or stale ranges refuse replacement.

There is no separate Create chat train command in the menu. The shared Output drag creates the requested train; Preserve model history controls its inherited context, exactly as it does for Images.

## Golden paths

1. Highlight a passage, ask about it below, and verify the excerpt persists. A rewrite proposal only changes that passage after explicit Apply; a changed source refuses a stale proposal.
2. Enable Preserve model history and exchange messages within this window. Drag Output and continue from a closed child with inherited dialogue.
3. Connect source text/image/style channels and graft a channel through the same input context menu used by Images. Alternatives appear as thumbnail revisions.

## Shared generator agreement

The Images review is the precedent. Reuse its implementation owners, not a separately styled approximation. This revision supersedes the v0.2 Settings submenu and separate activity-summary row.

| ID | Requested behavior |
|---|---|
| G01 | One compact top datum: model, adjacent three dots, working spinner, thumbnail result circles, hover-dependent single/double chevrons, option count and Play/Stop. No separate Complete row. |
| G02 | Provider, model, effort and speed are separate picker choices. Per-model capability declarations disable unsupported controls. Standard has no bolt; Fast one; Ultra fast two. Long model names truncate without hiding the chevron. API credentials have a dedicated Add/Change chevron, never a chat message. |
| G03 | Three dots contain one Preserve model history checkbox. Checking it stays visibly checked while the menu stays open. The dashboard no longer exposes Apply Preview or Discard Preview. Selected output is shown directly. No Fork, Fresh eyes, Inspect memory, Open source or redundant history submenu. |
| G04 | Preserved conversation and composer use the same component as Images: user indent, subtle ink/weight distinction, continuous unboxed final prompt, authored-width wrapping. No permanent scrollbar or reserved scrollbar gutter. Open dialogue captures ordinary wheel input; Ctrl+wheel still zooms. |
| G05 | Spinner and expansion controls occupy the header datum. Single-chevron changes one fold level; double-chevron reaches the endpoint. Collapsed, bounded 220-unit, and expanded 480-unit transcript views keep the composer available. Running public updates and generated script appear in this dialogue, never a separate activity tab or private reasoning trace. |
| G06 | Outside click/Escape closes model and action menus. A menu-closing click keeps the underlying agent open. Dismissing the card and returning within 1500 ms restores its previous editor. |
| G07 | Output drag creates a blank downstream surface with controls closed and no forced text focus. If Preserve model history is enabled, its independent context snapshot and gray wire accompany it automatically. Select its body and Agent to continue. Multiple sibling branches retain independent contexts. Each child starts its own result history and input-graft settings; the parent output becomes its primary input. |
| G08 | Only a terminal incoming history wire may be selected and deleted. Cutting retains output and old transcript, clears active inherited context and exits preservation for that branch; re-enabling does not resurrect cut context. History handles hide outside hover/selection. Ordinary output wiring is retained. |
| G09 | Primary-image, text and style-image channels accept connected sources. Blank-canvas right-click can add each source type. Drag its output to the target input. Right-click a receiving input, or its connected source, opens the same Right-click Menu Studio target used by Images; Try selected action exercises Graft inputs. |
| G10 | Grafted inputs split into parallel jobs using ImageGraft.jobs. Ungrafted channels remain bundles shared across jobs. A singleton repeats against longer lists; unequal grafted lists use longest-list matching. This is not an automatic Cartesian product. Empty grafted input blocks the run. Each job receives the requested number of options in the same result history. |
| G11 | Grafted handle uses the same capsule geometry as Images: 12-unit circle, 4-unit wire pitch, at most four visible slots / 24-unit height, further wires overlap the fourth slot. Ordinary handles retain the 65% outside / 35% inside placement and the shared notches. |
| G12 | Result count stays visible and produces 1–8 alternatives per input job. Circular thumbnails render the actual saved output of each image, text, dashboard or model option. With the agent closed, the same outputs appear as 24-unit rounded-square previews below the host, wrapping in the Images running-bond pattern. Preview selection retains the strip without opening property or agent controls; body selection opens those controls. Circles and squares are mutually exclusive. |

These are explicit owner requirements applied in the prototype. The three new host layouts still await visual approval; this does not claim their full design is locked.

## Behavior matrix

| Dimension | Behavior / boundary |
|---|---|
| Initiation | Body selection exposes the native property strip; Agent opens the generator. Source output drag connects an existing input or creates a downstream node. |
| State | Draft, selection, source connections, graft flags, option count, history and authored geometry persist per node. Model preferences persist across these three tabs and reloads. |
| Menus | S01 compact row/submenu pattern; S02 capability picker; shared right-click recipe editor retains dividers and direct row dragging. |
| Navigation | S08 right/middle/Space-drag pan and pointer-anchored wheel zoom. Only active text editing or explicitly open dialogue captures ordinary wheel. |
| Interruption | Stop prevents further simulated stages and retains completed output. Reload marks an in-flight preview interrupted. No real provider/host work runs. |
| History | G03, G07 and G08; independent snapshots, closed downstream controls, terminal-only cuts. |
| Inputs | G09–G11. Data matching and capsule geometry are shared with Images; provider scheduling remains a later native integration. |
| Styling | Keep native host property tools and ImageStudioLayout; do not redefine fill, stroke, crop, palette or production wire colors. |

## Constants and implementation owners

`generator-ui.js` / `generator-ui.css` own the actual shared dialogue, composer, count and model label for Images and these tabs. `elements/runtime.js` owns common host assembly and wire interaction. `elements/session-model.js` owns branch snapshots and alternatives. `image-generator/layout.js`, `graft.js`, `menu-studio.js`, `chat-train/agent-picker.js`, `agent-catalog.js`, `scroll-overlay.js` and `canvas-navigation.js` are reused directly.

Menus: 232 units. Dialogue: 14/20 type, 28-unit user indent, 16-unit horizontal inset. Result circles: 18 units. Input circles: radius 6, outward offset 1.427, notch clearance 1.4, shoulder radius 40% of indent radius. Pan threshold: 4 screen pixels. Return grace: 1500 ms. Camera zoom does not change authored wrapping. The Studio's provider list is test data, including explicitly declared Ultra fast support; it is not a live service availability claim.

## Verification and limits

See [parity validation](../validation/generator-parity-v3.md) for observed browser results and automated checks. No private reasoning is exposed. APIs, credentials, permission grants, native file saves and real Rhino/dashboard work are simulated or outside scope. Pixel-perfect native certification still requires the later native implementation review. Existing fill/stroke/crop tools and production wire colors are unchanged.

## Revision control

v0.3 applies the owner's instruction to reuse Images literally: shared conversation, compact history toggle, header controls, closed downstream creation, input wiring and grafting. It supersedes v0.2's native Settings commands and separate activity summary. Approved shared requirements are recorded in decisions.json; whole-host visual approval remains pending.

## v0.4 — Preview and linked-text review

Owner-requested update: honor the 1500 ms return window for both the agent card and its last-open model/options menu. A return after expiry selects the body without reopening the previous menu. Render each saved output as a real thumbnail, circular in the agent header and as rounded squares on hover/selection with the agent closed. Retain 24-unit squares, 4-unit fillets, eight-column running-bond wrapping, a bridged hover gap and persistent preview selection. Selecting a square does not open the toolbar or agent. Dashboard options are reduced to Preserve model history only; no Apply/Discard stage is presented.

Answered text selections become keyboard-focusable in-document links to their exact response. Hover/focus opens a compact question-and-answer viewer in place; clicking the link or Open response opens the agent and scrolls to the recorded response. The links retain exact text offsets and a stable answer identity. Unrelated edits rebase the offsets; editing the linked passage removes its stale link. This is local response navigation, not an external web search. Proposed text replacement remains explicit on the text surface.

Verified prototype behavior and captures are recorded in validation/preview-links-v4.md. No Rust changes or builds.
