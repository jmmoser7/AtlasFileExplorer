# Agent crosstalk — interaction contract

Status: **agreed** (27 September 2026; built on `feature/agent-crosstalk`, relays in Step mode pending the VIII.1a amendment; blister, capsule, collapsed cards and square routing added 28 September 2026 on `feature/crosstalk-capsule`)
Family: portal
Portal class: **host** (Art. V.3), unchanged · Type: **agent** · Subtype: **crosstalk between two coding chat cards**
Command: `portal.agent.crosstalk.link` (drag between crosstalk ports, or palette with two chat cards selected) · Key: none in v1 · Palette:
"Start crosstalk" (aliases: agent crosstalk, agents talk, reviewer)
Inherits: every approved row of **portal-agent-link**, P0.* (all, including P0.9), P1.node, P1.portal,
P1.wire.ports, **P1.portal.elevated-red**, P2.PortalHost — deviations and new behavior only below.

Owner: `slate-doc::crosstalk` (model, stop rule, anchors, export label); `board_agent.rs` and its `crosstalk` child module (ports, relays, blister and capsule); `board_wire.rs` (the drag gesture, routing commands); `slate-doc::wire` (square lanes, through `vector_ink::rails`); `atlas-agent` (turn policy, relay provenance, goal verdict); `atlas-codex` (read-only turns); `atlas-ai` (relay grant, sidecar policy); the Cursor sidecar (read-only tools).
Forbidden forks: a second wire painter (crosswires are ordinary connectors); a second chat send path (a relay is the composer's send with the partner's reply as its text); a `board_crosstalk.rs` pasted from `board_agent.rs`.

## What it is, and the 10% it implements

Two coding agents on one board talk to each other through deep-red wires
between the top and bottom edges of their chat cards. Each side reads the
other's messages the way it reads the person's. The first case is Cursor
building and Codex reviewing read-only, or the reverse. A stop rule per
crosstalk (turns, minutes, goal, or a combination) ends it. Later, expert
agents answer several askers (hub and spoke); v1 is two-party.

