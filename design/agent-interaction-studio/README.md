# Element design studio

The primary design iteration workflow is one element per tab, one concrete canvas, and a conversation with the user until behavior and presentation are agreed. Chat Train is the first tab and first locked reference. [AGENTS.md](AGENTS.md) records the workflow for future work.

## Open the Studio

Serve this folder with a static HTTP server and open http://127.0.0.1:8765/. The Studio must remain HTML/JavaScript/CSS. Native code and captures are references only; a native build is not part of the iteration workflow. No native renderer or compiled service is needed.

Chat Train is the first working review canvas. Images is the second tab and opens this iteration by default; `?tab=chat-train` opens the first tab. Dashboard Builder, Text Agent and Rhino 3D each have a separate working canvas. Open them directly with `?tab=dashboard-builder`, `?tab=text-agent` or `?tab=rhino-agent`. The plus button creates a named blank tab for a subsequent element. Debrief opens the current element's standalone contract, clearly distinguishing the image draft from the agreed Chat Train. Theme, native reference and review notes remain outside the canvas.

## Locked references

- [Chat Train contract](contracts/chat-train.md): preserved baseline, 23 accepted changes, 35 in-scope behavior dimensions, feel constants, golden paths, owners and verification limits.
- [Shared Studio patterns](contracts/PATTERNS.md): menus, agent configuration, conversation feedback, geometry, selection, header feedback and review-tab structure.
- [Contract template](contracts/TEMPLATE.md): format for subsequent element debriefs.
- [Approved decisions](contracts/decisions.json): machine-readable decisions and change IDs.
- [Native baseline](chat-train/BASELINE.md): original font, token and renderer provenance.

Contracts describe the current agreement. WORKLOG.md records iterations and superseded experiments. The original survey and broad prototype remain preserved but are not the active review surface.

## Chat Train

The canvas supports independent cards, single windows, pairs and individual messages, unsent continuation/forks, three fold states, width-driven fitting, inline agent configuration, transient scroll thumbs, drag-first selection, Stop, undo/redo and local persistence. Reset example restores three long pairs. The picker includes limited/no-capability cases and a long-name stress fixture.

Replies, model availability, access and schedules are simulations. API-key entry saves only sample status; key text is discarded. No provider or native production permission is changed by the Studio.

## Source and checks

Images is a self-contained HTML/CSS/SVG canvas with small, medium, large, connected-input and history-wire examples. Native source measurements, font and icon geometry are translated into `image-generator/layout.js`, `view.js` and `style.css`. `app.js` handles canvas gestures, per-instance state and history connections. The shared agent picker and image-specific capability catalogue are retained. Its [draft contract](contracts/image-generator.md) records the implementation and simulation boundaries. No provider is contacted.

- studio.js / studio.css: tab shell and debrief access.
- contract-viewer.html: readable rendering of standalone Markdown contracts.
- chat-train/model.js: canonical conversation state and reversible projections.
- chat-train/agent-catalog.js: capability validation and labels shared by requests and picker.
- chat-train/agent-picker.js: compact agent control.
- chat-train/app.js / style.css: rendering, direct manipulation and sample replies.
- chat-train/model.test.cjs and capabilities.test.cjs: focused state/capability checks.
- chat-train/check-ui.cjs: earlier browser regression harness; claim only results rerun for the current revision.

All tabs run without a build step or native runtime. `?reference=1` loads the Chat Train comparison transcript; `?sample=1` loads its long sample without reading or writing saved conversation state. The image tab uses local static assets only. Reload the page after editing HTML, JavaScript or CSS.

Shared navigation is defined in `canvas-navigation.js` and contract S08. The Images tab also covers image/tool selection, hover-only result strips and native image context-menu recipes. Mode and Settings live under the compact three-dot menu; custom aspect ratio is explicit.


## New element Studios — first pass

- [Dashboard Builder contract](contracts/dashboard-builder.md): select dashboard widgets, run staged revisions, Apply or Discard.
- [Text Agent contract](contracts/text-agent.md): highlight a passage, query it in the dialogue, review and apply only the selected replacement; continue in one window or a chat train.
- [Rhino 3D contract](contracts/rhino-agent.md): orbit the sample model, change viewport display, inspect simulated tool progress and generated Rhino Python.
- [Studio debrief](contracts/studio.md): shared runtime, persistence, validation and implementation boundaries.

All three are review drafts. They use the same provider/model/effort/speed picker and explicit API-key entry, dialogue controls, activity panel, history wires and Slate canvas navigation. Model and access preferences persist across the three new tabs; earlier locked canvases retain their existing setting scope. Replies and tools are deterministic previews. No live provider, file-write or Rhino call occurs.
