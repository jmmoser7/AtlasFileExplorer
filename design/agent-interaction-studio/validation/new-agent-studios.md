# New agent Studios — validation

30 October 2026. Browser preview at http://127.0.0.1:8765/. All work is plain HTML/CSS/JS; no Rust changes or builds in this iteration.

- Dashboard Builder: all metrics/chart/table views rendered; simulated run exposed current public step and Stop. Completed run staged a revision; Apply preview committed it to the sample. Canvas state survived tab changes and reload.
- Text Agent: keyboard-selected first paragraph from the editable document. The dock retained the exact quoted passage when focus moved. Sending proposed a range-only revision without changing the source. Replace highlighted text updated that paragraph. Chat train mode then created a second window and one inherited-history wire.
- Rhino 3D: rendered projected pavilion; Shaded/Wireframe and view cycling worked. A run showed the generated script incrementally; completion updated model revision. Reload retained model/camera/display and script output.
- Shared: all five tabs registered; new tabs use independent canvases. Debrief on Rhino opened the matching draft contract. Shared model capabilities and navigation remain covered by existing checks.
- Independent code review fixed history-off branches drawing a wire, branch-drop coordinates being recomputed after camera movement, stale selection highlights, and cut history resurfacing in future branches. Composer focus/selection and reading position now survive progress updates.
- Seven relevant Node test files passed: new session model, shared canvas navigation, Chat Train model/capabilities, image layout/history/grafting. Configuration modules passed syntax checks. Text range/staleness and Rhino configuration/tool behavior also passed isolated module checks during authoring.

Captures: dashboard-builder-first-pass.png, text-agent-first-pass.png, rhino-agent-first-pass.png. New layouts remain drafts for owner refinement; these checks do not certify native pixel parity or live API behavior.