The user reviewed the behavior matrix on 27 September 2026, answered its four
open questions, and said "looks good, please take it from here". Rows they
answered are `stated` at confidence 100. Rows they did not click keep the
recommendation, source and confidence the matrix proposed; they are approved
under that blanket statement and listed in the report so any of them can be
reopened.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | No armed tool and no palette placement. A crosstalk starts by dragging from one Cursor or Codex chat card's crosstalk port to another's. Keyboard path for parity (Art. VII.1): select two coding chat cards, then palette "Start crosstalk". Commands in SPECS: `portal.agent.crosstalk.link`, `.start`, `.pause`, `.resume`, `.stop`, `.edit`, `.send`, `.skip`, `.more`. No default hotkeys. | pattern | 70 |
| D02 | Stickiness & repeat | One-shot: after the drag the tool is still Select. Not repeatable through Space or Enter, since it needs two targets. | pattern | 80 |
| D03 | Gesture grammar | PortHover → Dragging (red rubber wire from the port) → released on a partner → owner crosswire committed, Start capsule open (roles, stop rule, goal, Start) → Running ⇄ Paused → Ended. A partner is another coding chat card (one provider-owned stream) with a conversation, past the chooser phase, in a different conversation. Released anywhere else, the drag is inert and nothing is journaled. Drawing never sends. Start relays the source card's latest finished reply, or waits for its next one if it has none; you type the task into the builder as usual. See X01–X04. | guess | 65 |
| D04 | Click vs drag rule | Travel under `draft.drag_threshold` (4 px) is a click. On a wired port it selects that crosswire; on an unwired port it does nothing. The wire's controls open from its blister (X17). | precedent | 80 |
| D05 | Modifiers | None in v1. Alt keeps its usual meaning (suppress snaps) during the drag; Shift and Ctrl are unassigned. | pattern | 75 |
| D06 | Constraints & snapping | The dragged end snaps to the partner's nearest crosstalk port within `WIRE_SNAP_PX` (14 px). Released anywhere on the partner's body, it lands on the port facing the source, as a wire dropped on a flow node does. | precedent | 80 |
| D07 | Direction / value locks | A cubic curve with vertical tangents out of the top or bottom ports, or square (orthogonal) like any wire (X19). Width `crosstalk.wire_width` = 4.5 like the history rails, deep red at `crosstalk.wire_alpha` = 0.85 in both themes (stored on the connector as the danger red, so both interpreters paint the same stroke). Geometry updates live while either card moves. | guess | 60 |
| D08 | Numeric / manual entry | In the capsule: Turns (whole number 1 to 200, default 10), Minutes (1 to 480, off by default), Goal (text, off by default). Click a value, type, and Enter or clicking away commits; Esc cancels. | precedent | 75 |
| D09 | Preview & readouts | Every crosswire carries a small blister at its routed mid-span; its centre dot is red while the crosstalk relays or a reply waits (X17). The open capsule reads for example "Turn 3 of 10 · 04:12 · Codex is replying", then the roles, and after a stop the reason. Received messages follow F1. During the drag: the red rubber wire and the partner's revealed ports. In Step mode a finished reply shows "Send to Codex" beside the newest wire's blister; Skip is in the capsule (X10). | stated | 100 |
| D10 | Cursor | Near a crosstalk port, resize targets and cursors are suppressed and the pointer is the grab hand, as for the output grip. | precedent | 85 |
| D11 | Commit | Answered by X09. | pattern | 65 |
| D12 | Cancel | Esc mid-drag drops the rubber wire and journals nothing. Esc, or a click anywhere but the capsule, folds it back to its blister; the committed wire stays "Not started", and Undo removes it. Board Esc never pauses or stops a running crosstalk, since agent work is out of process. Pause and Stop are explicit. | precedent | 80 |
| D13 | Selected presentation | Selecting a crosswire shows the ordinary connector highlight and no property strip; its controls are the capsule its blister opens (X17): status, roles, Send, Pause or Resume, Edit, 5 more turns, Square or Curved wires, Stop. Selecting the owner wire also highlights every downstream wire in its chain. Chat card selection is unchanged. | stated | 100 |
| D14 | Post-edit | Edit any crosswire's rule from its capsule; the override applies from that wire onward, and "Use inherited rule" clears it. Roles change only while paused. Presentation switches re-anchor wires (X05). | pattern | 70 |
| D15 | Non-goals | v1 leaves out: more than two parties (hub-and-spoke is designed for, X15); Ollama, text blocks and image agents as parties; crosstalk across workbooks or machines; relaying tool logs, diffs or reasoning; Slate-run check scripts (Art. VII.4); accepting a reviewer's suggestions automatically; a reviewer that writes files. | guess | 60 |
| D16 | Create-style inheritance | No. A crosswire is always deep red and ignores the drawing color and BoardLastStyle. Stroke color edits are refused on crosswires, because the color is the meaning. | guess | 60 |
| D17 | Hit-testing & pick | Crosswires pick on their stroke with `pick.slop`; the blister picks first and opens the capsule. Their ends cannot be dragged to another card, because anchors follow messages, like history rails. Delete on a crosswire ends the crosstalk at that wire: later relays stop, the wire leaves (hidden, so the message it carried keeps its red provenance), and one Undo restores it. | guess | 60 |
| D18 | Portal class & authority | Host, unchanged; no new portal. Each message stays owned by its own conversation's provider history. Slate owns only the card views, the crosswires and the rule. | precedent | 85 |
| D19 | Source binding | Each card keeps its AgentPortalRef. A crosswire's `crosstalk` binding holds the message it carries (sending session + turn index) and the message it landed as, a role per session (writes / reads only), an owner flag, a relay sequence number, a rule or none (inherit), and ended. No provider names in the model (Art. I.2); adapters map roles to provider policy. | guess | 65 |
| D20 | Query & parameters | Journaled: rule (turns, minutes, goal), roles, the Start baseline, per-wire overrides, ended. Derived: turns used, elapsed time, in-flight, paused, pending Step proposals. | pattern | 75 |
| D21 | Regeneration & staleness | A relay fires when the sending card's reply is finished, never on a partial stream. A background refresh that appends messages never relays a turn a crosswire already carries (keyed by session + turn index). A rule edit takes effect at the next relay. | precedent | 75 |
| D22 | Contents interaction | Red messages are ordinary transcript text: selectable, with Copy. The sender label names its source card. Both composers stay live (X07). | guess | 60 |
| D23 | Level of detail | Crosswires, ports, blisters and capsules scale with the board (P0.9). Below legible size the capsule's text drops, never clamped; a blister under 1.5 px drops and the red wire stays. Collapsed cards keep their ports. | pattern | 85 |
| D24 | Export serialization | Crosswires export as ordinary connectors with the same red stroke and the same routed path, curved or square with its bundle lanes, in both interpreters (Art. IV). The owner wire exports its chain as the wire label ("Crosstalk · 6 turns"), read from the scene by `slate_doc::crosstalk::export_label`; on the board the capsule's status is that label's live rendering, so a crosswire's label cannot be hand-edited. The red message treatment is part of each card's host poster. No live controls. | precedent | 80 |
| D25 | Bake | n/a: relayed messages are already cards, so there is no derived content to bake. | pattern | 80 |
| D26 | Collaboration & per-peer | Wires, rule and roles sync as document data. Running state is local to the machine running the agents. A peer who opens the board sees it paused and can Resume only with the same conversations connected on their machine. | pattern | 70 |
| D27 | Agent surface | Answered by X10. | pattern | 60 |
| D28 | Determinism & provenance | Host provenance: every red message names the sending agent and role; its crosswire names the conversation and turn it carries. The capsule names which wire's rule is in force. | precedent | 80 |
| D29 | Performance envelope | Relays are written by the link worker, off the frame loop. Crosstalk state recomputes on session or scene change only, not per frame; capsule layout goes through the shared canvas text cache. Budget: 20 crosswires add under 0.2 ms a frame. | pattern | 75 |
| D30 | Failure & honesty states | Answered by X16. | precedent | 70 |
| D31 | View-state ownership | Answered by X09. | pattern | 65 |
| D32 | Trust, sandbox & consent | Answered by X12 (roles enforced by provider policy), with Full access inside a crosstalk in X11. A crosstalk grants nothing by itself; permissions stay per user in the Atlas data folder. | research | 55 |
| D33 | Portal chrome | Cards unchanged. Received-message highlight: deep-red text, a danger fill at `crosstalk.message_tint` = 0.10, a 2-unit left bar, and the sender label in danger where the person's messages have none. F1 keeps it apart from Full access. | guess | 60 |
| D34 | Portal maximize | n/a: agent cards omit maximize. | precedent | 90 |
| D35 | Portal-local UI | Every crosstalk control lives on the wire: its blister, the capsule it opens, and the editor attached to that capsule (`selection_tools::crosstalk_editor`), or the card's ellipsis menu. Nothing on Document Settings and no property strip. Drawing a crosswire opens its capsule with the Start editor (X17). | stated | 100 |

