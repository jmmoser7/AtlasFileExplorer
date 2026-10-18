# Slate application connector protocol proposal

Status: **Architecture direction with v0.1 foundation implemented**, 16 October
2026. The [candidate implementation and validation status](v0.1/README.md)
describe what is now present. This proposal also includes subsequent Slate UI
integration work; it is not a frozen wire specification.

Slate supplies the canvas, links, common media tools, capture, presentation,
and the connection points through which independently developed applications
can participate. A connector supplies specialist application capabilities.
The external application keeps its document model, editing tools, and history.

## Project boundary

There are two independent projects. This repository owns the Slate protocol,
shared host, developer contract, mock connector, and conformance evidence.
A separate repository and agent team will build a Rhino plugin implementing
that contract. Rhino is the first intended consumer, not a special case in
the protocol or a new vendor-specific scene variant.

A Rhino plugin runs inside Rhino and outside Slate. Here, **sidecar** means
an adapter outside Slate's process; it need not be a separate executable from
the application it connects. Rhino.Inside is an optional implementation choice
for that independent project, not a requirement of this contract.

The developer handoff is [HANDOFF.md](HANDOFF.md). Exact message shapes belong to
`atlas-connector` and its generated schemas; the handoff does not maintain a
second wire model.

## Confirmed direction and proposed defaults

The user has established the project split, Slate as an armature for optional
applications, one extension system, and specialist tools activated from their
material on the canvas. Humans and agents should discover the capabilities a
connector actually exposes. Captures should preserve useful source and view
relationships, including a route to named views in the originating program.

The following are engineering proposals for the first implementation:

- Retain `atlas-packs` and its existing `Program` contract as discovery owners.
- Use one versioned application protocol with independently negotiated
  capability groups. Keep current chat and image protocols compatible.
- Start with local Windows connections over authenticated named pipes.
- Support plugins attaching from an already running application first;
  launching is a separate, optional, explicitly invoked capability.
- Permit one active embedded foreign surface at a time. Other cards keep
  their posters; attached application sessions need not be terminated.
- Freeze the first wire version only after a generic mock connector passes
  conformance tests. The Rhino project consumes that published version.

These defaults can be revised during protocol review. No constitutional
amendment or new canvas interaction contract is ratified by this proposal.

## Existing owners

`atlas-packs::PackManifest` and `Catalog` own pack identity, registration,
probes, and availability. Extend that catalog; do not create an application
registry beside it. Existing manifests name one contract and a version. A
successful probe currently establishes installation health, not a connected,
compatible editor session.

`slate-kit::Registry` owns declarative tool recipes. A pack can supply recipes
and a connector descriptor. Executable adapter code stays installed on the
machine, outside the workbook and outside Slate's process.

`board_portal` and `board_portal_chrome` own source resolution, contents focus,
the host shell, empty states, and bake. Generalize the native window lifecycle
demonstrated by `enscape_host` through an extract-first plan. Do not create a
Rhino copy of that file or of `board_web`. The existing owner brief already
reserves one general external-program portal in its phase 4.

`board_portal::capture` owns capture lifecycle and publication. `slate-doc`
owns durable authored references; `slate-artifact` interprets those references
for export. `model-preview::view_meta` owns today's model screenshot metadata;
it is a source for a shared view schema, not an excuse to duplicate one.

`atlas-commands` and Slate's `SPECS` own built-in command identity and input
dispatch. They currently describe static commands, not a dynamic catalog of
foreign tools. Register generic connector actions there; keep foreign command
descriptors in one separate negotiated catalog. Existing `atlas-agent` chat
and generation messages must not acquire unrelated application payloads.

The extraction review approved `atlas-connector` as the renderer-free wire
owner and `atlas-connector-runtime` as the local IPC/native-surface owner.
Discovery stays in `atlas-packs`; generic application execution does not become
another vendor branch in the AI runtime. Enscape now uses the shared borrowed
surface guard while retaining its existing dedicated-process policy.

## Three records with different lifetimes

### Installed pack

Extend the manifest with an optional application-connector descriptor so old
manifests remain valid. It identifies the protocol family and supported
versions, eligible formats and facets, and a machine-local registration for
the installed connector. Facets come from the existing facet taxonomy.

Manifest claims describe an offer. Session negotiation establishes what is
available now. Distinguish installed, compatible, connected, source health,
and operation status. A missing license or unavailable surface cannot become
an apparently working connection because the executable exists.

