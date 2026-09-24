# Agent portal — interaction contract

Status: **agreed** (15 September 2026; implementation verification in progress)
Family: portal
Portal class: **host** (Art. V.3) · Type: **agent** · Subtype: **local link**
Command: `board.portal.agent` (placement) · Key: none in v1 · Palette:
"agent portal" (aliases: cursor portal, local agent)
Inherits: P0.* (all, including P0.9), P1.node, P1.portal, P2.DragShape,
**P2.PortalHost** — deviations flagged below.

Owner: `atlas-agent` contracts; `slate-doc::agent_inputs` semantics; `atlas-codex` protocol; `atlas-comfy` checkpoint presets; `atlas-ai` supervision and picker; `board_portal` focus; `board_portal_chrome` hover chrome; `atlas-shell::home` album motion.
Forbidden forks: a second Cover Flow; a second contents-focus prelude.

## What it is, and the 10% it implements

A minimal sidecar for an installed program, placed on the board. The empty portal
shows only program icons and names. Cursor and Codex use a small chat interface;
configured image sidecars show their generated results inside the portal.
Ollama text chat is included in the 18 September refinement. ComfyUI is the local
image engine: Generate, Vary, and Render against installed checkpoints. The generic image link is
available to explicitly configured sidecars today.

The user approved this refinement with “ok go for it!” after reviewing the
[design report](../../agent/portal-evolution-plan.md) and concept images.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Palette: "agent portal" + Enter; Portals dock flyout; command `board.portal.agent`; board view only. Ollama and ComfyUI are no longer program tiles: text and image agents start from media, through the Agent squircle on a selected picture, video, 3D model, text or generated frame. Existing Ollama and ComfyUI cards keep working. | stated | 100 |
| D02 | Stickiness & repeat | One-shot: placement returns to Select; Space/Enter may repeat via the registry history. | pattern | 80 |
| D03 | Gesture grammar | P2.DragShape. Place the frame and choose a provider. Coding providers then show projects, followed by resumable conversations and New conversation. Discovery and history reads run off-thread. | stated | 100 |
| D04 | Click vs drag rule | Travel > 4 px defines the frame; below it places a 960x540 default portal centered on the click. | precedent | 85 |
| D05 | Modifiers | Shift locks 16:9 during drag, matching Repository Lens portal placement. Ctrl/Alt unassigned in v1. | precedent | 80 |
| D06 | Constraints & snapping | Grid snap and smart guides apply to the frame rect; agent contents never snap. | pattern | 80 |
| D07 | Direction / value locks | History wires use width 4.5 and opacity 0.42 in dark mode / 0.14 in light mode. Only immediate fork outputs stack downward from the primary top datum; upstream links stay single. Large forks compress strand spacing to remain within the parent height. Geometry updates during drag. | stated | 100 |
| D08 | Numeric / manual entry | n/a in v1; dimensions are edited by resizing the frame. | guess | 55 |
| D09 | Preview & readouts | Quiet at rest. Chooser contains only icons and names. Chat identity and actions reveal on hover or contents focus. Media an agent makes looks like the media: a picture an agent generates is an ordinary image card, and a note an agent writes is an ordinary sticky. Hover or selection reveals, on the card, its inputs as role chips (Prompt, Geometry, Image, Style) that select their source on click, the action Render, Transform or Generate named by what is wired (Run on a note), Keep while live, a waiting count, progress, and the prompt docked along the bottom as a capsule that expands on hover. Two or more results show as squares under the picture; a click picks one. Selected, a picture with two or more results cover-flows under the wheel; its labels, dock and ports hide while it flows and return when the picture is clicked again, and where the flow settles becomes the pick. Hover never flows: unselected, the wheel zooms the board. The card height follows the newest result. A live picture at rest shows one small dot. Model, count, aspect, seed and Live are on the Agent squircle, not on the card. Wire affordance is a subtle animated blister behind the edge; resting wires have no port rings. While a picture or note an agent makes is selected, hovered, or any wire is being drawn, it shows labeled input ports down its left edge, ringed when empty and filled when wired: a picture has Media (picture or 3D model), Prompt (any text) and Style (a second picture that guides the look); a note has Image and Prompt. One output port with a plus sits at the right middle. | stated | 100 |
| D10 | Cursor | Output-grip proximity suppresses body resize hit targets and resize cursors. The grip owns continuation/fork presses; surrounding body edges retain their existing resize behavior. | stated | 100 |
| D11 | Commit | Portal presentation, message ranges, bundle membership and branch-view pruning use SceneCmd. Transcript bytes remain linked source data. Sending from a historical checkpoint starts an independent provider session with an explicit prefix replay; undo never reruns inference. | stated | 100 |
| D12 | Cancel | Esc peels one layer per press (P0.1): maximize → contents focus → drag draft → armed tool → selection. Releasing contents focus leaves the shelf / poster intact. Agent sidecar work is out-of-process and is not cancelled by board Esc. | pattern | 80 |
| D13 | Selected presentation | Double-click the title to edit the name across its maximal linear branch segment, including hidden members. Codex and Cursor titles combine the custom conversation name with a model dropdown populated by the installed provider; the menu also offers Rename. Only the tail card (or an unsent draft) offers the dropdown; a card that has a child shows "Title · Model" as canvas text and refuses a model change. Model choice is journaled per card and inherited by new continuations. Renaming a fork does not rename its siblings or upstream prefix. | stated | 100 |
| D14 | Post-edit | Switch presentation only through the contextual menu. Single chat window converts each maximal linear segment, keeps the current viewport height as its ceiling and shrinks shorter content. Chat train restores message views and forks. Single windows only show full conversation, and never offer Unbundle. | stated | 100 |
| D15 | Non-goals | Codex and Cursor expose only conversations supported by their installed interfaces. Legacy Cursor desktop chats are not claimed as SDK agents. No hidden provider-history deletion. Ollama is local chat and the one-shot text block; it reads wired pictures only with an installed vision model. ComfyUI runs installed Stable Diffusion 1.5, 2, and XL checkpoints with core nodes only; the Style port runs the Stable Diffusion 1.5 T2I style adapter with CLIP ViT-L. IP-Adapter and other custom nodes, transformer checkpoints (FLUX, SD3, Qwen-Image), masked edit, upscale, and weight download remain later. Image agents may also use ChatGPT through the installed Codex's ChatGPT sign-in (no API key) or GPT Image through the person's own OpenAI API key; ComfyUI stays the default, so no image agent requires an account (Art. I.4). Video generation remains later; a video contributes the frame it shows. Direct OpenAI chat and local tool execution remain later increments. | stated | 100 |
| D16 | Create-style inheritance | No. Host portals use portal styling and do not consume shape/text style state. | pattern | 80 |
| D17 | Hit-testing & pick | The header and border pick or move the card. Transcript text supports native drag selection and Copy through the shared scaled-text owner, retaining glyph layout while zooming. Text selection does not move the card. Program tiles and project/conversation rows accept the first click throughout their visible hover target; contents focus is not a prerequisite. Conversation-title text renames on a single click; the separate model-name text opens the model dropdown across its complete visible area. A press in a tail card's "Message" area focuses that card's own composer on press, caret at the end, when that card is the topmost node there; the press never starts a move, marquee or selection drag. The rest of the card body still selects and moves it. A wire dropped on a generator's or text block's port binds to that port when it carries the port's kind (text on Prompt, pictures and 3D models on Media and Style) and is inert otherwise; dropped anywhere else on the node, edge or body, it lands on the first free port that reads it. A wire drawn back from an input port lands on the other node's output. Wires saved before ports keep their anchor and read by what they carry. Chat cards keep the left midpoint. A generator's input chip selects its source node. | stated | 100 |
| D18 | Portal class & authority | Host. Provider histories remain outside .slate. Branch deletion prunes this view and retains its source; one Undo restores view references without inference. Agents cannot mutate transcript ownership through the host. A picture or note an agent makes is media, not a host portal: its agent binding is journaled on the node, its results stay in the linked session folder, and the picture it shows or the words it holds are picked or edited by the person. Run completion never patches the scene. | stated | 100 |
| D19 | Source binding | SourceUri remains the relative-first project locator. AgentPortalRef stores provider, cache session, optional provider conversation id and model, bundle locator and immutable seed image. Coding train cards share one provider conversation. Unknown provider ids survive load. | stated | 100 |
| D20 | Query & parameters | Journaled binding includes provider/session/source plus ChatView ranges, immutable history ancestry, detail, draft status and hidden bundle members. Arbitrary reparenting is forbidden; validated subdivision may insert views of a contiguous range without changing its ancestry or checkpoint. Context connectors remain independently editable. A generator journals its checkpoint (absent means Auto) and its Live flag. | stated | 100 |
| D21 | Regeneration & staleness | Only explicit wires into the message card midpoint input supply additional context at Send. Selection, viewport, revision ids and ambient canvas data are not appended to messages. Editing connected text changes the next input without running an agent. ComfyUI accepts further presses while it works, up to eight waiting on one generator, and runs them in order. Its wires are reread on every run and are never consumed. A wired 3D model is captured when its run starts: shaded view and exact depth from its current camera, standing on a ground plane at its base. Live reruns whenever a wired input changes (the model's camera, a prompt, a source image, or the checkpoint), one run at a time with the latest inputs and one seed per generator; Keep leaves the shown frame in the album and continues in a new slot. Background history refresh preserves the visible answer, including on refresh failure. Auto-sizing invalidates on picker exit, workbook change, geometry change, typing and streamed output. Undo pauses sizing until new content arrives. Project and conversation pickers size to their visible rows with a bounded list height and a separate header. | stated | 100 |
| D22 | Contents interaction | Codex and Cursor have one linear stream, with continuation at the tail by clicking or dragging the dedicated top output circle; historical handles cannot fork coding agents. Train presentation is the default. Chooser phases hide input and output handles. The left context handle shares the top status dot's inset and is the same gray at zero resting opacity: hovering it shows the linked-context description and suppresses the split; hovering outward from the card edge slides that handle and any provenance wire up and reveals an equal-size human context handle just below, on the same inset column. A click on that dot shows the linked documents as a temporary cluster that collapses when the pointer leaves; a double-click pins them on the canvas. The human context handle holds the card's pocketed context. Deleting a node that a card already sent as context hides it and keeps its id and wires, in one journal step, and it shrinks into every consuming card's input. While the pocket is non-empty the handle stays visible at rest and its hover lists the pocketed context. A click shows the context beside the card, and a second click pockets it again. Deleting the last consuming card also removes its pocket. Every chat card except drafts and bundles has a quiet collapse chevron just left of the ellipsis. The card's hover reveals it. A leaf card deletes from Delete, or from the ellipsis menu's Delete card, unless its composer holds typed text. An empty focused composer passes Delete through and keeps Backspace. Dragging a train card moves it, except while a text field is being edited. The right edited-document handle appears only when that train card reported document changes, inset to match the top output dot. Hover reveals only those gray handles; the edge wire blister is not shown on an agent card. Clicking lists artifacts; choosing one opens an existing Web or filtered Atlas portal with a provenance wire. Midpoint context wiring stays editable and separate from immutable top history rails. Local providers retain checkpoint forks. Enter sends; Shift+Enter inserts a newline; the placeholder appears only on selected cards. Ollama has exactly one home tile; installed local models are chosen through the same title dropdown as Codex, without replacing the conversation. ComfyUI has exactly one home tile; installed checkpoints are chosen from the hover menu, without a chat transcript. Focusing an agent does not suppress canvas wheel zoom, except while the pointer is over an overflowing project or conversation list, or over a user-sized chat card whose text overflows, which scroll instead. Once a chat train has started, Choose program is not offered. Provider tiles use Codex, Cursor and Ollama marks; model labels omit GPT version prefixes for Astra, Sol, Terra and Luna. A focused live generator is a viewport onto its wired model: drag orbits, Shift+drag pans, and the wheel zooms that model; leaving focus locks the model at that pose as one journaled camera change. Clicking a generator's or text block's output port, or releasing its wire on empty board, opens a two-item menu: Text adds a text block (local language model) wired to its Image port, and Image adds a generator wired to Media from a picture or Prompt from text. The new node lands beside the source for a click, or with its port at the drop point, in one undo step. A text block spawned from a picture starts with a style-description instruction. It runs its wired prompts and then its instruction once per Run or Enter, with no conversation history; the reply replaces the previous one, grows the card to fit, and feeds any wire from its output. Its model menu offers Auto, which uses an installed model that reads pictures. From media, the Agent squircle opens an upward editor with Text or Image, the model menu, a prompt face, and for images a count (1 to 8), aspect (Source, 1:1, 3:2, 2:3, 16:9), seed lock and Live. Submit spawns a new frame downstream, wired from the source (a picture, a video's shown frame or a 3D view on Media, words on Prompt), journals the prompt and settings on it, and runs it; the source stays selected for the next variation. Pictures fill the album as they arrive, with square thumbnails under it while it is browsed. Since 24 September 2026, Text makes a sticky an agent writes and Image an ordinary picture; a wire released on empty board from any media that shows the Agent squircle opens the same modality menu (Text and Image today; 3D and video join the same list), and the new wire keeps the grip it left from. | stated | 100 |
| D23 | Level of detail | Cards display complete text, or a collapsed three-line capsule. A collapsed card keeps its width and is the header plus three lines and the text pad, ending in an ellipsis when more text exists. The tail keeps its composer below those lines. A resize records the card's size in the same undo step and expands a collapsed card. Text rewraps to that width, and overflow scrolls with the wheel inside the card. Fit to text clears the size. Name only and Summary choices are removed from menus and inspector. Full conversation offers inline input and response. Transcript text uses shared canvas text scaling, fixed wrap bounds and world-scaled scroll offsets during zoom. No blue selection outline or dimensions. | stated | 100 |
| D24 | Export serialization | Chat exports remain honest host posters and source identity; derived history rails serialize with the same cubic geometry. No live controls, credentials or inference are exported. A picture an agent makes exports as a plain image of the result it shows (the pick, else the newest); a note an agent writes exports the words it shows (its own, else the finished reply). A chat image bundle keeps its bundle export. | stated | 100 |
| D25 | Bake | Bundles may contain other bundles to arbitrary depth. Unbundle restores one presentation level, preserves inner bundles and lays visible cards side-by-side in train order. Bundle/Unbundle remain contextual squircle actions; single chat windows never offer Unbundle. Unbundle on a picture an agent makes places every other result as an ordinary picture beside it in one undo step; the card keeps its agent. Picking a result makes it the picture file. Copying a picture an agent makes copies only the picture it shows, as a plain placed image without the agent. Editing a note an agent writes makes the words belong to the person; running it again replaces them as one undo. | stated | 100 |
| D26 | Collaboration & per-peer | Frame and binding sync as document data. Runtime history, hover and album focus remain local view state. Linked assets are references. | stated | 100 |
| D27 | Agent surface | Adapters consume immutable snapshots. Proposed board edits retain staged SceneCmd acceptance. Coding tools operate under the provider configuration; Slate does not substitute a read-only persona or silently grant requested approvals. The only grant is the person's explicit per-conversation Full access (D32). | stated | 100 |
| D28 | Determinism & provenance | History checkpoints are exact exclusive transcript bounds; partially loaded prefixes are refused. Forks replay quoted ancestor text into a fresh source, not a claimed native provider fork. Shared source snapshots are immutable Arc values. | stated | 100 |
| D29 | Performance envelope | One atlas-ai AgentSources worker/cache per source path shared by its cards. Source reads and protocol work stay off-thread. Summary layout caches by source/scene revision and zoom resolution. Completed train runtimes are released; large-history frame-time validation remains pending. A generator's resolved inputs cache per scene and output revision. A 3D capture is two offscreen passes and two fast PNG writes on the frame that starts a run, into local app data; ComfyUI results land in local app data too, so only the small manifest syncs with the workspace. | stated | 100 |
| D30 | Failure & honesty states | Installed program, healthy server and installed model are separate states. Ollama never downloads a model or silently falls back to cloud. ComfyUI starts with API nodes disabled and never downloads a checkpoint. It names a checkpoint with no matching ControlNet, a transformer checkpoint, a model still loading, a missing GPU and extra sources. Failures name unsupported image inputs, context budget, model absence, transport errors and cancellation. A Style picture names a missing style adapter or CLIP vision model, a checkpoint family without an adapter, and a second style wire. A picture wired to a local model that cannot read it names an installed model that can, or asks for one. GPT Image names a missing or refused API key and offers the paste; ChatGPT names a Codex run that returned no picture. | stated | 100 |
| D31 | View-state ownership | Journaled: bindings, ranges, parent view references, detail, bundle membership and placement. Derived: source transcript snapshots, streaming text, readiness and hover. Pruning removes view references, not foreign provider records. A generator's live run identity, input signature, waiting runs and capture files are derived; kept frames live in its bundle manifest. | stated | 100 |
| D32 | Trust, sandbox & consent | The renderer-free Codex adapter uses installed authentication and provider permissions. Supported command/file approvals appear on-card with Allow once and Deny; unsupported requests fail explicitly. Cursor loads installed settings and enables SDK auto-review. **Full access** (ellipsis menu, Cursor and Codex, disabled while a reply runs) is the person's explicit grant for one conversation, applied from its next message: Codex turns carry approval policy `never` and a full-access sandbox; Cursor's sidecar restarts without auto-review. The model name turns deep red (the theme's danger color) on every card of that conversation the moment it is granted; the ellipsis row shows a check. The grant is stored for this user in the Atlas data folder (`agent-access.json`, keyed by session), never in the workbook or the synced link folder, so a received `.slate` never arrives with an agent trusted to act freely; packaging and export refuse that file. Opening a referenced Web or Atlas portal uses its existing consent and source owners. An OpenAI API key is pasted as bullets into the agent editor and stored for this Windows user in Credential Manager (`openai-api-key`); curl receives it on stdin, never on its command line, and it is never logged, exported or written to the workbook. ChatGPT images use the installed Codex's own ChatGPT sign-in, running in Slate's local output folder. | stated | 100 |
| D33 | Portal chrome | Minimal theme-aware cards have faint shadows and no bevel. Wires have no bevel. Selection retains Fill and Stroke squircles; default stroke weight is zero. Authored card colors and strokes persist and export. Compact title and ellipsis remain at the top. Agent cards do not offer maximize. | stated | 100 |
| D34 | Portal maximize | **P1.portal.maximize.** Fills the window at the screen aspect; Esc restores. | stated | 100 |
| D35 | Portal-local UI | P1.portal.local-ui. Provider, model, explicit input wiring, sidecar setup and bundle commands stay on the portal/inspector. The board-wide panel gains no provider-specific settings. The Agent squircle on media an agent makes edits that agent: model, count, aspect, seed and Live apply at once, and Submit runs it again. | stated | 100 |

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
| `GENERATION_QUEUE` | Presses waiting on one generator | `8` |
| `CAPTURE_TIMEOUT` | Wait for a wired 3D mesh before naming the failure | `30 s` |
| `capture_size` | Render capture area at the model card's aspect | `≈512²`, multiples of 8 |
| `GROUND_EXTENT` | Capture ground plane half-width | `60` model radii |
| `RENDER_DENOISE` / `RENDER_DEPTH` / `RENDER_EDGE` | Render from the shaded view under depth and edge control | `0.9` / `0.9` / `0.6` |
| `VARY_DENOISE` / `VARY_EDGE` | Vary a wired picture within its outline | `0.7` / `0.6` |
| `STYLE_STRENGTH` | Weight of the Style picture's tokens (1.0 also copies its subject) | `0.6` |
| `PORT_RADIUS` / `PORT_LABEL_PX` | Designed port size and label type (scale with zoom) | `5` / `11` |
| `PORT_REVEAL` | Pointer distance that reveals a flow node's ports | `56` designed px |
| `IMAGE_SIDE` | Long side of a picture sent to a local vision model | `1024` px |
| Text block card | Default size; grows to fit its reply | `440 x 320`, up to `1200` high |

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
9. Wire a 3D model and a sticky note anywhere onto a ComfyUI generator's edge.
   Hover reads Geometry and Prompt chips and the action Render. Render keeps
   the model's massing and camera and follows the note. Live then reruns as the
   model's camera or the note changes, one seed per generator; Keep leaves a
   frame in the album; Stop ends Live and clears waiting presses.
10. Click a generator's output port and choose Text. A text block appears beside
    it, wired to its Image port, with a style-description instruction. Run
    writes the description with the installed vision model. Drag the block's
    output onto empty board and choose Image: the new generator reads the
    description on Prompt. Wire a reference picture to its Style port and
    Generate follows the reference's look. One Undo removes each spawn.

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

Top history handles and midpoint artifact/context handles are separate. Codex model options come from the installed app-server model/list interface; the selected model travels with the next turn/start request. Cursor model options come from the sidecar catalog and travel on the next send; Stop cancels that run. Display text uses native egui selection on the shared canvas text transform. Background conversation refresh retains the last successful transcript.

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

## Chooser and train handles (21 September 2026)

Project and conversation lists use a quiet section label, inset rows with a
trailing chevron, and a filled primary action. Input and output handles stay
hidden through that phase. Train presentation is the default once a program is
chosen. The left context handle is the existing gray at zero resting opacity.
Hovering it shows the linked-context description and keeps a single handle.
Hovering outward from the card edge slides that handle, and any provenance
wire, up and reveals an equal-size human context handle just below. The right
edited-document handle is drawn only when that train card reported document
changes.

Chooser capsules are shorter and wrap into columns once the list is long.
The wheel zooms the board while the pointer is over a card. The project list
before a chat starts is the exception: the wheel scrolls that list.
A spinning mark
and a Stop control show while a response is in flight. Message pairs put one
user line and its reply on the same card, with a hairline between them.
Agent cards omit maximize.
The ellipsis is larger and sits nearer the right edge, clear of the output
dot. Once a chat train has started, Choose program is absent. Hover shows
only gray handles, inset on the status-dot column; the teal edge blister
is not drawn. Saving an API key clears the failure and leaves the card
ready to send.
A gray-dot click places each referenced file once, beside the card. A second
click retracts those cards into the dot. Delete on one of them plays the same
retract. Paste of a copied card is a normal board node.

## Local image generation (22 September 2026)

The user asked for a simple, powerful flow for rendering a 3D model with a
style prompt, borrowing Flora's and xFigura's dataflow: typed inputs visible on
the node, readiness checked when a run starts, every output kept, and a live
mode that follows the model. The generator reads what is wired and names its
action from it: a note alone is Generate, a picture is Vary, a 3D model is
Render. Wires land anywhere on the generator's edge. Hover shows each input as
a role chip; clicking one selects its source.

Render captures the model's shaded view and exact inverse depth from Slate's
own renderer, standing on a ground plane at its base, and runs DreamShaper 8
LCM (Hugging Face `Lykon/DreamShaper`) under the ControlNet 1.1 depth and
canny models (`comfyanonymous/ControlNet-v1-1_fp16_safetensors`) in six steps.
The adapter reads each checkpoint's and ControlNet's family from its
safetensors header, so Auto never pairs weights from different families. Live
reruns on any wired change with one seed per generator; Keep keeps the shown
frame. A focused live generator flies its wired model.

Validation: the library suites for atlas-comfy (6), atlas-agent (4), atlas-ai
(23), atlas-shell (129, one ignored), slate-doc (157, one ignored) and
slate-artifact (43) pass, with the document migration and artifact export
integration tests. The Slate suite passes 476 tests; its three failures predate
this work and exercise none of its code: `join_gp3_open_plus_closed`,
`maximized_restore_glyph_click_leaves_maximize` (agent cards omit maximize, D33)
and `agent_summary_cache_refreshes_for_theme_and_zoom`.
Regressions cover the wired note as the prompt in Live, one seed per run,
rapid presses waiting their turn, Stop, Keep, Auto, off-midpoint and legacy
wires, inert free and provenance ends, a named missing GPU, rerun on an edited
note, capture size and depth range, and the hover chips and actions painted in
a real frame.
A native release run of a saved Live workbook with a Rhino pavilion captured
the view and depth, chose DreamShaper 8 LCM through Auto, used the sticky text
as the prompt, and returned a grounded render in about three seconds cold.
Windows security refused synthetic mouse input and screen capture from the
shell, so pointer interaction in the native window was not exercised.

The DRY review returned extract-first. Applied: one model menu owner for chat
titles and the checkpoint chip (labels through `model_label`, default through
`model_fallback`), `is_image_generator` as the only generator test for grips
and the wheel, `ImageTask` in `atlas-agent` as the one Render/Vary/Generate
rule for the button and the adapter, words-only shape text emitted by
`agent_inputs::snapshot` for generators, one wheel owner (`image_wheel`), and
DV-22 widened to the server lifecycle with reciprocal `TWIN` markers.

### Sampling previews, live typing, orbit direction (23 September 2026)

The user asked to watch a render come into focus instead of waiting about 1.5
seconds for the finished frame, for a prompt edit to update a live render,
and for the 3D orbit to match Rhino. The adapter now subscribes to ComfyUI's
event socket before it queues a run and asks for TAESD previews when the
matching decoder is installed. Otherwise it asks for the latent RGB
approximation. The generator paints the newest frame over its album, with a
thin bar showing progress through the sampling steps. Preview frames are
derived state: they are never journaled or exported, and the finished image
replaces them. Live now reads a wired note's text while the note is being
edited, and renders it once typing has paused for 300 ms. Dragging right in a
3D viewport now turns the model right. Vertical orbit is unchanged.

## Train card refinement (23 September 2026)

The user specified six fixes to the chat train cards. The train model is
unchanged: each card links to its predecessor, and only the tail (the card
with no child) hosts the composer and its "Message" hint.

- **Model menu on the tail only (D13).** A sent card's header reads
  "Title · Model" as canvas text. Only the tail, or an unsent draft, opens the
  model dropdown, and `agent_set_model` refuses a card that has a child. A
  click on the title still renames.
- **Collapse (D22, D23).** `ChatView.collapsed` is journaled. A chevron just
  left of the ellipsis dispatches `portal.agent.collapse` for the selected chat
  cards. It is quiet at rest, the card's hover reveals it, and it scales with
  the canvas. A collapsed card keeps its width. Its height is the header plus
  three lines of its text and the text pad, and the lines end in an ellipsis
  when more text exists. The tail keeps its composer below them. The flag and
  the refitted rect are one patch, so one Undo restores both. Bundles and
  drafts have no chevron.
- **Authored size (D23).** When a resize gesture ends on a chat card,
  `ChatView.size` is recorded in the same patch as the rect, and a collapsed
  card expands. Fitting keeps that size. Collapse still wins the height, and
  the width stays. Text rewraps to the width. Overflow scrolls inside the card
  with the wheel, using the transcript scroll offset, instead of zooming the
  board. The ellipsis menu then offers Fit to text (`portal.agent.fit`), which
  clears the size and refits in one step.
- **Composer focus (D17).** Each painted tail registers its own composer field
  every frame. The old single field meant only the last-painted train was
  clickable. A primary press in a tail's field focuses that composer at once,
  with the caret at the end, when the card is the topmost node under the
  pointer. The press is eaten, so no move, marquee or selection drag starts.
- **Deleting the final card (D18, D22).** Any leaf card can be deleted, with
  or without a reply. Provider history is untouched. Delete works while that
  selected card holds the keyboard, as long as its composer is idle or empty.
  With an empty focused composer, Backspace stays with the field. With typed
  text, both keys edit it. The ellipsis menu offers Delete card on leaves.
  Unsent text is discarded. The parent becomes the tail again and gets the
  composer back, and one Undo restores the card.
- **Pocketed context (D22).** Deleting a node that has a consumed
  agent-input wire into an existing chat card does not remove it. One patch
  hides it, keeping its id and wires, and one retract ghost per consuming card
  shrinks into that card's left midpoint. This generalizes
  `begin_context_retract`; provenance retract is unchanged. Wires to a hidden
  node are neither painted nor hit. A click on the human context handle
  (`portal.agent.pocket`) shows that card's pocketed context beside it with the
  shared spawn placement. A second click pockets it again with the same
  animation. A shared node is one node, so showing it from one train shows it
  wired to every train. Deleting a card that leaves a pocketed node with no
  consuming card removes that node and its wires in the same commit. Chat
  cards wired as context are conversation views and are never pocketed.

Verification: headless regressions cover the tail-only model menu, collapse
height, width and one-step undo, resize recording and Fit to text, two trains
whose "Message" areas each focus their own composer, tail deletion by key with
the typing guard and undo, single-train pocket and restore, a shared node
pocketed into two trains, and orphan cleanup. The native app was not launched.

## Flow ports, Text and Image from an output (23 September 2026)

The user asked to take more cues from Flora and xFigura: once a generator is
selected it shows three permanent input ports (Media for a picture or 3D
model, Prompt for any text, Style for a second picture that guides the look),
and clicking or dragging out of its output offers Text or Image. Text is a
local language model that follows a prompt, with Prompt and Image inputs and a
link to the upstream node; the stated use is writing a style description of a
picture. The user asked for the ComfyUI back end to be configured to match.

Port positions have one owner: `slate_doc::agent_inputs` holds the table and
`WireHost::ports` reads it, so hit-testing, snapping and painting share one
list. A wire's slot is derived from its receiving anchor, and
`ContextItem::port` / `InputSnapshot::on` is the single rule for older wires
without one. ComfyUI's Style port runs core nodes only (`CLIPVisionLoader`,
`CLIPVisionEncode`, `StyleModelLoader`, `StyleModelApply`) with the TencentARC
T2I style adapter (`t2iadapter_style_sd14v1.pth`, Stable Diffusion 1.x) and
OpenAI CLIP ViT-L/14. A probe on the installed DreamShaper 8 LCM showed that
at strength 1.0 the reference also replaces the prompt's subject, and that
0.6 keeps the subject while borrowing framing and palette. The text block is
an Ollama agent portal (`PortalView::Text`); its instruction is journaled on
the portal, and each run is one-shot (`AgentRequest::oneshot`, history
cleared by the runtime). Wired pictures travel as downscaled base64 JPEG to
models whose `/api/show` capabilities include `vision`.

