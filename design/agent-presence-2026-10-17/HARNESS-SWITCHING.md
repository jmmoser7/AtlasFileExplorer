# Changing an agent's harness

Status: accepted implementation contract. Native source integration is in progress; tests and native UX checks must be recorded independently. The browser prototype simulates these controls. A harness is Codex or Cursor. Changing it keeps the same named agent and conversation on the board.

## Behavior

- Selecting the current harness does nothing. Validate the target and its availability before stopping work or changing any state.
- When idle, change the selected harness immediately. Show a quiet conversation marker: “Cursor will be used for the next message. Earlier messages remain visible.” Start no model call until the next message is sent.
- When working, offer **After this task**, **Stop task and switch**, and **Cancel**. Opening the menu or selecting a row must not silently stop work.
- **After this task** keeps the current harness running and shows the pending choice beside its name. Apply it only when this task has finished, including its reads, board actions, requested helpers, image jobs and proposal decisions. A reply ending while a proposal awaits acceptance is not completion. The pending choice can be canceled or replaced. Errors, explicit Stop and retirement cancel it; they do not silently apply it.
- **Stop task and switch** uses the existing cancellation owner, then changes the selection. Explain that unfinished helper work, pending proposals and this task's image jobs will stop. Keep completed changes and results. Do not resend the interrupted request automatically.
- Preserve the draft and visible conversation. Show canceled work as stopped, not completed. The next Send creates a fresh request under the selected harness.
- Pending changes are task-local. Do not restore them, a running task, authority or presence after restart.

## Identity, history and sessions

Keep three identities distinct:

1. **Agent profile:** durable name, appearance and selected harness.
2. **Conversation:** durable, visible messages with their authors, harness markers, cancellation/result status and artifact provenance. Opening the agent must show this history even before a new provider starts.
3. **Provider binding:** a Codex thread ID or Cursor agent ID, its provider-specific link directory and acknowledged history boundary. This is not board membership or permission.

Use separate directories and typed bindings for each provider session. Never give a Cursor ID to Codex or infer a Cursor ID from a Codex `session.json`.

A handoff starts a fresh provider session with an explicit checkpoint of the visible conversation. Existing `AgentRequest.history` is the replay mechanism. Pass messages as quoted history, not instructions; exclude public activity, hidden reasoning, pending tool calls, credentials and grants. Do not collect extra canvas or source context because the harness changed.

Returning to a previous harness must include the conversation that happened while it was away. The initial implementation should start another fresh session with the current checkpoint. Retain old provider bindings for provenance and inspection, not automatic stale resumption. A later resume optimization must track the acknowledged history boundary and supply missing visible turns without duplicating old ones.

## Cancellation and authority

Reuse `SlateApp::stop_presence`; do not create a second cancellation path. It stops the participant and its immediate helpers, cancels active/queued witnessed motion and reads, rejects pending presence proposals, revokes the old live request bridge and stops recorded image request IDs. Helpers currently remain present with their own harnesses; changing the parent must not silently migrate them. Already committed journal changes remain undoable and attributed.

Task ownership must include transitive helper descendants. The current immediate-helper stop is insufficient when a helper has spawned its own helpers; extend the existing lifecycle/session owner for that case. Fence old request authority before waiting for cancellation. Track owned operations reaching terminal states so a late reply or still-running image job cannot be mistaken for completed work. Recheck destination availability when applying a queued switch; if setup has become unavailable, retain the current selection and show the problem.

Keep image cancellation request-specific. A newer human request or unrelated queued work at the same generator must continue. Preserve completed images, artifact files and their originating session; changing a harness neither deletes nor copies them. Old sessions must not regain proposal acceptance or commit authority through late replies.

Board scope/autonomy and provider Full access are separate. A harness choice grants neither. Keep any continuing board grant attached to the same live agent, fence its old request and rebind only a new request. Provider Full access and relay permissions must be resolved for the destination binding; do not copy them from another harness. Retired/restored agents still receive fresh presence and explicit authority under Constitution VIII.5.

## Models and connection health

Use existing account health, authentication and model-catalog owners. Show Codex and Cursor with their existing provider glyphs. An unavailable option should explain the required setup before a switch changes work. Never fall back to a different harness or model silently.

Keep the model and supported parameter choices separately for each harness. Show **Default** when none is chosen; populate choices from that harness's actual asynchronous catalog. Capture the chosen model/parameters on Send. Credentials stay in the existing machine-local stores, never profile data or the workbook.

## Historical defects and current owners

The locations below describe the pre-integration findings and identify the existing execution owners. Source changes now implement validated no-op/switch/queue behavior, separate durable conversation storage, typed fresh bindings, explicit checkpoint replay and transitive cancellation. Old line numbers are historical; their correction still needs the applicable tests and native acceptance cases.

- `apps/slate/src/app/agent_presence/controls.rs:214`: `SetProvider` cancels before validating, resets even for the current provider, clears the snapshot and selects a new random directory. It neither transfers history nor retains a return binding.
- `apps/slate/src/app/agent_presence/submission.rs:50`: presence requests leave `history` and model choices at their defaults. Extend this request owner for explicit checkpoint replay and captured choices.
- `apps/slate/src/app/agent_presence/lifecycle.rs:189`: authoritative cancellation owner. `board_agent/results.rs:106` owns request-specific generator cancellation; `board_agent/agent_stage.rs:38` fences canceled proposals.
- `crates/atlas-agent/src/presence/session.rs:291`: cancels motion at its sampled position and drains pending plans; the avatar must not teleport during a handoff.
- `crates/atlas-agent/src/lib.rs:70`: portable request history. `crates/atlas-ai/src/agent.rs:130` owns asynchronous session snapshots; `runtime.rs:296` reloads provider history from the link directory.
- `crates/atlas-ai/src/runtime.rs:237` and `crates/atlas-codex/src/lib.rs:212`: Codex thread persistence/resume. `docs/agent/cursor-sidecar/index.mjs:88,119`: Cursor agent persistence/resume. The Cursor fallback to saved `conversation` makes sharing a directory between vendors unsafe.
- `crates/atlas-ai/src/runtime.rs:372` derives a grant lookup identity from the directory name; `sidecar.rs:62` receives the session identity explicitly. Resolve this mismatch when separating durable conversation bindings from ephemeral presence.
- `apps/slate/src/app/board_agent/models.rs:6`, `crates/atlas-ai/src/runtime.rs:92`, and `sidecar.rs:272` own asynchronous model catalogs. `atlas-ai::packs` owns capability health; `sidecar.rs:90` uses the existing Cursor key/setup. `atlas-codex::Client::probe` checks the installed Codex connection without a model call.

## Prototype and open native work

`prototype.js` currently implements browser-local Codex/Cursor presentation, selection, same-harness no-op, a visible history marker, a busy choice panel and pending-harness indicator. Its finite visible-work example calls `completeCurrentTask` at the end of the animation and applies the queued harness; Stop clears that pending choice. No provider is invoked and no actual task, proposal, helper or image request is canceled. Its **Default** row is presentation only. Source inspection confirms the simulated completion path; browser interaction validation belongs to the prototype review.

Native verification remains: durable conversation/binding storage, validated switch command and pending state, completion accounting, checkpoint replay, preserved artifacts, destination model/auth/grant resolution, and the shared shell menu presentation. Validate idle, working, awaiting approval, helper/image work, failed setup, switching back, restart and late replies using the existing owners. Review the actual open menu and conversation through computer use; a closed roster screenshot is not acceptance of these flows.
