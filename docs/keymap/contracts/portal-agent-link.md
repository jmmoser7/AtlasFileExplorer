# Agent portal — interaction contract

Status: **agreed** (15 September 2026; implementation verification in progress)
Family: portal
Portal class: **host** (Art. V.3) · Type: **agent** · Subtype: **local link**
Command: `board.portal.agent` (placement) · Key: none in v1 · Palette:
"agent portal" (aliases: cursor portal, local agent)
Inherits: P0.* (all, including P0.9), P1.node, P1.portal, P2.DragShape,
**P2.PortalHost** — deviations flagged below.

Owner: `atlas-agent` contracts; `slate-doc::agent_inputs` semantics; `atlas-codex` protocol; `atlas-ai` supervision and picker; `board_portal` focus; `board_portal_chrome` hover chrome; `atlas-shell::home` album motion.
Forbidden forks: a second Cover Flow; a second contents-focus prelude.

## What it is, and the 10% it implements

A minimal sidecar for an installed program, placed on the board. The empty portal
shows only program icons and names. Cursor and Codex use a small chat interface;
configured image sidecars show their generated results inside the portal.
Ollama text chat is included in the 18 September refinement; ComfyUI remains later work. The generic image link is
available to explicitly configured sidecars today.

The user approved this refinement with “ok go for it!” after reviewing the
[design report](../../agent/portal-evolution-plan.md) and concept images.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Palette: "agent portal" + Enter; Portals dock flyout; command `board.portal.agent`; board view only. | stated | 100 |
| D02 | Stickiness & repeat | One-shot: placement returns to Select; Space/Enter may repeat via the registry history. | pattern | 80 |
| D03 | Gesture grammar | P2.DragShape. Place the frame and choose a provider. Coding providers then show projects, followed by resumable conversations and New conversation. Discovery and history reads run off-thread. | stated | 100 |
| D04 | Click vs drag rule | Travel > 4 px defines the frame; below it places a 960x540 default portal centered on the click. | precedent | 85 |
| D05 | Modifiers | Shift locks 16:9 during drag, matching Repository Lens portal placement. Ctrl/Alt unassigned in v1. | precedent | 80 |
| D06 | Constraints & snapping | Grid snap and smart guides apply to the frame rect; agent contents never snap. | pattern | 80 |
| D07 | Direction / value locks | History wires use width 4.5 and opacity 0.42 in dark mode / 0.14 in light mode. Only immediate fork outputs stack downward from the primary top datum; upstream links stay single. Large forks compress strand spacing to remain within the parent height. Geometry updates during drag. | stated | 100 |
| D08 | Numeric / manual entry | n/a in v1; dimensions are edited by resizing the frame. | guess | 55 |
| D09 | Preview & readouts | Quiet at rest. Chooser contains only icons and names. Chat identity and actions reveal on hover or contents focus. Image portals are full bleed without labels; hover/focus reveals Generate and the shared Maximize control. Wire affordance is a subtle animated blister behind the edge; resting wires have no port rings. | stated | 100 |
| D10 | Cursor | Output-grip proximity suppresses body resize hit targets and resize cursors. The grip owns continuation/fork presses; surrounding body edges retain their existing resize behavior. | stated | 100 |
| D11 | Commit | Portal presentation, message ranges, bundle membership and branch-view pruning use SceneCmd. Transcript bytes remain linked source data. Sending from a historical checkpoint starts an independent provider session with an explicit prefix replay; undo never reruns inference. | stated | 100 |
| D12 | Cancel | Esc peels one layer per press (P0.1): maximize → contents focus → drag draft → armed tool → selection. Releasing contents focus leaves the shelf / poster intact. Agent sidecar work is out-of-process and is not cancelled by board Esc. | pattern | 80 |
| D13 | Selected presentation | Double-click the title to edit the name across its maximal linear branch segment, including hidden members. Codex titles combine the custom conversation name with a model dropdown populated by the installed provider; the menu also offers Rename. Model choice is journaled per card and inherited by new continuations. Renaming a fork does not rename its siblings or upstream prefix. | stated | 100 |
| D14 | Post-edit | Switch presentation only through the contextual menu. Single chat window converts each maximal linear segment, keeps the current viewport height as its ceiling and shrinks shorter content. Chat train restores message views and forks. Single windows only show full conversation, and never offer Unbundle. | stated | 100 |
| D15 | Non-goals | Codex and Cursor expose only conversations supported by their installed interfaces. Legacy Cursor desktop chats are not claimed as SDK agents. No hidden provider-history deletion. Ollama remains local text chat; direct OpenAI chat and local tool execution remain later increments. | stated | 100 |
| D16 | Create-style inheritance | No. Host portals use portal styling and do not consume shape/text style state. | pattern | 80 |
| D17 | Hit-testing & pick | The header and border pick or move the card. Transcript text supports native drag selection and Copy through the shared scaled-text owner, retaining glyph layout while zooming. Text selection does not move the card. Program tiles and project/conversation rows accept the first click throughout their visible hover target; contents focus is not a prerequisite. Conversation-title text renames on a single click; the separate model-name text opens the model dropdown across its complete visible area. | stated | 100 |
| D18 | Portal class & authority | Host. Provider histories remain outside .slate. Branch deletion prunes this view and retains its source; one Undo restores view references without inference. Agents cannot mutate transcript ownership through the host. | stated | 100 |
| D19 | Source binding | SourceUri remains the relative-first project locator. AgentPortalRef stores provider, cache session, optional provider conversation id and model, bundle locator and immutable seed image. Coding train cards share one provider conversation. Unknown provider ids survive load. | stated | 100 |
| D20 | Query & parameters | Journaled binding includes provider/session/source plus ChatView ranges, immutable history ancestry, detail, draft status and hidden bundle members. Arbitrary reparenting is forbidden; validated subdivision may insert views of a contiguous range without changing its ancestry or checkpoint. Context connectors remain independently editable. | stated | 100 |
| D21 | Regeneration & staleness | Only explicit wires into the message card midpoint input supply additional context at Send. Selection, viewport, revision ids and ambient canvas data are not appended to messages. Editing connected text changes the next input without running an agent. Background history refresh preserves the visible answer, including on refresh failure. Auto-sizing invalidates on picker exit, workbook change, geometry change, typing and streamed output. Undo pauses sizing until new content arrives. Project and conversation pickers size to their visible rows with a bounded list height and a separate header. | stated | 100 |
| D22 | Contents interaction | Codex and Cursor have one linear stream, with continuation at the tail by clicking or dragging the dedicated top output circle; historical handles cannot fork coding agents. Permanent midpoint left/right circles show referenced/changed artifacts on hover. Clicking lists artifacts; choosing one opens an existing Web or filtered Atlas portal with a provenance wire. Midpoint context wiring stays editable and separate from immutable top history rails. Local providers retain checkpoint forks. Enter sends; Shift+Enter inserts a newline; the placeholder appears only on selected cards. Ollama has exactly one home tile; installed local models are chosen through the same title dropdown as Codex, without replacing the conversation. Focusing an agent does not suppress canvas wheel zoom. Provider tiles use Codex, Cursor and Ollama marks; model labels omit GPT version prefixes for Astra, Sol, Terra and Luna. | stated | 100 |
| D23 | Level of detail | Cards display complete text; Name only and Summary choices are removed from menus and inspector. Full conversation offers inline input and response. Transcript text uses shared canvas text scaling, fixed wrap bounds and world-scaled scroll offsets during zoom. No blue selection outline or dimensions. | stated | 100 |
| D24 | Export serialization | Chat exports remain honest host posters and source identity; derived history rails serialize with the same cubic geometry. No live controls, credentials or inference are exported. Existing image-bundle export remains unchanged. | stated | 100 |
| D25 | Bake | Bundles may contain other bundles to arbitrary depth. Unbundle restores one presentation level, preserves inner bundles and lays visible cards side-by-side in train order. Bundle/Unbundle remain contextual squircle actions; single chat windows never offer Unbundle. | stated | 100 |
| D26 | Collaboration & per-peer | Frame and binding sync as document data. Runtime history, hover and album focus remain local view state. Linked assets are references. | stated | 100 |
| D27 | Agent surface | Adapters consume immutable snapshots. Proposed board edits retain staged SceneCmd acceptance. Coding tools operate under the provider configuration; Slate does not substitute a read-only persona or silently grant requested approvals. | stated | 100 |
| D28 | Determinism & provenance | History checkpoints are exact exclusive transcript bounds; partially loaded prefixes are refused. Forks replay quoted ancestor text into a fresh source, not a claimed native provider fork. Shared source snapshots are immutable Arc values. | stated | 100 |
| D29 | Performance envelope | One atlas-ai AgentSources worker/cache per source path shared by its cards. Source reads and protocol work stay off-thread. Summary layout caches by source/scene revision and zoom resolution. Completed train runtimes are released; large-history frame-time validation remains pending. | stated | 100 |
| D30 | Failure & honesty states | Installed program, healthy server and installed model are separate states. Ollama never downloads a model or silently falls back to cloud. Failures name unsupported image inputs, context budget, model absence, transport errors and cancellation. | stated | 100 |
| D31 | View-state ownership | Journaled: bindings, ranges, parent view references, detail, bundle membership and placement. Derived: source transcript snapshots, streaming text, readiness and hover. Pruning removes view references, not foreign provider records. | stated | 100 |
| D32 | Trust, sandbox & consent | The renderer-free Codex adapter uses installed authentication and provider permissions. Supported command/file approvals appear on-card with Allow once and Deny; unsupported requests fail explicitly. Cursor loads installed settings and enables SDK auto-review. Opening a referenced Web or Atlas portal uses its existing consent and source owners. | stated | 100 |
| D33 | Portal chrome | Minimal theme-aware cards have faint shadows and no bevel. Wires have no bevel. Selection retains Fill and Stroke squircles; default stroke weight is zero. Authored card colors and strokes persist and export. Compact title, ellipsis and maximize remain at the top. | stated | 100 |
| D34 | Portal maximize | **P1.portal.maximize.** Fills the window at the screen aspect; Esc restores. | stated | 100 |
| D35 | Portal-local UI | P1.portal.local-ui. Provider, model, explicit input wiring, sidecar setup and bundle commands stay on the portal/inspector. The board-wide panel gains no provider-specific settings. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `agent.link.poll_secs` | Minimum read/write poll interval | `1.0` |
| `agent.portal.default_size` | Click placement size | `960 x 540` |
| `agent.portal.turns_painted` | Recent turns painted in chat | `24` |
| `agent.await.sent_timeout_secs` | Named failure if Send never reaches a live sidecar | `20` |