Source values follow the tool-contract rubric. Rows answered by the user are
`stated` at 100; see the crosstalk table below for which.

## Crosstalk behavior (canvas rows S1–S4, X01–X21, F1)

These rows are specific to crosstalk, so they are not registry dimensions; the
D rows above cite them. They are mirrored in `decisions.json` under
`portal-agent-crosstalk.behaviors`.

| Row | Behavior | Source | Conf | Decision |
|-----|----------|--------|------|----------|
| S1 | Stop rule per task, configurable: a turn limit, a time limit, a goal, or a combination (whichever comes first). The first wire owns the rule and every later crosswire inherits it. You can open any downstream crosswire and change its settings; the change applies from that wire onward. | stated | 100 | yours, 27 Sep |
| S2 | A message that came from another agent shows on the receiving side as a user message in deep red, and every crosswire is deep red (chosen over a per-sender color). F1 settles how Full access stays distinct. | stated | 100 | yours, 27 Sep |
| S3 | The first shipped case pairs a Cursor conversation and a Codex conversation. One builds; the other reviews without writing. Which side builds is chosen when the crosstalk starts (X12). | stated | 100 | yours, 27 Sep |
| S4 | Designed for hub-and-spoke later: a set of wires with one owner per chain, not a two-party struct, so a card can carry many crosswires without a format change (X15). | stated | 100 | yours, 27 Sep |
| X01 | The top-edge and bottom-edge midpoints of a Cursor or Codex chat card are its crosstalk ports. Either port sends or receives; the route uses whichever port faces the partner card. They are separate from the top-right output circle, the left midpoint (context, Art. VIII.1), the human-context handle and the right edited-documents handle. Gray dot, zero opacity at rest, revealed on card hover or while a crosstalk drag is live, filled deep red once wired. Reach `crosstalk.port_reach` = 7 designed px, scaled with the board (P0.9). Port proximity suppresses resize targets, as the output grip does. | pattern | 65 | blanket |
| X02 | Single chat window: two single windows are joined by one deep-red wire between facing ports. Each message from the partner lands in the window's transcript on the user side, in red with the sender's name ("Codex · reviewer"). No cards or wires are added per turn beyond the relay's own crosswire, and every crosswire between the same two windows paints as one wire with one blister. | stated | 95 | blanket |
| X03 | Chat train (one message per card): A's reply card is wired to B's next user card, a new card in B's train, in red. B's reply card is wired to A's next user card, and so on. Each relay adds exactly one crosswire. Both trains keep growing rightward along their own history rails. | stated | 90 | blanket |
| X04 | Message pairs: A's reply becomes the user half of B's next pair card, in red with the sender's name. B's reply half becomes the user half of A's next pair. The crosswire leaves the sending card's port and lands on the receiving card's facing port. | stated | 95 | blanket |
| X05 | Each crosswire stores the message it carries (sending session + turn index) and the message it landed as. Its ends follow whichever card shows that message now: a presentation switch re-chunks the conversation, then `slate_doc::crosstalk::anchor_fixes` moves each crosswire to the card now showing its message, in the same journal step. One Undo returns the previous presentation and every anchor. | precedent | 80 | blanket |
| X06 | Forks leave the crosstalk behind. Cursor and Codex are single streams; their only fork is paste, which replays into a new independent conversation, and a fork never inherits crosswires. Deleting a card that holds a crosswire end removes that wire in the same undo step (views only; provider history stays). If the removed card was a running tail, the crosstalk pauses. | precedent | 75 | blanket |
| X07 | **Altered by you:** "Your message goes first and the exchange carries on. We can pause explicitly from the wire interface." Both composers stay live (Art. VIII.3). A message you send goes out as a normal user turn (not red) and does not pause the crosstalk; a relay waiting for that side is held while you have unsent text in its composer, so yours goes first. The reply to your message is that side's newest reply and relays onward like any other. Pause, Resume and Stop sit in the wire's capsule (X17). The card's Stop square cancels only that reply and pauses the crosstalk; the partial reply is not relayed. | stated | 100 | yours, 27 Sep |
| X08 | **Altered by you:** "The builder claims done; the reviewer must agree." The rule is a closed set: max turns (one turn = one relayed message), max minutes of running time (paused time excluded), and a goal you write, combined as whichever comes first. With a goal, the builder claims it by writing `"goal": {"status": "met", "reason": "…"}` in that message's `return.json`; the reviewer's next reply agrees (`met`) or disagrees (`not_met`) the same way. A read-only reviewer cannot write that file, so its transport guide asks for the verdict as the reply's strict last line, `Slate goal: {"status": "met" | "not_met", "reason": "…"}` (`atlas_agent::goal_in_reply`); nothing else in the prose is read. Only an agreed claim stops the crosstalk ("Goal met · Cursor claimed, Codex agreed"); a disagreement clears the claim and the exchange carries on. Slate reads the file or that one line; other prose is never scraped. The goal line in the transport guide is added only when the wire has a goal. At a limit, no new relay starts; a reply already streaming finishes and stays on its card, unrelayed. The capsule reads "Stopped · 10 turns", "Stopped · 30 min", "Goal met · …" or "Stopped by you". To continue, raise the limit or press "5 more turns". No check command runs (Art. VII.4). | stated | 100 | yours, 27 Sep |
| X09 | Journaled as you (`CmdAuthor::Human`): the first crosswire, its rule and roles, Start, every edit to a downstream rule, and Stop (sets `ended`). Each relay is one journal group authored `CmdAuthor::Agent("<provider>:<conversation name>")` of the sending agent, holding the partner's new card or cards and the new crosswire (`SceneJournal::merge_since`). The red highlight is stored provenance on the crosswire, not a guess. Derived, never journaled: turns used, elapsed time, in-flight, paused, pending proposals and the goal verdict (read from the link folder). One Undo removes one relay's cards and wire and pauses the crosstalk; it never recalls the message from the partner's provider history. | pattern | 65 | blanket |
| X10 | **Answered by you:** "Start is a bounded grant, read as conforming." Drawing the crosswire sends nothing. Start is the person's explicit grant to relay messages between these two conversations and add the cards and wires that show them, until the stop rule fires; it grants nothing else. Board edits either agent proposes still stage through StageFeed; tool approvals still appear on the card; Full access is untouched. Agents cannot start, resume or edit a crosstalk. **Built for now as Step mode:** each finished reply waits as a quiet "Send to Codex" pill beside the newest wire's blister and relays only when you press it, one click, because an autonomous relay puts agent-authored text into a conversation without your wire into its midpoint (Art. VIII.1). Draft amendment VIII.1a in `docs/audit/amendments/2026-09-27-crosstalk-amendments.md`; ratifying it flips `CROSSTALK_AUTONOMY_RATIFIED`, the only code change, and Start becomes the grant you chose. | stated | 100 | yours, 27 Sep |
| X11 | A reviewer cannot hold Full access: Start refuses and says why, and while the crosstalk is live the card's Full access toggle refuses a reviewer the same way, naming the fix (pause and swap roles in the capsule). Swapping roles refuses while the side about to review holds Full access. A builder with Full access runs relayed turns under the provider's default approvals, so approvals surface on the card, unless you tick "Let <builder> act on <reviewer>'s messages without asking" in the Start capsule. That tick is stored for this user in `agent-access.json` beside the Full access grant, never in the workbook. A crosstalk never grants Full access by itself. Cursor applies approvals per sidecar, not per turn, so a Cursor builder with Full access needs the tick or Start refuses. | guess | 55 | blanket |
| X12 | Roles are chosen in the Start capsule and journaled on the owner wire, per side: Builds (provider default permissions) or Reviews (read-only). The default is Cursor builds and Codex reviews. Codex reviewer turns carry `sandboxPolicy: readOnly` with `approvalPolicy: never`, so it cannot escalate. A Cursor reviewer's sidecar starts with a read-only tool allowlist (read, grep, glob, ls, semantic search, lints) and sends in `plan` mode; a read-only request to a sidecar started with write tools fails closed. Enforcement is provider policy, never a persona in the prompt. A read-only reviewer reports a goal verdict in its reply, not a file (X08). Windows enforcement of Codex's sandbox and Cursor's plan mode are unverified locally. Both sides must be bound to the same project folder, or Start refuses with "Bind Codex to <project>". A role only restricts, so it may travel in the workbook; grants stay per user. | research | 55 | blanket |
| X13 | B receives A's final reply text verbatim as B's user turn: no tool logs, reasoning or diffs. It also gets B's own midpoint wires (Art. VIII.1) and B's own provider history. The first relay carries the owner wire's goal text as your line. A's wired inputs never leak to B. A reply longer than `crosstalk.relay_max_chars` (30,000) pauses the crosstalk with "Send in full" or "Send the first 30,000". Conflicts with Art. VIII.1 when relays run without you; see X10 and draft VIII.1a. | pattern | 60 | blanket |
| X14 | Saved: crosswires, their message anchors, rule, roles, owner, overrides, Start baseline and ended. Not saved: running state and timers. On open, every crosstalk that has not ended opens Paused ("Paused · reopened") and nothing resends. Replies that finished while Slate was closed are shown but not relayed until you press Resume. | pattern | 80 | blanket |
| X15 | Later: an expert card may carry many crosswires, queries queue first in, first out on the expert (`crosstalk.hub_queue` = 8), and each asker-expert wire has its own stop rule. v1 refuses a second partner on a conversation with "Several partners comes later". | guess | 45 | blanket |
| X16 | Failures pause, never auto-approve. Error: the crosstalk pauses and the capsule reads "Paused · Codex failed" with Retry, which resends the same relayed message. Timeout: after `crosstalk.turn_timeout_min` (20) the capsule reads "Codex has not answered in 20 min" and the crosstalk pauses. Needs approval: Allow once / Deny appear on the card and the capsule reads "Waiting for you · Cursor needs approval"; the time limit stops counting. Partner card deleted: the crosstalk ends; one Undo restores it. | precedent | 70 | blanket |
| X17 | **Yours, 28 September 2026:** "everytime a new wire is linked between agents ther should be a subtle blister at the mid span of the wire that when clicked expands to a capsule or node expanded at the mid span of the wire with the wire pluged to its inputs and outputs … accesable from all previously drawn conector wires." Every visible crosswire, including ones saved before this, shows a blister at its routed mid-span, derived on paint, never stored. Clicking it (`portal.agent.crosstalk.capsule`) expands it in place into a capsule the wire runs through: its input and output are where the wire enters and leaves the capsule's edge, marked with the card's handle dot. The capsule holds status, roles, and the actions the old chip had (Send, Skip and pause, Pause or Resume, Edit, 5 more turns, Square or Curved wires, Stop). Edit attaches the stop-rule editor beside it. Esc or a click away folds it back. Open or folded is view state, never journaled; every edit inside it is. The old chip and the crosswire's property-strip panel are gone, so there is one surface. Drawn with `selection_tools` (`blister`, `crosstalk_capsule`, `place_popup`) and scaled with the board (P0.9). Ordinary connectors get no blister: their mid-span holds their label, their editor is the selection strip, and they have no running state a capsule would own. | stated | 100 | yours, 28 Sep |
| X18 | **Yours, 28 September 2026:** "for crosstalk lets keep the chat train capsules in there colapsed state. to minimize the visual cluter". Every new card of a conversation in a crosstalk, relayed or typed, arrives collapsed (`slate_doc::crosstalk::collapses_new_cards`). A person can still open any card by hand, and Slate never collapses it again. | stated | 100 | yours, 28 Sep |
| X19 | **Yours, 28 September 2026:** "user should have option to toggle wires to orthognal mode same as with normal wires. look to file atlas orthgnal wire drawing for president on avoiding colissions between wires of multiple nodes particularly when doing many to one bundeling." A crosswire routes curved or square through the board's own routing commands and persistence: `board.wire.routing` / `.bezier` / `.orthogonal` toggle the selected wires as one journaled edit, or, with nothing selected, the board's routing for new wires (settings, not journaled). A new link takes the board's routing like any connector; a relay keeps its conversation's newest wire's routing. The capsule's "Square wires" / "Curved wires" runs the same command on every wire of that conversation. Square crosswires meeting at one port bundle by the File Atlas nested-rail rule, now shared as `vector_ink::rails` and called by both the folder map and `slate_doc::scene_ortho_lanes`: per side of the port, the farthest wire exits outermost along the edge and its trunk runs nearest the port, so wires converging on one card neither touch nor cross a card. The blister, capsule and export follow the routed path in both routings. | stated | 100 | yours, 28 Sep |
| X20 | Permissions change from the next message. Full access can be toggled while a reply runs; the toast says "…, from the next message. The reply in progress keeps its permissions." A Cursor sidecar restart waits until that conversation's reply finishes. Codex turns carry an explicit sandbox and approval policy only when overriding, and the first turn after an override restores the thread's own settings, read from `thread/start` or `thread/resume` (or the provider default), so a reviewer's read-only turn cannot leak into the builder's next turn. | pattern | 70 | 28 Sep |
| X21 | **Yours, 28 September 2026:** "colission avoidence should be best efort. not absolute. alocate a zone for conection where bundeling wires can spred out but once that space is filled up the packing and bundeling simply densifeies with optimaly sorted incoming wires." Every square wire, crosstalk or not, bundles by the one rule (P1.wire.rails): each connection has a bounded zone (`slate_doc::CONNECTION_ZONE`: 96 along the edge, 48 deep); lanes sit 10 apart while they fit, then pack evenly denser inside the same zone down to 3; incoming wires are sorted so they do not cross, and a wire goes around a node body where it can. Best effort: the zone never grows and no wire is rerouted far away. The blister stays on each wire's routed mid-span and the export draws the same path. | stated | 100 | yours, 28 Sep |
| F1 | **Altered by you:** "Same red, different places. Red label for Full access. Red text for inter-agent crosstalk. General interpretation: red is for elevated privileges. A visual cue that this is not a typical chat train." Full access keeps its red model-name label; a message another agent wrote shows its body text in red with a red sender label. Promoted as **P1.portal.elevated-red**. | stated | 100 | yours, 27 Sep |

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `crosstalk.port_reach` | Crosstalk port hit radius (designed px, scales with zoom) | `7` |
| `HANDLE_DOT` | Painted port dot, the chat card's shared handle dot (`paint_handle_dot`) | `3.5` |
| `crosstalk.wire_width` | Crosswire painted width, same as history rails | `4.5` |
| `crosstalk.wire_alpha` | Crosswire opacity, both themes | `0.85` |
| `crosstalk.message_tint` | Danger fill behind a received message | `0.10` |
| `crosstalk.default_turns` | Turn limit on a new crosstalk | `10` |
| `crosstalk.more_turns` | "N more turns" on a stopped chip | `5` |
| `crosstalk.relay_max_chars` | Longest reply relayed without asking | `30,000` |
| `crosstalk.turn_timeout_min` | Minutes without an answer before pausing | `20` |
| `crosstalk.hub_queue` | Queries waiting on one expert (later) | `8` |

