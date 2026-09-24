# Web portal — interaction contract

Status: **agreed**
Family: portal
Portal class: **host** (Art. V.3) · Type: **web** · Subtypes: **remote URL**, **local HTML file/folder**
Command: `board.portal.web` (placement) · Key: none in v1 · Palette:
"web portal" (aliases: embed, web page, iframe, url, dashboard, html)
Inherits: P0.* (all), P1.node, P1.portal, P2.PortalPlace, **P2.PortalHost** — deviations flagged below.
Canvas: `portal-web-embed-contract` (volatile) · Precedent:
`portal-agent-link` (host), `portal-atlas-lens` (host).

> **Implementation.** Model: `slate-doc::scene` (`PortalKind::Web`,
> `WebPortalRef`, `classify_web_locator`). Runtime: `apps/slate/src/app/board_web.rs`
> — states, LOD, pool admission, painting, and the `WebHost` pixel backend
> behind a trait; `board_web_win.rs` is that backend on Windows (composition
> controller → captured visual → D3D11 readback). Export: `slate-artifact`
> (`assets.rs`, `render.rs`).
> Golden paths: `gp1_*`–`gp12_*` in `apps/slate/src/app/tests.rs`, driven by a
> fake host so the pool is exercised without a browser; policy unit tests live
> beside the code in `board_web.rs`.
> **Reuse (Art. XII / P2.PortalHost).** Owner: `board_portal_chrome` +
> `WebHost`. Forbidden: a second `resolve_source`, bake-PNG writer, or
> contents-focus prelude. Known debt: DV-18, DV-20.

## What it is, and the 10% it implements

A journaled frame on the board whose contents are a web page: either an
`http(s)` URL, or a standalone `.html` file / a folder with an entry file — a
custom dashboard, a chart export, a report. The frame, the locator, and the
viewport parameters are ordinary journaled data. The rendered page is derived
and never journaled.

Contents composite **offscreen into a texture**, so a web portal is an ordinary
board node: it obeys z-order and opacity, stays axis-aligned (no rotate), and
many portals render at once. What is rationed is not liveness but cost — a pool of at most
`portal.web.live_pool` webviews, admitted by on-screen priority, with everything
else showing its last frame (D23, D29). The motivating case is a hundred pages
tiled as a research hub: a hundred cached textures, at most six processes, and
zero processes when the board is zoomed out.

The implemented fraction is deliberately small: **one page, one locator, no
browser.** A Slate identity tab (P1.portal.chrome) shows the locator; there
is no browser tab strip, address bar, history, or downloads, and no way
for the page to reach Slate.

The 90% cut is listed in D15.

## Constitutional readings (Art. XI)

1. **Art. V.3 says a host portal "exports as a poster and a pointer."** For a
   *remote* page that is exactly right, and D24 does it. For a *local*
   dashboard the exported artifact is itself HTML and packaging (Art. IX.4)
   already copies the material beside it, so an `<iframe>` over the packaged
   copy is the serialization Art. IV.1 asks for, and a screenshot of a
   dashboard is the lossy imitation it forbids. **Ratified by the user
   (OQ2 = A):** packaged iframe for local sources, poster + pointer for remote.
2. **Art. VII.4 forbids script execution pending amendment.** A page running
   its own JavaScript inside an isolated webview is not an automation driving
   the application, so no amendment is needed. The **control surface** named as
   a future exception in D18 stays on the right side of that line because it
   carries typed parameter values *into* the page and never lets the page issue
   a command *out*. Arbitrary bridges, host objects, and page-initiated
   `postMessage` into Slate remain permanently cut (D15).
3. **Determinism is not re-litigated here.** Art. V.3 imposes it on generated
   portals only; the rule is now stated once as **P1.portal.determinism** so no
   future host or document portal has to argue it again (D28).

Neither reading changes the renderer-agnostic rule: `slate-doc` gains only
`PortalKind::Web` plus a `WebPortalRef` of plain data. Every line of WebView2
code lives in `apps/slate` behind a non-Windows stub (Art. I.1, I.3).

## Behavior matrix

