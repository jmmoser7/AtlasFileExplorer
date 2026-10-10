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
`%LOCALAPPDATA%/NativeFileAtlas/segmentation`. Install 64-bit Python 3.11–3.13
first (3.13 is the validated runtime). Dependencies and weights are machine resources, never
workbook content. Setup requires internet access; inference is offline.

Both lockfiles beside the setup script pin every runtime and build dependency,
including transitive packages. Pip requires their published SHA-256 hashes.
SAM's source archive is pinned by commit and SHA-256; the checkpoint is checked
against the SHA-256 in Meta's [published Git LFS pointer](https://huggingface.co/facebook/sam2.1-hiera-tiny/blob/main/sam2.1_hiera_tiny.pt).
An existing checkpoint is verified too. A mismatch stops setup; an incomplete
or corrupt download is never promoted into the installed checkpoint.

Run `scripts/setup-segmentation.ps1 -VerifyOnly` to check installed versions,
SAM source identity, and model integrity offline without installing anything.
When updating dependencies, update both lockfiles from official release
metadata and validate a fresh installation before shipping them.

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

Requests share already-resident preview pixels without copying them on the UI
thread. The background worker downsamples to a maximum edge of 768 pixels and
converts them to RGB before inference. The adapter never opens source files, hydrates
cloud placeholders, uploads images, or downloads a model during hover.
If no pixels are resident, it waits for the existing preview pipeline.
Fine details depend on preview resolution and the model's prediction.
There is no fabricated geometric fallback and no generic agent prompt action.

Worker replies are tagged by hover generation. Leaving the image, changing
tabs/tools, editing the scene, or dismissing the preview invalidates the reply.
There is one running inference and at most one queued request.
