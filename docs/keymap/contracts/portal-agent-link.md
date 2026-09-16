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
Ollama and ComfyUI protocol adapters remain later work. The generic image link is
available to explicitly configured sidecars today.

The user approved this refinement with “ok go for it!” after reviewing the
[design report](../../agent/portal-evolution-plan.md) and concept images.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Palette: "agent portal" + Enter; Portals dock flyout; command `board.portal.agent`; board view only. | stated | 100 |
| D02 | Stickiness & repeat | One-shot: placement returns to Select; Space/Enter may repeat via the registry history. | pattern | 80 |
| D03 | Gesture grammar | P2.DragShape. Place the frame, then choose a program in its icon grid. The program choice configures that same portal. | stated | 100 |
| D04 | Click vs drag rule | Travel > 4 px defines the frame; below it places a 960x540 default portal centered on the click. | precedent | 85 |
| D05 | Modifiers | Shift locks 16:9 during drag, matching Repository Lens portal placement. Ctrl/Alt unassigned in v1. | precedent | 80 |
| D06 | Constraints & snapping | Grid snap and smart guides apply to the frame rect; agent contents never snap. | pattern | 80 |
| D07 | Direction / value locks | Default source output is on the right; destination input is on the left. Wire direction follows semantic A/B binding and survives moving nodes. | stated | 100 |
| D08 | Numeric / manual entry | n/a in v1; dimensions are edited by resizing the frame. | guess | 55 |
| D09 | Preview & readouts | Quiet at rest. Chooser contains only icons and names. Chat identity and actions reveal on hover or contents focus. Image portals are full bleed without labels; hover/focus reveals Generate and the shared Maximize control. Wire affordance is a subtle animated blister behind the edge; resting wires have no port rings. | stated | 100 |
| D10 | Cursor | P1.portal.contents-focus. Hover gives feedback without taking the board wheel. Contents focus enables chat input and album gestures. | stated | 100 |
| D11 | Commit | One journaled `Add` of `PortalNode { class: Host, kind: Agent, agent: Some(..) }`. Session/status/turns are derived. | stated | 100 |
| D12 | Cancel | Esc peels one layer per press (P0.1): maximize → contents focus → drag draft → armed tool → selection. Releasing contents focus leaves the shelf / poster intact. Agent sidecar work is out-of-process and is not cancelled by board Esc. | pattern | 80 |
| D13 | Selected presentation | Windows-style hover resize on the frame (P1.node.transform) — no prior selection. Portals do not rotate. Agent transcript/proposals are inspector controls, not canvas grips. | stated | 100 |
| D14 | Post-edit | Choose program, source folder, context scope, prompt, Open in program, Stop and Unbundle are portal-local controls. Program changes journal a fresh session. | stated | 100 |
| D15 | Non-goals | Minimal sidecars for real programs. No embedded IDE, dashboard, or claim to attach to the desktop task. Ollama and ComfyUI adapters are future work. | stated | 100 |
| D16 | Create-style inheritance | No. Host portals use portal styling and do not consume shape/text style state. | pattern | 80 |
| D17 | Hit-testing & pick | The frame picks on its rect. Turn text and proposal badges painted inside the portal are not independently selectable. | pattern | 75 |
| D18 | Portal class & authority | **Host.** The foreign local agent owns its own runtime; Slate owns no mutations inside the portal. Agent board edits stage for acceptance. | stated | 100 |
| D19 | Source binding | SourceUri remains the relative-first project locator. AgentPortalRef stores provider, independent session, optional bundle locator and optional immutable seed image. Unknown provider ids survive load. | stated | 100 |
| D20 | Query & parameters | Journaled binding: provider, session, scope, opaque channel, view kind and seed output provenance. Existing ConnectorNode A/B endpoints carry typed Text or Images semantics, optional output pin, All-images mode and stable ordering tokens. No parallel graph. | stated | 100 |
| D21 | Regeneration & staleness | Context updates are throttled to one second and snapshot inputs at Send/Generate. Editing connected text changes the next run input; it does not execute a program. Missing upstream results prevent a run with an explicit message. | stated | 100 |
| D22 | Contents interaction | P1.portal.contents-focus. Double-click/Enter enables composer or image album. Send/Ctrl+Enter captures context and runs once. Multiple image references retain wire order. Album motion uses the shared home owner and settles to full bleed. Esc leaves focus; explicit Stop interrupts the portal Codex turn. | stated | 100 |
| D23 | Level of detail | P0.9. Icons, type and controls scale with the canvas. Small type is omitted. Image previews are lazy, budgeted and use the shared preview pool; only nearby album entries request textures. | stated | 100 |
| D24 | Export serialization | HTML exports completed images as a full-bleed scroll-snap bundle, active image first. Chat remains an honest host poster. No execution controls are exported. | stated | 100 |
| D25 | Bake | Unbundle creates a generator portal for every completed image in one undo group. The active image retains the original NodeId; children retain settings, prompt provenance and incoming wires, with fresh independent session ids. Redo reuses those identities; image files and histories are retained. | stated | 100 |
| D26 | Collaboration & per-peer | Frame and binding sync as document data. Runtime history, hover and album focus remain local view state. Linked assets are references. | stated | 100 |
| D27 | Agent surface | Adapters consume immutable snapshots. Proposed board edits continue through staged SceneCmd acceptance; the new Codex thread enforces a read-only filesystem sandbox. | stated | 100 |
| D28 | Determinism & provenance | Request ids combine process, clock and monotonic sequence. Sidecars persist submitted request identities before execution; restart never silently replays a paid request. Image ids and provenance remain stable through unbundle. | stated | 100 |
| D29 | Performance envelope | No protocol or file polling on the frame loop. Bounded worker queues, one-second context/read cadence, bounded preview requests, shared texture LRU. Program discovery runs in the background. | stated | 100 |
| D30 | Failure & honesty states | Application presence is separate from agent readiness. No session means Not connected; a historical idle session means Ready, never proof that the application is live. Failure names the cause and setup action. Codex uses its own installed sign-in; Cursor retains key setup. | stated | 100 |
| D31 | View-state ownership | Journaled: binding, connector semantics, split command group and immutable child seeds. Derived: draft prompt, completed session outputs, hover, album focus and running status. | stated | 100 |
| D32 | Trust, sandbox & consent | Codex adapter is a separate renderer-free leaf crate; installed Codex owns ChatGPT authentication. Portal-owned conversations start/resume read-only with approvals disabled; unsupported tool approval requests are rejected. Existing Cursor sidecar policy remains separate. | stated | 100 |
| D33 | Portal chrome | No identity tab. Shared Maximize and portal actions reveal on hover/focus. Image pixels fill the node at rest. Program grid has neither header nor footer. | stated | 100 |
| D34 | Portal maximize | **P1.portal.maximize.** Fills the window at the screen aspect; Esc restores. | stated | 100 |
| D35 | Portal-local UI | P1.portal.local-ui. Provider, scope, sidecar setup and bundle commands stay on the portal/inspector. The board-wide panel gains no provider-specific settings. | stated | 100 |

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
