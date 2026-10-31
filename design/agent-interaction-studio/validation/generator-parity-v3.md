# Generator parity v0.3 — validation

30 October 2026. Plain HTML/CSS/JS only; no Rust edits or builds.

## Shared ownership

Images and all three newer generators call StudioGeneratorUI for the dialogue/composer, option count and compact model label. They reuse ImageStudioLayout for frame geometry, ImageGraft for input matching/capsules, ImageMenuStudio for input recipes, and the existing model picker/capability catalogue and Slate navigation.

## Browser observations

- Dashboard: a two-option request increased revision circles from 2 to 4. With two style sources connected and grafted, another two-option request increased them from 4 to 8: two input jobs × two alternatives.
- Both input sources connected by dragging their output handle onto the Style input. Enabling Graft inputs through the shared Right-click Menu Studio produced the same 16-unit two-wire capsule as Images. Both wires and graft state survived reload. Source cards retained 220 × 160 dimensions after correcting a legacy sizing migration.
- Dashboard and Text history checkboxes stayed visible and checked after clicking. Outside click closed the action menu while leaving the underlying agent open. Rhino action menu uses the same single history option; its model picker exposes Provider / Model / Effort / Speed and supports Ultra fast for the explicitly capable Sol fixture.
- Text and Rhino input-port right-click opened the same Graft inputs control as Dashboard and Images. Text's existing selected passage remained visible above the shared conversation.
- Output drag from a preserved Dashboard created a blank unselected child with no open agent. Opening that child showed the parent's dialogue. Selecting the exposed history-wire segment and pressing Delete removed the history connection and retained all three ordinary data connections.
- Running preview showed one header spinner and public stage content, with no separate Complete/activity row. The compact composer remains in the same conversation body. Full provider/host execution remains simulated.
- Images still loads the existing small, medium, large, connected and series fixtures using the shared component. Its normal provider has no imaginary effort/speed controls.

## Automated checks

All seven focused suites pass: canvas navigation; Chat Train model and capabilities; element sessions and independent options; image layout, history and graft matching. The session suite now checks blank closed downstream creation, independent result histories, and clearing dangling graft flags on new children. Capability tests check explicit Ultra fast availability and unsupported-model rejection. JavaScript syntax checks and git diff whitespace checks pass.

## Limits

This verifies the prototype interaction contract, not a native pixel match or production API integration. Real provider scheduling, credentials, global permissions, Rhino execution, native undo and exports remain later work. Model names/speeds are declared Studio fixtures. Existing image samples and user-authored menus were retained.
