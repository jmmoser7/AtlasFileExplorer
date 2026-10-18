# Agent conversation — interaction contract

Status: agreed
Family: tool
Reference: Explicit first-use chat feedback and four supplied screenshots, 18 October 2026
Command: Registered presence.* commands, including presence.chat, presence.address, presence.draft, presence.photo, presence.provider.switch and presence.own_chats
Inherits: P0.* (all), especially P0.1, P0.7 and P0.9. No new tool grammar or scene node.

This is a revision of collaborator presence interaction, not a drawing tool.
Visual authority: `design/agent-presence-2026-10-17/metrics.json`; compact chat and behavior revisions below supersede the old panel size and outside-dismissal rules.
Revision 2 was accepted by the user on 17 October 2026, followed by an explicit instruction to build it into the final board. This is design agreement, not a shipped-build, native-interaction or user-acceptance verdict. The independent intent reviewer and the full native acceptance catalog remain required.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Existing agent icon, name and caret open its persistent conversation directly. Add shows only the harness picker and enters the new conversation. Human name/caret opens the aligned profile; avatar double-click opens the recent conversation beneath it. | stated | 100 |
| D02 | Stickiness & repeat | Chat stays open while clicking or manipulating the board outside it. Explicit Close or Escape dismisses it while retaining history and draft. Addressing the canvas avatar pauses board work; explicit Resume continues without discarding the conversation. | stated | 100 |
| D03 | Gesture grammar | Working → avatar double-click → paused/listening → recent message bubbles and composer beneath avatar. No board edit commits after pause until explicit resume. | stated | 100 |
| D04 | Click vs drag rule | Single-click anywhere on an existing agent roster segment opens chat without toggling it closed. Human icon opens chat and human name/caret opens profile. Avatar double-click pauses/listens; chat owns only its visible pointer region. | stated | 100 |
| D05 | Modifiers | n/a; no new modifier chord. Existing canvas camera controls remain available outside chat editing. | pattern | 85 |
| D06 | Constraints & snapping | n/a; presence is not authored geometry and is not a snap target. | pattern | 85 |
| D07 | Direction / value locks | n/a; no geometric value lock. | pattern | 85 |
| D08 | Numeric / manual entry | Composer supports ordinary multiline text/paste and explicit local image attachments. It remains editable during work; Codex updates steer the active turn, other supported harnesses queue follow-ups with truthful delivery status. | stated | 100 |
| D09 | Preview & readouts | Approved eye-only face and independent expression/gaze timings. Listening when addressed. Avatar, hat, prop, name and attached conversation scale in canvas space under P0.9, with world wrapping preserved. Roster conversation remains window chrome; no attached-overlay scale exception. | stated | 100 |
| D10 | Cursor | Existing pointer over canvas; text caret in editor. Agent animation never moves or captures the human pointer. | pattern | 90 |
| D11 | Commit | Visible messages and drafts persist separately from ephemeral runtime sessions. Stale missing-rollout Codex bindings recover through a fresh thread with visible history replayed once. Stop preserves conversation and revocable human permission policy; new requests remain available. Grants authorize registered journaled commands, authored text and saving; pending proposals require acceptance unless the exact action is explicitly autonomous. | stated | 100 |
| D12 | Cancel | Escape dismisses the innermost picker/menu then chat; explicit Close also dismisses chat. Outside clicks leave chat open and the board interactive. Dismissal retains history and draft and never implicitly resumes paused work. | stated | 100 |
| D13 | Selected presentation | One continuous roster capsule with discreet vertical separators. One enclosing conversation panel includes the compact identity/control capsule, alternating bubbles and bottom composer. Hats scroll in a compact album flow owned by atlas-shell::home, with a fine selected outline and no All hats step. Cursor/Codex use canonical provider glyphs; retirement uses a gentle sunset glyph. | stated | 100 |
| D14 | Post-edit | Agent identity and explicit revocable permission preferences persist per identity, harness and workspace scope. Joining creates a fresh session from matching human policy; jobs, positions and live membership never restore. Retired agents return through Advanced. Revocation takes effect immediately. | stated | 100 |
| D15 | Non-goals | Never fabricate remote delivery, unsupported menu outcomes or board progress. Human messages require the real participant-message transport, separate from the Slate–Atlas bridge and from shared-board synchronization. Profiles stay separate from transcript content; no source-write or implicit board authority comes from joining a conversation. | pattern | 90 |
| D16 | Create-style inheritance | n/a; conversation and avatar are ephemeral presence, not scene nodes. Hats never encode permission or provider capability. | pattern | 90 |
| D17 | Hit-testing & pick | Roster chat is compact nonmodal window chrome with reachable Close, transcript and pinned composer. Attached chat and avatar/hat/prop/name retain canvas-space scale under P0.9. Public progress streams inside chat; private reasoning is never displayed. Hat geometry/approved fits remain unchanged and may protrude above the header. | stated | 100 |

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

## First-use correction, 18 October 2026

The user reopened acceptance after real interaction exposed stale Codex resume failures, oversized dismissing chats, busy-message rejection and nonpersistent privilege choices. These explicit corrections supersede the earlier checkpoint; earlier render evidence does not establish acceptance of this revision. Validate multi-turn recovery, pause/resume, mid-turn updates, images, board manipulation outside chat, remembered/revoked policy, authored text and saved-file round trip. Public tool activity and agent commentary are visible progress, never private reasoning.