An installation path, endpoint, or launch configuration is machine state.
Workspace trial JSON can advertise a trial, but cannot silently authorize
launching an executable or executing a command.

### Connected session

The session owns verified peer identity, application and connector versions,
the selected protocol version, capability intersection, runtime document
handles, native window handles, operation state, and a fresh generation.
None of these handles is durable workbook data.

Capabilities are independent: a working surface does not imply capture,
command access, view restoration, or source write permission. Optional groups
can be absent without preventing a useful connection.

### Workbook reference

Store stable pack identity, the relative-first source locator, authored frame
and binding configuration, an optional versioned view snapshot, and a linked
poster with provenance. Installation health, endpoints, PIDs, HWNDs, session
IDs, credentials, and launch instructions are excluded.

The workbook remains openable without the connector. Missing or incompatible
connections show the last poster and source pointer with a specific status.
Packaging remains an explicit fork under Article IX.4.

## Connection and lifecycle

The proposed baseline is a local duplex Windows named pipe. Slate creates an
instance-specific endpoint in its per-user machine state; an explicitly
enabled application plugin attaches to it. The endpoint rejects remote peers,
uses current-user access control, and verifies the operating-system peer
process. A claimed pack ID alone is not authentication or execution consent.
Rendezvous state is machine-private and excluded from packaging and export.

The proposed framing is a four-byte unsigned little-endian payload length
followed by UTF-8 JSON, capped at 4 MiB per frame. Binary captures travel by
negotiated bounded transfer references, not base64 inside control frames.
Both sender and receiver enforce limits before allocating or reading payloads.
These limits and framing become part of the first frozen schema and fixtures.

Negotiation exchanges protocol family, supported versions, pack/application
identity, and offered capability groups. Slate returns the compatible version,
allowed intersection, session identity, and generation. No shared version
produces an explicit incompatibility result. A connection grants no document
access until the user binds a source.

After negotiation, every request and result identifies its session and
generation. Requests have unique IDs and an expected source revision where
applicable. Results report success, failure, or an uncertain outcome. Unknown
optional fields are tolerated; unknown required capabilities and operations
are refused. The first schema must define exact envelopes and error codes.

Attach, detach, hide, focus, and application shutdown are distinct acts.
Detaching returns a hosted window to its application and preserves unsaved
work. Slate must not terminate a pre-existing Rhino or Excel process merely
because its card closes. A connector declares whether Slate launched a
dedicated process or attached to an existing one. Close requests can report
dirty state and refuse closure; saving or discarding is never implicit.

Reconnect creates a fresh generation and invalidates old runtime handles.
Delayed replies cannot update a replacement binding or another tab. A lost
reply after a possible mutation means outcome unknown. Never replay such an
operation automatically. Request IDs are deduplicated for a live session;
restart recovery must query status or require an explicit fresh user action.
Cancellation acknowledges a request to stop; it must not claim rollback of
effects already committed by the external application.

All transport, probes, bulk transfers, and waits run off the frame loop.
Inbound work is bounded per frame and rejects stale generations. Unfocused
or hidden surfaces must not capture board input.

## Negotiated capability groups

The names below describe proposed groups, not shipped method identifiers.
The first schema will assign exact request and result shapes.

- **Sources:** bind an explicitly chosen source, report resolved identity,
  runtime document identity, revisions, dirty state, and link health. Opening
  an existing document and creating an isolated working copy are distinct.
- **Surfaces:** offer an embedded native window, companion window, or headless
  connection. Describe the supported mode, geometry, DPI, visibility,
  focus/blur, detach recovery, and failure behavior. Full application UI and
  viewport-only hosting are different advertised capabilities.
- **Commands:** describe and invoke namespaced domain operations with typed
  parameters/results, availability, authority, mutation effects, cancellation,
  and native undo behavior. Generic script evaluation is not a v0 command.
- **Views:** snapshot and restore versioned view state against a specified
  source. Provider-specific state remains namespaced and versioned. Shared
  schemas are introduced only for concepts that genuinely travel across apps.
- **Capture:** return still pixels with source, revision, view state, time,
  dimensions, and fidelity. Video is a later optional extension of the same
  shared capture lifecycle.
- **Publication:** identify portable outputs, their media types, source
  origins, and export behavior. No capture or export silently edits an original.

The first implementation needs lifecycle plus one advertised surface mode.
Command discovery, one read-only command, capture, and view restoration then
exercise the independent seams. Surface-only connectors remain valid.

## Authority for humans and agents

