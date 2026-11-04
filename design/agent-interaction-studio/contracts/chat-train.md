# Chat Train interaction contract

Status: **accepted design baseline for native implementation; native acceptance pending**.
Date: 30 October 2026. Family: host portal / canonical conversation.
Source: reviewed Studio checkpoint `a7599065` plus subsequent owner native-build/headless-Rhino direction.
Command: existing native owners; new commands must use the production registry.
Inherits: native patterns where applicable, [Studio shared patterns](PATTERNS.md), and [native acceptance gate](../native-implementation/ACCEPTANCE.md).

## Current authority and native baseline

This current section governs native implementation. Chat Train and Images are the interface precedents where details are ambiguous. The owner authorized Rust implementation; this does not claim a native build has passed review. Historical Studio evidence below is superseded where it conflicts with this section or a later explicit owner instruction. Real native capabilities omitted from a fixture are retained unless explicitly removed.

One canonical conversation and branch tree supports individual message cards, message/reply pairs and a single continuous window. Keep existing placement, rename, tail/draft composer, output/Stop control, checkpoints, reversible geometry, native theme/font and historical provider provenance.

Presentation choices occupy one menu section; Full conversation has its own section; Fork / Full access / Schedule form the next. Upstream, terminal and unsent cards share the vocabulary, disabling inapplicable actions with a reason. Full conversation fits every line at the authored width and retains the pair divider. Generator-menu simplification does not remove these Chat Train actions. Only the tail/draft owns a composer; do not add permanent Send buttons to ordinary cards.

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
| N08 | Canonical individual/pair/window views; separately grouped Full conversation and Fork/Full access/Schedule; complete text and stable drafts. |

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
| Sizing | Card radius8; ordinary fitted width176–320 unless authored; single-window default440 and normal height130–560 |
| Text/header | Header13; single-window text14/20; side inset16; user indent28; turn gap20 |
| Full conversation | No normal height cap; preserve width and pair divider |
| Native header | 7-unit dots at y10; hover folds centered39.7 from right, half-width2.4/rise1.35/stroke0.55 |
| Rails | Native derived rail width4.5/palette; not the removable generator-history wire |
| Common menus | Width232 screen units; row30; native icon/hover/divider language |
| Model status | Standard0 / Fast1 / Ultra2 complete bolts; muted effort; full long identity retained with visual ellipsis |
| Rebound / gestures | <=1500 ms return; 4 screen-pixel drag threshold |
| Canvas | Pointer zoom coefficient0.0015/range1/128–128; right/middle/Space-left pan; Shift-wheel horizontal pan |
| Scroll overlay | Width3, min length24; 650 ms idle then350 ms fade; no gutter |

## Golden paths

1. Switch individual/pair/window presentation repeatedly with a draft; retain content, settings, branch identity and composer.
2. Resize during streaming, fold and fully reopen; no disappearing text, stale empty height or lost authored width.
3. Full conversation on a pair and a long single window exposes all lines and keeps the separator.
4. Spawn Chat Train from an artifact; verify the same picker, key route and durable access preference.
5. Fork a checkpoint and Stop a response; branches remain independent and received text remains.

Also execute the shared picker/key, access, rebound, failure, long-work, reload and visual checks in [ACCEPTANCE.md](../native-implementation/ACCEPTANCE.md). One static screenshot is insufficient.

## Implementation reuse and verification

Existing slate-doc::agent_chat, board_agent conversation/card metrics/header/menu/edge and atlas-shell menu/token owners.

Use real capability validation and stable native data. The Studio demonstrates interaction/geometry, not native authentication, export, permissions or execution. A submitted native build requires source/build provenance, focused checks and actual Slate visual/interaction review. Capability moderator, DRY reviewer and Design Intent Agent must clear their gates. Record necessary departures with evidence; do not silently redesign.

## Historical Studio record — superseded where inconsistent

The complete previous debrief follows. Its old draft labels, no-Rust restrictions, obsolete numeric values/menus and simulation limits describe past review stages. They do not restore rejected UI or restrict the newly authorized native implementation.

# Chat Train interaction contract

Status: **agreed — Studio baseline locked, 30 October 2026**
Family: portal
Reference: native Slate Chat Train; native agentic transcript; dashboard agent picker; owner-reviewed Studio session
Command: existing production agent portal commands; this Studio adds no native command
Inherits: applicable P0.*, P1.node, P1.portal and P2.PortalHost from the [native pattern hierarchy](../../../docs/keymap/contracts/PATTERNS.md), plus S01–S07 in [Studio shared patterns](PATTERNS.md). Explicit Studio refinements below govern this review surface.
Owner: Chat Train Studio model, rendering and capability modules; production owners remain the existing Slate/atlas crates.
Scope: agreed behavior and presentation for implementation/parity reference. **Agreed does not mean shipped natively.** The local preview uses sample replies, a fixture catalogue, simulated access/scheduling, and no secret storage.