| ID | Dimension | Agreed behavior | Source | Conf |
|----|-----------|-----------------|--------|------|
| D01 | Initiation & arming | Command `board.portal.web`, board tabs only. Palette: "web portal" (aliases: embed, web page, iframe, url, dashboard, html). Portals dock flyout row. No single-key chord. Three drop-in entry paths create a **bound** portal at the pointer without arming a tool: **dragging a page link or selected URL text** out of Edge / Chrome / Firefox onto the board (Windows OLE `UniformResourceLocatorW`, `text/uri-list`, or `CF_UNICODETEXT`, received by Slate `external_drop`; native browser tab-strip docking is unsupported, see the browser-drop research note); **pasting an `http(s)` URL** onto the board (Ctrl+V of clipboard text that parses as one); and **dropping a `.html`/`.htm` file or a folder containing `index.html`**, which makes a portal instead of a text card (Alt-drop keeps the text card, D05). | stated | 100 |
| D02 | Stickiness & repeat | P2.PortalPlace: one-shot, commit returns to Select, Space/Enter re-arms. | precedent | 90 |
| D03 | Gesture grammar | P2.PortalPlace: `Armed -> Dragging(rect) -> Committed(bound to start locator)`, with `https://www.google.com/` as the authored start page. Binding a different page remains a separate non-modal step (D19) — no file dialog and no network fetch inside a draw gesture. The three drop-in paths of D01 still skip straight to `Committed(bound)` with their supplied locator. | stated | 100 |
| D04 | Click vs drag rule | P2.PortalPlace: travel > `draft.drag_threshold` (4 px) uses the dragged rect; below it places `portal.web.default_size` (960×540) centred on the click. | precedent | 85 |
| D05 | Modifiers | P2.PortalPlace Shift = 16:9. Addition: Alt while dropping a `.html` file bypasses the portal diversion and creates the legacy text snippet card. Ctrl unassigned in v1. | precedent | 80 |
| D06 | Constraints & snapping | P2.PortalPlace: grid snap and smart guides on the frame rect, never on contents. | precedent | 85 |
| D07 | Direction / value locks | `n/a` — P2.PortalPlace has no directional parameter. | precedent | 85 |
| D08 | Numeric / manual entry | No digit entry during placement. One numeric knob exists after commit and it is authored: `viewport.width_css` (default 1280), edited in the Portal inspector. Typed frame dimensions stay a non-goal (D15). | guess | 60 |
| D09 | Preview & readouts | During the drag: frame outline in `Palette::portal` + live w×h in the dock readout. Placed: **P1.portal.chrome** — one Slate identity tab showing the display locator (or the page currently being visited), a live dot while pooled, and no default frame outline. A minimalist stroke appears on edge hover. Contents focus suppresses the selection cast, outline, and that stroke (P1.portal.contents-focus). Tab text clips to the tab bounds. Contents clip full-bleed to the frame fillet (**P1.portal.clip**), with no visual bezel. Dock readout when selected: locator · source kind · viewport width · zoom mode · state. | stated | 100 |
| D10 | Cursor | Crosshair while armed. Over a portal that is not input-focused — poster or live-rendering alike — the normal board arrow, because clicks select the frame. Over the input-focused portal, Slate applies the cursor the page requests through the composition controller. Over the chrome strip: always arrow, because the strip stays a Slate target. | pattern | 78 |
| D11 | Commit | One journaled `Add` of `PortalNode { class: Host, kind: Web, web: Some(WebPortalRef { source, entry, viewport, export_mode }) }`. Poster texture, live state, scroll offset, and page history are derived and never journaled (Art. VI.3). One gesture = one undo. No style state consumed (D16). | precedent | 90 |
| D12 | Cancel | Esc peels one layer per press (P0.1): **maximize →** page input focus → drag draft → armed tool → selection. Leaving maximize restores the authored frame and leaves page focus intact. Releasing input focus returns the portal to ordinary live rendering; it does **not** tear the webview down. | stated | 100 |
| D13 | Selected presentation | Windows-style hover resize on the frame (P1.node.transform) — no prior selection. Contents expose no grips. Resize scales the rendered page (D20 zoom `Fit`); it does not reflow unless viewport is `Auto`. Portals stay axis-aligned: no rotate chrome, including in a mixed group rotate. | stated | 100 |
| D14 | Post-edit | Portal sidebar section when selected, mirrored by `portal.web.*` commands. Journaled `Patch`es: source (URL field / Choose file… / Choose folder + entry), viewport width and zoom mode, export mode, Package into workbook folder (Art. IX.4). Derived actions: Home (navigate back to the authored locator), Reload, Recapture poster, Open in system browser, and Allow this origin (local trust, not journaled — D32). | pattern | 80 |
| D15 | Non-goals | Cut, each a decision (Art. III): **browser** chrome (multiple tabs, address bar / omnibox, bookmarks, history UI, devtools, downloads, printing, a cookie jar shared with the user's real browser, sign-in cookies or tokens written into the workbook or an export, notifications / geolocation / camera / mic / clipboard-read, scraping page DOM into board nodes, writing back to the source HTML file (Art. IX.5), and typed frame dimensions (D08). **Not cut:** in-page back/forward/reload/home commands, the Slate identity tab of P1.portal.chrome (one tab, the locator, maximize / fold), and `_blank` links navigating the same webview instead of opening popups. **Not cut, deferred to its own contract:** the declared control surface of D18. Arbitrary JS bridges, host objects, and page-initiated `postMessage` into Slate remain permanently cut. **Amended 24 September 2026 (user-ratified):** when a person submits an agent whose Prompt is wired from this portal, Slate reads the page's visible text (`document.body.innerText`) once, read-only, for that run only; it never reads cookies or storage, never runs page-supplied code, and never writes the text into board nodes. Page-internal navigation is derived session state: losing a pool slot (zoom out, off-screen, budget) resumes the last URL, not the authored home. It is never written into the `.slate`. Home returns to the authored locator. The default Google start page is a locator, not Slate chrome. Sign-in for a remote origin persists for this Windows user in a WebView2 profile derived from that origin (`web_profile_name`). Opening the same file as another user starts signed out. | stated | 100 |
| D16 | Create-style inheritance | **No** — P1.portal.style. | precedent | 90 |
| D17 | Hit-testing & pick | The frame picks on its rect, including marquee, whenever it is not input-focused. Only the single input-focused portal routes pointer and keyboard to the page — except the identity tab, the reveal strip, and a `portal_frame.border_hit_px` (6 px) border band, which stay Slate targets as invisible input chrome. The border band may reveal a minimalist hover stroke but never creates a visual bezel. Contents focus does not add a highlight stroke. **Right-click** on the tab or anywhere on the portal is Slate's (Copy URL, Paste URL, Maximize, Hide tab), never the page's. No board tool reaches the contents. | stated | 100 |
| D18 | Portal class & authority | **Host, with one named future exception.** The page owns its own runtime and Slate owns no mutations inside it; the only journaled acts are on the frame — place, move, resize, rebind, re-parameterize, delete, bake. **The exception, deferred to its own contract:** a hosted surface may declare a *control surface* of named, typed parameters, which board wires drive from other nodes — the case being a dashboard with sliders wired to an input node so other board content moves them. Values written that way are authored journaled data on the Slate side and travel one way, Slate → page, over a declarative typed channel; the page never issues commands back, which keeps Art. VII.3 (data, not code) and VII.4 intact and makes the exception explicit rather than accidental. Determinism is not required of host portals (P1.portal.determinism). | stated | 100 |
| D19 | Source binding | `SourceUri.locator` holds exactly one of: an `http(s)` URL; a path to an `.html`/`.htm` file; or a path to a directory plus `WebPortalRef.entry` (default `index.html`) for a multi-file dashboard. Newly placed web portals default to the remote start locator `https://www.google.com/`; that URL is journaled like any other source. Paths are relative to the workbook first, absolute as fallback (Art. IX.2). Rebound by the inspector, the empty-state button after clearing, or by dropping URL text/a link on an unlocked web portal (one journaled rebind; locked portals refuse it); rebinding is a journaled `Patch` that discards the cached poster. Refused: `javascript:` and `data:` URIs, non-`http(s)` schemes (`ftp:`, `about:`, custom protocol handlers), `.slate` files (diverted to open as a tab), and non-HTML files (which stay ordinary board items). | stated | 100 |
| D20 | Query & parameters | Journaled: `viewport.width_css` (default 1280) — the CSS width the page is laid out at; `viewport.zoom` = `Fit` (scale the render to the frame width, default) \| `Fixed(f32)` \| `Auto` (frame size drives the CSS viewport, so resize reflows like a browser window); `entry` (folder sources); `interactive_allowed` (default true); `poster_capture` = `OnBind` \| `Manual` (never a timer in v1); `export_mode` (D24); `title`; frame rect and fill. Derived: scroll offset, page history, live state, poster texture, load errors. | guess | 62 |
| D21 | Regeneration & staleness | Poster recapture triggers: bind/rebind, viewport change, explicit `portal.web.recapture`, pool eviction (D29), and — for local file/folder sources only — an mtime change found by a worker-thread poll no more often than `portal.web.poll_secs` (1.0). Remote URLs never auto-recapture: a network fetch is never implicit. GPU capture/copy is asynchronous and readback is polled without waiting, generation-tagged; a stale capture is discarded, never painted (Art. II.3). While capturing, the last valid poster stays at full authored opacity until a valid replacement arrives; recapture never clears it. Source gone: `Missing`, last poster retained and dimmed, locator named (P1.portal.health). | pattern | 85 |
| D22 | Contents interaction | **Many render, one listens.** Hover / click select the frame; double-click (or Enter) takes input focus; Esc or clicking elsewhere releases it without stopping the page. Ctrl+double-click opens the locator in the system browser. Right-click anywhere on the portal (tab or body, focused or not) opens the Slate portal menu — Copy URL / Paste URL rebind that frame. The maximize button is **P1.portal.maximize**. Home navigates the live page back to the authored locator without journaling. User-initiated `_blank` links navigate the same webview, never a popup. While input-focused the wheel scrolls the page and Ctrl+wheel still zooms the camera (P0.5; plain wheel surrendered for that one frame). | stated | 100 |
| D23 | Level of detail | New portals below 160 physical pixels high use their poster; on-screen portals at or above 160 are eligible for the bounded live pool. Input focus overrides the initial size gate. Once admitted, a portal keeps its browser session while visible, even when zoomed smaller; off-screen or capacity eviction still releases it. Below 96 pixels, fresh readback pauses and the last poster remains. Captures use physical displayed pixels, including monitor DPI, with upward power-of-two quality tiers and a monitor-sized ceiling. Maximized and interaction-focused portals retain full displayed resolution; an already-sharp tier is retained while zooming out until eviction. CSS layout stays fixed through quality changes; only the raster scale changes. Safety limits are 8192 pixels per edge and 33,177,600 pixels (8K display support). The previous valid frame remains while a new tier becomes ready. Maximize may change viewport aspect without replacing the browser. Once a valid frame exists, low-resource/off-screen states, host recreation, viewport changes, and explicit recapture retain it at full authored opacity until a new valid frame arrives. Fully transparent or uniform black/white compositor clears cannot replace it. A different source clears the old poster; a genuinely missing source may dim it. | stated | 100 |
| D24 | Export serialization | Two modes, journaled per portal, defaulting by source kind (OQ2 = A). Local file/folder: **packaged iframe** — `build_assets` copies the entry file and its folder into `assets/` and rewrites the locator (Art. IX.4 fork, origin recorded in the manifest), and the writer emits `<iframe src="assets/…" sandbox="allow-scripts allow-same-origin" title=…>` plus a caption naming the origin. Remote URL: **poster + pointer** (Art. V.3) — `<a href=url><img poster></a>` with a caption reading "Live page, captured &lt;date&gt;", and an honesty note where the site would refuse framing. `export_mode: Iframe` is opt-in for remote and says in its caption that the frame loads live from the network. Machine-private paths (the WebView2 folder, the secret store, a legacy API-key file) are never packaged; those portals export as a poster. | stated | 95 |
| D25 | Bake | `portal.web.bake` emits one journaled `Add` batch: an `Image` node holding the captured poster (written into the workbook assets) plus a provenance `Text` node naming the locator, the source kind, and the capture time. The portal is **left in place** — bake copies, it does not convert. Exact repo-lens D25 precedent. | precedent | 85 |
| D26 | Collaboration & per-peer | Frame, locator, entry, viewport, and export mode sync as ordinary journal deltas. Poster texture, live/pool state, scroll offset, page history, per-origin consent, and per-origin sign-in are per-peer and never transmitted — each peer runs its own pool, and only the peer who focuses a portal gives it input. A peer that cannot resolve the locator paints `Unknown` naming the locator it tried (P1.portal.health). Consent is deliberately not journaled: a `.slate` you received must never arrive carrying someone else's decision to trust a host. | pattern | 85 |
| D27 | Agent surface | **Parity, mediated by the existing autonomy grant.** Agents and humans drive the identical `board.portal.web` / `portal.web.*` commands with identical privileges (Art. VII.1); agent-issued mutations stage for acceptance by default and apply directly where the workspace has granted autonomy (Art. VII.6) — the same rule that governs every command, not a portal-specific leash. Origin consent (D32) is a command like any other and follows that same path. The remaining restrictions are **capability-level, not agent-level**: nobody, human or agent, can read page DOM, cookies, or storage, or make the page issue a command, because no such channel exists (D15). The one exception (amended 24 September 2026, user-ratified) is the page's visible text, read once when a person submits an agent run wired from the portal; an agent cannot request that read itself. Art. IX.5's "never available to an agent" clause governs write-back to sources and does not bite here — this portal never writes to its source. The context beacon carries locator, source kind, viewport, and state. | stated | 100 |
| D28 | Determinism & provenance | `n/a` — **P1.portal.determinism**: determinism is required of *generated* portals only (Art. V.3), and Art. IV.2 governs extracted graphs; neither describes a hosted page. What this portal owes instead is provenance, shown on the frame and in every export: resolved locator, source kind, content hash (local) or capture timestamp (remote), viewport width, and the origin recorded by packaging. | stated | 100 |
| D29 | Performance envelope | At most six webviews exist, prioritized by input focus, on-screen area, and recency. Focused interaction targets 60 fps; other visible live portals target 30 fps. At most two texture uploads per frame; excess work remains queued. Existing textures update in place. Two staging textures pipeline GPU copies; Map uses DO_NOT_WAIT so an unfinished copy leaves the previous image visible and retries later. Resize discards mismatched capture frames. Deferred admissions are deduplicated and cancelled on eviction. Hidden/off-screen portals release their slots; visible zoomed-small portals retain sessions within the same six-view cap. Local source probes remain asynchronous. Missing runtime reports NoRuntime. | pattern | 80 |
| D30 | Failure & honesty states | `Unbound` ("Choose page or file…"). `Unknown` — not yet resolved, neutral marker, never blocking (P1.portal.health). `Blocked` — remote origin not yet permitted, host named. `Loading`. `Live`. `Budgeted` — eligible but outside the live pool, showing its last frame and saying so. `Poster(age)` — stale by construction, age stated. `Missing` — file gone, DNS failure, or 404, locator named, last poster kept. `Refused(reason)` — unsupported or dangerous scheme. `NoRuntime` — WebView2 runtime absent, with what to install. `TooSmall` — below `live_min_px`. `NotFramable` — export-time note when a remote site refuses iframing. Every state names itself in the frame rather than blanking it. | pattern | 85 |
| D31 | View-state ownership | Journaled authored intent: frame rect / rotation / opacity / fill, title, locator, entry, viewport width and zoom mode, `interactive_allowed`, `poster_capture` mode, export mode. Derived per-peer: poster texture and its capture time, live/pool membership, input focus, scroll offset, page navigation history (survives pool eviction this session), load errors, origin consent grants, per-origin web sign-in, WebView2 runtime availability, **maximize**, **identity-tab fold**. | stated | 100 |
| D32 | Trust, sandbox & consent | Remote origins are `Blocked` until the human permits them; grants key on (workbook, origin), live in local app state, and are **never journaled or written into the `.slate`**, so a workbook you receive cannot arrive pre-trusting a host. Grants for a saved workbook persist for this user in the Atlas data folder (`web-consent.json`), so reopening that board in a later session is not asked again; unsaved boards keep session grants only. Packaging and export refuse that file. Placing a default web portal is the human permitting the start locator's origin for that workbook, matching paste/drop behavior. Local sources under the workbook root need no prompt. The webview user-data folder lives under the per-user Atlas data directory, never beside the workbook. Each remote origin gets its own WebView2 profile, so a sign-in on one site is not a sign-in on another; local files share a profile with no remote cookies. Cookies are not shared with the system browser. Packaging and HTML export refuse the webview folder and the machine secret store. Denied outright: downloads, popups, notifications / geolocation / camera / mic / clipboard-read, and any host object or `postMessage` bridge (D15). Exports sandbox what they embed: packaged local content gets `sandbox="allow-scripts allow-same-origin"`, because a dashboard must fetch its own sibling data files; remote iframes get `allow-scripts` only. | stated | 100 |
| D33 | Portal chrome | **P1.portal.chrome.** One plain full-width bar shows the page name or URL centered on the whole bar; web viewers have no blister or live-dot tab. The bar overlays the full-bleed page and retracts after 1.2 seconds without activity or when the pointer leaves. Page interaction or hovering its top edge reveals it. Native page scrollbars share that visibility and a proportional thickness; transparent idle thumbs preserve their gutter so hiding never reflows the page or disables wheel scrolling. Maximized bars use the Slate index top-bar height and typography, and scrollbars use the same scale. Canvas bars retain node-local scaling. Maximize remains at the far right; Escape restores even while controls are hidden. Explicit Hide/Show tab remains available; manually folded chrome uses the existing top-interior reveal strip. | stated | 100 |
| D34 | Portal maximize | **P1.portal.maximize.** The tab's maximize button fills the window at the screen aspect, covers Slate chrome, keeps the tab at the top of the screen, and does not mutate the node rect. Esc restores. The page is laid out against the screen body while maximized. | stated | 100 |
| D35 | Portal-local UI | **P1.portal.local-ui.** Bind, consent, viewport, and page controls live on this portal (inspector / chrome). None appear in Document Settings. | stated | 100 |

Source values: stated (user), precedent (approved or proposed for an
overlapping portal), pattern (catalog or constitution article), research
(source app), guess (agent proposal — must be confirmed before `agreed`).

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `portal.web.start_locator` | Default authored source for newly placed web portals | `https://www.google.com/` |
| `portal.web.default_size` | Click placement size | `960 x 540` |
| `portal.web.poll_secs` | Local-source mtime poll floor | `1.0` |
| `portal.web.stale_alpha` | Poster alpha while recapturing | `0.6` |
| `portal.web.lod_strip_px` | Below this physical height, no live webview (poster still paints) | `96` |
| `portal.web.raster_max_px` | Safety ceiling beyond display-sized quality tiers | `8192` |
| `portal.web.raster_max_pixels` | Total live capture pixel budget (8K display support) | `33177600` |
| `portal.web.live_min_px` | On-screen height that makes a portal pool-eligible | `160` |
| `portal.web.live_pool` | Webviews alive at once | `6` |
| `portal.web.focused_fps` | Readback / upload cadence for a still focused portal | `30` |
| `portal.web.interactive_fps` | Readback while the focused page is being scrolled / dragged / typed | `60` |
| `portal.web.interactive_hold_secs` | How long the interactive cadence holds after the last page input | `0.18` |
| `portal.web.idle_fps` | Render rate for pooled portals without input focus | `30` |
| `portal.web.uploads_per_frame` | Contents textures uploaded per frame | `2` |
| `portal.web.border_hit_px` | Border band that stays a Slate target while focused (alias of `portal_frame.border_hit_px`) | `6` |
| `portal.web.viewport_default` | Authored CSS viewport width | `1280` |
| `portal_frame.corner_radius` | Screen-space fillet contents clip to | `8` |
| `portal_frame.reveal_strip_px` | Folded-tab recover hit | `14` |

## Golden paths

1. **GP1 — place start page:** Portals flyout → Web portal →
   press-drag-release → host portal appears selected, bound to
   `https://www.google.com/`; that origin is allowed locally; tool = Select;
   one undo removes the portal.
2. **GP2 — bind local dashboard:** Choose folder… → pick a folder containing
   `index.html` → locator stored workbook-relative, `entry = index.html`,
   poster captured off-thread, chrome strip shows file glyph + folder name +
   `Ok`.
3. **GP3 — drop a link or URL text:** drag a page link or selected `http(s)`
   URL from Edge / Chrome onto the board → a bound portal appears at the OS
   drop point. The human drop grants origin consent (D32), without a second
   prompt. Undo removes it; redo restores it. Drop onto an unlocked web
   portal → one source rebind; undo restores its previous locator. Locked,
   read-only, non-board, and outside-canvas targets refuse URL drops. Ordinary
   prose, URL lists, and non-HTTP schemes create nothing. HTML file drops
   keep their existing behavior, including Alt for a snippet card.
4. **GP4 — focus and release:** double-click a pooled portal → it takes input
   focus, clicks and keys reach the page, the accent border appears → Esc →
   focus released, the portal keeps rendering, scroll position preserved.
5. **GP5 — no live below threshold:** double-click a portal painted 120 px
   tall → the camera zooms to fit it, then it takes focus; no webview is
   created while it is below `live_min_px`.
6. **GP6 — portals stay axis-aligned:** hover just outside a corner does not
   offer rotate; hovering an edge or corner still resizes without a prior
   selection.
7. **GP7 — missing source:** delete the bound local file → within one poll
   interval the frame paints `Missing` naming the locator, keeps the last
   poster dimmed, and never blocks a frame.
8. **GP8 — pool budget:** tile twelve bound portals above the live threshold →
   exactly `live_pool` webviews exist, the other six report `Budgeted` with
   their last frame; scrolling a different six into view swaps membership and
   each evicted portal keeps a fresh poster.
9. **GP9 — export local:** export a board holding a local dashboard portal →
   `assets/` holds the copied folder, the manifest records the origin path, and
   the slide holds an `<iframe>` with a caption naming the origin.
10. **GP10 — export remote:** export a board holding a URL portal → the slide
    holds `<a href=url><img poster></a>` with the capture date; no network
    fetch happens during export.
11. **GP11 — agent parity:** an agent issues `portal.web.source` with a URL →
    without an autonomy grant it appears as a staged, attributed proposal;
    with one it applies directly as a `CmdAuthor::Agent` journal group. Both
    paths are the same command a human runs.
12. **GP12 — bake:** `portal.web.bake` → one undo step adds an `Image` node of
    the poster plus a provenance `Text` node; the portal is still there.
13. **GP13 — maximize:** click □ on the identity tab → the portal fills the
    window at the screen aspect and Slate chrome is gone; Esc restores the
    authored frame; the node rect is unchanged.
14. **GP14 — fold tab:** right-click → Hide tab (or `portal.chrome.toggle`) →
    the tab hides; hover the top interior and click the reveal → the tab
    returns. Works in both ordinary and maximize mode.
15. **GP15 — search home:** click Home after following a result → the live page
    navigates back to the authored locator; the node source and journal do not
    change.

## Open questions

None. The four canvas questions were answered on 2026-08-08 (offscreen
composition with a live pool; packaged iframe for local sources; authored CSS
viewport with `Fit` zoom; `.html` drop makes a portal), the ten rows the
composition choice rewrote were confirmed, and the determinism complaint was
settled by promoting **P1.portal.determinism** rather than amending
`CONSTITUTION.md`.

## Resolved decision notes

- **Browser-tab drag (D01)** was added at the user's request and is the entry
  path that matters most in practice; the `.html` drop and URL paste flank it.
- **The control surface (D18)** — wires driving declared parameters inside a
  hosted dashboard — is a ratified *future* exception to host-portal
  immutability, one way (Slate → page) and typed. It needs its own contract
  before any of it is built.
- **Agent parity (D27)** replaced a portal-specific leash with the workspace
  autonomy grant that already governs every other command; what remains
  forbidden is forbidden to humans too, because the channel does not exist.

### Browser drop feasibility (2026-09-15)

See [browser-drop research](../research/browser-drop.md). The earlier D01/GP3
claim that a native browser tab necessarily supplies a URL was incorrect.
This implementation accepts standard link/text payloads and does not attach
foreign browser tabs or share browser profiles.

## Agent squircle on web portals (24 September 2026)

An image agent started from a web portal receives the page's shown pixels,
captured the way posters are. A text agent receives the page's visible text:
the user ratified an amendment of D15 and D27 that allows one read-only
`innerText` read per human Submit, through `WebHost::request_text`. The text
goes to that run's prompt only. It is never journaled, never written into
the workbook, and never read from cookies or storage. There is no headless
fallback: without WebView2 the run fails and says so.
