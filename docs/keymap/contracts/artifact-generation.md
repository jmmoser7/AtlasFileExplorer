# Artifact generation — interaction contract

Status: agreed
Family: portal
Reference: User request, 16 October 2026; image-workflow; portal-agent-link; portal-web-embed
Command: portal.agent.spawn / portal.agent.run / portal.agent.stop; portal.artifact.accept / reject / revision / fork / memory_disconnect / memory_inspect / source_open
Inherits: P0.* (all), P1.node, P1.wire, P1.image.frame, P1.generator, P1.shape.properties, P1.portal, P1.portal.local-ui, P2.PortalPlace, P2.PortalHost — deviations below.

This is an artifact-generation capability attached to the existing Web host,
not a new portal kind. The user has requested the fourth blank-wire choice,
image-generator presentation, Cursor/Codex harness execution, content and
removable memory wires, immutable evolution/forking, and source fidelity.
All 35 behavior rows were accepted by the user on 16 October 2026, with one offline self-contained HTML file per revision, source/data/assets preservation first, live controls as session state, and explicit Play/Single. Implementation proceeds under the DRY extract-first plan. This is not a shipped claim.

Review: [interactive matrix](C:/Users/jmoser/.cursor/projects/c-Users-jmoser-source-repos-AtlasFileExplorer/canvases/artifact-generation-tool-contract.canvas.tsx).

## Behavior matrix

