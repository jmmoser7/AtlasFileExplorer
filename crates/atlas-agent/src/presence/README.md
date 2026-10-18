# Agent presence contract

`atlas_agent::presence` is a provider- and renderer-independent foundation.
It paints nothing, moves no operating-system pointer or camera, and executes
no document commands. Slate's native adapter connects this reducer to the board
and the existing Codex/sidecar provider adapters.

Persist `AgentProfile` definitions (ID, name, hat, retired flag) and UI settings
as machine preferences, outside the workbook. Register profiles
with `PresenceRoster`; `join` creates a fresh `PresenceSession`. Sessions,
requests, activity, cursor position, queues, and reservation hints are runtime
only and cannot be serialized through this API. Retirement cancels a session
and its helpers. Restoration starts at the supplied spawn point with a fresh
session identity and no old request or job. `leave` drops runtime membership
without retiring the profile.
Sessions, grants and motion remain ephemeral and never enter a workbook or
export. Profile/settings changes are machine preferences, not document edits.
Authored board mutations use Slate's existing native command/staging owners
and commit invertible `SceneCmd`s to the scene journal with `CmdAuthor::Agent`.
This roster never writes or saves a workbook.

See [the native implementation and validation record](../../../../design/agent-presence-2026-10-16/IMPLEMENTATION.md)
and [the scoped board-link protocol](../../../../docs/agent/board-link.md).

Call `begin_request` with a fresh request ID, then reduce typed `PresenceEvent`s
using strictly increasing sequences. Wrong session/request IDs and stale
sequences are rejected before state changes. Generic provider activity does not
infer a target. `MotionPlan` names stable node IDs separately from coordinates.

Enqueue a plan whose start matches the previous planned endpoint. `advance`
samples Travel → Grab → Work → ReadyToCommit using visible elapsed time, capped
at 50 ms per call. A background gap cannot drain queued actions or teleport
through their previews. Travel and Work last at least 150 ms each, and Grab at
least 50 ms. Each delivery also stops at the next phase boundary, discarding
overshoot so even coarse frames show intermediate travel, Grab, and Work before
ReadyToCommit. `interrupt` cancels dependent plans and takes the last
witnessed position as the replacement start. Frames do not allocate.

At ReadyToCommit, call `resolve_ready` with a check against the actual command
and current scene. A valid check emits one `ReadyToCommit`; an invalid check
emits one `Cancelled`. The host must journal its real command immediately after
a ready signal and handle journal failure itself. Neither signal claims command
execution. Cancellation before resolution emits only `Cancelled`. Reservation
hints acquire no external lock and disappear when the action/session ends.

Bounds are explicit: 128 profiles, 32 sessions, 8 helpers total, 4 children per
parent, depth 2, 8 queued plans plus one active plan, 16 targets per plan,
128 action IDs per request, and 128 requests per session. IDs are at most 128
ASCII bytes; world coordinates are finite and within ±1e9. Each phase is
150–30,000 ms for Travel/Work and 50–30,000 ms for Grab. At a memory limit,
the host must apply backpressure or leave and
join a fresh session; old IDs are not evicted into replayable state.
