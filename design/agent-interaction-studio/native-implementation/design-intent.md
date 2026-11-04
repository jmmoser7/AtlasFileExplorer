# Native agent interfaces — design intent and acceptance authority

Date: 30 October 2026, America/New_York. Reviewer: Design Intent Agent.
Scope: Chat Train, Images, Dashboard Builder, Text Agent and headless Rhino/3D Agent Builder.
Reference checkpoint inspected: `a75990653b5cf97484d03c2c36d34920f615a831` plus the owner's subsequent native-build authorization and headless-Rhino instruction relayed by the architect.

This is a design review, not native implementation or certification. No Rust was edited or built in this review. The user has now authorized native implementation; the older Studio-only/no-Rust scope statements describe the completed prototype phase, not a current prohibition.

## 1. Authority and the actual design objective

The owner wants the existing Slate interfaces refined to the Studio specification, with their existing capabilities retained. The Studio is a highly specific executable specification, not an invitation to design another application shell. The repeated rejection was of gratuitous reconstruction: extra headers, status rows, mode selectors, nested Settings commands, progress tabs and inconsistent copies of already solved controls.

Resolve competing evidence in this order:

1. Latest explicit owner correction or steering, including headless Rhino and removal of Dashboard Apply/Discard from the agent menu.
2. The latest reviewed interaction in the Studio implementation and its applicable locked requirement.
3. Chat Train and Images as the common-pattern authorities, especially where Dashboard/Text/3D are underspecified.
4. Existing native capability and contract ownership, preserving functionality outside the requested change.
5. Historical notes and screenshots, which are evidence of evolution rather than instructions to restore rejected states.

When Studio code conflicts with a clear owner instruction, do not port the bug. Record the discrepancy and implement the instruction. When native capability conflicts with a Studio omission, preserve the capability without adding an unrequested prominent control; the regression moderator and architect must review the placement. A builder cannot independently waive an acceptance criterion.

The current HTML, per-element contracts and screenshots form one reference set. Neither a screenshot alone nor passing unit tests establish interaction parity. Browser fixture provider names, pretend API key status and sample output are not production implementation requirements.

### Native simple-source correction (owner, 1 November 2026)

Image, Text, Web/Dashboard and Model nodes retain their native simple content/output until that individual node's robot opens the approved Agent dialogue. Reuse existing per-node panel state and preserve authored bindings, output and history; expanding one node never activates another. Native Text has its own plain white text sheet and no legacy overlaid Prompt/Run field. When the dialogue closes, only genuine connected input slots remain inspectable at their canonical positions; unused roles stay hidden.

Dropping an output on empty canvas opens the existing Text/Image/Dashboard/3D modality chooser before any child is created. Cancel creates no node; only the selected modality creates its blank closed downstream source, with no automatic generation and with the appropriate preserved immutable history. This supersedes automatic image-only creation in the prototype. Existing Web revocation moves into its Web context menu, with Website access settings; blocked Allow remains available inline.

## 2. P0 acceptance requirements

These are release-blocking for the affected surfaces. IDs below are this review's checklist IDs, not replacements for the production command/contract registry.

