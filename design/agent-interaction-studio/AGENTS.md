# Studio design iteration

This is the primary workflow for iterating on interface designs in this project, directed by the owner on 30 October 2026.

1. Start from the actual existing element. Inspect its source, captures and contract. Preserve what the user has not asked to change.
2. Put one element in one Studio tab and one independent canvas. Chat Train stays first. Avoid broad multi-modality previews or control dashboards inside the element.
3. Keep Studio chrome outside the element. Make interactions concrete and reversible so the user can try them during the conversation.
4. Refine through the conversation. Apply explicit corrections without repeatedly asking for confirmation. Do not invent approval or repeat a completed survey.
5. Consult contracts/PATTERNS.md and related locked contracts before adding menus, configuration, toolbars or gestures. Reuse design and implementation owners; record requested deviations.
6. Keep the element's standalone debrief current using contracts/TEMPLATE.md and the native tool-contract vocabulary. Record retained baseline, agreed changes, behavior matrix, constants, golden paths, owners, verification and limits. Accepted rows go in contracts/decisions.json.
7. Lock only what the user approved. Chat Train v1 is agreed. Future tabs remain draft until reviewed; do not silently apply unfinished designs to production or rewrite another locked contract.
8. Test affected behavior and inspect the browser result. Retain conversations, drafts and notes. Identify simulated provider/credential interactions clearly.
9. Native implementation must reconcile these agreements with the native contract registry and shared source owners. This review folder does not authorize forks of production chrome, provider logic, secrets or commands.
10. Keep the Studio a fast, self-contained HTML/JavaScript/CSS prototype. Native code, measurements and screenshots are read-only reference material. Do not modify or build Rust, or require a native runtime service, just to preview a Studio iteration unless the user explicitly requests that approach.

Primary references: contracts/chat-train.md, contracts/PATTERNS.md, contracts/decisions.json, and the repository tool-contract skill/template. WORKLOG.md is history, not a competing current specification.

11. Every new canvas must follow Slate navigation (S08) and reuse canvas-navigation.js. Do not invent per-tab mouse or wheel rules. The live HTML, captured states and element contract together form the implementation specification; match them in Rust only when explicitly requested.
