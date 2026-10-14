# Four sibling worktrees — October 14 cleanup

The authorized 4 AM pass removed all four named checkouts through Git's worktree lifecycle. Removal finished at **04:13:38 Eastern**; final recovery verification passed at **04:15:24**. `git worktree list --porcelain` now lists only the primary AtlasFileExplorer checkout.

No application code was merged or transplanted. Existing primary implementations already incorporate or supersede much of the older board, chat and text work. Uncertain older changes remain recoverable for review rather than replacing the current tested source.

## Recovery branches and original state

All refs below are local branches in the primary repository, under **`codex/recovery/20261014/`**. Original feature/preview branches were also retained. The `head`, `staged` and `working` suffixes distinguish original history, the original index tree and exact changed working-file bytes.

- **`atlas-preview` — removed.** `atlas-preview-head`, `atlas-preview-staged` and `atlas-preview-working` all point to `b97b97e252599e2fb67d2789459ec121729887bf`. The checkout was clean. Its history contains 69 commits absent from reviewed `e3f1fc84`; the reviewed line has 23 commits absent from preview. Those divergent histories were preserved, not merged.
- **`atlas-ws-board` — removed.** `atlas-ws-board-head` and `atlas-ws-board-staged` point to `d172cca5b9289df806b14842dab7b9693cfa4782`; `atlas-ws-board-working` points to **`24f48e8a3c292983406e26f83d8d040c2a8be105`**. Preserved 13 modified tracked files and one untracked regression-test file. Older PDF sizing, portal overlap and image-path work has evolved into the primary owners. Cursor refinements and exact old test coverage remain review items.
- **`atlas-ws-chat` — removed.** `atlas-ws-chat-head` points to `d172cca5b9289df806b14842dab7b9693cfa4782`; `atlas-ws-chat-staged` points to **`185cdc3aa919318320048b5a35efe5a61fdc01a0`**; `atlas-ws-chat-working` points to **`b951be45519531a6f9cff97f5c35776030af087b`**. Preserved 21 changed tracked paths and three untracked files, including the distinct staged/unstaged versions. Header/port/train behavior is substantially incorporated or refactored; old seed/polish tests and remaining refinements need comparison before adoption.
- **`atlas-ws-text` — removed.** `atlas-ws-text-head` and `atlas-ws-text-staged` point to `a9fddfbb7b7e8a0baa534475a7826ebcb9815457`; `atlas-ws-text-working` points to **`9488bb73fa37b6226d2ed8b5d9cefb3be49ed026`**. Preserved three branch commits absent from the reviewed line, two modified module-registration files and two untracked test/support files. The primary checkout has the corresponding zoom-stability architecture and evolved tests; exact historical differences remain available in these refs.

## Ignored files and recovery evidence

Every ignored root was moved intact, on the same volume, beneath:

`C:/Users/jmoser/source/repos/AtlasFileExplorer/target/worktree-recovery-2026-10-14/<worktree-name>/ignored/<original-relative-path>`

This preserves preview's `docs/agent/cursor-sidecar/node_modules` and each other worktree's entire `target` directory, including build/audit artifacts. Their combined file size is **12,915,698,654 bytes (12.92 GB)**. This pass cleaned the sibling folders; it did **not reclaim that disk space**. These ignored assets remain local and are not included in the documentation backup.

The same recovery area contains `manifest.json`, each original index, binary staged/unstaged patches, and original worktree Git metadata. The manifest records all twelve refs, per-file Git blobs/SHA-256, ignored-directory metadata inventories, and verification/removal timestamps. Two build-directory metadata digests drifted during initial collection, with unchanged entry counts and byte totals. The original digests were retained, two stable current scans established the relocation baseline, and complete destination inventories matched before and after removal. No cause is claimed for the earlier metadata drift.

## Verification and scope

- A capable independent review required fresh path/byte verification immediately before removal. That gate was implemented; checking porcelain status alone would miss edits to an already-dirty file.
- Exact changed/untracked path sets, source bytes, deletions, staged-index copies and recovery refs were verified. Git object connectivity passed. No active executable was observed beneath the sibling paths.
- Every ignored destination matched its recorded inventory, and no ignored roots remained in a checkout before removal. Resolved absolute paths were checked against the four explicitly authorized targets.
- Final verification found all four folders absent and all recovery material present. Primary **HEAD, real index hash and tracked/untracked status signature were unchanged** throughout cleanup. Main remains `d172cca5b9289df806b14842dab7b9693cfa4782`.
- The original Slate process **PID 47568** remained responsive. Its open workbook was not closed, saved, replaced or otherwise manipulated by this pass. Native executable refresh still awaits the owner's save/close choice.
- No Cargo run, provider call, security change, default-branch push/merge or recovery-branch deletion occurred. New recovery branches are local; the cleanup report is backed up separately on the documentation records branch.

The remaining action is to review uncertain historical tests/refinements if they are wanted. Their physical checkouts are no longer required for that review. Recreate a fresh worktree from the relevant recovery ref; use the staged ref and recorded patches/index evidence when the original staging distinction matters. Never restore an old index over the owner's primary checkout.

This overnight cleanup is outside the feedback timing cutoff in [the incident report](postmortem.md). The 7 AM follow-up will reconcile any later material outcomes without rerunning this cleanup.