Model constants live in `slate_doc::crosstalk`; painting constants beside the
chip painter in `board_agent/crosstalk.rs`.

## Golden paths

1. **GP1.** Cursor and Codex tails on the same project. Drag Cursor's bottom
   port onto Codex. The owner crosswire is red and the capsule opens: Cursor
   builds, Codex reviews, goal "tests pass and the reviewer approves", 10
   turns, Start. Type the task into Cursor. When its reply finishes the chip
   reads "Send to Codex?"; Send puts the reply in Codex as a red user
   message on a new card, with one red crosswire, in one undo step authored
   by Cursor. Codex's review comes back the same way. Cursor claims the goal,
   Codex agrees, and the chip reads "Goal met · Cursor claimed, Codex agreed".
2. **GP2.** Mid-run, switch Codex to a single chat window: its crosswires
   paint as one wire whose chip shows the count. Switch back: the zipper
   returns. One Undo restores the previous presentation and anchors.
3. **GP3.** Type into Codex while Cursor is replying: your message goes out at
   once, the crosstalk keeps running, and the relay of Cursor's reply waits
   until Codex is idle and your composer is empty.
4. **GP4.** Save, close, reopen: the crosstalk shows "Paused · reopened",
   nothing resends, and Resume continues from the stored turn count.
5. **GP5.** Open a downstream crosswire and set it to 4 turns: relays after
   that wire follow the new rule; the owner wire's rule is unchanged.