| `PROGRAM_HOVER_SECONDS` | Program tile reveal | `0.12` |
| `WIRE_BLISTER_SECONDS` | Wire blister emergence | `0.14` |
| `ALBUM_SETTLE_SECONDS` | Return to full bleed | `0.18` |
| `ALBUM_BROWSE_SCALE` | Artwork size during browsing | `0.78` |

## Golden paths

1. Place → icon grid → choose Codex → minimal chat; journal undo restores the picker.
2. Connect a text node → Send/Generate captures its current text. Editing the note
   changes the next snapshot while the submitted input remains unchanged.
3. Sidecar publishes completed images → one full-bleed portal; focus and scroll
   browses images; the shared motion settles to one full-bleed preview.
4. Unbundle → independent sessions and preserved incoming wires. One Undo restores
   the original bundle, and Redo restores the same child identities.
5. A detached or externally pasted wire has a Free endpoint and supplies no data.
6. Switching contents focus releases the prior host without destroying its runtime.
7. Stage accept/reject keeps the existing human acceptance gate.
8. Export contains linked completed image assets and no live agent controls.

## Open questions

None for this approved refinement. Provider-specific local adapters are sequenced
in the design report; they are not represented as installed programs.

## Chat-train refinement verification (18 September 2026)

The reference fork uses a common outgoing top datum, curved upper/lower departures and top-aligned continuation lanes. Source tests cover rail tangents, exact checkpoint prefixes, incomplete history, fork-safe bundling and undo identities. Source ownership passed the required DRY review after extracting AgentSources. The release build and chat-history unit tests passed; Windows blocked a circle-pack documentation test. The follow-up visual correction adds on-card controls, a persistent title and a scrollable messenger transcript; native sample review verified common-anchor forks, menu access and messenger rendering. Headless pointer input verifies the visible Train action. Synthetic partial-source updates verify the streaming UI; provider-authenticated runs remain unverified. This is implementation in progress, not shipped status.