| ID | Dimension | Agreed behavior / proposal | Source | Conf |
|----|-----------|---------------------------|--------|------|
| D01 | Initiation & arming | Pull an existing wire to blank canvas and choose Artifact, the fourth choice beside Text, Image and Agent. The artifact lives in the existing browser portal. | stated | 100 |
| D02 | Stickiness & repeat | Inherit P1.shape.properties.palette-return: selection reveals the attached generator; outside click dismisses it; quick return restores its editor and open dropdown through the shared recovery owner. Camera navigation retains it. Placement is one-shot; repeat never silently runs an agent. | stated | 100 |
| D03 | Gesture grammar | Artifact → prompt → Generate → browser result → downstream Artifact. A downstream spawn from an artifact creates an ordinary content wire and a separate memory trace automatically; deleting only the trace requests fresh eyes. | stated | 100 |
| D04 | Click vs drag rule | Inherit P1.wire and image-workflow D03/D04: existing drag threshold, retained blank-wire choice after release, no node before a choice. Browser contents receive input only after Enter/double-click. | precedent | 92 |
| D05 | Modifiers | No new modifier or keyboard binding. Existing wire selection, Delete, undo/redo, camera navigation and browser keys apply in their current contexts. | pattern | 88 |
| D06 | Constraints & snapping | Inherit existing agent_spawn_rect / resolve_point_snap placement and P2.PortalPlace. Preview and commit use the same resolved position. Artifact layout is page-owned, not a new board snap system. | pattern | 90 |
| D07 | Direction / value locks | n/a for generation. Existing board placement locks remain inherited; no generator lock icon (image-workflow D07). | precedent | 90 |
| D08 | Numeric / manual entry | One artifact per explicit Play, prompt text, installed harness/model dropdown and Settings. Speed and thought level appear only with provider-advertised choices (P1.generator.model-controls); each request captures and validates them. Default is Single. Image count/seed controls remain capability-specific. | stated | 100 |
| D09 | Preview & readouts | Inherit P1.generator and the shared style guide for compact prompts, flowing public activity, chat-train expansion/follow-scroll, approval and deselected busy-perimeter feedback. The browser result must preserve fidelity to the explicitly wired reference. | stated | 100 |
| D10 | Cursor | Inherit P1.wire on board connections and the existing Web host cursor when contents-focused. Generator controls use shared shell cursors. | pattern | 90 |
| D11 | Commit | Creating the destination and its explicit wires is one undo step. Completed output is validated and frozen, then its binding is staged, visible and rejectable under the existing workspace policy. Acceptance (or a prior autonomy grant) makes one attributed journal commit. Ancestors never change. | pattern | 88 |
| D12 | Cancel | Esc peels the current menu/draft, maximize and contents-focus layers in existing order. Stop cancels the run. Failed/cancelled/stale output never publishes a revision or replaces a completed source. Pending wire choice leaves no orphan. | precedent | 92 |
| D13 | Selected presentation | Inherit the current Web portal and shared rectangular selection policy, not the Agent PortalNode squircle exception. The generator apron uses shared capability-driven chrome and canvas-space scaling. Memory follows the persistent chat train's top rail; content keeps the ordinary midpoint port. | precedent | 90 |
| D14 | Post-edit | Every completed dashboard preserves its revision so the user can inspect evolution and fork from an earlier phase. A content link supplies that old artifact to the next generation; source and memory ancestry remain independent. | stated | 100 |
| D15 | Non-goals | Proposed first slice: local self-contained HTML dashboards; no package installation, development server, deployment or collaboration feature. No provider migration promise, opaque provider-reasoning replay, or general browser process snapshot. | guess | 55 |
| D16 | Create-style inheritance | Inherit portal-web-embed D16 and image-workflow: neutral ungenerated content, ordinary portal styling, no invented preview. Reference look and feel guide the generated page; board shape defaults do not become dashboard CSS. | precedent | 90 |
| D17 | Hit-testing & pick | Inherit P1.node, P1.wire and P1.portal.contents-focus. Trace selection/deletion is separate from the content connector. Deleting a trace never invokes chat-history pruning or deletes a dashboard. | pattern | 90 |
| D18 | Portal class & authority | Host, reusing PortalKind::Web and WebHost. Slate journals frame, generation binding, explicit wires and revision pointers. Page-owned code/data live in linked external source history; page JavaScript has no Slate command channel. | precedent | 95 |
| D19 | Source binding | Proposed first slice: each revision is one self-contained index.html with embedded CSS/JS/data. A source locator plus immutable revision ID pins the result. The version owner stores exact contents/deltas outside .slate; materialized revision files are browser inputs. Relative-first source resolution remains authoritative. | guess | 55 |
| D20 | Query & parameters | Harness/model and advertised model parameters are authored on the artifact and captured immutably at Play, with prompt, explicit content inputs and trace boundary. Browser viewport settings stay with WebPortalRef. Only authored content wires supply reference pixels/source revisions. | stated | 100 |
| D21 | Regeneration & staleness | Each explicit Play snapshots the input graph, model choices and parent revision. Work is tab/run-tagged; late results cannot bind an unrelated card. Completed revisions never follow a mutable source. | stated | 100 |
| D22 | Contents interaction | Inherit the Web portal: click selects the frame; Enter or double-click interacts with the page; Esc or outside click returns to the board. Generator controls remain available without forwarding their clicks into the page. | precedent | 95 |
| D23 | Level of detail | Inherit existing Web portal pool, poster and capture-tier policy. History chips and generator chrome follow shared canvas-space/LOD owners; no extra browser host or uncapped live previews. | precedent | 95 |
| D24 | Export serialization | Inherit portal-web-embed D24: package the exact selected local revision as an interactive iframe with provenance. Preserve its data and required assets. Exclude harness sessions, secret stores and browser profiles; no silent replacement with latest source. | precedent | 95 |
| D25 | Bake | Inherit existing portal Capture/Bake commands for a still/video when explicitly requested. Baking is separate from freezing an artifact source revision; it does not discard the interactive revision or its ancestry. | precedent | 90 |
| D26 | Collaboration & per-peer | Inherit P1.portal and agent contract: scene bindings/trace identities are authored; live provider processes and permissions remain local. A peer missing the external source sees named Unknown/Missing. No new collaboration transport. | pattern | 88 |
| D27 | Agent surface | Installed Cursor or Codex sidecars build the artifact asynchronously. Both consume one provider-neutral artifact profile and immutable explicit input snapshot. Existing command registration and permissions apply; no vendor-specific document model. | stated | 100 |
| D28 | Determinism & provenance | Generated apps are not claimed deterministic. Record exact source hash/revision, content parents, originating prompt, harness/model and conversation checkpoint. Mainline uses one session; earlier forks use a separate workspace seeded only from the chosen revision, replaying its complete bounded ancestor transcript when native fork is unavailable. Label replay honestly; incomplete checkpoints refuse. | precedent | 90 |
| D29 | Performance envelope | Reuse bounded background generator/input-prep/version workers, request-scoped public activity and existing Web capture budgets. Source reconstruction, hashing, validation and protocol work stay off-thread; paint never traverses projects or sources. No unbounded activity/history/browser previews. | stated | 100 |
| D30 | Failure & honesty states | Inherit P1.generator.feedback. Name unconfigured harness, unavailable model, permission denial, missing reference/revision, unreadable/cloud-only input, invalid output, cancellation and unsupported fork/checkpoint. Preserve the last completed view. Never replace an unavailable provider or invent chart values silently. | stated | 100 |
| D31 | View-state ownership | Proposed first slice: preserve exact source, embedded data/assets and revision ancestry. Browser scroll, focus and live filters/control values remain page-owned session state. Saving filter/control checkpoints would add an explicit typed state schema; it must not copy cookies, browser profiles or arbitrary JS memory. | guess | 45 |
| D32 | Trust, sandbox & consent | Inherit P1.generator.dashboard-access: one machine-local dashboard Full access default with explicit red-text indication; session denial, ReadOnly and separate relay authority remain enforced. Changes apply next turn, with idle Cursor restart when needed. Harness editing never auto-Applies a revision or grants Web, OS or build consent. The isolated Web boundary and staged descendant outputs remain; removing Memory gives future runs a fresh session. | stated | 100 |
| D33 | Portal chrome | Inherit shared generator chrome and the chat-train expansion/follow-scroll owner. Expand retained public commentary/tool summaries, never private reasoning or raw tool payloads. Bounded activity is separate from replayable checkpoints and Memory. Memory rails remain deletable authored edges; inspection/pruning keeps human-pinned rules separate from agent-learned history. | stated | 100 |
| D34 | Portal maximize | Inherit P1.portal.maximize and the Web host: maximize fills the window, uses the existing shared fit, and Esc restores. Generator apron geometry remains attached through the existing source-plus-apron fitting owner. | precedent | 95 |
| D35 | Portal-local UI | Artifact prompt, model choices, versions/forks and Memory inspection stay on the artifact. The user-requested Full access default is global to dashboard builders and machine-local. The bundled dashboard profile preserves reference composition, colors, typography, labels and proportions, verified at the source viewport. Unreadable values remain estimates or require clarification. | stated | 100 |

