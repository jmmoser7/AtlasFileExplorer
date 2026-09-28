# Proposed amendment VIII.1a (and a VII.6 note) — 2026-09-27

**Source:** the agent-crosstalk behavior matrix
(`docs/keymap/contracts/portal-agent-crosstalk.md`, rows X10 and X13), reviewed
by the user on 27 September 2026.

**Article XI.2 status: UNRATIFIED.** Nothing here is law until the user edits
`CONSTITUTION.md`. The crosstalk build conforms to the constitution as written:
every relay waits for the person to press Send (Step mode), and the autonomous
relay the user chose is behind one constant, `CROSSTALK_AUTONOMY_RATIFIED` in
`apps/slate/src/app/board_agent/crosstalk.rs`, which stays `false` until this
amendment is ratified.

## Why an amendment is needed

The user chose "Start is a bounded grant, read as conforming" (X10). Measured
against the articles:

- **Art. VII.6 (proposal by default)** lets agent mutations skip staging when
  "the agent has been explicitly granted autonomy for that workspace". Start is
  explicit, and a grant limited to one crosstalk until its stop rule fires is
  narrower than a whole workspace, so the user's reading holds. No amendment is
  required for VII.6; the note below only writes that reading down so a later
  contributor does not argue it away.
- **Art. VIII.1 (canvas context is explicit)** does not hold for an autonomous
  relay. It says agent messages carry the user's text and conversation history,
  and that any other canvas context arrives only through a wire the user draws
  into the card's midpoint input. A relayed reply is neither the user's text nor
  that conversation's history, and it arrives through a top or bottom port. In
  Step mode the person presses Send on each relay, so the relayed text is the
  person's message by the person's act, as if they had pasted it; that is why
  Step mode conforms and autonomous relays do not.

Damage if built without the amendment: agent-authored text would enter another
conversation's prompt with no human act per message, which is exactly the
automatic context the 21 September amendment removed.

## Paste-ready text

Insert under Article VIII, after clause 1:

> **VIII.1a — Crosstalk.** A crosstalk wire the human draws between two
> conversations, and then starts, delivers each finished reply of one as the
> next user turn of the other, marked on the canvas as authored by that agent.
> It carries the reply's text and nothing else: no tool logs, reasoning, diffs,
> or the sender's wired inputs. It relays only until the stop rule the human
> set fires, and the human may pause, edit, or stop it at any time.

Optional, under Article VII, appended to clause 6:

> A crosstalk the human starts is an explicit autonomy grant limited to
> relaying messages between the wired conversations, and adding the cards and
> wires that show them, until its stop rule fires. It grants nothing else:
> other agent edits still stage, and tool approvals and permissions are
> unchanged.

## What ratifying unlocks

- Start becomes the grant X10 describes: finished replies relay without a
  Send press, one journal group per relay authored by the sending agent,
  until the turn, time, or goal limit fires.
- The change in code is one constant (`CROSSTALK_AUTONOMY_RATIFIED = true`)
  plus flipping the Step-mode golden test to the autonomous one, which already
  exists behind that constant (`autonomous_relays_run_until_the_turn_limit`).
- Nothing else changes: roles, read-only enforcement, the Full-access rule
  (X11), and the stop rules behave the same in both modes.

## Amendment log entry

Append to the **Amendment log** in `CONSTITUTION.md`:

```markdown
- **2026-09-27 — Crosstalk (VIII.1a).** At the user's direction, a crosstalk
  wire the human draws and starts delivers each finished reply of one
  conversation as the next user turn of the other, marked as authored by that
  agent, carrying only the reply's text, until the human's stop rule fires.
  Supplements VIII.1; does not change its rule for ordinary messages. Records
  that a started crosstalk is an explicit, bounded autonomy grant under VII.6.
```
