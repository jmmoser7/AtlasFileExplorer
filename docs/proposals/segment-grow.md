# Growing a highlight — proposal

Status: proposal plus one debug prototype. Nothing here is the default
highlight. The prototype runs only when `SLATE_SEGMENT_GROW=1` (or a test
sets the same flag). Hover, the tag, layers, and stickers are unchanged
when the flag is off.

The gesture the owner described: a slow drift grows the selection, a quick
retreat undoes that growth, and the zoom level sets the feature scale.

## Options

### 1. Kinematics on the current outline (prototype)

While the pointer stays inside the mask and moves slowly, scale the contours
out from their centroid. The step is `1 + 0.03 / zoom`, so a zoomed-in view
takes smaller steps (finer features) and a zoomed-out view takes larger ones.
A jump longer than a short threshold pops the previous mask. The stack is
derived hover state and is never journaled (Article VI).

- Cost: no model call, no extra memory, a few polygons per step.
- Risk: the outline leaves the object. It does not know an edge, so a drift
  across a boundary still grows. Retreat only undoes our own steps.

### 2. Re-prompt the local model along the drift

Keep one worker (Article II: inference stays off the UI thread). On a slow
drift, queue a new point nudged along the motion, and send a zoom-sized
window as the prompt scale: zoomed in, a tighter window so the decoder sees
smaller parts. A quick retreat drops the queued prompt and restores the last
accepted mask instead of waiting for the reply.

- Cost: another decoder pass per step (the embedding can stay cached), and
  the existing one-in-flight queue can lag the pointer.
- Risk: flicker when a late reply lands after a retreat, and a tight window
  can clip the object. Needs the SAM weights installed.

### 3. Multi-scale embeddings chosen by zoom

Encode the resident preview at two or three scales when the hover starts.
The camera zoom picks which embedding the decoder uses: close zoom uses the
finer grid. Drift still re-prompts, but on the scale the zoom selected.
Retreat pops masks as in option 1.

- Cost: two or three embeddings resident per image (memory and startup),
  still off the UI thread.
- Risk: the tiny SAM checkpoint is not trained as a feature pyramid, so the
  scales can disagree at their boundary. Highest machine cost of the three.

## Edge quality

- **Mask refinement on preview pixels.** After any option, snap the contour
  to a contrast edge in the resident preview (a short morphological pass).
  Cost is a CPU pass on the preview, not a new model. Risk is a halo on soft
  edges and on highlights that are already smaller than the preview.
- **Multi-scale prompts (option 3).** Better small parts when zoomed in,
  at the memory cost above. Refinement can sit on top of it; the two are
  not exclusive.

Option 1 is the prototype behind `SLATE_SEGMENT_GROW=1` so the feel can be
tried without committing the machine to another model pass.