## What it is

A conversation represented as one window, connected message/reply pairs, or individual connected messages on an infinite canvas. The same canonical conversation and branch checkpoints underlie every presentation. The Studio tab is the design review surface; this document is the durable agreement.

## Preserved baseline

- Independent native-style cards and derived header-to-header conversation rails remain directly on the canvas.
- Preserve the native Ubuntu font, light/dark palette, compact plain header, rounded card, status/output dots and ellipsis. Reference values and unmodified renderer captures remain in [BASELINE.md](../chat-train/BASELINE.md).
- Message pairs remain the ordinary initial presentation. Pair mode retains its user/reply separator; individual-message mode remains a connected train.
- Only terminal cards and unsent drafts own a composer. Enter sends; Shift+Enter inserts a newline. There is no persistent send button in ordinary train cards; the single window keeps its compact Send action.
- Output click creates an unsent continuation. Drag positions it. Historical output drag or explicit Fork creates a branch at that checkpoint; creating it does not run an agent.
- Keep inline rename, three fold states, Stop with retained partial output, reversible edits, pan/zoom, and light/dark themes.
- A setting changed for the next response never rewrites historical model/provider/effort/speed provenance.
- Shared native owners remain authoritative when porting. The browser rendition is a review implementation, with small native/browser font-shaping differences already documented.

## Locked changes from this session

| ID | Decision | Locked outcome |
|---|---|---|
| C01 | One element per Studio tab | Chat Train is the first, persistent primary tab. Future reviews start in their own isolated canvas tab. |
| C02 | Remove invented enclosing chrome | Cards sit directly on the canvas. No giant outer conversation frame, role bubbles, or modality dashboard surrounds them. |
| C03 | Four independent agent settings | Provider, Model, Effort, and Speed are distinct compact rows. |
| C04 | Anchored submenus | The clicked header keeps its icon, label and selected value on the same datum. Options replace lower rows; the same chevron closes it. No Back row. |
| C05 | Shared menu width | Picker pages use the chat ellipsis menu width, not a wide settings panel. Explanations appear on hover. |
| C06 | Provider identities | Use existing Cursor and Codex/OpenAI catalogue marks where available; Local may use a generic computer mark. |
| C07 | Dedicated API-key route | The API provider has a separate key chevron and explicit Add/Change key action with a masked input. Selecting a provider does not open the key editor or send a request. |
| C08 | Model capability rubric | Each model declares effort and speed options. Omitted capability arrays are invalid; empty arrays mean unsupported. No unavailable option is shown as selectable. |
| C09 | Header selection feedback | Show a muted effort label beside the selected model. Standard has no lightning bolt, Fast one complete bolt, Ultra two complete bolts. Other speed metadata is explicit. |
| C10 | Long-name containment | A deliberately oversized model name elides in header and menus, remains complete in the stored selection and hover help, and cannot push effort/chevrons off the card. |
| C11 | Readable single window | Use a wider plain-text window, visible user/agent distinction by indent/ink, and one pinned composer. Preserve history, branches and drafts across layout changes. |
| C12 | Transient scroll thumb | No permanent scrollbar gutter. An overlay thumb appears during scrolling and fades after scrolling stops; text column geometry does not change. |
| C13 | Width-driven height | Horizontal resize derives height from current wrapped text and recomputes transcript/composer bounds every frame. It does not accumulate clipping offsets. |
| C14 | Remember authored width | Collapsing and reopening retain width and appropriate height policy. Fit to text is the explicit sizing reset. |
| C15 | Full conversation semantics | Fit every line within the card, bypassing manual/normal scroll height limits. Preserve width and the message-pair divider. It is equivalent in intent to full expansion. |
| C16 | Menu section grouping | Presentation choices have their own section; Full conversation has its own section; Fork/Full access/Schedule form the next section; fitting/deletion follow. |
| C17 | Menu parity | Upstream, terminal and draft cards expose the same section vocabulary. Invalid actions are disabled with a reason rather than silently absent. Delete remains leaf-only. |
| C18 | Drag-first body interaction | An unselected card can be dragged by its text/body. A stationary click selects it; text highlighting is available on the following text gesture. |
| C19 | Reliable fold hit regions | Single and double fold actions own separate hit regions. Painted glyphs do not intercept the neighboring target. |
| C20 | Long review fixture | Three substantial message/reply pairs exercise full/partial/collapsed states and single-window scrolling. |
| C21 | State reliability | Unsent text, next-turn settings and branch identity survive rechunking and reload. Historical messages retain their captured settings. |
| C22 | Live-resize/run isolation | Streaming and camera handling cannot destroy a live geometry preview. Stop preserves partial output. |
| C23 | Durable debrief | Each reviewed element has a standalone contract with agreed behavior, constants, tests, preserved baseline, and dated changes. It is the parity reference for later elements. |