6. **GP6.** With a 2-turn rule, the third finished reply is not offered; the
   capsule reads "Stopped · 2 turns"; "5 more turns" continues.
7. **GP7.** Esc mid-drag, or release on empty board: nothing is journaled.
8. **GP8.** Undo after a relay removes that relay's card and wire in one step
   and pauses the crosstalk.

## Build status (27 September 2026)

Built on `feature/agent-crosstalk`, not shipped: relays run in Step mode until
VIII.1a is ratified, and provider read-only enforcement is unverified live.
Headless tests cover GP1 (link, Step relays both ways, goal claimed and
agreed), GP2 (switch to a single window and back), GP3, GP4, GP5, GP6
(through the autonomous seam), GP7 and GP8, plus Full access refusal, delete
and undo, and chip scale (P0.9).

## Build status (28 September 2026)

Refined on `feature/crosstalk-capsule` after first use. Headless tests
(`egui::Event`s only, fake providers) cover: a blister on every crosswire,
including a board reopened from disk (X17); the blister opening the capsule
and click-away or Esc folding it without journaling; Step mode sending from
the pill beside the blister and staying the default while
`CROSSTALK_AUTONOMY_RATIFIED` is false (X10); relayed cards arriving
collapsed and opening by hand (X18); Full access applying from the next
message and refusing a live reviewer (X11, X20); a read-only reviewer's
verdict read from its reply (X08); squaring and curving a conversation's
wires through the board routing commands, one journal step each, with the
blister and capsule ports on the routed path and the HTML export matching
the board; and five square crosswires converging on one card without
touching or crossing a card, on the board and in the export (X19).

