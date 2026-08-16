# status-board

Pure parse + layout for Slate's **Status Board** portal (Constitution Art. I).
No renderer, no `egui`, no document types beyond the snapshot schema.

A status snapshot (`project-state.json`) plus a journaled query and a frame
size produce a [`StatusLayout`] of rectangles, bars, and labels. The board
painter and `slate-artifact` are two interpreters of that one layout
(Art. IV). Contents are derived and never journaled (Art. V / VI.3).

## What this deliberately does not do

- Live git / CI polling
- Editing the snapshot
- Hallucinating progress the JSON does not contain
- Theme chrome (colors here are the instrument's own, shared by both interpreters)