## Behavior matrix

Registry order and dimension names follow the repository tool-contract template. D36–D40 are media-only dimensions and are out of scope for this portal review.

| ID | Dimension | Agreed behavior | Source | Conf |
|---|---|---|---|---|
| D01 | Initiation & arming | Open the Chat Train tab in Studio. Native placement commands remain owned by the existing agent portal contract; this review adds no production command. | precedent | 90 |
| D02 | Stickiness & repeat | Sending leaves the composer available at the active tail. It does not create a second run while a response is active. | precedent | 90 |
| D03 | Gesture grammar | Idle → static selection or drag; output click → unsent continuation; historical output drag or Fork → branch; send → responding; Stop → retained partial reply. S01, S05. | stated | 100 |
| D04 | Click vs drag rule | Movement exceeding 4 screen pixels is a drag. An unselected body drag moves the card. A stationary click selects it; a subsequent text drag selects text. Controls retain their own actions. | stated | 100 |
| D05 | Modifiers | Enter sends; Shift+Enter inserts a newline. Middle drag, right drag, or Space+left drag pans. Wheel zooms; Shift+wheel pans horizontally. Ctrl+Z / Ctrl+Y undo/redo. Selected overflowing text consumes scrolling; the single window scrolls under its pointer. | precedent | 90 |
| D06 | Constraints & snapping | No new grid, ortho, or object-snap behavior is introduced by this Studio review. Keep production snapping with its existing owner. | precedent | 90 |
| D07 | Direction / value locks | Horizontal resizing fixes width and derives height from that width. Explicit vertical resizing may retain an authored height. Full expansion releases the height limit but keeps width. S04. | stated | 100 |
| D08 | Numeric / manual entry | Typing edits the composer or an explicitly opened name/key field. No numeric geometry entry is introduced. | precedent | 90 |
| D09 | Preview & readouts | Resize preview recomputes text/composer bounds from the current width on every frame. Responding and estimated count replace the model label during a run; Stop replaces the output dot. S03, S04. | stated | 100 |
| D10 | Cursor | Unselected text body uses grab/move behavior; selected text uses text selection; edge cursors identify resize direction. Small interactive controls retain pointer feedback. | precedent | 90 |
| D11 | Commit | Card moves, resize, layout changes, folds, agent choices, and deletion are reversible model actions. One drag commits once. Streaming bytes and per-keystroke draft edits are not individual undo steps. | precedent | 90 |
| D12 | Cancel | Escape closes transient menus/dialogs or cancels a live pointer gesture. Cancellation preserves the composer and returns geometry to its pre-gesture state. Apply the production cancel stack when implementing natively. | precedent | 90 |
| D13 | Selected presentation | Selection enables text highlighting after the stationary selection click. It does not add a competing toolbar or message bubble. Header and edge controls stay in place. | precedent | 90 |
| D14 | Post-edit | Rename inline, adjust settings from the header, resize at edges, and reflow through the presentation menu. A chosen width survives fold/reopen and reload. | stated | 100 |
| D15 | Non-goals | This lock covers Chat Train and the reusable Studio workflow. It does not claim live provider connections, credential storage, scheduled execution, production permission grants, or native renderer pixel identity. | stated | 100 |
| D16 | Create-style inheritance | Preserve the native light/dark palette, Ubuntu font, compact header, plain text and connected-card presentation. Global theme changes do not rewrite conversation content. | precedent | 90 |
| D17 | Hit-testing & pick | Card control, edge, body, and text-selection hit regions have separate owners. Fold SVGs cannot intercept the neighboring fold hit target. History rails are derived, not removable context wires. | precedent | 90 |
| D18 | Portal class & authority | The production agent is a host portal; inherit the existing portal authority boundary. The Studio is a local simulation and does not grant host authority. | precedent | 90 |
| D19 | Source binding | Conversation order is the canonical turn tree. Cards are views over stable turn IDs. Additional source context remains explicit; this Studio adds no hidden canvas context. | precedent | 90 |
| D20 | Query & parameters | Provider, model, effort, and speed are separate fields. Allowed choices come from one model capability catalogue and are validated again for the submitted turn. S02. | stated | 100 |
| D21 | Regeneration & staleness | Only an explicit send begins a reply. Changing presentation, opening menus, configuring an agent, or creating an unsent branch never begins inference. | precedent | 90 |
| D22 | Contents interaction | S01 governs compact menus; S03 governs scrollbar/text treatment; S05 governs selection and dragging. Composer remains independently editable. | stated | 100 |
| D23 | Level of detail | World-space card/text geometry scales with canvas zoom without rewrapping because of zoom alone. Menus remain screen-sized. Long labels truncate, not expand the card. S06. | precedent | 90 |
| D24 | Export serialization | n/a to this local preview. The standalone Markdown contract and readable viewer are review artifacts, not exported live agent behavior. | precedent | 90 |
| D25 | Bake | n/a: this session adds no bake operation. | precedent | 90 |
| D26 | Collaboration & per-peer | Local Studio conversation and notes survive reload. Provider-key status is shared across Studio instances; secret text is not stored. Production collaboration stays with its canonical journal/source owners. | precedent | 90 |
| D27 | Agent surface | Use the existing production agent/command surface when porting. No new hidden agent permission or tool is authorized by a Studio control. | precedent | 90 |
| D28 | Determinism & provenance | Baseline font/tokens and renderer provenance are pinned in chat-train/BASELINE.md. The sample model catalogue and long conversation are deterministic fixtures, not availability promises. | precedent | 90 |
| D29 | Performance envelope | A live resize never accumulates prior-frame clipping offsets. Streaming cannot replace an active resize preview. The normal single window is height-bounded; Full conversation explicitly exposes all text. | stated | 100 |
| D30 | Failure & honesty states | Unsupported settings stay visible but disabled and explain why on hover. Invalid settings normalize to an allowed model value before send. Stop keeps received text. An expired/missing key always has a reachable Add/Change route. | stated | 100 |
| D31 | View-state ownership | Canonical messages/branches, model choices, authored width, fold, and drafts persist. Camera, hover, scrollbar fade, and stream animation are view state. Presentation changes retain draft identity and history. | stated | 100 |
| D32 | Trust, sandbox & consent | API-key entry is a dedicated masked field reached through its own provider-row chevron. In Studio, only sample status is retained; real integration must use the existing secure credential owner. Access and schedule controls remain simulations here. | precedent | 90 |
| D33 | Portal chrome | Header: status dot, title, selected model/effort/speed feedback, fold control, ellipsis, output/Stop. S01/S06. The action menu uses separate presentation, Full conversation, action, and sizing/deletion sections. | stated | 100 |
| D34 | Portal maximize | Single chat window combines each linear conversation path into one scrollable card with a pinned composer. Branch topology remains explicit. Full conversation means fit all card text, not remove pair formatting or open a different transcript. | stated | 100 |
| D35 | Portal-local UI | Provider configuration, fold, text, and composer belong to their card. Studio tabs, Debrief, native reference and review notes remain outside the element canvas. S07. | stated | 100 |