Not verified: the real Codex app-server's handling of per-turn overrides and
the `thread/start` permission fields; Windows enforcement of Codex's sandbox
and Cursor's plan mode; a real Cursor sidecar restart; the look on screen.

Where the build falls short of an approved row (the row stands; the code owes it):

- D13: selecting the owner wire does not yet highlight its downstream wires.
- D22: the red sender label does not yet highlight or select its source card.
- X12: both providers offer Reviews. A Cursor sidecar asked for a read-only
  turn without read-only tools fails closed, but Slate does not confirm the
  provider's read-only at startup before offering the role.
- X13: the goal reaches both sides through the goal line of the transport
  guide on every turn, not as a visible line of yours on the first relay.
- X14: elapsed running time restarts at zero when a board reopens (turns used
  are rebuilt from the wires); `crosstalk.json` is not written.
- X16: a failed relay pauses with "Paused · <agent> failed" and Resume; the
  card's own recover actions retry. There is no capsule Retry that resends
  the same relayed message.

## Open questions

None. The autonomy question is answered (X10); what remains is the user's
ratification of VIII.1a, which is an amendment, not a contract question.

## Implementation reuse (required when Family: portal)

Owner: `slate-doc::crosstalk` for the model and pure rules;
`board_agent.rs` for relays (a relay calls `send_agent_prompt` with the
partner's reply as the prompt); `board_wire.rs` for the drag (a crosstalk
port is one more grip, and the commit branches on it); `atlas-shell::canvas_scale`
/ `canvas_text` / `selection_tools::{blister, crosstalk_capsule, place_popup,
crosstalk_editor}` for the blister, the capsule and its editor;
`board_agent::paint_handle_dot` for the ports and the capsule's input and
output (DV-23 closed). Crosswires paint through `paint_connector` like every
connector and route through `slate_doc::wire::connector_route_in_scene`, so
the board and the artifact draw one path. The bundle rule is
`vector_ink::rails::nested_rails`, extracted from the File Atlas folder map
(28 September 2026), which now calls it with unchanged output.

DRY review (27 September 2026): **extract-first**, applied. The handle dot
became one function before the ports used it; the Start capsule is a property
strip panel drawn by atlas-shell, not a second floating editor; Step
proposals stay derived in the crosstalk runtime rather than going through
`StageFeed`, which stages agent-proposed scene edits, not human-pressed sends.
Forbidden forks: a second connector painter, a second send path, a
crosstalk-only history rail, `board_crosstalk.rs` started from a paste.