Machine configuration performed with the user's request: installed
`qwen2.5vl:7b` in Ollama; added `t2iadapter_style_sd14v1.pth` to ComfyUI's
`style_models` and `clip_vit_large_patch14.safetensors` to `clip_vision`.
Slate itself still never downloads weights.

The DRY review returned extract-first. Applied: ports through
`WireHost::ports`; the slot accessor in `atlas-agent`, used by
`atlas-comfy::sources`, `atlas-ollama` and `resolve_generator`; one
`InputRole::look` table for chips and ports; `atlas_shell::menu::anchored`
for the preview-face menu, the artifact list and the new output menu;
`bind_program` / `program_card_size` extracted from `set_agent_program`;
`agent_spawn_rect` extracted from the continuation drag; one-shot history
owned by the runtime; the workspace `base64` crate. Not applied: moving the
five existing gray handle-dot call sites onto one helper, recorded as DV-23.
Flow ports are a different visual and do not add a sixth copy.

Validation: slate-doc (159, one ignored), atlas-comfy (8, one ignored),
atlas-ollama (2, one ignored) and atlas-agent (8) library tests pass. They
include port binding and slot refusal, default ports, style routing and named
failures. Four new headless Slate regressions cover a dropped output wire
opening the menu, a spawned text block wired to Image in one undo step, its
one-shot request, a text block feeding a generator's Prompt, a generator
feeding Media (Vary), a Style wire that does not become the Vary source, and,
in a real frame, port labels that appear near a generator but not at rest,
plus the text block's Run and Auto controls.
The full Slate library run passes 499 tests; its three failures are the
pre-existing ones listed on 22 September. Live checks against the installed
engines: the generated style graph ran on ComfyUI in about 7 seconds, and the
text block adapter chose `qwen2.5vl:7b` under Auto and described a picture in
1.8 seconds warm (24 seconds cold). The native window was not launched, so
pointer interaction with the ports and menu is unverified.

