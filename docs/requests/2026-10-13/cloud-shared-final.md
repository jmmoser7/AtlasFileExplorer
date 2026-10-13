# Cloud shared-package final verification

Snapshot verified: `3949649d17ba9ca31bbfe13fd564dfb1e57c0aea` (plus the scoped feedback-dialog fix documented below).

## Scope

This run was limited to the requested shared/pure packages. It did not compile or test the Slate application, run ignored corpora, GUI or live-provider tests, inspect secrets, or read source cloud placeholders.

The requested regression areas were exercised by the package suite, including feedback dialog reopen/resize/header dragging, strip-layout notch placement, canvas-text emphasis caching, `TextStyle` serde compatibility, artifact emphasis/light-wire fallback, and trim internal-vertex behavior.

## Commands and results

| Command | Exit code | Result / totals |
|---|---:|---|
| `cargo test --locked -p atlas-shell -p slate-doc -p slate-artifact -p vector-ink -p xtask -j4` (initial snapshot run) | 101 | Stopped in `atlas-shell`: 268 passed, 1 failed, 2 ignored. The sole failure was `feedback::panels::tests::feedback_window_can_be_dragged_and_remembers_the_position`; later packages did not run after that failure. |
| `cargo test --locked -p atlas-shell feedback::panels::tests::feedback_window_can_be_dragged_and_remembers_the_position -- --exact --nocapture` | 0 | 1 passed, 0 failed, 0 ignored (270 filtered out). |
| `cargo test --locked -p atlas-shell -p slate-doc -p slate-artifact -p vector-ink -p xtask -j4` (post-fix verification) | 0 | 24 test/doc-test binaries: 837 passed, 0 failed, 6 ignored, 0 measured, 0 filtered out. |
| `cargo xtask contracts` | 0 | 40 registered dimensions; contracts, registry, and `decisions.json` agree. |
| `cargo xtask kits` | 0 | 1 kit file and 8 tools; audit passed. |
| `cargo xtask size` | 0 | 786 Rust files scanned; 500-line cap enforced; 111 existing allowlisted oversized files; lint passed. |
| `cargo fmt --all -- --check` | 0 | Formatting passed with no output. |

## Failure and scoped fix

The feedback window suppresses egui's native title bar in favor of the shared custom header. The window was marked movable, but the custom header's child UI consumed the pointer interaction, so dragging that visible header did not move the dialog.

The fix keeps the behavior in the existing shared feedback owner:

- store the feedback dialog's last transient position in `FeedbackHub`;
- make the bespoke header itself expose a drag interaction while excluding the close-button end;
- apply and constrain the requested position on subsequent frames, preserving it across close/reopen and keeping it inside a resized canvas.

No expected values, allowlists, or tests were weakened or disabled.

## Remaining failures

None in the requested shared/pure verification scope.