## Two independent connections

**Content** is an ordinary midpoint input. It carries only explicitly wired
material: the selected immutable artifact revision, its source/data, and any
separately wired reference image, text or table. A screenshot alone is
insufficient to iterate a real application. A new work area is seeded from
the referenced revision before the harness edits it. The content edge's
revision pin does not follow a later rerun.

**Memory** is a separately typed, journaled connector with train-rail
presentation. Its source identifies the provider-neutral conversation and
exclusive checkpoint captured for that generation. Mainline continuation
resumes one provider session when its current head matches that checkpoint.
This connector does not use ChatView.parent and does not make existing chat
rails disconnectable. Removing it leaves content and every completed
revision intact; the next run uses a new session with no ancestor replay.
An in-flight run retains its submitted snapshot; unlinking affects subsequent
runs. Undo restores the authored edge without running a provider.

An earlier-version fork never resumes a session that has later descendants.
Use atlas_agent::checkpoint with a complete ancestor prefix and a separate
branch workspace seeded only from the explicitly wired content revision.
Current adapters replay supported visible history; they do not clone hidden
reasoning/tool state. Surface that distinction as checkpoint replay.
A busy linear session is serialized; simultaneous branches get independent
sessions. Runtime association must not leak late output to another tab/card.

Source parents and memory parents are independent. Removing a content wire
does not erase conversation history; removing a memory wire does not remove
the referenced code. Copy/paste remaps authored connectors using the existing
graph authority and cannot silently grant trust in a copied provider session.

## Revision preservation

Extend atlas-ai::versions and atlas-agent::Versions as the sole history owner.
Current best-effort per-message capture is insufficient: it can skip output
changed before capture, flatten dependencies, and expose the mutable latest
file. Artifact publication must be strict and atomic.

