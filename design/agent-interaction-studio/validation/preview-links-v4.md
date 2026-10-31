# Preview and answer-link validation — v0.4

30 October 2026. Plain HTML/CSS/JS; no native changes or builds.

- Dashboard options display only Preserve model history, including when an older preview is pending.
- A canvas click followed by a quick body click reopened the last options menu. A later body click after the 1500 ms window selected the card without opening the agent. Boundary tests cover 1500 and 1501 ms.
- Saved Dashboard outputs render as miniature dashboard content, both in circular header controls and in the rounded-square strip. Selecting Revision 3/4 in the square strip changed the output with zero agent panels and zero property-tool buttons open. The strip persisted after moving focus to Studio chrome.
- Rhino thumbnails contain the actual projected model preview. Text thumbnails render the saved document text. ImageStudioLayout.resultGrid remains the shared running-bond geometry owner.
- A query about selected text generated an inline answer link. Activating it opened the agent, scrolled to the actual response and highlighted the destination. Hover/focus displayed the question, exact answer and Open response action in a small in-place viewer.
- New anchor tests cover exact matching, prefix/suffix edits preserving links, edited passages invalidating links, overlap and repeated queries replacing a passage's answer link.

Captures: text-answer-links-v4.png and output-thumbnails-v4.png.

Eight suites passed: text answer anchors, element sessions, canvas navigation, Chat Train model/capabilities, image layout/history/graft. JavaScript syntax and diff whitespace checks passed. Provider generation and tool actions are simulated; native output rendering remains future implementation work.
