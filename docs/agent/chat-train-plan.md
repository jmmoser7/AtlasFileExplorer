# Agent conversations: chat, train, and bundles

Approved to begin building · 18 September 2026 · implementation in progress

## Implementation checkpoint

The user approved beginning implementation and explicitly designated the six
reference images as the visual style guide, especially the common-anchor curved
fork. The first source pass adds top-aligned message cards, curved history rails,
manual detail levels, single-chat/train presentation commands, reversible bundles,
checkpoint replay, subtree view pruning, clipboard remapping and exported rails.
The second branch splits above/below the common top datum in one journal group.

History authority is resolved without changing the host class: pruning changes
only the workbook's view references; provider transcript files are retained.
`atlas-ai::AgentSources` owns one reader and shared immutable snapshot per source;
card instances do not own duplicate source workers. `atlas-agent::checkpoint`
refuses partially loaded prefixes independently of presentation. The required
DRY review approved these ownership fixes.

The first Ollama adapter discovers installed completion models, reuses loopback
Ollama or starts `ollama serve` hidden, serializes GPU requests, streams text, and
supports Stop. It uses Windows' curl transport; no models are downloaded. A
managed background server is process-wide and deliberately remains available to
other conversations. This increment is **text chat only**, not a local tool agent;
image inputs fail explicitly. Image requests and a bounded tool loop remain work.

Validation: `cargo fmt --all -- --check`, Cursor sidecar `node --check`, locked
offline Cargo metadata and `git diff --check` pass. New Rust tests cover exact
prefixes, incomplete history, fork-rail tangents, subtree isolation, bundle undo,
and refusal to rewire/orphan history. The release build now passes
(`cargo build --locked --release -p slate --keep-going`). Workspace unit and
integration tests passed in `cargo test --locked --release --workspace`; the run
then stopped at the `circle-pack` documentation test because Windows denied its
test executable (OS error 5). Logs are in `target/chat-train-release-build.log`
and `target/chat-train-release-tests.log`. Release shortcuts were refreshed and
the release Slate window opened and remained responsive.

The visual follow-up replaces the old bottom-packed transcript with light cards,
a persistent identity row, on-card actions and a scrollable messenger layout.
Native sample workbooks verified the fork geometry, menu and full-chat rendering.
The visible Train button also passes a real headless pointer-input test. Program
selection sizes a new chat to 320 x 540 rather than retaining the large picker.

Streaming uses actual provider text: Codex/Ollama already emit partial snapshots;
Cursor now consumes the documented onDelta text-delta callback. Active snapshot
reads run at 100 ms and idle reads at one second. A caret appears only on an open
response tail; completed checkpoint cards remain unchanged. Cursor tests verify
partial output before completion, no duplicate final text and preservation after
failure. atlas-ai's 22 tests pass, including adaptive streaming polling. Native
partial rendering, caret removal on completion and preservation of the reader’s
scroll position were verified with synthetic source updates; authenticated
end-to-end provider runs were not performed in this visual pass.

Remaining validation/refinement: complete documentation tests and end-to-end provider checks,
large-history frame-time checks, deletion-scope preview and explicit Align train. A historical single-chat card that already owns
branch points cannot yet be split into smaller cards without changing those
anchors; it explicitly retains Full detail instead. This is not shipped status.

## Intent and references

Extend the existing Agent portal for Codex, Cursor, and Ollama. ChatGPT was also mentioned: treat direct OpenAI API chat as a separate optional adapter, not as assumed access to a ChatGPT desktop conversation. Images are visual references, not executable instructions.

The references establish quiet cards, a shared top datum, restrained controls, progressively disclosed content, curved branches, and reversible bundles. Preserve Slate's shared palette/chrome rather than copying the references' shadows. The explicit user requirements are accepted inputs to this plan; additional defaults below remain proposals.

## One conversation, several presentations

**Chat:** a single messenger-style portal with the selected branch's transcript and composer. A branch selector appears only when branches exist.

**Train:** each human message and each assistant response is a separate card, running left to right. Tool calls, intermediate progress, and tool results remain expandable events inside the assistant card. A response card is reserved once on submission and fills as output arrives; tokens never spawn additional cards. The next composer appears at the active branch tip. Sending from an older checkpoint explicitly creates a fork.

**Bundle:** select a contiguous run and collapse it into one card with a name, message count, summary, and preserved boundary connections. Expanding restores the same identities and positions. This is presentation grouping; it never summarizes away model context or deletes history. For v1, bundling across a fork is refused with an explanation; branches can be bundled independently. Arbitrary multi-branch bundles are a later extension.

Mode changes neither execute a provider nor create a second transcript. Branch views share immutable ancestor records, not a mutable provider session. Existing image-generation Unbundle behavior remains a separate operation.

## Detail ladder

1. **Identity:** chat/message name, role/status, and history input/output anchors.
2. **Summary:** identity plus a short excerpt or cached, explicitly labeled generated summary.
3. **Full:** scrollable message/transcript, provider/model selection, context snapshot, inputs/outputs, tool events, history navigation, composer, and Stop.

