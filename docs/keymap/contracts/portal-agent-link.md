# Agent portal — interaction contract

Status: **draft**
Family: portal
Portal class: **host** (Art. V.3) · Type: **agent** · Subtype: **local link**
Command: `board.portal.agent` (placement) · Key: none in v1 · Palette:
"agent portal" (aliases: cursor portal, local agent)
Inherits: P0.* (all, including P0.9), P1.node, P2.DragShape — deviations flagged below.

## What it is, and the 10% it implements

A journaled frame on the board whose live contents are a local agent session.
The v1 provider is Cursor, reached through a user-run Cursor SDK sidecar and
plain JSON files under the AI workspace. The generic part is the file contract:
other local agents can read the same context/request files and write the same
session/proposal files without Slate embedding their runtime.

The 90% deliberately not implemented: embedded webviews, direct `.slate` file
edits, direct journal writes, autonomous acceptance, network services, script
execution, MCP transport, and a canvas overlay for staged geometry.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Palette: "agent portal" + Enter; Portals dock flyout; command `board.portal.agent`; board view only. | stated | 100 |
| D02 | Stickiness & repeat | One-shot: placement returns to Select; Space/Enter may repeat via the registry history. | pattern | 80 |
| D03 | Gesture grammar | `Armed -> Dragging(rect) -> Commit(host portal)`. Binding is the in-portal Select folder / Cover Flow, not the draw gesture. | pattern | 80 |
| D04 | Click vs drag rule | Travel > 4 px defines the frame; below it places a 960x540 default portal centered on the click. | precedent | 85 |
| D05 | Modifiers | Shift locks 16:9 during drag, matching Repository Lens portal placement. Ctrl/Alt unassigned in v1. | precedent | 80 |
| D06 | Constraints & snapping | Grid snap and smart guides apply to the frame rect; agent contents never snap. | pattern | 80 |
| D07 | Direction / value locks | n/a: no directional parameter. | pattern | 85 |
| D08 | Numeric / manual entry | n/a in v1; dimensions are edited by resizing the frame. | guess | 55 |
| D09 | Preview & readouts | Drag preview shows the frame. Unbound: Cover Flow whose title-faces set in **Courier New** (`AGENT_TITLE_FAMILY`; P0.9). Bound: a Cursor-like composer (header + status chip + transcript + input). After Send the chip and a transcript row animate **Thinking** / **Responding** (pulsing dots, P0.9). The idle chip is green when the Cursor **IDE** is running — that is the visual "Cursor is live" signal, independent of sidecar `session.json`. Transcript shows file-link turns and optimistic local sends. All of it follows P0.9. | stated | 100 |
| D10 | Cursor | Crosshair while armed; normal board cursor after placement. Unbound Cover Flow is interactive only in contents-focus or maximize (**P1.portal.contents-focus**). Bound poster is not independently selectable. | pattern | 80 |
| D11 | Commit | One journaled `Add` of `PortalNode { class: Host, kind: Agent, agent: Some(..) }`. Session/status/turns are derived. | stated | 100 |
| D12 | Cancel | Esc peels one layer per press (P0.1): maximize → contents focus → drag draft → armed tool → selection. Releasing contents focus leaves the shelf / poster intact. Agent sidecar work is out-of-process and is not cancelled by board Esc. | pattern | 80 |
| D13 | Selected presentation | Windows-style hover resize on the frame (P1.node.transform) — no prior selection. Portals do not rotate. Agent transcript/proposals are inspector controls, not canvas grips. | stated | 100 |
| D14 | Post-edit | Provider, context scope, prompt sending, link reveal, and proposal accept/reject live in the selected portal inspector. | stated | 100 |
| D15 | Non-goals | No embedded Cursor window, no live IDE-thread attach, no deriving the portal from Cursor's own wrap (Art. I.2 / VII.8). The composer is a Slate recreation of the 10% (type, send, transcript, live chip). Launching the IDE and spawning the out-of-process sidecar are in. | stated | 100 |
| D16 | Create-style inheritance | No. Host portals use portal styling and do not consume shape/text style state. | pattern | 80 |
| D17 | Hit-testing & pick | The frame picks on its rect. Turn text and proposal badges painted inside the portal are not independently selectable. | pattern | 75 |
| D18 | Portal class & authority | **Host.** The foreign local agent owns its own runtime; Slate owns no mutations inside the portal. Agent board edits stage for acceptance. | stated | 100 |
| D19 | Source binding | `PortalNode.source` is the project folder (`SourceUri`, relative-first, same locator as repo lens). Rebinding is a journaled `Patch`. `agent.provider` is a provider id (`cursor` today, `local` generic fallback); `agent.session` keys the AI-workspace folder. No vendor type appears in the scene model. Opening a saved workbook restores the folder. | stated | 100 |
| D20 | Query & parameters | Journaled knobs: provider id, session id, context scope (`selection`, `frame`, `board`), optional opaque `channel` (saved chat id), frame rect/title/fill. Prompt text, turns, IDE process status, and the chat catalog are derived UI state. | stated | 100 |
| D21 | Regeneration & staleness | Slate writes context at most once per second, fingerprint-gated. It reads `session.json` and stage files at most once per second, mtime-gated. Missing session paints offline, not error. | pattern | 85 |
| D22 | Contents interaction | **P1.portal.contents-focus**. Click selects; the board keeps the wheel. Double-click / Enter focuses the composer (type + Send / Ctrl+Enter). Esc releases. Unbound + focused: Cover Flow. Bound: the composer is always painted; it only accepts input when focused. Send writes `request.json` and may spawn the Cursor sidecar (`docs/agent/cursor-sidecar`) against the bound folder. Right-click: Open in Cursor, Switch chat, Enter/Leave contents. | stated | 100 |
| D23 | Level of detail | Poster type and the empty-state shelf scale with zoom (P0.9). Type below the legibility floor is dropped, not clamped. Turns stay capped to the last few. | stated | 100 |
| D24 | Export serialization | Artifact writer emits a host-poster caption naming provider/session and states live agent state is not exported. | stated | 100 |
| D25 | Bake | n/a in v1. Agent output becomes authored content only by accepted staged `SceneCmd`s. | stated | 100 |
| D26 | Collaboration & per-peer | Frame/provider/session/context sync as document data. Session files, prompts, turns, and pending proposals are local workspace state, never journaled. | pattern | 85 |
| D27 | Agent surface | Agent may read context/request and write session/proposal files. It may never edit the workbook or journal directly; Slate accepts proposals as attributed commands. | stated | 100 |
| D28 | Determinism & provenance | Host portal contents are not deterministic. Provenance is the provider id, session id, request ids, proposal author, and stage result. | pattern | 80 |
| D29 | Performance envelope | All file I/O is throttled and mtime/fingerprint gated. No agent work runs on the UI thread. Portal paint is bounded to a few text rows. | pattern | 85 |
| D30 | Failure & honesty states | Unbound; Cursor not found; Cursor not running; Cursor running; file-link missing / idle / thinking / responding / error; no saved chats; pending / stale proposal. **A failed send is never a blank transcript and never a dead end.** The chip reads **Unreachable**. The transcript names the cause and offers a next step: **Get a key** opens `https://cursor.com/dashboard/api`, **Paste key** stores it on this machine and retries, **Setup steps** opens `docs/agent/cursor-sidecar/SETUP.md`, **Choose workspace** / **Download Node.js** when those are the block. **IDE status and sidecar status are separate lines.** A missing `session.json` is "no sidecar session", never "Cursor is offline" while the IDE is running. | stated | 100 |
| D31 | View-state ownership | Journaled: frame and `AgentPortalRef`. Derived: prompt draft, transcript, request state, staged-proposal list, sidecar status. | pattern | 90 |
| D32 | Trust, sandbox & consent | Slate may launch the Cursor **IDE** the human already installed, on a bound folder. It still does not embed a Cursor runtime or attach to a live chat thread (D15, Art. VII.8). The file-link sidecar remains user-run and is reached only through JSON under the AI workspace. Staged proposals are the acceptance gate (D27, Art. VII.6). | pattern | 85 |
| D33 | Portal chrome | **No identity tab** (web-only). Maximize is the four-corner square in the upper-right. Right-click: Maximize, Open in Cursor, Switch chat, Enter/Leave contents. | stated | 100 |
| D34 | Portal maximize | **P1.portal.maximize.** Fills the window at the screen aspect; Esc restores. | stated | 100 |
| D35 | Portal-local UI | **P1.portal.local-ui.** Provider, scope, and sidecar controls live on this portal's inspector. None appear in Document Settings. | stated | 100 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `agent.link.poll_secs` | Minimum read/write poll interval | `1.0` |
| `agent.portal.default_size` | Click placement size | `960 x 540` |
| `agent.portal.turns_painted` | Recent turns painted in poster | `3` |
| `agent.await.sent_timeout_secs` | Named failure if Send never reaches a live sidecar | `20` |

## Golden paths

1. **GP1 — place:** Portals flyout -> Agent portal -> click board -> host portal
   appears selected with no identity tab and a painted (non-interactive) Cover
   Flow of recent Cursor / Slate agent projects. Wheel still zooms the board.
   Double-click (or Enter) takes contents focus; then the shelf and Select
   folder pill respond.
2. **GP2 — prompt:** Select portal -> type prompt -> Send -> `request.json`
   appears under the session folder. The composer shows animated Thinking
   immediately. A reply updates to Responding, then the assistant turn. If
   the sidecar cannot be reached, the chip reads Unreachable and the
   transcript names the failure — never a blank.
3. **GP3 — transcript:** Sidecar writes `session.json` -> portal poster updates
   status/turns within two poll intervals.
4. **GP4 — staged edit:** Sidecar writes `stage/<id>.json` -> inspector shows
   proposal -> Accept commits one `CmdAuthor::Agent` journal group.
5. **GP5 — reject:** Reject writes `<id>.result.json` and leaves the scene
   untouched.

## Open questions

Draft status: user confirmation still required for the full matrix before this
contract can become `agreed`.
