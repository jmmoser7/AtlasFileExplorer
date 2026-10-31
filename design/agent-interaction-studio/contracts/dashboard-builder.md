# Dashboard Builder interaction contract

Status: v0.4 working design specification; host layout remains under review.
Date: 30 October 2026. Family: native canvas element / agent.
Scope: plain HTML/CSS/JS Studio only; no Rust edits, builds or live execution.
Inherits: Studio S01–S10 where applicable and the reviewed Images rules.
Native source evidence: [chrome audit](../research/native-agent-chrome.md).

## Preserved host baseline

A dashboard is a Web artifact, not a fixed dashboard application. The neutral chart is sample output only. Native property order stays Fill, Stroke, Agent, Capture (`board_properties/property_strip.rs`). No invented navigation bar, reporting-period controls, widget-selector system or revision footer. Prompt: **Describe the dashboard or the next change**. The three-dot menu contains Preserve model history only. Selecting a result directly displays it.

## Golden paths

1. Open Agent, enter a change, request two options, Play: see working feedback in the header/dialogue and two new thumbnail results; select either to show that output.
2. Add two style inputs on the canvas, connect both to Style input, enable grafting in the Right-click Menu Studio and request one option: two independent candidate results are added.
3. Enable preservation, continue in place, then drag Output: a closed blank child appears with the context wire; opening its Agent reveals inherited dialogue.

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

Verified prototype behavior and captures are recorded in validation/preview-links-v4.md. No Rust changes or builds.