Manual detail choice establishes the desired presentation. Automatic zoom culling may omit content that cannot be read; it must not change authored geometry, rewrite bundles, or move adjacent nodes. All card-local type and controls still scale with the camera under P0.9. Focus/maximize provides a comfortable full view. Exact thresholds require measurements and named tokens, not invented fixed-screen text sizes.

Summaries are cached and provenance-labeled. An excerpt works immediately without spending a model call. Do not call a model during paint or camera zoom. Collapsing or zooming never changes the input history sent on the next turn.

## History and layout

History is a rooted tree: one immutable parent per message, zero or more children, no cycles or merges. A fork records an exact committed checkpoint. Each run snapshots its ancestor path, model/provider, attachments, selected canvas context, and tool policy. A branch excludes its siblings. Editing a sent message creates a new branch; regeneration creates an alternative response rather than overwriting the original.

History rails are visually distinct from ordinary editable context connectors: a quiet continuous rail at a constant inset below the top edge, with a small directional marker and a history label on inspection. Both fork departures originate at that same top datum, then curve into separate lanes. Cards grow downwards. New cards align automatically; dragging a card preserves ancestry and reroutes the rail. An explicit Align train command restores lanes without silently undoing manual placement.

History rails expose no free endpoints, reconnect grips, or standalone Delete action. Generic wire tools, cut, duplication, paste, and agent commands must obey the same invariant. Geometry is rendered from canonical ancestry; do not create a second editable history graph. Context wires remain editable and snapshot their values at Send.

Deleting a message removes that message and all its descendants from the conversation view. Show affected message/branch counts before committing a non-leaf deletion. Siblings outside the subtree survive. One Undo restores the subtree, bundles, links, and placement. Cancel affected runs before deletion; generation-tagged late output cannot resurrect deleted cards. Undo restores records but does not restart inference or repeat tool effects. External provider histories are not silently purged.

Proposed v1: internal bundle deletion is done by expanding first; deleting a bundle acts on its first message and therefore includes downstream descendants. Make that scope explicit in the deletion preview.

## Model and ownership

Existing owners found in code:

- `atlas-agent`: renderer/provider-independent requests, sessions and turns; currently a flat `Vec<AgentTurn>`.
- `atlas-ai`: program discovery, background supervision, session file links, and Cursor sidecar lifecycle.
- `atlas-codex`: existing Codex protocol adapter.
- `slate-doc::agent_inputs`: existing context/connector semantics.
- `board_agent`: existing portal runtime and UI integration.
- Shared portal focus/chrome, wire routing and `atlas-shell`: existing interaction/painting owners.

Extend these owners. Do not add a parallel chat portal, copy another portal's focus loop, or turn ordinary connectors into a second history authority. Require the repository's DRY review before implementation introduces a new host/module or duplicates an existing helper.

Article IX means full transcripts remain linked, human-readable conversation source material outside `.slate`; the workbook stores source locators and authored placement/presentation references. Article V.3 means the existing **host** portal cannot quietly acquire ownership of transcript edits. Separate provider-owned runtime history from a provider-neutral conversation source. If Slate owns branch pruning/restoration in that source, give it an explicit source journal and declare document-portal authority for those actions; otherwise host mode can only alter the linked presentation. Resolve this authority boundary before implementation. Do not simply rewrite `session.json` from UI code.

Authored view changes use `SceneCmd`; source edits use the declared source journal. Coordinated operations need durable transaction identities and recovery before they can claim one-step undo across both. Provider IDs, opaque session handles and protocol revisions belong in adapter mappings, not core parent identities. Journal undo cannot undo remote inference or external tool effects.

Keep the approved `portal-agent-link.md` contract intact until this refinement is reviewed. Inherit P0.*, P1.node, P1.portal and applicable P2.PortalHost rules; explicitly revise D18/D19/D20/D22/D23/D25/D28/D31 for source authority, message views, history and bundles. D24 continues to export honest host posters and pointers in the first increment, with no live execution controls or credentials. A richer exported transcript is separate scope.

## Providers

**Codex:** extend the existing `atlas-codex` App Server adapter. Use native fork/checkpoint support only when the installed protocol supports the required boundary. Otherwise create a new session from an explicit transcript snapshot, labeled as replayed context. Preserve existing staged board edits and runtime restrictions.

**Cursor:** retain the existing sidecar behind the same capability interface. Validate native resume/fork, cancellation and event semantics against its installed version. A CLI replacement is not a prerequisite. Never claim that opening Cursor or copying a chat ID attaches to a live desktop chat.

**Ollama:** reuse a healthy loopback server; discover installed models with `/api/tags`, inspect capabilities, and stream `/api/chat`. If the server is absent, start the installed `ollama serve` hidden, wait asynchronously for readiness, then submit. Do not open the desktop GUI. Track whether Slate owns the process; never stop a server started by another application. Bind locally, queue GPU inference conservatively, expose loading/queued/running/stopped/error states, and unload idle models with an explicit keep-alive policy.