| ID | Acceptance criterion | Scope |
|---|---|---|
| DI-01 | Reuse common native code for common generator layout, model configuration, menus, dialogue, result selection, scrolling and history behavior. A modality supplies content and capabilities, not a parallel control system. | All |
| DI-02 | Keep each content surface native to Slate. Do not add a product title/navigation frame, fixed activity rail, extra Complete row, Progress/Script tabs or a second application toolbar. Studio tabs/notes/debrief chrome are not embedded in Slate nodes. | All |
| DI-03 | Provider, Model, Effort and Speed are separate choices. A model's real capability metadata controls selectable values and is validated at submission. Unsupported/fixed controls remain understandable and disabled with hover help. Do not fabricate Ultra fast, effort or provider support because a fixture advertises it. | All agent pickers, including Chat Train opened from an artifact |
| DI-04 | API provider rows have a separate, obvious Add/Change key chevron and masked credential entry. An expired or missing credential is replaceable without asking in chat. Selecting a provider/model does not send a message or expose the secret. Use the native machine-private credential owner; never store secrets in a workbook or exported artifact. | API-backed agents |
| DI-05 | Access preferences are global across agents, instances and days for this user, as explicitly requested. A remembered authorized scope must not re-prompt every few seconds. Preserve actual native authority boundaries and request only genuinely new/material scope, not routine operations already covered by the grant. A decorative checkbox must not pretend to grant permission. | Agent access |
| DI-06 | Compact popup rows: short label, optional selected value, icon, chevron/check. Explanations live on hover. Picker submenus expand beneath a stationary header and replace lower rows; the same chevron collapses them. No Back row, paragraphs or shifting header datum. | All applicable menus |
| DI-07 | Model label and its chevron align on the header datum; three dots immediately follow the model control with minimal dead padding. Long names ellide visually while preserving full identity and accessible/hover text. Effort and chevron cannot be pushed out of reach. | All |
| DI-08 | Standard speed has no bolt, Fast one whole bolt, Ultra/Ultra fast two whole bolts. Selected effort is shown as muted text beside the model. Use real provider marks for Cursor/Codex/OpenAI where applicable, not substituted decorative symbols. | All |
| DI-09 | Clicking off closes the popup. Preserve the last dismissed agent/popup for a 1500 ms return window: a quick body return reopens it; a later return only selects the node. A popup-closing click must not also unexpectedly dismiss its underlying open agent. Do not resurrect a popup merely because another Studio/native tab was visited. | All applicable node menus |
| DI-10 | Unselected body drag moves a node. A stationary selection enables later text selection. Interactive controls, active text editing and an explicitly selected 3D viewport keep their own interactions. Cancellation restores the pre-gesture geometry. | All |
| DI-11 | Resize from current authored width and current content, never a mutated previous preview. Text reflows and height follows so no clipped lower text or stale empty lower area remains. Width survives collapse/expand and reload. Camera zoom does not change wrapping. | Chat Train; applicable text/dialogue hosts |
| DI-12 | A generator's single Preserve model history checkbox stays visibly checked in its open menu. It enables repeated in-window dialogue and automatic inherited history on downstream output drag. Do not add separate Fork/Fresh eyes/Inspect memory/Open source/New downstream/History connection commands to these generator menus. Dashboard's three-dot menu contains Preserve model history only. | Images, Dashboard, Text, 3D |
| DI-13 | One continuous dialogue surface, with a plain composer belonging to that surface. Subtle indent/ink/weight distinguish user and agent; no role bubbles, extra cards or isolated final text-field box. The working spinner and hover-dependent single/double chevrons share the top datum. | Generator dialogue; Chat Train single-window precedent |
| DI-14 | Long-running operations expose truthful public progress, tool outcomes and generated scripts as available, plus Stop. A spinner alone for minutes is unacceptable. Do not show private chain-of-thought, manufacture stages, claim an API ran merely because code was generated, or reset reading position/caret on progress updates. | Every operation regularly exceeding 60 seconds; Dashboard/3D especially |
| DI-15 | Scrollbar is an overlay, appears during scrolling and fades; it never reserves a gutter or shifts the text column. Plain unfocused prompt hover allows canvas zoom. An explicitly open generator dialogue captures ordinary wheel over transcript/composer; Ctrl+wheel still zooms. | All applicable scrollable menus/dialogues |
| DI-16 | Output drag first opens the existing Text/Image/Dashboard/3D modality chooser; cancel creates nothing. The chosen modality creates a blank downstream node with menus closed, no unsolicited composer focus and no generation until a prompt is submitted. Preserved context travels separately in the gray history wire, above the primary data wire. Siblings receive independent immutable snapshots. | Images, Dashboard, Text, 3D |
| DI-17 | History dots hide with ordinary interaction handles and wires end correctly when not hovered. Only a terminal incoming history edge may be cut; cutting leaves data wires and output intact, exits downstream history mode and archives old context outside future requests. Re-enabling preservation cannot resurrect cut history. Explicit reconnection affects a future turn and never rewrites prior outputs. | Generator chains |
| DI-18 | Primary/image, text and style/reference inputs are connectable and graftable where the modality supports their meaning. Grafted branches produce separate jobs; ungrafted inputs are shared bundles. Longest-list matching repeats a shorter list's last item, not an implicit Cartesian product. Count multiplies each branch. Empty grafted input is reported before submission. | Images and applicable Dashboard/Text/3D inputs |
| DI-19 | Circular header selectors are actual saved-output thumbnails. With the agent/palette closed, hover shows rounded-square thumbnails beneath the host. Circles and squares are mutually exclusive. Image, gap and all preview rows form one hover region; selected previews persist when the cursor leaves. Clicking a square selects that output without opening the body toolbar or agent. | Images, Dashboard, Text, 3D |
| DI-20 | Thumbnail menus target the clicked/selected result rather than the whole generation stack. Native fill/stroke/crop/filter/layer controls and wire colors retain their native design and capabilities. Copy/duplicate of an image result flattens only the active result into an ordinary image. | Result menus and Images |
| DI-21 | Text selection stays visibly targeted when focus moves to the composer. Answers create local in-document links to stable response identities. Hover/focus previews the question and answer in place; activate to open/scroll the dialogue to that exact response. Links are not external searches, and opening an archived response must not restore it to active model memory. | Text Agent |
| DI-22 | Unrelated text edits rebase answer links; edits to the linked range invalidate stale links. Proposed replacement validates the exact selected range/revision and changes only that range after explicit application. Correct Unicode offsets and undo/redo are mandatory in the Rust port. | Text Agent |
| DI-23 | Rhino computation is headless and returns real native 3D model artifacts to the Slate viewport. Do not open a visible Rhino document/window or foreground an external modeling UI. Preserve native Slate model interaction/property tools and preview history. An internal temporary computation document is an implementation detail only if required by the chosen API, never a user-visible document workflow. | 3D Agent Builder, latest steering |

