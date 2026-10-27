# Declared Web controls — version 1

Status: agreed
Family: portal
Reference: User-requested reusable dashboard controls for the Rhino integration, 26 October 2026; implementation authorized, remaining native acceptance pending.
Command: `board.model.connector.invoke` through the existing declared native adapter; no new command or key binding.
Inherits: P0.*, P1.node, P1.portal, P2.PortalHost and portal-web-embed. Only the declared controls below extend portal-web-embed D15, D18, D27 and D32; the general messaging and script bridge remain prohibited.

This is the 26 October 2026 user-requested extension of the existing
[Web portal contract](portal-web-embed.md), especially D15, D18, D27 and D32.
It does not enable a general script or messaging bridge.

`atlas_agent::control` owns the serializable schema and fixed injected queue.
`board_web::table_links::dashboard_link_script` remains the only dashboard
injection owner. `board_web::control` adapts declarations to existing command
owners. `WebHost` only polls the fixed expression and returns typed results.
Web messages and host objects remain disabled.

A local HTML file declares `application/slate-control+json` data:

```json
{
  "version": 1,
  "controls": [{
    "id": "B01.floors",
    "input": "model",
    "command": "board.model.connector.invoke",
    "arguments": {
      "command_id": "org.slate.rhino.draft_building_floors",
      "parameters": {"building_id": "B01"}
    },
    "value": {"type": "integer", "path": ["parameters", "floors"], "min": 1, "max": 80}
  }]
}
```

The page calls `window.slateControl.set("B01.floors", 12)`. Its return value
is a monotonic sequence number, not a success result. The emitted intent has
only `version`, `control_id`, `sequence`, and `value`. A `slate-control` event
returns current readiness and recent replies. Only a `completed` reply with
the native operation's successful domain result confirms a change; partial
effects and uncertain outcomes must not be presented as success.

The manifest supports trigger, integer, finite number, boolean, and bounded
choice values. Parameter paths contain only object keys, at most four levels;
there are no expressions or template substitutions. The host limits a packet
to 32 KiB, a manifest to 32 controls, and a queue batch to eight intents. IDs
are bounded ASCII identifiers. Replay sequences are rejected within each
loaded document. The host polls at most eight times per second and admits
one native ticket per connected source.

An explicit semantic `InputKind::Control` wire identifies the target. It runs
from the model into the local Web portal's left midpoint. The wire is inferred
only after the runtime has validated a single-input local manifest. A bare
decorative wire does not authorize anything. Removing or rebinding the wire,
navigating, reloading, replacing the source, and leaving the isolated draft
invalidate pending publication. Source IDs, sessions, revisions, handles,
private addresses and caller authority are never accepted from the page.

The first adapter is the existing registered `board.model.connector.invoke`.
Its v1 native capability registry admits only `org.slate.rhino.buildings` and
`org.slate.rhino.draft_building_floors`, with matching advertised inspection or
isolated-draft mutation descriptors,
then uses the existing native ticket, protocol validation, bounded receipt and
reconciliation owners. It cannot invoke lifecycle, source-write-back or script
actions. Mutation also requires focused, recent direct human page input;
agent-owned page focus is not relabeled as human authority. Agents retain the
existing scoped BoardCall command route.

