# Shared desktop color sampling

Status: implemented on Windows; native acceptance verification pending. Scope approved and implementation authorized 2026-09-17.

## One sampling scope everywhere

The shape Fill and Stroke editors, standalone Eyedropper (I), Brush's temporary Alt picker, foreground/background swatches, inspector color controls and any future color-edit surface use the same desktop sampler. Other app color controls, including shared theme tuning when supplied with an eyedropper, call this owner rather than implementing a second sampler. Existing direct RGB editors are not themselves eyedroppers; audit them for a shared picker entry point during implementation.

`atlas-shell::desktop_color` now owns virtual-desktop capture and the native picking session. Slate tools, selection editors, inspector and foreground/background controls share it. Capture takes one displayed desktop snapshot before drawing the magnifier, on a worker thread; no captured pixels are persisted.

## Interaction

1. Invoke the eyedropper from a property editor or the existing tool/temporary modifier. Capture the destination property, selected node IDs and requesting tab/session before leaving Slate's window.
2. Enter a desktop picking session across connected monitors, including different DPI scales and negative virtual-desktop coordinates. The picker shows a magnified local patch and an RGB candidate beside the pointer. Its own cursor, ring and chrome must not contaminate the sampled pixel.
3. Sample **displayed RGB at the pointer** consistently, including inside Slate. A click on a fill samples that pixel, not a guessed salient node style. Raster images and other applications work by the same rule. Displayed composited RGB is distinct from an object's authored pre-transparency color; exact style copying remains a separate command.
4. A primary click accepts once, consumes the click so it does not activate the underlying application, returns to the requesting editor, and changes RGB while preserving that property's opacity. The desktop pixel has no source-object alpha to copy. Esc cancels and returns without changing values; temporary modifier release cancels an uncommitted sample. Do not override the invoking entry point's established foreground/background target semantics.
5. A sample from an open property editor joins its pending preview and commits with Apply/outside click; an inspector sample commits one named authored journal action; a foreground/background tool-color edit updates the existing tool state. No desktop images are retained as document assets. If the target tab closes or the destination is no longer editable, discard the result visibly instead of applying it to another tab.
6. If the OS cannot provide a pixel, show an unavailable candidate and retain the old value; never treat a failed capture as black. Restore pointer capture/focus on accept, cancel and failure. The normal frame loop must not block on an unbounded full-desktop capture.

The public owner should be a shared color-picker/sampling service used by `atlas-shell` color controls, with Windows capture isolated behind a platform adapter. Scene/geometry crates stay renderer- and OS-independent; callers pass a destination rather than owning capture. No new implementation module is mandated by this specification.

Microsoft documents PowerToys Color Picker as a Windows reference for picking a visible color from any screen and refining RGB values. Its implementation is a research reference, not a runtime dependency for Slate. [Microsoft Color Picker](https://learn.microsoft.com/en-us/windows/powertoys/color-picker)

## Acceptance coverage

- Pick from a Slate rectangle fill, its stroke, an image pixel, another application's content and the desktop background; each returns the displayed pixel's RGB.
- Repeat from the toolbar Fill/Stroke buttons, standalone I and temporary Brush picker; sampling scope stays identical and destination semantics remain correct.
- Cross monitors at different DPI scales and monitors to the left/above the primary; sampled coordinates match the pixel under the cursor.
- Select a fill with alpha 30%, sample another color, accept -> RGB changes and alpha remains 30%; undo restores the prior RGB/alpha together.
- Esc, modifier release and unavailable capture leave the original value; accepted sampling never sends a click to the underlying window.
- Change or close the originating tab before completion -> no cross-tab edit, no orphaned capture.

These are native acceptance checks. RGB/alpha preservation has automated coverage; stale-selection guards passed code review; monitor layout and native overlay input need live Windows verification.