## 3. Deliberate modality differences

Uniformity means shared semantics and chrome, not replacing native capabilities with image-only behavior.

- **Chat Train:** Three presentation styles remain Single chat window, Message pairs and Chat train/individual messages, over one canonical conversation tree. Full conversation is a separate menu section and must fit every line at the authored width without removing the pair divider. Fork, Full access and Schedule remain a distinct Chat Train action section; their removal from generator menus does not remove them from Chat Train. Only a tail/draft owns a composer. Ordinary cards have the native output/Stop control; the single-window Send control remains compact. Derived conversation rails are not the removable gray generator-context wire.
- **Images:** Keep Aspect Ratio (including Source and Custom last), Mode (Single Image Mode / Live Mode) and existing legitimate settings beneath the three-dot menu. The image-specific result count and source handling remain. Do not copy fixture-only Seed unlocked/locked choices as a complete replacement for existing native image settings. Preserve small/medium detached versus large attached/blister behavior.
- **Dashboard:** It remains an arbitrary generated Web artifact, not a fixed metrics app. The mock chart is content only. Property order remains Fill, Stroke, Agent, Capture. The three-dot agent menu is exactly Preserve model history; do not reintroduce Apply Preview/Discard Preview. Preserve recovery/undo of prior results through actual result history and the native journal rather than silently deleting the capability.
- **Text:** The text itself is the content surface with Fill, Stroke and Agent. In-document answer links are new; they do not turn the node into a separate browser/editor application. An explicit scoped text-replacement action is retained on the text surface. Removing Dashboard Apply Preview does not authorize silently applying a text rewrite.
- **3D:** Preserve Slate viewport orbit/pan/zoom/picking, units and the real display/measurement/capture/maximize/filter controls where available. The agent's computation is headless. Retain the native connector controls required by existing capability, but do not use them to force the new generation flow through a visible Rhino document. A licensing/API availability limitation must have an honest user state; a simulated mesh or displayed Python string is not the requested implementation.

## 4. Geometry and feel constants to port

All sizes below are world/UI logical units at zoom 1 unless explicitly screen-sized. These derive from the current executable Studio reference, not superseded prose. Theme colors remain owned by native tokens.