Adding a future domain means adding an adapter to the command owner and its
tests, not adding a new page transport or target-selection convention. Existing
table wires and `window.slateLink` are unchanged.

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Use the existing local Web portal, a validated version-1 declaration and an explicit semantic Control input wire; no new tool, binding or implicit target. | stated | 100 |
| D02 | Stickiness & repeat | The declaration remains available for the current page; each value change is a separate intent. Uncertain native effects are never automatically replayed. | pattern | 90 |
| D03 | Gesture grammar | The fixed queue progresses from declared and ready to admitted native ticket to terminal reply; a returned sequence or queued ticket does not imply native completion. | pattern | 90 |
| D04 | Click vs drag rule | Inherit portal-web-embed contents focus and page click/drag handling. A packet cannot assert its own human-input provenance. | pattern | 90 |
| D05 | Modifiers | Inherit portal-web-embed and P1.portal contents interaction; no additional Slate modifier grammar. | pattern | 90 |
| D06 | Constraints & snapping | Frame and wire edits inherit P1.node. Control values use their declared type and bounds; native parameters do not acquire canvas snapping. | pattern | 90 |
| D07 | Direction / value locks | No additional Slate value-lock mode; the declaration fixes the allowed parameter leaf and value domain. | pattern | 90 |
| D08 | Numeric / manual entry | Accept only the declared trigger, integer, finite number, boolean or bounded choice value; replace only its declared object-key parameter leaf. | pattern | 90 |
| D09 | Preview & readouts | Publish readiness and bounded typed replies. Only completed native domain success confirms a change; partial and uncertain effects remain explicit. | stated | 100 |
| D10 | Cursor | Inherit the existing Web page cursor and portal frame cursor; no second host pointer. | pattern | 90 |
| D11 | Commit | Frame and wire edits use the SceneJournal. Native effects use the existing command ticket, validated receipt and native authority owner; the page cannot invent an author or receipt. | pattern | 90 |
| D12 | Cancel | Inherit the portal cancel stack. Leaving or rebinding invalidates pending publication; an already admitted native effect is retained truthfully rather than claimed rolled back. | pattern | 90 |
| D13 | Selected presentation | Inherit portal-web-embed and P1.node selection chrome; controls do not create additional board objects. | pattern | 90 |
| D14 | Post-edit | Edit the linked page or its ordinary frame/wires through their existing owners. Withdrawing or changing a declaration invalidates its pending result. | pattern | 90 |
| D15 | Non-goals | No arbitrary command names, lifecycle actions, source write-back, scripts, general page messaging, host objects or page-supplied identities and handles. | stated | 100 |
| D16 | Create-style inheritance | Inherit portal-web-embed frame styling; page CSS stays page-owned and does not inherit board shape defaults. | pattern | 90 |
| D17 | Hit-testing & pick | Inherit the existing portal and page hit-testing/input owner; controls do not add a parallel board pick route. | pattern | 90 |
| D18 | Portal class & authority | This remains a host portal. Native mutation requires the existing isolated draft and focused recent direct human page input, without an agent focus lease; page data grants no authority. | stated | 100 |
| D19 | Source binding | Bind only the validated local page within its existing source root and its explicit Control input wire. A decorative wire or a page-provided source ID cannot select a native target. | stated | 100 |
| D20 | Query & parameters | The local declaration supplies a fixed registered command and bounded parameter leaf. Packets carry only version, control ID, sequence and value, never session or revision authority. | pattern | 90 |
| D21 | Regeneration & staleness | Navigation, reload, declaration withdrawal/change, wire removal/rebind and native source replacement invalidate pending publication through existing epochs and binding snapshots. | pattern | 90 |
| D22 | Contents interaction | Inherit Web contents focus. Native mutation additionally requires recent input recorded by the host page-input owner; agent-owned focus never becomes human authority. | stated | 100 |
| D23 | Level of detail | Inherit portal-web-embed WebView pooling, canvas scale and contents LOD; controls do not introduce a separate live view. | pattern | 90 |
| D24 | Export serialization | Inherit the existing local-page packaging/iframe and remote poster rules. Exported page controls acquire no Slate/native authority outside the running host. | pattern | 90 |
| D25 | Bake | No new native bake command; inherit the existing Web poster bake. Native modeling remains in the registered native command owner. | pattern | 90 |
| D26 | Collaboration & per-peer | Authored page locators and semantic wires follow the document. Native sessions, request tickets, input focus and control readiness remain per-peer runtime state. | pattern | 90 |
| D27 | Agent surface | Agents retain the existing scoped BoardCall native route. Declaring or editing a page does not allow an agent to obtain the direct-human control route. | stated | 100 |
| D28 | Determinism & provenance | Validate each declared control and monotonic sequence in its current document epoch; retain the actual native receipt and recheck the same source/wire binding before publishing. | pattern | 90 |
| D29 | Performance envelope | Packets are at most 32 KiB, manifests 32 controls, batches eight intents and object-key paths four levels; poll at most eight times per second with one native ticket per connected source. | pattern | 90 |
| D30 | Failure & honesty states | Missing local sources, invalid declarations, values, commands or bindings and revoked input authority fail closed. Partial and outcome-unknown native replies remain distinct from success. | pattern | 90 |
| D31 | View-state ownership | The ordinary frame and wire are authored document state; queue sequences, tickets, readiness, replies and recent-input timestamps are ephemeral runtime state. | pattern | 90 |
| D32 | Trust, sandbox & consent | Only the closed v1 local declaration adapter reaches registered native commands after all host checks. Existing Web messages, host objects and arbitrary script/transport bridges remain disabled. | stated | 100 |
| D33 | Portal chrome | Inherit portal-web-embed identity, fold and context actions; this extension adds no separate host chrome. | pattern | 90 |
| D34 | Portal maximize | Inherit the shared portal maximize/restore and cancel owner; page controls do not resize or rehost a native window. | pattern | 90 |
| D35 | Portal-local UI | Dashboard widgets stay inside their linked local page. No control-specific settings are added to Document Settings or another board-wide panel. | pattern | 90 |

## Golden paths

- GP1: Focus the valid local page with a Control wire to an isolated draft, then change one declared value. The existing native ticket returns its actual completed receipt. Covered by `declared_control_uses_same_human_ticket_and_native_receipt`.
- GP2: Use a decorative wire, navigate away or give an agent the page focus lease. Native mutation is refused. Covered by `decorative_wire_navigation_and_agent_focus_never_grant_control_authority`.
- GP3: Remove the wire or change the declaration while a ticket is pending. Its native effect remains audited, but the stale page result is not published. Covered by `lost_wire_suppresses_a_completed_native_result`, `changed_declaration_suppresses_an_in_flight_result` and `pump_observes_withdrawn_or_changed_declarations_before_completion`.

## Open questions

None for the declared v1 scope. Remaining live acceptance is tracked separately; this contract is not marked shipped by compilation.

## Implementation reuse

Owner: `atlas_agent::control` schema/queue; `board_web::table_links::dashboard_link_script` injection; `board_web::control` admission/publication; existing native ticket and receipt owner; existing P2.PortalHost focus, source and chrome.

Forbidden forks: no second page transport, target-selection convention, native worker, authority interpreter or WebView host.
