# Scoped board collaborators

The native collaborator adapter uses the ordinary registered command surface.
Its provider-neutral contracts live in `atlas_agent::collaborator`; transport
lives in `atlas_ai::board_link`. The host owns command applicability, visible
progress, staging, authorship and final journal commits. A provider permission
or an agent-made wire never enlarges a human's board grant.

## Membership and grants

`AgentRequest.collaborator` is optional. Old requests and ordinary chat trains
retain their existing behavior. An explicit collaborator binding names three
opaque IDs: `session_id`, `grant_id`, `workspace_id`. A fresh membership gets a
fresh session ID. Profile restoration does not restore this binding or its job.

The human-created `CollaboratorGrant` names readable board/node/source scope,
allowed registered command IDs, permitted conversation IDs, mutation autonomy,
helper limits and a task action budget. Source entries are stable source IDs,
not arbitrary filesystem paths. Whole-board reads require `readable_board`;
otherwise the host filters every returned node against `readable_nodes`, including
an empty read request. Conversation scope does not itself implement a relay;
the host still calls the established messaging/crosstalk owner.

`grant.authorize(call, current_session, current_request)` fences identity,
command and node scope before the host resolves arguments. The host repeats
that check when an animation reaches the commit boundary. Mutation autonomy
defaults to false. Proposal acceptance stays in the existing stage owner;
agents cannot use this bridge for source write-back or File Atlas mutations.
Helpers need a distinct grant ID and a subset of readable scope, commands,
conversations and action budget. Helpers receive no inherited mutation autonomy
or further helper allowance.

## Frame-loop API

`BoardLinks::set_session(dir, binding, grant, request_id)` validates and queues a
binding before provider submission. `poll(dir)` drains at most eight cached
calls per frame; `poll_limit(dir, limit)` drains a smaller shared frame budget
without discarding the remainder, capped at eight. `reply(dir, reply)` queues an attributed result. `revoke(dir)`
immediately disables the in-memory receiver; cleanup runs on its worker. A new
task on that link reuses the same worker, avoiding old/new worker file races.
`retain` releases workers for departed participants. The
host also cancels its provider request and any uncommitted visible actions.
No method performs filesystem I/O on the frame loop. A busy queue is an error,
not permission to skip a command or silently drop a reply; the host can retain
and retry its response.

The worker maintains `board-session.json`, `board-calls/<id>.json` and
`board-replies/<id>.json` inside the existing agent link directory. These are
ephemeral machine-local protocol files, refused by the shared export/privacy
filter. They are not workbook data. A worker ownership nonce prevents an old
retiring worker from deleting its replacement's membership file.

## Call and result

```json
{
  "id": "call-unique-1",
  "session": {"session_id": "s", "grant_id": "g", "workspace_id": "w"},
  "request_id": "r",
  "sequence": 0,
  "operation": {"kind": "read", "node_ids": [], "offset": 0, "limit": 32}
}
```

For an invocation, `operation` is
`{"kind":"invoke","command":"registered-id","targets":["42"],"arguments":{}}`.
The host's read result supplies the actual registered command schemas; providers
must not infer a mutation API from display text. Targets are explicit stable
IDs, never the human's transient selection. A result has `id`, `session_id`,
`request_id`, `success`, and JSON `result`; it must match the originating call.
A proposed change reports its proposal identity and current status rather than
claiming the document already changed.

Two app-level actions use the same invocation envelope. `app.save` takes no
targets or arguments and requires an independent collaborator's explicit
mutation-autonomy grant. It saves only the current already-saved writable
workbook; an unsaved workbook needs the human to choose Save As first. The reply
means the save was requested, and identifies when a background asset save may
still be pending. `app.feedback.report` accepts `what` (required), `tried`, and
`missing` (optional), each at most 800 characters. It writes a local note to
that collaborator's feedback log. It never opens the human form, reads a prompt,
or files an external issue.

Reads default to offset 0 and limit 32, with an offset ceiling of 1,000,000 and
limit range 1–64. The host returns scoped `total` and `next_offset`; null means
the final page. Scope filtering precedes pagination, so private nodes neither
consume a page nor leak into its total. Providers follow successive offsets
to discover large boards and can lower the limit for text-heavy objects.

Participant outcomes use the separate compatible read operation
`{"kind":"read_participants","participant_ids":[],"offset":0,"limit":8}`.
Its offset defaults to 0 and its limit defaults to 8 and cannot exceed 8. Empty
IDs enumerate only the live grant's conversation scope; explicit IDs allow a
known outcome to be retrieved directly and must also be in that scope. Retained
helper results additionally require the caller to be that helper's parent. The
reply's `participants`, `total`, and `next_offset` form an independent cursor.
The caller is included when its own ID is granted. The host may emit fewer
records than the requested limit to keep the serialized reply bounded; always
follow the returned `next_offset`. Board object offsets and replies are unchanged.

