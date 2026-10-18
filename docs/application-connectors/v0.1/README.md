# Application connector v0.1 candidate

Status: **Implemented foundation; executable validation blocked**, 16 October
2026. This is a development candidate, not a frozen protocol release. Windows
has refused to execute newly generated Rust build helpers. Schemas and message
fixtures must be generated and checked, and the conformance suite must pass,
before another project treats this candidate as a stable dependency.
The [validation record](VALIDATION.md) retains the exact blockers and remaining
checks.

## Owners and integration boundary

[`atlas-connector`](../../../crates/atlas-connector/src/lib.rs) owns the pure
wire types, admission policy, schema generator and mock connector.
[`atlas-connector-runtime`](../../../crates/atlas-connector-runtime/src/lib.rs)
owns local transport, artifact transfer, and reversible native-window hosting.
[`atlas-packs`](../../../crates/atlas-packs/src/application.rs) remains the
installation discovery owner. It returns compatible, healthy `Program` offers;
installation health does not establish a running application session.

The [mock pack example](mock-pack.json) shows the optional manifest descriptor.
Its registration name is a machine-local identity, never a command string. This
fixture does not install a registration resolver or authorize an executable.

`host::HostChannel` is the reusable worker-side host SDK. It pins the selected
pack/application identities, negotiates, admits trusted actions, validates
replies and retains uncertain receipts after broken exchanges. The mock runner
calls this same SDK. Native window work remains on the UI thread.

The Enscape host now uses the shared native-surface guard and retains its own
dedicated-process launch and termination policy. A borrowed application is never
terminated by dropping the generic guard.

The foundation does not yet add a connector icon to Slate's media blister,
register a machine rendezvous service, or route foreign captures into the
existing board capture journal. Those are subsequent Slate integration work,
using the existing portal and capture owners and an approved interaction
contract. No Rhino implementation is included.

## Connection and framing

The Slate host creates a fresh 256-bit random token and binds a one-use listener
before publishing the token through a trusted, explicit launch or attach action.
The host independently knows the selected application's PID; the application
independently knows the host's PID. Neither PID is trusted merely because a
message supplies it. There is no automatic application launch or discovery by
scanning other processes.

The Win32 pipe path is `\\.\pipe\slate-connector-{token}`. A .NET
`NamedPipeClientStream` uses local server `.` and the bare pipe name
`slate-connector-{token}`. Tokens are exactly 64 hexadecimal characters, normalized
to lowercase. The listener has an explicit current-user-only protected DACL,
rejects remote clients, and refuses an existing pipe instance. Both ends verify
the expected process, user SID, OS session and process creation time.

Every frame consists of a four-byte unsigned **little-endian** length followed
by that many bytes. Control payloads are UTF-8 JSON, without a BOM or trailing
delimiter. Lengths must be within 1..4 MiB. Reject oversized lengths before
allocation. Clean EOF before a header means disconnect; partial headers or
payloads are malformed. Invalid JSON, partial frames, cancellation and timeouts
close the connection. Never reuse the partially consumed stream.

The application sends `WireFrame::Hello`; the host answers `Welcome`. Thereafter
the host sends `Request` and the application sends `Reply`. `kind` discriminates
these frames; `operation.method` and `outcome.status` discriminate their payloads.
The source types and generated JSON schemas define exact field spellings.
Schemas describe message structure; admission validation additionally enforces
capability, authority, size, identity and lifecycle constraints.

All pipe work and artifact hashing belongs on workers. A transport deadline
covers a whole frame, including partial reads/writes. A replacement connection
gets a new token and generation; the source is explicitly rebound. Requests
with an uncertain outcome are never automatically sent again.

## Negotiation and source ownership

The protocol family is `slate.application`. v0.1 implements the exact version
intersection `{ "major": 0, "minor": 1 }`; it does not assume minor-version
compatibility. Unknown optional capability names are ignored. Unknown,
unoffered or disallowed required capabilities reject negotiation.

`lifecycle` is mandatory. `surfaces`, `commands`, `views`, `capture` and
`publication` additionally require `sources`. A surface profile therefore offers
`lifecycle`, `sources`, `surfaces`; it need not offer commands or capture.
Capabilities do not imply one another's implementation.

The host binds only material selected by the human, carrying its stable source
identity, opaque revision, relative-first locator, selection receipt and linked
or isolated-draft mode. Binding returns a session-local native document identity.
Every source operation names both source and revision. Wrong identities, stale
revisions and wrong-session replies are refused. Locators do not authorize file
I/O; an adapter resolves the approved root and checks access. Do not hydrate
cloud files to invent revision tokens.

Binding changes and surface lifecycle transitions are serialized. Rebinding,
unbinding or detaching waits for admitted source work; new source work waits for
binding changes. A dirty source cannot be silently replaced or closed. Preserve
detach keeps the external document and unsaved changes open. Disconnect is not
permission to save, discard, or kill an attached application.

## Commands, authority and receipts

A command has a namespaced identity, effect, native or Slate undo owner,
cancellation support, agent eligibility and bounded parameter schema.
v0.1 parameters are a closed object with at most 64 scalar properties: string,
integer, number or boolean, with enums and simple numeric/string bounds. Nested
objects, references, coercion and executable expressions are unsupported.

