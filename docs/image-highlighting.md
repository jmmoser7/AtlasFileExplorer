# Local object highlighting

With Select active, rest over an object in a placed or generated picture for
400 ms. Slate previews its silhouette as a translucent orange highlight.
Move to the shared **Highlight** capsule beside the picture to keep it.
The action creates a separate paint layer; Undo removes the highlight.
The ordinary layer controls provide opacity and visibility. Escape dismisses
the offer until the pointer moves again. No region box or resize handles are
part of the segmentation preview.

## Local model setup

Run `scripts/setup-segmentation.ps1` once. It installs an isolated CPU runtime
and Meta's `facebook/sam2.1-hiera-tiny` model under
`%LOCALAPPDATA%/NativeFileAtlas/segmentation`. Python must be installed to run
the setup script. Dependencies and weights are machine resources, never
workbook content. Setup requires internet access; inference is offline.

The adapter uses Meta's official SAM 2 implementation:
<https://github.com/facebookresearch/sam2>.
It caches image embeddings in one resident worker process, so subsequent
points on the same image only run the prompt/mask decoder. CPU inference
uses at most four threads. First use includes model startup and encoding;
the capsule reads “Finding object...” while it runs. A failure appears as
“Highlight unavailable” with its explanation on hover.

## Ownership and limits

- `atlas-segment` owns the offline subprocess protocol and model adapter,
  isolated as a leaf capability (Constitution I.2).
- `slate-doc::image_paint` owns the visible image window and mask-to-path
  representation. The image's crop, rotation, fillet, trim and mirror apply.
- Slate's existing path interpreter paints previews; paint layers and the
  artifact writer serialize committed paths, including silhouette holes.
- The UI uses the shell's existing capsule and popup placement primitives.

Requests contain only already-resident preview pixels, downsampled to a
maximum edge of 768 pixels. The adapter never opens source files, hydrates
cloud placeholders, uploads images, or downloads a model during hover.
If no pixels are resident, it waits for the existing preview pipeline.
Fine details depend on preview resolution and the model's prediction.
There is no fabricated geometric fallback and no generic agent prompt action.

Worker replies are tagged by hover generation. Leaving the image, changing
tabs/tools, editing the scene, or dismissing the preview invalidates the reply.
There is one running inference and at most one queued request.
