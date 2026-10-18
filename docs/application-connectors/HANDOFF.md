# Independent application connector project handoff

Status: **Candidate implementation available; release gate pending**, 16 October
2026. The [v0.1 foundation](v0.1/README.md) contains the developer contract, pure
protocol owner, local runtime and mock harness. Windows execution auditing has
blocked validation. Start against a published tested version when the readiness
conditions below are satisfied; do not treat this candidate as frozen.

## Purpose and project split

Build a plugin in a separate Rhino project that lets an installed Rhino
application participate in Slate's external-application protocol. The plugin
owns translation to Rhino APIs and the behavior of the Rhino side. Slate owns
the generic protocol, discovery, canvas host, shared capture/publication,
command policy, and conformance harness.

The architecture direction is [PROPOSAL.md](PROPOSAL.md). Exact wire types and
generated schemas belong to `atlas-connector`; the [candidate guide](v0.1/README.md)
defines framing and transport interoperability. This handoff does not define a
second wire format. Rhino is the first intended implementation; the public
contract must remain usable by other applications and languages.

## Deliverables required from the Slate team

- A published protocol version with lifecycle, framing, peer verification,
  message schemas, errors, capability negotiation, and compatibility rules.
- An optional `Program` pack descriptor and a working machine-registration
  example, preserving existing manifests and provider protocols.
- A public mock connector, test runner, and message fixtures covering all
  capabilities required for the first integration.
- A generic host that declares embedded versus companion-window support,
  preserves foreign document ownership, and returns a useful poster on loss.
- Published source/view/capture schemas with supported fidelity and revision
  behavior, plus the command authority policy for human and agent callers.
- Retained Windows evidence for native hosting and cross-platform evidence
  for pure protocol types, fixtures, and validation.

Before these exist, the Rhino team can investigate its plugin lifecycle,
window ownership, camera APIs, and document commands. It should not invent
Slate endpoints, persist process handles in workbooks, or fork a provisional
message shape as its own protocol.

## Deliverables owned by the Rhino team

The plugin discovers an explicitly enabled Slate instance, connects through
the published local transport, negotiates capabilities, and binds only a
source the user chose. It maps stable source identity to Rhino's runtime
document and revision identities without silently opening a different file.

It advertises whether it can supply the actual Rhino application interface,
an interactive viewport, or a companion window. It owns application-side
focus, geometry, DPI, native dialogs, hide/show, and restoration of normal
window ownership on detach. Attaching to an existing Rhino process does not
give Slate permission to terminate that process or discard its documents.

It implements the command descriptors and operations it declares. Initial
examples are document inspection, view inspection/restoration, still capture,
and a human-invoked named-view transfer. It must enforce the caller authority
contract; a raw arbitrary-code execution endpoint is outside this scope.

It first maps a declared native view schema to Rhino cameras and named views,
including projection, units, axes, framing, and display limitations. A shared
portable 3D schema is a subsequent Slate-owned contract derived from its existing
camera model; v0.1 does not yet define one. Capture pixels and reported view/source
state remain consistent. Rhino's native undo and save lifecycle remain Rhino's
authority.

It provides application-specific install/probe information and a development
guide for its own repository. The plugin depends on Rhino APIs and the
published protocol; it does not import Slate application modules or require
Slate to vendor Rhino SDKs or binaries.

Rhino.Inside may be useful in that repository, but the agreed project is a
Rhino plugin. A helper-host architecture is a deliberate project decision,
not an implied requirement of the word sidecar.

## First joint acceptance path

1. Select a linked model in Slate and explicitly activate its Rhino connector.
2. Negotiate the published version and bind the selected source correctly.
3. Enter the declared Rhino surface, exercise native interaction, and return
   to Slate while keeping the document and its unsaved state intact.
4. Capture a still with restorable view provenance through Slate's shared path.
5. Restore that view in the same Rhino document. Report incompatible state
   or changed source explicitly rather than substituting another view.
6. Have the human invoke a named-view transfer to the specific document.
   Journal the per-action human direction and named target through the declared
   command journal. Verify that Rhino owns the resulting edit and undo; the
   direction or receipt must not imply Slate undo reverses the native edit.
   An agent may propose this transfer but cannot perform write-back to the
   linked original.
7. Disconnect, remove the connector, and reopen/export the workbook. The
   poster, source reference, and authored board content remain useful.

The test runner must also reject unsupported required capabilities, stale
session replies, wrong-source commands, duplicate mutation execution, and
agent source write-back. A surface profile offers `lifecycle`, `sources` and
`surfaces`; it can pass that narrower profile without claiming the complete
view-transfer acceptance path. Native embedding is cooperative; the plugin
must also declare a companion-window fallback if its full application window
cannot be safely reparented or focused.

## Coordination without shared implementation

Publish protocol releases and their fixtures from the Slate project. Record
which version the Rhino plugin implements. Resolve a missing general
capability in the Slate contract first; do not introduce an undocumented
Rhino-only message and later treat it as standard.

The user directs the independent Rhino project and its agent team. The
Slate-side project publishes the contract, fixtures, and handoff used by that
team. The two teams coordinate through versioned protocol releases and joint
acceptance evidence.