## Direct manipulation follow-up (18 September 2026)

Native synthetic-workbook review verified title editing and propagation across six forked cards, context-only ellipsis actions, conversion of a fork into three linked windows and back, and drag placement of a draft continuation with Undo. Cards were inspected in both themes and at 150% zoom. Theme-switch review exposed stale summary glyph colors; cache keys now include ink color and physical zoom resolution, covered by a regression test. The full Slate/document library run passed 553 tests with four ignored; 22 focused agent tests, including continuation, projection, theme-switch and high-zoom cache regressions, also pass. Provider-authenticated inference and large-history performance remain outside this visual review.


## Compact cards and fork strands (18 September 2026)

The latest refinement replaces persistent mode switches with contextual menu actions, scopes renaming to a maximal linear branch, and gives new train drafts an immediately focused, frameless Enter-to-send composer. Cards fit measured text; single windows retain a viewport ceiling and scroll overflow. Bundles show miniature member cards, with separate bundle/unbundle and window/train glyphs. Native synthetic-workbook review confirmed six separate shared-trunk strands, threefold rail weight at reduced opacity, and quieter card shadows. It also exposed overlapping maximize/output-grip targets; their hit regions are now disjoint and covered by a regression assertion.

Validation: 409 Slate and 149 document library tests passed (four ignored), followed by 24 focused agent regressions and three icon geometry tests (one ignored). The contract-check command was attempted but Windows denied execution of `target/debug/build/quote-a8e57fc9d2758a0c/build-script-build.exe`; no security settings or artifact locations were changed. Live authenticated provider inference is outside this visual refinement.