| Item | Current reference |
|---|---|
| Font | Existing native Ubuntu Light/300; conversation 14 with 20 line height; generator label/plain prompt 11 with 16 line height; Chat Train header 13 |
| Conversation layout | Side inset 16; user indent 28; turn separation 20; plain continuous composer; generator dialogue starts at y36, plain prompt at y28 |
| Generator normal heights | Transcript bounded 220; current expanded generator transcript bounded 480 with scrolling; composer interactive 20–100, plain 32–144. This is not Chat Train Full conversation, which removes the cap. |
| Card / detached palette radius | 8 |
| Popup | Width 232 screen units; compact 30-unit rows using the native shared icon/hover/divider language |
| Model/header packing | Model measured width clamped 56–120; three-dot target width14 follows after1; header begins y2, height24; Play20; Count32; native icons reused |
| Attached eligibility | `natural = tool_count * 48 + 168`; natural <= host_width −16 and natural*40 <= host_width*host_height*0.2 |
| Detached controls | Palette width420, centered; image-to-strip gap14; buttons30 with gap7; panel y = host_height+14+30+8 |
| Attached blister | Band40, collapsed relief20, shoulders84; backing extends8 above the bottom image edge so it reads as underlying the image; no faint seam/fringing |
| Result circles | Cap18; Images may shrink to14 before paging. Actual previews, no border/padding/ring. |
| Result squares | 24×24, corner radius4, gap4; <=8 columns; alternate rows offset12 (half an image), not a diagonal staircase |
| Result vertical offset | No body controls: host_height+8. Body-selected closed palette: +28 attached, +52 detached. Do not use detached offset on the large attached case. |
| Hover departure | 180 ms bridge grace for unselected previews; selection persists without a timer until deselection or circle presentation |
| Reopen grace | <=1500 ms from outside dismissal; no reopen after1500 ms |
| Port geometry | Circle radius6; horizontal center1.427 outside edge (65% of area outside); clearance1.4; smooth shoulder radius40% of indent radius; input centers y48/90/132 and output y48 |
| History datum | y10, embedded near upper edge; separate from primary input/output; current image history circle7 |
| Graft capsule | Width12; height12 +4*(min(max(count,1),4)−1), maximum24; 4-unit wire pitch; fifth+ wires overlap fourth slot |
| Overlay scrollbar | Width3; min length24; 650 ms idle then350 ms fade; no reserved space |
| Canvas | Pointer-anchored exponential zoom coefficient0.0015, range1/128–128; right/middle/Space-left drag pans; 4 screen-pixel drag threshold; Shift-wheel horizontal pan |

The header datum, relative sizes and attached/detached treatment are more important than superficially similar spacing. Do not hard-code one screenshot's card width into all modalities. Preserve authored geometry and render all sizes at multiple zoom levels.

## 5. Documentation/prototype conflicts that must not become native regressions