Messages and replies are limited to 64 KiB, target lists to 64 IDs, simultaneous
Codex tools to eight, the worker output queue to 16, and each task to at most
128 calls. Calls include a sequence number; duplicate sequences and stale
request IDs are rejected. A task's human grant may lower those limits. Invalid
calls cannot reach the UI dispatcher. The host's bounded read result can ask
the provider to request fewer nodes.

## Providers

Codex registers `slate_board_read`, `slate_participants_read`, and
`slate_board_command` through app-server
`thread/start.dynamicTools` only for a fresh explicit collaborator session.
The adapter handles `item/tool/call` and sends a dynamic-tool result after the
native host replies. The actual installed app-server schema was generated
using `codex app-server generate-json-schema --experimental`; the retained
reference is `target/codex-presence-schema`. The [official app-server documentation](https://learn.chatgpt.com/docs/app-server)
describes this dynamic-tool flow as experimental and requires the initialize
capability `experimentalApi: true`; the existing Client already advertises it.
Ordinary chat sessions receive no board tools or context.

Native collaborator model turns default to the local filesystem read-only
sandbox, independently of the board mutation/autonomy grant. A human may
separately opt an independent Codex collaborator into provider Full access via
the existing machine-local access owner. The runtime resolves that trusted
decision; board grants and request payloads never confer it. An explicit
request-level ReadOnly policy still overrides provider access and refuses board
invocation. Helpers and relays retain their separate restrictions. All board
changes still use authorized host commands; source write-back remains human-only.
Filesystem read-only does not itself restrict configured external MCP service
authority or establish restricted source read roots.
The host injects `atlas_core::cloud::is_dehydrated` through
`Client::set_board_cloud_guard`; Codex board protocol reads fail closed without
that callback. Both transport readers test the existing cloud owner before
metadata/open, so polling a stale OneDrive placeholder never downloads it.

Cursor and other file-capable local providers can use the identical typed
file bridge. `sidecar::CursorLink` wraps the established sidecar launcher and
`AgentSources::prepare_request` sender on a worker; startup, dependency setup,
cancel files and retirement never run on the UI thread. Startup errors are
available through `take_error`. No key is returned to the host UI or written
to the board protocol. Both Rust and Cursor read one instruction profile,
`cursor-sidecar/board-link-v1.md`, with JSON-quoted identity and link folder.
External file providers need write capability for their command mailbox. Their
host command grants are enforced by Slate; the file bridge is not an operating
system sandbox for the provider's separate filesystem or external-tool actions.

## Optional live read smoke

`cargo run --locked --release -p atlas-ai --example board_read_smoke` performs
one optional model turn using the configured signed-in Codex CLI and its default
model. It is not part of ordinary tests. The temporary workspace lives beneath
the repository's existing `target` directory, and the host returns exactly two
synthetic objects. The grant allows one read and no commands, helpers, source
paths or conversations. The prompt requests no other tools; observed unexpected
public tool activity cancels the run. No host code reads user documents, submits
board mutations or sends messages to another person.

The existing Client's optional bounded-start API caps connection/RPC/model work
at 80 seconds, cancellation is requested by 75 seconds, and the overall smoke
reserves the remainder of its 90-second budget for cleanup. Ordinary Client
startup keeps its existing timeouts. The normal Client Drop stops its app-server;
board membership and protocol files are removed. A small `report.json` remains
inside that smoke workspace, containing boolean/count outcomes, opaque call
IDs, a classified failure and a fixed safe summary. It prints no credentials,
raw app-server environment, account details or arbitrary provider response text.

Public tool lifecycle events supply activity categories. The Codex board-read
tool reports Reading; exact manipulation categories/targets come from the host
command adapter. Neither private reasoning, tool argument contents nor output
sentiment are used to infer a face.

## Validation

Pure Rust tests cover scope/identity/budget fencing, helper restrictions,
backward-compatible requests, bounded messages, worker roundtrip, retirement,
stale requests and duplicate sequences. Provider tests cover the installed
dynamic-tool shape and strict typed argument parsing. JavaScript tests cover
the shared instruction profile, identity refusal, real watcher behavior with
an isolated SDK double, public-activity privacy and packaged-resource refresh.
Native execution and GUI acceptance require the ordinary Windows build checks;
compilation evidence alone is not a native UX acceptance result.
Mailbox scanning retains its directory iterator across bounded 32-entry ticks;
malformed, temporary or unreadable files cannot repeatedly monopolize the first
batch. Regression coverage places a valid call beyond persistent malformed and
temporary entries, and verifies limited polling preserves queued calls.