Article V.3 makes an external application a host with its own editing and undo
authority. Slate journals placement, binding, authored view changes, and
accepted captures. A native edit receipt is not a reversible `SceneCmd` and
must not appear as though Slate undo will reverse the foreign edit.

Command descriptors distinguish read-only inspection, ephemeral view changes,
proposals for Slate content, edits to isolated draft material, and source
write-back. Both Slate's dispatcher and the connector enforce the negotiated
authority; a descriptor flag or prompt is insufficient. Direct human use of
the foreign interface follows that application's document controls.

Article IX.5 makes canvas-to-source write-back human-only and explicit for a
specific file and action. An agent can propose a named-view transfer; it cannot
perform source write-back, even under general workspace autonomy. Accepting an
agent proposal must not silently confer that power. The human invokes the
specific source action with its target visible. The per-action human direction
and specific target are journaled through the declared command journal. The
foreign application retains ownership of the resulting edit and its undo;
recording the direction or receipt does not imply Slate undo reverses that edit.
The first implementation should expose read-only agent commands until draft
editing and attributed foreign history have an enforceable contract.

Article VIII.1 still governs agent context. Installing a connector or exposing
a command catalog does not automatically append its documents, selection, or
viewport to agent messages. Context reaches agents through explicit user wires.

## View continuity and capture

A view snapshot identifies its source, source revision, schema and version,
and data sufficient for the declared restoration fidelity. A portable 3D
schema must specify coordinate axes, handedness, units, camera position,
target/up orientation, projection, field of view or orthographic scale,
aspect/crop, and supported clipping/display state. These fields must be
derived from the current camera owner, not separately reimplemented.

A native state extension is data, never executable content. A connector may
restore its own native state exactly while restoring a shared schema only
approximately. Unsupported fields and source/revision mismatches are reported;
the adapter must not silently substitute a different view. Revisions are
opaque identities, not universally ordered numbers. Cloud placeholders are
never bulk-read or hydrated to invent a hash or revision.

Captured pixels and view metadata refer to the same source/view revision. If
the connector cannot provide that consistency, it states the weaker fidelity.
Slate publishes through the existing capture helper and journals a board add
as one attributed action. Capture source identity remains tied to the initiating
tab and binding while background work completes.

Host export defaults to a frozen poster and source pointer under Article V.3.
Portable data supplied by an adapter is a separate declared output with its
own provenance; it does not make native Rhino UI executable in exported HTML.

## Implementation sequence

1. Review this boundary and record decisions on transport, authority, view
   fidelity, lifecycle, and the shared runtime owner. Specify any new portal
   interaction through the existing tool-contract workflow before UI changes.
2. Publish renderer-free wire types, exact schemas, version/error behavior,
   and fixtures. Extend optional `Program` manifest discovery compatibly.
3. Build a minimal generic mock connector and an IPC conformance harness.
   Test negotiation, source scoping, deduplication, uncertain results, stale
   generations, cancellation, and authority before embedding a real program.
4. Extract the shared native host lifecycle and exercise attach, focus,
   geometry, hide/show, detach, and crash recovery through existing portal
   chrome. Preserve current Enscape behavior and avoid vendor-specific hosts.
5. Add shared capture and versioned view interchange. Verify missing-connector
   workbook behavior and poster/pointer export in both interpreters.
6. Freeze a tested protocol version and publish the developer handoff. The
   independent Rhino team then builds its plugin against those artifacts.

Conformance must cover: old manifests; incompatible versions; optional missing
capabilities; two application identities without vendor branches; wrong-source
or stale-revision requests; old-session replies; duplicate command IDs; denied
agent write-back; dirty-document detach; connector/Slate loss recovery; capture
and metadata consistency; unsupported view schemas; missing-pack export.

Completion means an independent connector can pass the public harness without
importing Slate app code, and every supported path has retained evidence.

## Governing references

- [Constitution](../../CONSTITUTION.md): I, III, V.3, VI, VII.8, VIII.1, IX, XII.
- [Existing pack registry brief](../requests/2026-10-10/pack-registry.md): phase 4.
- [Facet taxonomy](../facet-taxonomy.md): shared media capabilities.
- [Portal patterns](../keymap/contracts/PATTERNS.md): P1.portal and P2.PortalHost.
- [Existing capture contract](../keymap/contracts/portal-capture.md).
- [Pack manifest owner](../../crates/atlas-packs/src/manifest.rs).
- [Model view metadata owner](../../crates/model-preview/src/view_meta.rs).
