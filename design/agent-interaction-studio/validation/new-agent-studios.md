# New agent Studios — validation

30 October 2026. Browser preview at http://127.0.0.1:8765/. All work is plain HTML/CSS/JS; no Rust changes or builds in this iteration.

- Dashboard Builder: all metrics/chart/table views rendered; simulated run exposed current public step and Stop. Completed run staged a revision; Apply preview committed it to the sample. Canvas state survived tab changes and reload.
- Text Agent: keyboard-selected first paragraph from the editable document. The dock retained the exact quoted passage when focus moved. Sending proposed a range-only revision without changing the source. Replace highlighted text updated that paragraph. Chat train mode then created a second window and one inherited-history wire.
- Rhino 3D: rendered projected pavilion; Shaded/Wireframe and view cycling worked. A run showed the generated script incrementally; completion updated model revision. Reload retained model/camera/display and script output.
- Shared: all five tabs registered; new tabs use independent canvases. Debrief on Rhino opened the matching draft contract. Shared model capabilities and navigation remain covered by existing checks.
- Independent code review fixed history-off branches drawing a wire, branch-drop coordinates being recomputed after camera movement, stale selection highlights, and cut history resurfacing in future branches. Composer focus/selection and reading position now survive progress updates.
- Seven relevant Node test files passed: new session model, shared canvas navigation, Chat Train model/capabilities, image layout/history/grafting. Configuration modules passed syntax checks. Text range/staleness and Rhino configuration/tool behavior also passed isolated module checks during authoring.

Captures: dashboard-builder-first-pass.png, text-agent-first-pass.png, rhino-agent-first-pass.png. New layouts remain drafts for owner refinement; these checks do not certify native pixel parity or live API behavior.


## Native-derived correction — supersedes first-pass layout

Inspected native artifact editor, generator header/body/history/activity, property-strip and attached-apron owners, text-note renderer and model display/connector sources. Compared the saved native image capture with the reviewed Images layout. There was no running Slate window; no native application was built or changed.

Replaced invented wrappers with the shared ImageStudioLayout geometry. Browser verified: Dashboard has only Fill/Stroke/Agent/Capture and model/ellipsis/revision/Play header; Settings reveals native Fork/Fresh eyes/Inspect memory/Open source, adding Apply/Discard only for pending output. A simulated run reached pending state and Apply worked from Settings. Bare Text retains exact highlighted-word capture and its history checkbox remains open after selection. Rhino exposes the five native display modes; Arctic and Shaded selections worked. Unselected body drag moved the artifact exactly 30 by 20 units and was reversed to its original position. Seven focused suites remain green; the layout suite now also checks 3-, 4- and 10-tool native strips and detached fallback. JavaScript syntax checks pass.

Current review captures: dashboard-native-derived-v2.png, dashboard-native-blister-v2.png, text-native-derived-v2.png, rhino-native-derived-v2.png. Earlier first-pass layouts are rejected references, not current specs. Full source mapping: research/native-agent-chrome.md.