## Feel constants

These are the locked review values. Semantic changes require explicit review; tuning a value must update this table and the corresponding owner together.

| Token | Meaning | Agreed value |
|---|---|---|
| card radius | Native card rounding | 8 world units |
| train width | Fitted ordinary card width | 176–320 world units, unless user-authored |
| single window width | Default readable window width | 440 world units |
| single window normal height | Automatic scroll-window bounds | 130–560 world units; Full conversation removes this cap |
| text inset | Ordinary text column | 16 world units left/right |
| text size | Conversation text | 14 world units |
| header size | Title/model/effort | 13 world units |
| window line height | Readable window transcript | 20 world units |
| user indent | User vs agent distinction in a single window | 28 world units; user ink stronger than agent ink |
| menu width | Shared screen-space popup width | 232 CSS pixels |
| menu row | Shared compact row | 30 CSS pixels, inherited icon/alignment system |
| branch gap | Initial adjacent-card spacing | 96 horizontal; 72 lane gap |
| drag threshold | Click vs body/output drag | 4 screen pixels |
| scroll thumb | Overlay, no allocated gutter | 3 world units wide; minimum length24;650ms idle delay before350ms fade |
| fold regions | Two independent hit regions; one shared visual datum | Native header dimensions; glyph pointer events disabled |
| bolt count | Speed feedback | Standard0, Fast1, Ultra2; future options declare metadata |

## Golden paths