Proposed source layout: one history per artifact family, immutable revision
IDs with content-parent IDs, exact source hash, entry, provenance and the
matching exclusive conversation checkpoint. The first slice uses embedded
HTML/CSS/JavaScript/data. Store a base source plus hash-verified reversible
byte deltas and periodic full checkpoints behind the existing version owner;
materialize exact source off-thread for the browser/export. This is Git-style
lineage and source reuse, not a promise of a new user-facing Git repository.
Delta encoding is a storage optimization: exact reconstructed bytes and
immutable revision identity are mandatory. Do not mistake the existing
added/removed line counts for reconstructable diffs.

Each request writes to an owned staging output directory. The runtime validates
the expected entry, containment and source format, freezes successful output,
then offers the resulting source binding through the existing staging policy.
Cancel, transport failure, incomplete output, read-only tab, stale generation
or rejected publication cannot change an accepted revision. Never run a
provider in the upstream snapshot directory, and never write back to a
user-linked ancestor. Cleanup may remove owned failed drafts only.

Version history is external linked material, not a source database inside
the workbook. The scene stores source/revision/trace references through named
invertible SceneCmds. Resolve locators relative-first. Unsaved work uses the
configured AI workspace's durable `.atlas-ai/artifact-sources` history; save/package uses the existing asset
lifecycle. Package the selected exact revision and origin, not conversation
records or the entire private history store.

The implemented store is `versions/artifacts/<family>/` within that history
directory: owned `.drafts/<run>/output/index.html`, immutable `revisions/`,
and verified disposable `materialized/<revision>/index.html` files. It stores
reversible byte deltas when smaller and a full checkpoint every eight
generations. Existing package/export workers verify the selected source before
copying it. A permanent package gets a revision-specific destination and an
explicit revision/hash pin; it can be verified without private conversations.
Save As rebases source locators and undo/redo snapshots without copying history.

**Agreed state boundary:** the proposed first slice preserves
code, embedded data and assets. It does not promise restoration of live
filter/control values, browser storage, navigation or a process heap. If
control checkpoints are requested now, add a versioned, bounded JSON schema
and a host-initiated read/bootstrap contract for declared page state. Treat
checkpointing as explicit authored data. Never copy cookies, credentials,
profiles or arbitrary browser storage; no page-to-Slate command bridge.

Wired live tables need the same boundary: a generation freezes the table
snapshot it used. Older artifact revisions use their frozen data on reopen
and export rather than silently reading a newer table. A future refresh is
an explicit new revision, not a mutation of an ancestor. The profile may
declare the existing one-way Slate Link input for staged development, but
publication must preserve the generation's data snapshot.

## Implementation reuse and extraction order

DRY review: **extract-first**, 16 October 2026. Implementation follows the
following seams; the gate does not approve a copied
portal, editor, endpoint resolver or second history store.

1. **Shared generator presentation — atlas-shell::selection_tools.**
   Extract common model/prompt/Play-Stop/progress/history controls from
   selection_tools/agent_image.rs. Keep image count, seed and image-only
   settings in its adapter. Shell receives renderer-facing data and actions;
   it must not import slate-doc. Existing image visuals/feel are the baseline,
   including the source-aligned apron, independent history row, bare controls,
   canvas scaling, outside-click dismissal and menu-wheel ownership.
2. **Strict history publisher — atlas-ai::versions.**
   Extend existing provider-neutral Versions records with a complete immutable
   artifact revision and parent/checkpoint metadata. Atomic capture, verified
   reconstruction, bounds and cloud guards precede the new consumer.
3. **Host/agent binding seam — existing board_agent binding owners.**
   Split provider binding from host locator/title/frame-size conversion in
   bind_program. Keep Web source and dimensions intact. Reuse
   connection::agent_link_dir / AgentPortalRef.bundle for the conversation;
   audit agent_folder_for, which currently reads the portal source.
4. **Inputs and memory — slate-doc / board_agent.**
   Extend agent_inputs::WireBinding, input_ports_of and source_kind once.
   Use source_end/target_end; editing board_flow must close DV-24's local
   endpoint-selection duplication. Reuse agent_chat::rail geometry and the
   common history-rail painter without reusing ChatView.parent semantics.
   Trace mutations/deletion use the ordinary scene journal.