| Evidence | Conflict | Resolution for the native build |
|---|---|---|
| Image contract title says draft v0.9 and lists y30/72/114 ports, history y6, circles up to26 | Current code and later reviewed changes use y48/90/132, history y10 and cap18 | Correct the finalized debrief; use the current geometry above. Preserve the historical values only as history. |
| Three new contracts still say Studio-only/no Rust, visual review open; `studio.md` repeats earlier phase restrictions | Owner now explicitly authorizes native Rust implementation and has accepted the present direction | Mark the reference checkpoint accepted for implementation, while retaining truthful distinction between prototype approval and native acceptance. |
| `native-agent-chrome.md` final v0.3 paragraph still permits pending Apply/Discard; early `studio.md` retains Settings/Fork/progress tabs/widget-selection language | Owner subsequently removed Dashboard Apply/Discard and repeatedly rejected menu bloat and application wrappers | v0.4 and the latest user correction govern. Archive/relabel obsolete statements, do not restore those controls. |
| Rhino contract describes displayed-only scripts and native-connected host switching | New steering requires headless computation of native model artifacts in Slate | Update the finalized Rhino contract to DI-23. Do not claim generation because only a script was shown. |
| Image draft golden paths mention disconnect/reconnect generally; later v1.1 and owner restrict snipping to the last image | An unrestricted disconnect would alter established downstream history | Terminal-only cut is mandatory. Distinguish explicit future-turn reconnect from rewriting prior history. |
| `elements/result-previews.js` currently supports single selection, whereas Images and earlier owner instructions support Shift range selection | Blindly copying the newer helper would drop reviewed multi-result semantics where applicable | Share the Images selection semantics in native thumbnails; preserve single/multi save applicability per artifact type. Record non-image save formats via the capability moderator. |
| Studio defaults/providers and model names are fixtures; most image models deliberately expose no effort/speed | Browser UI is not current capability discovery | Real native/provider capability metadata governs available values. Empty/unsupported states are honest and readable. |
| Image fixtures reuse one photograph; Dashboard/Text/3D use deterministic sample transforms | A visually matching fixture is not real output provenance | Native thumbnail must represent the saved result being selected, with independent result/source/model provenance. No duplicated placeholder thumbnail for every result. |
| Image input right-click opens Right-click Menu Studio; new host result-menu execution is largely a placeholder | Authoring workflow and production context actions are different things | Native right-click executes the configured native action set. Do not ship the Studio's menu editor as the default production click outcome or delete existing unrelated actions. |
| Studio initializers can migrate dimensions and expose only some controls; HTML build has no native journal/export stack | Convenience fixture code is not a durable document migration policy | Do not reset native authored sizes, drafts, cameras, results or existing settings while loading. Extend the native model/journal/serialization owners. |
| Chat Train Full conversation is unbounded, while generator fold level2 is currently480 and scrollable | Using one numeric cap for both would violate explicit Full conversation behavior | Preserve the semantic distinction. Never cap Chat Train Full conversation or remove its pair divider. |
| Early baseline transcript uses equal alignment/ink; latest single-window reference uses user indent/stronger ink | Baseline native evidence predates the reviewed refinement | Use latest single-window styling for the continuous dialogue, while preserving pair/single-message presentation semantics. |

Some requests not fully exercised by the HTML remain explicit requirements: the model picker when Chat Train is spawned from an artifact, global durable access preferences, expired-key replacement, changing effort where supported, and sustained meaningful feedback during a real >60-second operation. They cannot be dropped because a short browser fixture did not prove them.

## 6. Native review golden paths

1. **Configuration:** Create/open a normal Chat Train and one spawned from an artifact. Open each model picker, switch provider/model, inspect limited/no-effort and single-speed models, exercise a very long model name, and replace a missing/expired API key through its own chevron. No chat prompt is needed and no request is submitted while configuring. Record historical provenance before and after.
2. **Consent:** Use an explicitly authorized scope, perform several ordinary tool calls in multiple nodes, restart Slate and revisit. The remembered permission remains effective; a genuinely larger scope asks once with the actual requested boundary. No repeated prompt loop or accidental new authority.
3. **Chat presentation:** Fill three long turn pairs. Switch individual/messages/pairs/window repeatedly with an unsent draft, fork a historical checkpoint, resize narrower/wider while streaming, collapse and fully expand. Text remains complete; width/draft/branch identity persist; Full conversation retains pair separators and fits all text.
4. **Menu return:** Open agent/model/options, click canvas, return at roughly1000 ms, then repeat beyond1500 ms. The first returns to the remembered state; the second does not. Escape, tab switches, and explicit control closure do not resurrect stale popups. Checkbox feedback stays visible.
5. **Image geometry:** Use small220×260, medium360×360, large680×300, blank-white, connected-input and series examples. Inspect port notch shoulders, backing under the bottom fillet, icon fringing/seams and preview offsets at zoom0.5/1/2 in both themes. Every size follows the same policy.
6. **Output previews:** Generate >8 distinct outputs. Inspect actual circular thumbnails, close the agent, hover across the image→gap→second preview row, select one then leave, Shift-select a range, and right-click a result. No disappearing strip, border rings, unwanted toolbar launch or wrong-result action. Reopen the agent: squares give way to circles.
7. **Grafting:** Connect one primary plus two styles. Ungrafted gives one bundled job; grafted styles give two outputs, Count3 gives six. Test A/B/C against X/Y and verify A+X, B+Y, C+Y; test empty graft and five+ wires. Completed jobs remain independently inspectable after one job fails or Stop. Native scheduling is bounded and nonblocking.
8. **History:** Enable preservation without closing its menu. Converse repeatedly inside one image/artifact, drag output to a blank child, continue and create sibling branches. Verify inherited snapshots are isolated. Attempt an interior cut (refused), then terminal cut (history exits, outputs/data wire retained); reconnect and generate a new turn without changing prior result history.
9. **Text answers:** Select a range including Unicode; query it; move focus to the composer; hover/focus the resulting local link and open the exact recorded answer. Edit preceding/succeeding text (link rebases), edit the linked text (link invalidates), undo/redo, save/reopen. A stale replacement never edits an unrelated range; viewing an old answer never restores model context.
10. **Dashboard:** Request two alternatives and select each actual preview. Preserve history and repeat. Agent options remain one checkbox. Undo/recovery and existing artifact source/capture behavior remain reachable through their proper existing owners; no simulated Apply/Discard menu returns.
11. **Headless 3D:** Start a real geometry request without opening a Rhino document/window. Observe actual public work/script events and Stop, then inspect the resulting native Slate 3D model, dimensions/units, display modes, camera/picking, captures and saved output thumbnails. Test independent alternatives, grafted references, history continuation, failure and restart. A displayed script without generated artifact fails.