1. GP1 — Open Studio → Chat Train is first and active → the saved conversation appears on its canvas; the Studio bar remains outside it.
2. GP2 — On an unselected text body, drag → card moves. Stationary click → selected. A later text drag → highlight text without moving the card.
3. GP3 — Drag the horizontal edge wider, then narrower several times → height reflows each time; no text disappears and no stale bottom space accumulates. Collapse then fully reopen → authored width retained.
4. GP4 — Single-step fold → partial; second step → collapsed; double expansion → all text. Each hit invokes its own action.
5. GP5 — In pair mode choose Full conversation → all text fits and the pair separator remains. In the normal single window choose it → all lines fit beyond the normal height cap.
6. GP6 — Type an unsent draft → switch pairs/window/messages → draft, settings and canonical history remain; no inference starts.
7. GP7 — Fork a historical checkpoint → separate unsent branch; original path unchanged. Both branch menus retain the same section vocabulary; unavailable pre-send actions are disabled with hover reasons.
8. GP8 — Open agent settings → Provider/Model/Effort/Speed appear separately. Open a section → its header stays fixed. Click the same header → collapse to original rows.
9. GP9 — Select a model with only Low/Medium → High/Extra high are absent. Select a model with no effort or speed → those controls are disabled. Invalid saved/request values cannot bypass the catalogue.
10. GP10 — Select Standard/Fast/Ultra → header and option feedback use zero/one/two complete bolts. Select the long-name fixture → ellipsis appears while effort and chevron remain accessible; hover exposes the full name.
11. GP11 — Open API provider's key chevron → Add/Change key → masked field. Cancel preserves draft and provider choice. Studio save retains status only, not the key text.
12. GP12 — Scroll a long single window → overlay thumb appears; stop → fades out. Left/right text-column positions do not change.
13. GP13 — Send a follow-up in one window → response stays in that window with one composer. Stop → received text stays. Resize during a run → the live preview is not replaced by stream repaint.
14. GP14 — Open Debrief → this complete contract is readable and downloadable. Create a later element tab → its independent canvas and unreviewed status cannot modify the Chat Train agreement.

## Implementation reuse

- Durable conversation/tree and sizing policy: [model.js](../chat-train/model.js).
- Provider capability validation and speed metadata: [agent-catalog.js](../chat-train/agent-catalog.js). The picker and submitted-turn capture consume this same owner.
- Card rendering, gestures and local persistence: [app.js](../chat-train/app.js); appearance: [style.css](../chat-train/style.css).
- Compact model menus: [agent-picker.js](../chat-train/agent-picker.js). Future Studio elements reuse this owner instead of a parallel picker.
- Shared review navigation/document presentation: [studio.js](../studio.js) and [contract-viewer.html](../contract-viewer.html).
- Native porting: consult [agent portal contract](../../../docs/keymap/contracts/portal-agent-link.md) and its crate owners. Reconcile the agreed Studio changes there during an explicitly scoped native implementation; do not fork production chrome, secrets, commands or provider logic.

## Verification and limits

- 22 model behavior groups pass, plus the capability-contract checks. Browser review covered resizing in both directions, folding, all-text expansion, branch-menu parity, disabled options, bolt counts, anchored headers, draft retention and the long-name fixture.
- Native CT5 renderer captures, source hashes and font provenance remain in BASELINE.md. They establish the reference, not a claim of current native pixel equality.
- This agreement does not represent live provider authentication, actual schedules, granted OS permissions or production secret storage. Those integrations must satisfy their existing owners/contracts; their Studio controls demonstrate the agreed interaction.
- No unresolved design question blocks this lock. Future issues are amendments to this contract, not permission to silently substitute a different pattern.

## Revision and change control

Version 1 — 30 October 2026. Owner approved locking the reviewed Chat Train and adopting element-by-element Studio iteration. C01–C23 record accepted changes; the preserved baseline records retained behavior. Later changes must state the affected IDs, update tests/evidence, and retain a dated amendment. Rejected or superseded designs stay in WORKLOG.md, not in the active agreement.

## Baseline lock manifest

[chat-train.lock.json](chat-train.lock.json) records the approved version and source hashes. Future Studio work must treat a changed hash as a reason to review an amendment, not to silently update the lock. The manifest is a provenance record, not a claim that unrelated native implementation is frozen.

## Shared navigation amendment — 30 October 2026

The owner requested consistent Slate navigation for every Studio canvas in the Images review. S08 now owns the wheel coefficient and zoom bounds used by Chat Train. Existing right/middle/Space drag handling remains; conversation-specific scroll ownership is preserved. No conversation state or presentation decision is changed.