5. **Thin app adapter — board_flow / board_agent / board_properties.**
   Replace AgentDraft.image boolean with modality/capability identity; add
   Artifact to SPAWN_CHOICES and the existing command path. Create an
   ordinary Web node carrying an agent binding and a frozen source reference.
   Adapt shared generator chrome and current bounded input/generation queues;
   preserve requesting tab/node/run identity throughout publication.
6. **One dashboard generation profile — atlas-agent::transport.**
   Add an explicit artifact request/profile to the provider-neutral envelope.
   Both sidecars consume the same canonical instruction resource. Replace
   artifact mode's current “Edit existing files where they are” guidance with
   revision-local editing; do not append conflicting instructions or create a
   second Rust/JavaScript guide twin. Ordinary chat behavior is unchanged.
7. **Browser and export — existing owners.**
   WebHost, board_portal source/focus/capture, shared portal chrome and
   slate-artifact consume the pinned materialized revision. Directory bundles
   (if chosen) bind a directory + entry; passing an HTML file with sibling
   dependencies to the current exporter would omit those dependencies.

Proposed command surface: extend portal.agent.spawn with artifact modality;
reuse run/cancel where capable. Register revision selection, fork, memory
inspect/disconnect and source-open actions in Slate SPECS and dispatch.
Ordinary Delete may call the same typed trace-delete command planner.
A card creation, its content wire and auto memory trace share one undo step.
No new key binding is proposed. Preserve closed DV-18/DV-20 owner paths;
DV-19 is outside this change.

## Dashboard profile draft

The shipped profile is owned once and sent to both installed harnesses:

- Work only from the user's prompt and explicit wired inputs. No ambient
  canvas selection, viewport, hidden board contents or future branch history.
- Seed the new owned work directory from the pinned content revision.
  Change only that staged descendant. Return one validated artifact entry
  through the existing return-manifest protocol; never edit an ancestor.
- Prefer the chosen self-contained offline HTML format. Embed CSS, small JS
  and generation-specific data. Do not install packages, fetch fonts, use
  remote chart/CDN libraries or start a dev server in the first slice.
- Preserve the reference's composition, palette, typography, spacing, chart
  proportions, label positions, legend and background. Make the requested
  interaction work while keeping the initial appearance faithful.
- For a pie-chart image, read visible values/labels and preserve their order,
  start angle and sector treatment. Keep unreadable/estimated values visibly
  identified; never present guessed data as exact extraction. Do not add
  decorative dashboard panels unrelated to the reference.
- Compare the rendered result to the wired reference at its viewport size,
  then test hover, legend/filter behavior, resize and keyboard interaction.
  Report unperformed visual verification honestly.
- Keep learned iteration notes readable and prunable. Human-pinned
  requirements are not agent-editable. Memory disconnection removes
  inherited conversation context while preserving explicit source inputs.
- The page owns its JavaScript in the existing isolated browser. It cannot
  invoke Slate commands, host objects, secret access or filesystem write-back.

This profile is a versioned instruction recipe, not a new control-flow language.
No constitutional amendment is needed: portal-web-embed's ratified
constitutional reading permits isolated page JavaScript and prohibits a
page-to-Slate automation channel (Article VII.4).

## Verification, 16 October 2026

Acceptance is **failed / reopened**. Earlier release compilation, launch,
29 sidecar tests and a separate installed-Codex/Chrome smoke did not establish
Slate's end-to-end flow. Native re-evaluation of the pre-fix build reproduced
the faint OS error 1, no observable Play transition, zoom-induced prompt
rewrapping at a fixed host width, and excess space below the datum.
Shared-owner fixes and regression tests are in progress; patched native
acceptance, including approval actions, is pending. Test logs live under
`target/artifact-*.log` and `target/dashboard-*.log`; execution evidence must
distinguish test-binary OS error 5 from the submission error. The contract
remains agreed, not shipped.

## Feel constants

Inherit existing drag threshold, wire ports, train-rail routing, generator
reveal/fade, history-chip motion, attached-apron proportions, typography,
shared menu and Web capture/pool budgets. Do not mint second local copies.
Editor/dropdown recovery inherits P1.shape.properties.palette-return.
Any new artifact-only timing/bounds token needs a named owner before code.

