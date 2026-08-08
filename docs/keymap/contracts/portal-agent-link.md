# Agent portal — interaction contract

Status: **draft**
Family: portal
Portal class: **host** (Art. V.3) · Type: **agent** · Subtype: **local link**
Command: `board.portal.agent` (placement) · Key: none in v1 · Palette:
"agent portal" (aliases: cursor portal, local agent)
Inherits: P0.* (all), P1.node, P2.DragShape — deviations flagged below.

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
| D03 | Gesture grammar | `Armed -> Dragging(rect) -> Commit(host portal)`. Binding and prompting happen later in the inspector. | pattern | 80 |
| D04 | Click vs drag rule | Travel > 4 px defines the frame; below it places a 960x540 default portal centered on the click. | precedent | 85 |
| D05 | Modifiers | Shift locks 16:9 during drag, matching Repository Lens portal placement. Ctrl/Alt unassigned in v1. | precedent | 80 |
| D06 | Constraints & snapping | Grid snap and smart guides apply to the frame rect; agent contents never snap. | pattern | 80 |
| D07 | Direction / value locks | n/a: no directional parameter. | pattern | 85 |
| D08 | Numeric / manual entry | n/a in v1; dimensions are edited by resizing the frame. | guess | 55 |
| D09 | Preview & readouts | Drag preview shows the frame. Placed portal poster shows provider, session, status, recent turns, and proposal count. | stated | 100 |
| D10 | Cursor | Crosshair while armed; normal board cursor after placement. Contents are not directly interactive in v1. | pattern | 75 |
| D11 | Commit | One journaled `Add` of `PortalNode { class: Host, kind: Agent, agent: Some(..) }`. Session/status/turns are derived. | stated | 100 |
| D12 | Cancel | Esc peels drag draft, then armed tool, then selection, following P0.1. Agent work is out-of-process and not cancelled by board Esc in v1. | pattern | 75 |
| D13 | Selected presentation | The frame uses normal board selection handles. Agent transcript/proposals are inspector controls, not canvas grips. | pattern | 80 |
| D14 | Post-edit | Provider, context scope, prompt sending, link reveal, and proposal accept/reject live in the selected portal inspector. | stated | 100 |
| D15 | Non-goals | No embedded Cursor window, no spawned process, no direct document edits, no autonomy grant, no scripts, no MCP server in v1. | stated | 100 |
| D16 | Create-style inheritance | No. Host portals use portal styling and do not consume shape/text style state. | pattern | 80 |
| D17 | Hit-testing & pick | The frame picks on its rect. Turn text and proposal badges painted inside the portal are not independently selectable. | pattern | 75 |
| D18 | Portal class & authority | **Host.** The foreign local agent owns its own runtime; Slate owns no mutations inside the portal. Agent board edits stage for acceptance. | stated | 100 |
| D19 | Source binding | `agent.provider` is a provider id (`cursor` today, `local` generic fallback); `agent.session` keys the AI-workspace folder. No vendor type appears in the scene model. | stated | 100 |
| D20 | Query & parameters | Journaled knobs: provider id, session id, context scope (`selection`, `frame`, `board`), frame rect/title/fill. Prompt text and turns are derived UI state. | stated | 100 |
| D21 | Regeneration & staleness | Slate writes context at most once per second, fingerprint-gated. It reads `session.json` and stage files at most once per second, mtime-gated. Missing session paints offline, not error. | pattern | 85 |
| D22 | Contents interaction | The portal surface is a status poster. Prompt, provider, reveal, launch, and Accept/Reject controls are in the inspector. | stated | 100 |
| D23 | Level of detail | No zoom buckets in v1; poster text is capped to the last few turns and proposal count. | guess | 60 |
| D24 | Export serialization | Artifact writer emits a host-poster caption naming provider/session and states live agent state is not exported. | stated | 100 |
| D25 | Bake | n/a in v1. Agent output becomes authored content only by accepted staged `SceneCmd`s. | stated | 100 |
| D26 | Collaboration & per-peer | Frame/provider/session/context sync as document data. Session files, prompts, turns, and pending proposals are local workspace state, never journaled. | pattern | 85 |
| D27 | Agent surface | Agent may read context/request and write session/proposal files. It may never edit the workbook or journal directly; Slate accepts proposals as attributed commands. | stated | 100 |
| D28 | Determinism & provenance | Host portal contents are not deterministic. Provenance is the provider id, session id, request ids, proposal author, and stage result. | pattern | 80 |
| D29 | Performance envelope | All file I/O is throttled and mtime/fingerprint gated. No agent work runs on the UI thread. Portal paint is bounded to a few text rows. | pattern | 85 |
| D30 | Failure & honesty states | Unbound, offline, idle, thinking, error, pending proposal, stale proposal. Each names the state rather than blanking the frame. | stated | 100 |
| D31 | View-state ownership | Journaled: frame and `AgentPortalRef`. Derived: prompt draft, transcript, request state, staged-proposal list, sidecar status. | pattern | 90 |
| D32 | Trust, sandbox & consent | The agent runtime is out-of-process and **user-run**: Slate never spawns it (D15) and reaches it only through JSON files under the AI workspace the human established. Slate loads no page, script, or network endpoint on the portal's behalf, so there is no origin consent to grant — the trust boundary is the human's decision to run the sidecar, and staged proposals are the acceptance gate for everything it produces (D27, Art. VII.6). | pattern | 85 |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `agent.link.poll_secs` | Minimum read/write poll interval | `1.0` |
| `agent.portal.default_size` | Click placement size | `960 x 540` |
| `agent.portal.turns_painted` | Recent turns painted in poster | `3` |

## Golden paths

1. **GP1 — place:** Portals flyout -> Agent portal -> click board -> host portal
   appears selected with Cursor provider and a generated session id.
2. **GP2 — prompt:** Select portal -> type prompt -> Send -> `request.json`
   appears under the session folder.
3. **GP3 — transcript:** Sidecar writes `session.json` -> portal poster updates
   status/turns within two poll intervals.
4. **GP4 — staged edit:** Sidecar writes `stage/<id>.json` -> inspector shows
   proposal -> Accept commits one `CmdAuthor::Agent` journal group.
5. **GP5 — reject:** Reject writes `<id>.result.json` and leaves the scene
   untouched.

## Open questions

Draft status: user confirmation still required for the full matrix before this
contract can become `agreed`.