## Sessions, outputs and full access (23 September 2026)

Stated by the user in one request; implementation choices are noted where
the user delegated them.

- **Rejoining after close or relaunch.** Closing a tab stops the Cursor
  sidecars and Codex links that no other open tab uses; a sidecar stops any
  stale one before it spawns, records `sidecar.pid` (newest wins) and exits
  when its parent Slate process is gone. The agent runtime is parked per tab
  and restored on return, so documents with colliding node ids never share
  waits, drafts or history. When a document becomes active, each saved
  Cursor/Codex conversation reconnects in the background once per session
  per process. A Send pressed before that finishes waits on the card as
  "Connecting…" and goes out once connected; a failed load names itself.
- **Reattaching a conversation.** Picking an existing conversation with at
  least one answered message lays its history out as one card per message,
  bundles them, and ends with an empty draft tail whose composer is focused.
  One Undo removes it. Every bundle carries a quiet count ("12 earlier
  messages"). Nothing is added when a train for that conversation exists.
- **Outputs.** The right output circle opens a capsule stack beside the card
  instead of a list of links: requested items first, then a hairline and
  "Also changed". Each capsule shows a short name, a file-type chip, and a
  "vN" badge when versions were captured. Click spawns or retracts one;
  Shift-click collects; "Spawn all" / "Spawn N" spawns one frame laid out per
  the spawning table in `docs/agent-link-contract.md`, with a table that
  feeds a dashboard wired left of it; drag spawns at the drop point.
  Requested means named in that message's `return.json`, or, when a message
  has none, created or modified in its output folder.
- **Output folder and versions.** Each send names a default folder,
  `slate-outputs/<board>/<date>-<title>-<id>/` under the conversation's
  project (or the AI workspace), recorded once in the link folder. After each
  message, changed outputs are copied to `versions/tNNN/` beside the
  conversation record, never into the workbook (Art. IX). A capsule's "vN"
  names the version its own card produced, so an edited dashboard reads v1
  on the first card and v2 on the next; an older card spawns its own
  version's copy ("<name> v1") while the newest spawns the live file. The
  badge spawns "<name> evolution": the versions left to right, each labeled
  with its number, the message that produced it, and lines added and removed.
- **Full access** (D32): an explicit per-conversation grant from the
  ellipsis menu, stored for this user outside the workbook.
- **Ellipsis menu (24 September 2026).** Every row carries a catalog glyph
  (Single chat window, Chat train, Message pairs, Full conversation, Choose
  program, Stop response, Fit to text) or the menu family's Lock and Trash
  (Full access, Delete card), so all labels start on one line after the icon
  column. Rows group as presentation, conversation, then this card, with
  hairline separators; Delete card is red.
- **Scheduled messages (24 September 2026).** Ellipsis → Schedule… on
  Cursor and Codex cards opens a dialog: the message (the composer's text or
  the last message sent), a date and time typed as people say them (tonight,
  Oct 1, 4:55 am), and Once / Every hour / Every day / Every week, with a
  preview line. Windows Task Scheduler runs it while Slate is closed; replies
  arrive through the conversation folder and appear when the board opens.
  While a run is still ahead, a clock glyph sits in the top strip just left of
  the collapse chevron; hover names when, a click reopens the dialog. Local
  only for now: the computer must be on.
- **Wires into a train (24 September 2026).** A wire bound into a chat card
  starts in the train's history-rail gray instead of the drawing color. The
  gray is only the default: a color the person picks for that wire is stored
  and never reset.
- **Typing continues after Send.** When the person sends from a card's field,
  the caret moves to the new tail's field at once; a send that waited to
  connect leaves the caret where the person has moved since. A chat field
  taking the caret also takes native keyboard focus back from a web page, so
  keys never keep going to a dashboard the person clicked earlier.
- **Web context in later sessions.** A web page allowed on a saved board
  stays allowed when the same user reopens that board (portal-web-embed
  D32); a received board still asks.

## Media as the front door to agents (24 September 2026)

The user split AI into two modes. Cursor and Codex keep the agent portal and
its program, project and conversation tree. Ollama and ComfyUI leave that
chooser: the person starts from a piece of media, and the agent is one of the
tools applied to it. The user chose ChatGPT through both routes (the installed
Codex's ChatGPT sign-in, and an OpenAI API key). Submission options are image
count, Live, aspect and seed lock. Text results always go to a new frame
downstream, and existing Ollama and ComfyUI cards keep working.

The Agent squircle sits in the selection strip of a single picture, video, 3D
model, text document, note or generated frame; PDFs and design files are not
offered. Its editor (`selection_tools::agent_editor`) submits through
`portal.agent.spawn`, the same command as the output-port menu, with the
prompt and settings. The new generator or text block is wired from the source
by `build_connector` at the matching port, journals `instruction` and
`ImageSettings { count, aspect, seed }`, and runs at once, except Live, which
starts on its own. The source stays selected. What a node carries has one rule,
`agent_inputs::source_kind`: a placed text document now carries its words
(read once through the board's snippet cache, never by the snapshot); a video
carries the frame it shows, captured beside 3D views with the one PNG writer.

Engines: ComfyUI runs `count` graphs on consecutive seeds, and aspect sets the
Generate latent at the checkpoint's native area. ChatGPT runs as a branch of
the one Codex `Client::run`: it asks the built-in image tool for `count`
pictures and copies each finished file from `generated_images/<thread>` into
Slate's local output folder as it appears. GPT Image (`atlas-openai`) calls
the Images API, with edits when a Media or Style picture is wired. All three
share `runtime::image_output_dir`. `atlas-curl` is now the one curl transport
(Ollama, ComfyUI, OpenAI); requests travel as stdin configuration, so the key
never reaches argv. That closes DV-22's transport half.

The DRY review returned extract-first. All eight required changes are applied:
the shared prompt field, the one media rule, the one command, `atlas-curl`,
`write_fast_png` for video, Codex image collection inside `Client::run`, one
`Aspect` in `atlas-agent`, and `album_browsing` for the index strip.

Validation: library tests pass for atlas-curl (2), atlas-openai (3),
atlas-codex (4, one ignored), atlas-comfy (9, two ignored), atlas-ollama (2,
one ignored), atlas-agent (8), atlas-ai (44) and slate-doc (160, one ignored).
Five new headless Slate regressions cover:
- a picture's submit spawning a wired generator that journals its settings and runs in one undo step;
- ChatGPT routing;
- a text document driving a text agent through its words;
- chat cards and wires staying out;
- the editor painting its choices in a real frame.

The full Slate library run passes 534 tests. Of its four failures, two are the
pre-existing ones already listed. The other two are
`live_strip_shows_pages_for_a_single_pdf`, which now differs only by another
session's new crop squircle, and `wire_shift_and_crossing_marquee_selection`,
in marquee code this change does not touch. Both were in flight in concurrent
sessions.

Live runs:
- ComfyUI made two 16:9 pictures, each landing separately, in about 7 seconds.
- ChatGPT through the Codex app-server, with no API key, made one picture in 23.5 seconds.
- The Ollama text block ran through `atlas-curl` with a picture attached.
- The GPT Image API was not exercised, because no key is stored.

The native window was not launched; pointer interaction with the squircle,
editor and strip is covered only headlessly.

### Fixes after first use (24 September 2026)

The user reported four problems; all four are fixed.

- **GPT Image hung.** The adapter polled curl without reading its output, so a
  multi-megabyte base64 response filled the pipe and curl never exited.
  `atlas_curl::Request::send_until` now drains the response while it arrives
  and kills the transfer on Stop. A test sends 6 MB through curl.
- **Model list.** GPT Image models now come from the key's own `/v1/models`
  listing, newest first, with dated snapshots hidden behind their undated
  names. The listing refreshes when a key is saved. GPT Image 2 and later get
  flexible sizes.
- **Model chip.** It no longer wraps a missing arrow glyph onto a second line;
  the chevron is painted.
- **Wheel over menus.** An open model list (the chip menu, chat title menu or
  agent editor popups) now keeps the wheel instead of zooming the board.

Live check: the stored key listed the `gpt-image-2.5` family, `gpt-image-2`,
`chatgpt-image-latest`, `gpt-image-1.5`, `gpt-image-1-mini` and `gpt-image-1`,
and generated one picture in 13 seconds.

### Second round of fixes (24 September 2026)

Changes after the user's second review:

- **Progress.** A running generator or text block shows a pill with what it is
  doing, image *n* of *m* and the elapsed time. Engines without step previews
  also get a sweeping bar.
- **One model list per kind.** Every generator and text block chip lists every
  engine for its kind: ComfyUI checkpoints, ChatGPT through the sign-in, and
  the key's GPT Image models for pictures; local models, ChatGPT and the key's
  OpenAI chat models for text. The chip keeps its controls painted while its
  list is open, so the pointer can reach it.
- **Remembered model.** The last model chosen (chip, editor or submit) is
  saved per user and used by every new frame, including frames spawned from a
  dropped wire, until another is chosen.
- **Typed prompts.** An empty generator is a prompt field ("Wire a note to
  Prompt, or type here"); Enter journals the prompt and runs. With pictures, the
  prompt sits in the bottom bar as a capsule that expands on hover for
  re-editing.
- **Action labels.** Vary is shown as Transform, and each action explains itself
  on hover.
- **Text engines.** Text blocks run on `ollama`, `codex-text` (ChatGPT sign-in,
  a fresh Codex thread per run) or `openai-text` (Responses API with wired
  pictures).

Live check: the key's chat listing (legacy GPT-3.5 and GPT-4 hidden) answered
through `gpt-6-luna` in 3.5 seconds.

### Second round of fixes (24 September 2026)

Changes after the user's second review:

- **Progress.** A running generator or text block shows a pill with what it is
  doing, image *n* of *m* and the elapsed time. Engines without step previews
  also get a sweeping bar.
- **One model list per kind.** Every generator and text block chip lists every
  engine for its kind: ComfyUI checkpoints, ChatGPT through the sign-in, and
  the key's GPT Image models for pictures; local models, ChatGPT and the key's
  OpenAI chat models for text. The chip keeps its controls painted while its
  list is open, so the pointer can reach it.
- **Remembered model.** The last model chosen (chip, editor or submit) is
  saved per user and used by every new frame, including frames spawned from a
  dropped wire, until another is chosen.
- **Typed prompts.** An empty generator is a prompt field ("Wire a note to
  Prompt, or type here"); Enter journals the prompt and runs. With pictures, the
  prompt sits in the bottom bar as a capsule that expands on hover for
  re-editing.
- **Action labels.** Vary is shown as Transform, and each action explains itself
  on hover.
- **Text engines.** Text blocks run on `ollama`, `codex-text` (ChatGPT sign-in,
  a fresh Codex thread per run) or `openai-text` (Responses API with wired
  pictures).

Live check: the key's chat listing (legacy GPT-3.5 and GPT-4 hidden) answered
through `gpt-6-luna` in 3.5 seconds.

### Second round of fixes (24 September 2026)

Changes after the user's second review:

- **Progress.** A running generator or text block shows a pill with what it is
  doing, image *n* of *m* and the elapsed time. Engines without step previews
  also get a sweeping bar.
- **One model list per kind.** Every generator and text block chip lists every
  engine for its kind: ComfyUI checkpoints, ChatGPT through the sign-in, and
  the key's GPT Image models for pictures; local models, ChatGPT and the key's
  OpenAI chat models for text. The chip keeps its controls painted while its
  list is open, so the pointer can reach it.
- **Remembered model.** The last model chosen (chip, editor or submit) is
  saved per user and used by every new frame, including frames spawned from a
  dropped wire, until another is chosen.
- **Typed prompts.** An empty generator is a prompt field ("Wire a note to
  Prompt, or type here"); Enter journals the prompt and runs. With pictures, the
  prompt sits in the bottom bar as a capsule that expands on hover for
  re-editing.
- **Action labels.** Vary is shown as Transform, and each action explains itself
  on hover.
- **Text engines.** Text blocks run on `ollama`, `codex-text` (ChatGPT sign-in,
  a fresh Codex thread per run) or `openai-text` (Responses API with wired
  pictures).

Live check: the key's chat listing (legacy GPT-3.5 and GPT-4 hidden) answered
through `gpt-6-luna` in 3.5 seconds.

### Web pages as agent media (24 September 2026)

A bound web portal now shows the Agent squircle. On a Media or Style port it
carries the picture it shows, captured when the run starts
(`capture_web_page`: the newest frame, else a poster capture, written with the
one PNG writer). The image agent transforms that picture, for example by
removing road markings from a map. A text agent spawned from the page is wired
to Prompt instead. When the run starts, Slate reads the page's visible text
once (`read_web_text`, capped at 30,000 characters), and that text replaces
the locator in the text block's prompt. This follows the user-ratified
amendment of `portal-web-embed` D15 and D27. Cookies and storage stay out of
reach. Chat cards still receive the locator as words. A local model accepts up
to about 64 KB in a one-shot text block and names a remote model when the
text is longer.

### Media is the interface (24 September 2026)

The user ratified that an agent makes media, not a card type of its own:

- **Image** spawns an ordinary picture (`NodeKind::Image`) with the agent
  attached (`ImageNode::agent`). Its linked file is the result a person
  picked; until then `item` is `ItemId::NONE` and the picture shows its newest
  result (`agent_inputs::newest_image`, one rule for the board, the wire
  snapshot and the artifact). The picture keeps every image tool: crop,
  corners, stroke and adjustments. Squares under it are every result; a click
  picks one, which is one journaled patch. Turning Live on lets the pick go,
  so a live picture always shows its newest frame, on the board and in export.
- **Text** spawns an ordinary sticky (`NodeKind::Text`) with the agent
  attached (`TextNode::agent`). While its own words are empty it shows the
  agent's finished reply. That reply is derived, not journaled. Editing it
  bakes the words into the note (Art. VI.3); closing the editor unchanged
  keeps them the agent's. Running again clears the person's words as one
  undo, then shows the new reply.
- The prompt stays inside the card, docked along the bottom as a capsule that
  expands on hover. Model, count, aspect, seed and Live moved to the Agent
  squircle, which edits the attached agent. The lower-right model chip is
  gone.
- A wire released on empty board from any media that shows the Agent
  squircle opens the modality menu. `MODALITIES` in `board_flow.rs` is the
  one list the menu and the editor's mode switch read, so 3D and video become
  one entry each when an engine makes them.
- Saved generator and text-block portals open as these media
  (`Scene::migrate_agent_cards`, the same per-node rule `bind_program` uses),
  keeping their id, rect, binding and wires. Chat cards stay portals.
- **Class and authority (Art. V.3).** A picture or note an agent makes is
  media, not a host portal. Its binding is journaled on the node. Its results
  stay in the linked session folder, owned by the session. What it shows is
  picked or edited by the person. Run completion never patches the scene
  (Art. VII.6). It exports as the media it shows (D24).