## 7. Submission gate and allowable deviations

Every builder submission must name its changed common owners, mapped DI IDs, preserved native capabilities, relevant unit/integration tests, and captures/video for changed interactions. Include native app revision, build profile, viewport/DPI, theme, zoom and fixture identity so a comparison can be repeated. A cached renderer executable with unknown current-source identity is historical evidence, not validation of the submitted binary.

The Design Intent Agent can return **accept**, **accept with documented deviation**, or **revise**. Acceptance requires the capability moderator and DRY reviewer to clear their respective gates as well. Pixel parity is not certified in this document: no current native render was inspected here. Passing tests or a process launching is not enough; inspect the actual Slate board and exercise the gestures.

Allowable differences require an explicit log entry: requirement/reference, native constraint, chosen change, evidence, capability impact, and approving reviewer. Legitimate examples are native font rasterization differences while measured geometry matches; a capability being disabled with an honest provider/API reason; native undo/secure-store integration absent from the browser fixture; or a headless computation implementation replacing the old visible-host assumption. None permits arbitrary spacing/radius changes, a new application shell, dropped controls/capabilities, simulated output, renewed prompt loops or unchecked permission expansion.

There is no blanket numerical pixel tolerance granted by this review. Compare matched logical geometry at identical DPI/zoom and explain any rasterization difference individually. If an oversight would require losing real functionality, preserve the capability and escalate the design placement rather than silently deleting it.

## Sources reviewed

- The full owner conversation and latest headless-Rhino steering relayed by the architect.
- `contracts/chat-train.md`, `image-generator.md`, `dashboard-builder.md`, `text-agent.md`, `rhino-agent.md`, `PATTERNS.md`, `studio.md`, `TEMPLATE.md`.
- `chat-train/BASELINE.md`, `agent-catalog.js`, `agent-picker.js`, relevant `app.js` layout/scroll/menu behavior.
- `generator-ui.js`, `generator-ui.css`, `canvas-navigation.js`, `scroll-overlay.js`.
- `image-generator/layout.js`, `menus.js`, `view.js`, `history.js`, `graft.js`, `app.js`, relevant final CSS.
- `elements/runtime.js`, `session-model.js`, `result-previews.js`, `text-agent.js`, `text-links.js`.
- `research/native-agent-chrome.md`, `research/image-input-grafting.md`, latest v3/v4 validation notes.
- Visually inspected saved `chat-window-refined.png`, `images-dialogue-v11.png` and `text-answer-links-v4.png`; these show their recorded historical states, not a fresh native build.