Source reconstruction, validation, source image preparation, hashing,
checkpoint loading and sidecar protocol work are background tasks.
No per-frame source reads or unbounded history/browser previews.

## Golden paths

1. **GP1 — fourth choice:** drag a reference-image wire to blank board;
   release; move into the retained menu; choose Artifact. One ungenerated
   Web artifact and the content wire appear together. No agent runs.
   Esc before choosing leaves no node; undo removes the whole spawn.
2. **GP2 — pie from image:** explicitly wire the chart image and prompt,
   choose an installed capable harness/model, Play. The immutable request
   contains those references only. The staged browser output matches the
   visible palette/layout/labels; hover reveals values and a legend action
   works. Ambiguous source values remain identified. Accept → one revision.
3. **GP3 — downstream generation:** from that artifact spawn another
   Artifact. One undoable operation creates the descendant, pinned content
   wire and memory trace. Run a feedback tweak. The mainline resumes the
   same session; the upstream source hash/pixels/data stay fixed.
4. **GP4 — fresh eyes:** select/delete only the memory trace. Content and
   both dashboards remain. Next Play starts a fresh provider session without
   replay. Undo/redo restores/removes the connector without inference and
   without deleting provider history.
5. **GP5 — earlier fork:** complete r1 → r2 → r3; fork at r1. The seed bytes
   equal r1 and the conversation prefix ends at r1's checkpoint. No r2/r3
   prompt/file is automatically supplied. The branch uses a separate work
   area/session. Incomplete checkpoint fails honestly; original chain stays.
6. **GP6 — independent lineage:** disconnect the content wire, retaining
   memory; then restore content and disconnect memory. Input snapshots and
   session choices follow the two edges independently. Neither gesture
   prunes the other connection or a completed revision.
7. **GP7 — cancellation/staleness:** stop a run, delete its node, switch
   tabs, make the tab read-only, or issue a replacement run while output is
   arriving. Late results cannot publish to the wrong target. Failed output
   never overwrites an accepted revision; the previous preview stays.
8. **GP8 — persistence/remap:** save/reopen, copy/paste a chain, undo/redo
   trace deletion and source acceptance. Revision pointers remain exact;
   remapped trace endpoints remain valid. No copied workbook imports
   permissions, credentials or a silently trusted live provider process.
9. **GP9 — export:** export an older local revision while a later one exists.
   Its packaged interactive iframe reproduces its exact source/data offline.
   No private history, credentials, secret store or browser profile is copied.
10. **GP10 — retained image UX:** compare image and artifact generator
    chrome using image-workflow GP10, including focused/overflowing prompts,
    continuous zoom and optional rows. Select every Settings action with and
    without its prerequisite; Play immediately reports state or an actionable
    error; public updates expand and follow scrolling independently of Memory.
    Click away during a run: the host pulses; quick return restores its editor
    and dropdown, with expiry/explicit-close behavior shared with images. Pending
    approval stays readable. Verify actual catalog speed/thought choices and
    next-turn Full access across two dashboards, including explicit denial,
    ReadOnly and relay restrictions; output still requires Apply. Image-only
    controls remain on images. Web focus/scroll/escape and capture still work.
11. **GP11 — external/cloud input:** unavailable/dehydrated source gives a
    named state and never hydrates bytes. A missing harness/model or denied
    command keeps prior content and states the actual failure.
12. **GP12 — storage integrity:** reconstruct several revisions from base
    and deltas; hashes match exact original bytes. Corrupt/missing deltas,
    traversal paths and oversized outputs refuse atomic publication.
13. **GP13 — data evolution:** wire a table, generate r1, change the table,
    generate r2. Reopen/export r1: its frozen data still matches r1. Forking
    r1 seeds r1 data; explicit new input is the only way to refresh it.

Every golden path becomes a focused headless/model test as appropriate after
contract acceptance; visual fidelity and real WebView/harness behavior also
need retained live evidence. No shipped status before build, registry and
golden-path verification succeed.

## Open questions

None. User accepted the recommended choices on 16 October 2026.