The 38 artifact library tests also passed. The release build completed, but Windows denied launching `target/release/slate.exe` for the post-hit-target native pass. The earlier native screenshots therefore verify rail/card styling, not the final hit-target correction or the final bundle/menu appearance.

The final simulated pointer regression passes: clicking the actual output grip creates a distinct draft, leaves maximize unset, focuses its composer, grows with typed text, and dispatches Enter without inserting a newline.

## Context diagnosis and pending policy proposal (18 September 2026)

The inspected local Ollama/llama3.1:8b session begins with the user's “hey!” but its reply discusses an empty canvas snapshot and revision `bb5425ee58ee894a`. The adapter appends serialized inputs to every user message, even when both context and wired inputs are empty, and supplies a Slate system message. Subsequent history repeats that distraction. A later request includes an agent card as selection context; the request asking for a connected sticky note contains empty context and wired arrays. The log proves that the number was not sent in that request; it does not establish which canvas connection was present.

No provider prompt policy changes are included here. Article VIII.1 currently mandates automatic canvas context. Proposed amendment, pending explicit user approval: ordinary chat receives user text and the selected conversation history; additional canvas context requires an explicit attachment/scope action and is shown in a send-time preview. Empty snapshots and internal revision IDs stay out of model-visible prose. Provider safety/tool restrictions remain separately declared. This proposal is not a ratified amendment.

## Downward forks and nested bundles verification (18 September 2026)

The library run passes 414 Slate tests, 149 document tests and 39 artifact tests (four ignored). New regressions cover downward-only immediate forks, live drag cache invalidation, grip/resize separation, nested one-level expansion with horizontal spacing, automatic two-branch output from full windows, in-place full-window sends, transcript zoom scaling, and persisted/exported fill and stroke. Agent contract rows match the decisions database. Provider prompts remain unchanged pending the context policy decision above. Native visual verification and the release build are recorded separately below.

The preserved workbook confirms sticky node 14 contains “42” and connector 16 targets response card 13. The later question is card 17, whose request snapshot has no wired items. Inputs are currently resolved for the sending card, so explicit attachment inheritance across history edges needs a protocol decision. The previous unsaved workbook was preserved at `C:/Users/jmoser/Downloads/Slate workspace - 2026-09-18.slate` before closing Slate to release its executable lock.

