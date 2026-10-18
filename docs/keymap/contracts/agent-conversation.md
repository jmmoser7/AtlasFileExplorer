# Agent conversation — interaction contract

Status: agreed
Family: tool
Reference: User feedback and four supplied chat/hat images, 17 October 2026
Command: Registered presence.* commands, including presence.chat, presence.address, presence.draft, presence.photo, presence.provider.switch and presence.own_chats
Inherits: P0.* (all), especially P0.1, P0.7 and P0.9. No new tool grammar or scene node.

This is a revision of collaborator presence interaction, not a drawing tool.
Visual authority and geometry: `design/agent-presence-2026-10-17/metrics.json`.
Revision 2 was accepted by the user on 17 October 2026, followed by an explicit instruction to build it into the final board. This is design agreement, not a shipped-build, native-interaction or user-acceptance verdict. The independent intent reviewer and the full native acceptance catalog remain required.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Participant icon opens that participant's persistent conversation. Name/caret opens their profile, aligned below and to the left of that segment. Avatar double-click opens its recent conversation below it. | stated | 100 |
| D02 | Stickiness & repeat | Addressing the canvas avatar pauses its board work. Closing the attached conversation leaves it paused; only explicit Resume continues. Reopening reuses the same conversation and draft. Opening roster chat alone does not silently resume a paused task. | stated | 100 |
| D03 | Gesture grammar | Working → avatar double-click → paused/listening → recent message bubbles and composer beneath avatar. No board edit commits after pause until explicit resume. | stated | 100 |
| D04 | Click vs drag rule | Single-click roster icon opens chat; single-click name/caret opens profile. Double-click board avatar pauses and listens. These hit regions do not also select an underlying canvas node. | stated | 100 |
| D05 | Modifiers | n/a; no new modifier chord. Existing canvas camera controls remain available outside chat editing. | pattern | 85 |
| D06 | Constraints & snapping | n/a; presence is not authored geometry and is not a snap target. | pattern | 85 |
| D07 | Direction / value locks | n/a; no geometric value lock. | pattern | 85 |
| D08 | Numeric / manual entry | n/a for geometry. The chat composer accepts text; profile name is directly editable and profile photo persists. | stated | 100 |
| D09 | Preview & readouts | Approved eye-only face and independent expression/gaze timings. Listening when addressed. Avatar, hat, prop, name and attached conversation scale in canvas space under P0.9, with world wrapping preserved. Roster conversation remains window chrome; no attached-overlay scale exception. | stated | 100 |
| D10 | Cursor | Existing pointer over canvas; text caret in editor. Agent animation never moves or captures the human pointer. | pattern | 90 |
| D11 | Commit | Messages append to a durable machine-local participant conversation. Provider sessions bind separately to that identity. Selecting the current harness is inert; busy handoff offers after-task or stop-and-switch without automatic resend. Self-chat stays local; real human delivery uses the bounded encrypted atlas-relay owner and recipient persistence before acknowledgment. Board edits use existing SceneCmd staging/journal and explicit grants. | pattern | 85 |
| D12 | Cancel | P0.1: Escape dismisses the innermost open picker/menu first, then conversation. Dismissal retains history and unsent draft and does not implicitly resume paused work. | pattern | 85 |
| D13 | Selected presentation | One continuous roster capsule with discreet vertical separators. One enclosing conversation panel includes the compact identity/control capsule, alternating bubbles and bottom composer. Hats scroll in a compact album flow owned by atlas-shell::home, with a fine selected outline and no All hats step. Cursor/Codex use canonical provider glyphs; retirement uses a gentle sunset glyph. | stated | 100 |
| D14 | Post-edit | Icon reopens the previous conversation. Human name/photo are persistent. Agent name and hat are persistent identity preferences. Retired agents can return through Advanced; old authority and task position never auto-restore. | stated | 100 |
| D15 | Non-goals | Never fabricate remote delivery, unsupported menu outcomes or board progress. Human messages require the real participant-message transport, separate from the Slate–Atlas bridge and from shared-board synchronization. Profiles stay separate from transcript content; no source-write or implicit board authority comes from joining a conversation. | pattern | 90 |
| D16 | Create-style inheritance | n/a; conversation and avatar are ephemeral presence, not scene nodes. Hats never encode permission or provider capability. | pattern | 90 |
| D17 | Hit-testing & pick | Painted board avatar is world-size × zoom with no size floor or ceiling (P0.9); hit slop may remain screen-constant. Roster icon and name/caret are distinct targets. Popovers intercept their own input only. | pattern | 90 |

## Feel constants

The accepted metric ledger supplies the visual checkpoint; the shared atlas-shell presence and face-rig owners implement it. Expression 180ms and gaze 475ms remain independent. The default face is 36 world units, with no screen-space diameter clamp. Attached conversation is world-scaled; the roster is 40px window chrome.

## Golden paths

- GP1: Click human name/caret → simple name/photo profile below and left; its right edge matches the segment. Edit name/photo → restart → both retained.
- GP2: Click human icon → self conversation; send note → dismiss/reopen/restart → note and draft retained; no provider invoked.
- GP3: Click agent icon → prior conversation and composer immediately. Send message → existing provider conversation binding receives it; no duplicate membership or grant restore.
- GP4: Start visible board work → double-click avatar → pause before the next mutation; listening animation and recent exchanges beneath avatar. Close chat → remains paused until explicit Resume (D02).
- GP5: Zoom 25%,100%,200% → avatar sizes 9,36,72 logical screen pixels for a36-world-unit face; attached chat scales and keeps world wrapping; roster stays40px.
- GP6: Open hats → scroll, drag or use arrows through all26 hats and none → header and board use the selected identity and calibrated neutral fit. No separate full-inventory step. Picker consumes wheel input without zooming the board.
- GP7: At720px viewport height and narrow width → header/composer reachable, messages scroll, all controls reachable. Long names/peers never overlap action targets.
- GP8: Retire then restore → same identity/history, fresh ephemeral membership, no old task or authority automatically restored.

## Open questions

None. Revision-2 acceptance selects explicit Resume after addressing and world-scaled attached conversation.

## Decision history

The original draft proposed D02 (keep paused versus resume on dismissal) and D09 (world-scaled attached conversation versus a readable overlay exception) at confidence 55. These alternatives remain historical proposals, superseded by the user’s accepted revision-2 checkpoint and full-build instruction on 17 October 2026. The accepted choice keeps board work paused until Resume and follows P0.9 for the attached conversation; no new scale exception is introduced. Original proposal rows are retained in decisions.json proposal_history.

All D01–D17 dimensions already exist in DIMENSIONS.md and apply to this tool-family contract. No new dimension or cross-contract pattern is introduced by this checkpoint. Status remains agreed until required builds, golden-path checks and command registrations are verified; actual native and user verdicts are recorded separately in design/agent-presence-2026-10-17/native-acceptance.cases.json and its interactive document.

## Implementation reuse

Owner: `atlas-shell::agent_presence` for chrome; Slate adapter for lifecycle/camera; `atlas-agent::AgentSession` and `atlas-ai::AgentSources` for provider history. Human participant messaging is owned by atlas-relay and the Slate conversation adapter; it is separate from shared-board synchronization. No copied portal host or speculative delivery status.

Cover Flow projection and motion belong to `atlas-shell::home`. The browser review consumes poses exported by that owner; its DOM scroll/snap motion is approximate. Production must call or extend the existing album owner rather than copy its projection or physics. Detailed harness continuity and permission boundaries: `design/agent-presence-2026-10-17/HARNESS-SWITCHING.md`.