The host derives caller authority from its real human or agent action before
dispatch. `Session::begin` requires that independent authority as well as the
request. Copying an untrusted request's `caller` into that argument does not
establish authority. Native mutations require a human receipt for this request
and the specifically named target. Agents may invoke eligible inspection
commands; workspace autonomy or a permissive connector descriptor cannot grant
native mutation authority. Native edits and undo remain in the application.
Production Slate dispatch must journal the human direction and target before
source write-back; the foundation supplies policy, not that UI/journal route.

The host validates provider replies with `Session::accept_reply` before applying
local state. Correlation alone is insufficient: returned bindings, revisions,
views, command results and effects must match the admitted operation.

Repeated identical request IDs replay their terminal receipt. Reusing an ID with
different content is refused; a pending duplicate does not execute twice. The
default ledger retains at most 1,024 requests within 8 MiB, including uncertain
tombstones. It refuses new work when full rather than evicting retry protection.
A deadline produces `outcome_unknown`, not proof of rollback. Cancellation asks
the native application to stop; its receipt explicitly has `rollback: false`.

## Surfaces and views

A connector declares full UI, viewport, companion window or headless support.
After binding, surface discovery returns session-local descriptors. A native
HWND is a decimal string, never durable workbook data. Before attaching it,
the host checks that the window belongs to the authenticated peer PID.
Geometry uses physical parent-client pixels plus explicit DPI, not board units.

The generic `NativeSurface` guard is owned by the host UI thread and preserves
the original parent/owner, styles, placement and visibility. It allows one
borrowed surface per host and restores it on ordinary detach/drop. An abrupt
process kill cannot run restoration; the plugin must retain its own recovery
path. Generic cross-thread focus is not a safe substitute for dispatching a
bounded request to the application's own UI thread.

Native Win32 embedding is cooperative. Cross-thread `SetParent` joins input
queues and cannot guarantee isolation from an unresponsive editor; a companion
window is the fallback. [Microsoft explains the input-queue relationship](https://devblogs.microsoft.com/oldnewthing/20130607-00/?p=4143).

View snapshots carry source/revision, schema namespace and version, restoration
fidelity and bounded data (256 KiB maximum). v0.1 supports negotiated native
schemas; it does **not** yet standardize a portable 3D camera model. That model
must derive from Slate's existing camera owner, including projection, axes,
units and framing. A Rhino connector can first publish its own namespaced view
schema, then implement the shared schema when it is ratified.

## Capture and binary transfer

Capture metadata names its source/revision and view, timestamp, dimensions and
atomic or best-effort consistency. Artifact descriptors contain an immutable
transfer ID, byte length, MIME type and lowercase SHA-256. Maximum artifact size
is 64 MiB; maximum capture dimension is 16,384. A receiver can impose a lower
budget. The store retains at most 128 artifacts and never silently overwrites
an ID. MIME validation is structural; consumers still decode and validate data.

Binary bytes use a separate authenticated pipe. Its token is lowercase hex
SHA-256 of `ASCII(control_token) + byte(0) + UTF8(transfer_id)`. For control token
`aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` and ID
`capture-1`, the derived token is
`395d6749d2ead254b559ac20402526e35026ae1b5c0b10434069a1cae08651bf`.

The sender binds this listener **before** returning its descriptor on the control
pipe. The transfer then uses one overall deadline:

1. Sender transmits the exact descriptor as a length-prefixed JSON frame.
2. Sender transmits nonempty length-prefixed raw byte chunks, each at most
   4 MiB and no larger than the remaining announced byte count.
3. Sender transmits JSON `{ "transfer_id": "…", "complete": true }`.
4. Receiver verifies the exact descriptor, length and SHA-256, then transmits
   JSON `{ "transfer_id": "…", "sha256": "…", "verified": true }`.
5. Sender validates the receipt before closing. EOF alone is not completion.

No peer-supplied path or URL is opened to retrieve an artifact. Hash failures,
truncation and mismatched completion/receipt frames fail the transfer. Native
application interfaces do not run in exported HTML; publication describes a
poster/pointer or a separately supplied portable artifact.

## Build, generate and exercise

Use the repository's [Windows build workflow](../../windows-builds.md). Preserve
the release profile and generated artifacts when an execution audit blocks a
helper. After approval, run:

```powershell
. ./scripts/_msvc-env.ps1
cargo test --locked --release -p atlas-connector -p atlas-connector-runtime -p atlas-packs --all-targets
cargo run --locked --release -p atlas-connector --example export_protocol
cargo run --locked --release -p atlas-connector --example export_protocol -- --check
cargo run --locked --release -p atlas-connector-runtime --bin connector-conformance -- self-test
cargo check --locked --release -p slate
```

The exporter derives schemas and JSON fixtures from Rust types and the same
mock implementation used in tests. Do not hand-maintain a second schema model.
`self-test` spawns only a disposable mock process and exercises authentication,
negotiation, bind, inspection, replay, stale-source refusal, denied agent write,
view restore, actual PNG transfer and preserve detach. Its adversarial command
offer is expressly a refusal test; it never writes a source file.

`connector-conformance serve TOKEN CONNECTOR_PID` runs that **mock profile**
against an explicitly selected process. It is not a production Slate rendezvous
endpoint or a universal certification of a vendor's native editing tools.

Before freezing: retain passing tests and generator checks; verify real native
window restoration; integrate explicit Slate activation and journaled capture;
verify missing-connector posters and export; then run joint acceptance with the
independent application project. See the [handoff](../HANDOFF.md).