The release build completed successfully after preserving and closing the running workbook. Native review of the new executable confirmed six immediate downward-stacked fork strands with a single upstream connection, faint light-mode rails and unbeveled cards, the reduced ellipsis menu, and two draft siblings created by a full-conversation output grip. The synthetic preview was closed without saving test edits; the user's preserved workbook was reopened in the new executable and restored to light mode. No provider inference was invoked during this review.

## Coding sidecar refinement (21 September 2026)

The user approved building the provider → project → conversation workflow and a single stream for Codex/Cursor. Artifact circles remain visible: references on the left, changed documents on the right. Hover and the artifact list do not add scene nodes. Explicit opening reuses the Web/Atlas constructors and adds the portal plus decorative provenance wire as one journal operation. Atlas exact-file queries serialize and appear in export posters. Structured successful tool results own artifact records; assistant prose is never scraped for paths.

Cursor lists SDK-local agents, not legacy desktop composer chats. Codex uses the installed app-server catalog and history interfaces; unsupported history formats surface a named failure. The automatic context policy above remains unchanged.

Validation: the release library suites pass 631 tests (four ignored), including the final 416-test Slate run. Four Cursor normalization tests pass. The installed Codex API returned three saved projects and loaded a 24-turn conversation; Cursor's SDK-local catalog was reachable but empty for the Slate project. Native review caught and corrected Atlas's unmatched-file visibility flag and the project-list clipping/provider filtering. The corrected release opens the filtered single-file Atlas view and shows the three actual Codex projects inside the card. No paid model inference or live tool approval was exercised. Windows denied execution of `target/release/xtask.exe contracts`; a direct parity check verified all 35 agent contract rows against decisions.json. This does not replace the blocked workspace-wide contract audit.

## Explicit inputs and stable conversation refinement (21 September 2026)

The user explicitly approved replacing automatic context: “context should need to be explicitly provided by me the user” through the message input wire. This supersedes the pending 18 September proposal and ratifies the scoped Article VIII.1 amendment. Existing provider/project instructions remain owned by the provider. Slate no longer adds its own canvas persona or unwired snapshots. Known transport suffixes are hidden when displaying old user messages; provider history is not rewritten.

Top history handles and midpoint artifact/context handles are separate. Codex model options come from the installed app-server model/list interface; the selected model travels with the next turn/start request. Display text uses native egui selection on the shared canvas text transform. Background conversation refresh retains the last successful transcript.

Validation: 754 Rust library tests passed (five ignored), including explicit
input provenance, refresh failure retention, provider identity during continuation,
and native egui Copy at 50%, 100% and 200% scale. Five Cursor adapter tests pass.
The installed Codex model catalog returned Astra, Sol, Terra, Luna and GPT-5.5.
All 35 agent contract rows match decisions.json. The release build succeeded.
Windows denied launching `target/release/slate.exe`; native visual verification
of this refinement and live inference remain unperformed. Both open workbooks
were saved before closing the previous executable. No security policy or artifact
location was changed to bypass the launch denial.

The final 17-test agent regression run also passes, including legacy coding
cards whose saved ChatView predates the linear-stream flag. Provider identity
still prevents those cards from forking.


## Selection-path refinement (21 September 2026)

Program, project and conversation choices respond to the same visible target
that provides hover feedback, on the first click. Ollama discovery exposes a
single home tile and a provider-specific catalog in the shared title dropdown.
Local model selection preserves the session and is applied to the next request;
models are never downloaded implicitly. Picker transitions and workbook identity
participate in sizing invalidation; geometry and streaming content are measured
again when they change. Long model titles set a minimum card width.

Validation: 420 Slate library tests and 22 atlas-ai tests passed (3 ignored),
including first-click tile corners at 50%, 100% and 200% zoom, picker exit and
streaming-size invalidation, and local model switching without session replacement.
The Ollama library test target also passed (no tests). The installed catalog was
queried with `ollama list`. Release replacement was blocked by access denied on
`target/release/slate.exe` while the existing Slate process was running; the
new native UI has not been launched or visually verified.

[Codex selection walkthrough](../../agent/images/codex-selection-flow.png) is an
illustrative UI concept using example project and conversation names, not a native
application screenshot.