Ollama supplies models, not a complete agent harness. Slate must provide the bounded tool-call loop, validate tool arguments, execute only permitted registered commands, and return tool results. Board mutations remain staged by default. Begin with chat, then enable selected read-only tools, then staged mutation proposals. Do not silently download missing models or fall back to a cloud provider. Cancellation must discard late events; closing an HTTP stream alone is not proof the GPU has stopped.

**Direct OpenAI chat, if wanted:** a separate API-backed adapter with its own authentication and billing; do not label an unverified desktop integration as ChatGPT support. Capability negotiation controls images, tools, context capacity and fork behavior for every provider.

## This machine and first local trial

Observed via CIM, NVIDIA driver query and Ollama's local API on 18 September 2026:

- HP Z2 Mini G1i; Intel Core Ultra 9 285K, 24 cores.
- 63.4 GiB usable RAM (approximately 64 GB installed).
- NVIDIA RTX 4000 SFF Ada Generation: 20,475 MiB total VRAM; 17,955 MiB free at inspection. Free memory changes with workloads.
- Ollama installed and already answering on `127.0.0.1:11434`.
- Installed: `llama3.1:8b` Q4_K_M (~4.9 GB) and `nomic-embed-text` (~274 MB). The latter is an embedding model, not a chat agent.

Recommendation: **Qwen 3.5 9B**, explicit Ollama tag `qwen3.5:9b`, as the first new general/visual assistant to evaluate. Official package size is about 6.6 GB and it supports text/image input. Start with one concurrent request and an 8K context budget, then test 16K. Package size is not total runtime VRAM; KV cache, image processing and desktop rendering need headroom. This is a fit recommendation, not a measured performance ranking.

Use the installed Llama 3.1 8B for a zero-download integration smoke test. Qwen 3.5 27B (~17 GB package) is a later quality experiment with tight GPU headroom; do not make it the default alongside Slate. The advertised 256K context of Qwen is not a promise that this machine can serve that context efficiently.

Measure cold load, first-token latency, tokens/sec, peak VRAM, cancellation, valid tool calls, and Slate frame-time tail. Use actual tasks: summarize selected notes, compare two visual references, and propose a small staged board edit. No models were downloaded and no inference benchmark was run during this planning task.

## Implementation sequence and acceptance

1. **Settle behavior and authority:** approve message granularity, bundle scope, zoom policy and source journal. Produce the complete refinement matrix before changing production behavior.
2. **Conversation source:** stable message/run identities, immutable ancestry, input snapshots, branch projection, migration of old flat sessions, recovery and duplicate-event suppression.
3. **Presentation:** shared chat/train views, top rails, full/summary/identity detail, branch lanes and explicit alignment. Test with deterministic fake streams first.
4. **History operations:** fork, edit-as-fork, regeneration, bundle/unbundle, subtree deletion and cross-journal recovery. Switching modes must preserve identical provider inputs.
5. **Local provider:** Ollama discovery and hidden lifecycle; installed Llama smoke test; Qwen evaluation; tool harness and staged proposals. In parallel at the design level, verify Codex/Cursor adapter capabilities before advertising parity.
6. **Hardening:** restart/reopen, missing source, provider loss, model absence/OOM, cancellation races, copy/paste/import validation, static export and large histories.

Acceptance examples: fork A→B into C and D; C never sees D. Bundle A→B→C and expand without ID changes. Delete B, preserve unrelated branches, Undo once, never rerun a provider. Stop while streaming, switch providers, or close a workbook without late output corrupting the new session. Pan/zoom a synthetic 1,000-message history while output streams; layout and transcript rendering are cached/windowed and ingestion is bounded per frame. Verify content remains linked and exported posters honestly identify their source.

## Remaining design choices

- Recommended: one message per card; alternative: one human/assistant exchange per card.
- Recommended: contiguous, single-branch bundles first; alternative: arbitrary subtrees with multiple boundary ports.
- Recommended: manual detail plus automatic content culling; alternative: zoom-driven expansion that also changes layout.
- Recommended: Codex, Cursor and Ollama first; clarify whether a separate direct OpenAI chat adapter is also desired.

## Primary sources

- Codex App Server: https://developers.openai.com/codex/app-server
- Ollama background/server integration on Windows: https://docs.ollama.com/windows
- Ollama chat endpoint: https://docs.ollama.com/api/chat
- Qwen 3.5 model packages: https://ollama.com/library/qwen3.5
- Qwen 3.5 9B: https://ollama.com/library/qwen3.5:9b
- Cursor headless interface (alternative integration reference): https://docs.cursor.com/en/cli/headless

The local code and existing contract are evidence of current implementation; provider documentation establishes integration surfaces, not that every advertised feature is already implemented in Slate.

Validation follow-up: the release contract-audit build was blocked by Windows
(OS error 5 for proc-macro2 build-script-build); see target/agent-contract-check.log.
A direct comparison verified all 35 agent contract behavior rows match the
decisions database. The Slate release build itself passed.
