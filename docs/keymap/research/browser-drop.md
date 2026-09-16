# Browser links and tab drops — Windows, 2026-09-15

## Implemented boundary

Slate receives `UniformResourceLocatorW`, `CF_UNICODETEXT`, and UTF-8
`text/uri-list` through a Windows OLE `IDropTarget`. One HTTP(S) URL creates
a portal at the drop point, or rebinds an unlocked web portal there. The
native adapter queues data; existing web portal commands own mutations.
File names (`CF_HDROP`) still use the existing file-drop path, including
Alt-drop for HTML snippets. No dropped file contents are read by the adapter.

The root window disables winit's file-only drop target and registers Slate's
target instead. Linked File Atlas windows retain their own existing target.
Accepted effects are copy/link, never move. Escape/leave does not enqueue a
drop. Text buffers and the pending-drop queue are bounded.

## Native tab dragging investigation

The [current Chromium tab drag controller](https://raw.githubusercontent.com/chromium/chromium/main/chrome/browser/ui/views/tabs/dragging/tab_drag_controller.cc)
finds destinations with `GetLocalProcessWindow` and `CanAttachTo`, then asks
the destination `BrowserView` for its tab context or internal tab target.
`ShouldDragWindowUsingSystemDnD` uses system drag-and-drop only when the
platform does not support the window move loop. The system-drag fallback
does not call `SetURL` to publish a normal link.

Consequently, Chrome tab-strip docking is not a general external URL-drop
protocol that Slate can register for. Edge shares Chromium foundations,
but its installed tab UI has not been tested interactively here; no claim
of native Edge tab-drop compatibility is made.

Direct tab docking is deferred under the user's condition that it require
little overhead or risk. Window hooks, accessibility polling, or an extension
and native messaging bridge would add a separate lifecycle and permission
surface. An extension's explicit “Send to Slate” action could be considered
later, but does not by itself make native tab-strip dragging interoperable.

Drag a page link or select and drag its address-bar URL instead. This opens
the locator in Slate's existing WebView2 portal; it does not transfer the
browser tab, cookies, form state, or navigation history. WebView2's storage
is app-specific ([Microsoft user-data folder documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/user-data-folder)).

## Validation

Headless frame-loop tests cover placement at OS coordinates under zoom/pan,
rebind/undo, invalid text, locked/read-only targets, and HTML/Alt file drops.
Windows COM tests supply actual HGLOBAL-backed IDataObjects to decode URL,
Unicode text, URI-list, and file-name payloads. A hidden-window IDropTarget
test covers registration, copy-effect negotiation, cancellation, rejection
of move-only sources, and physical-pixel to egui-point conversion.
These are synthetic integration tests, not a live Chrome/Edge gesture test.

Validation on this machine: `cargo check --release -p slate` and
`cargo test --release -p slate --no-run` succeeded. Windows refused to launch
the test executable and `cargo xtask contracts` with OS error 5 (Access is
denied), before either could run. Test outcomes and the contract audit are
therefore unverified. The open Slate process was not restarted.
