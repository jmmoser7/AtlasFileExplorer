# Web portal — interaction contract

Status: **agreed**
Family: portal
Portal class: **host** (Art. V.3) · Type: **web** · Subtypes: **remote URL**, **local HTML file/folder**
Command: `board.portal.web` (placement) · Key: none in v1 · Palette:
"web portal" (aliases: embed, web page, iframe, url, dashboard, html)
Inherits: P0.* (all), P1.node, P1.portal, P2.PortalPlace — deviations flagged below.
Canvas: `portal-web-embed-contract` (volatile) · Precedent:
`portal-lens-repository` (generated), `portal-agent-link` (host).

> **Implementation.** Model: `slate-doc::scene` (`PortalKind::Web`,
> `WebPortalRef`, `classify_web_locator`). Runtime: `apps/slate/src/app/board_web.rs`
> — states, LOD, pool admission, painting, and the `WebHost` pixel backend
> behind a trait; `board_web_win.rs` is that backend on Windows (composition
> controller → captured visual → D3D11 readback). Export: `slate-artifact`
> (`assets.rs`, `render.rs`).
> Golden paths: `gp1_*`–`gp12_*` in `apps/slate/src/app/tests.rs`, driven by a
> fake host so the pool is exercised without a browser; policy unit tests live
> beside the code in `board_web.rs`.

## What it is, and the 10% it implements

A journaled frame on the board whose contents are a web page: either an
`http(s)` URL, or a standalone `.html` file / a folder with an entry file — a
custom dashboard, a chart export, a report. The frame, the locator, and the
viewport parameters are ordinary journaled data. The rendered page is derived
and never journaled.

Contents composite **offscreen into a texture**, so a web portal is an ordinary
board node: it obeys z-order, rotation, and opacity, and many portals render at
once. What is rationed is not liveness but cost — a pool of at most
`portal.web.live_pool` webviews, admitted by on-screen priority, with everything
else showing its last frame (D23, D29). The motivating case is a hundred pages
tiled as a research hub: a hundred cached textures, at most six processes, and
zero processes when the board is zoomed out.

The implemented fraction is deliberately small: **one page, one locator, no
browser.** No tab strip, no address bar, no history, no downloads, and no way
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
| D01 | Initiation & arming | Command `board.portal.web`, board tabs only. Palette: "web portal" (aliases: embed, web page, iframe, url, dashboard, html). Portals dock flyout row. No single-key chord. Three drop-in entry paths create a **bound** portal at the pointer without arming a tool: **dragging a browser tab or link** out of Edge / Chrome / Firefox onto the board (the OLE payload carries `UniformResourceLocatorW`, `text/uri-list`, or `CF_UNICODETEXT`; `atlas-core::shell_drag` grows a URL arm beside its file arm); **pasting an `http(s)` URL** onto the board (Ctrl+V of clipboard text that parses as one); and **dropping a `.html`/`.htm` file or a folder containing `index.html`**, which makes a portal instead of a text card (Alt-drop keeps the text card, D05). | stated | 100 |
| D02 | Stickiness & repeat | P2.PortalPlace: one-shot, commit returns to Select, Space/Enter re-arms. | precedent | 90 |
| D03 | Gesture grammar | P2.PortalPlace: `Armed -> Dragging(rect) -> Committed(unbound)`, empty state "Choose page or file…", binding is a separate non-modal step (D19) — no file dialog and no network fetch inside a draw gesture. The three drop-in paths of D01 skip straight to `Committed(bound)`. | precedent | 90 |
| D04 | Click vs drag rule | P2.PortalPlace: travel > `draft.drag_threshold` (4 px) uses the dragged rect; below it places `portal.web.default_size` (960×540) centred on the click. | precedent | 85 |
| D05 | Modifiers | P2.PortalPlace Shift = 16:9. Addition: Alt while dropping a `.html` file bypasses the portal diversion and creates the legacy text snippet card. Ctrl unassigned in v1. | precedent | 80 |
| D06 | Constraints & snapping | P2.PortalPlace: grid snap and smart guides on the frame rect, never on contents. | precedent | 85 |
| D07 | Direction / value locks | `n/a` — P2.PortalPlace has no directional parameter. | precedent | 85 |
| D08 | Numeric / manual entry | No digit entry during placement. One numeric knob exists after commit and it is authored: `viewport.width_css` (default 1280), edited in the Portal inspector. Typed frame dimensions stay a non-goal (D15). | guess | 60 |
| D09 | Preview & readouts | During the drag: frame outline in `Palette::portal` + live w×h in the dock readout. Placed: a chrome strip carrying a globe glyph (URL) or file glyph (local), the display locator, the health dot (`Ok`/`Unknown`/`Missing`), and the poster capture age. A portal currently rendering live shows a small live dot in the strip; the one **input-focused** portal additionally gets a 2 px accent border, so "this is the frame holding my keyboard" is never ambiguous on a board where several are animating. Dock readout when selected: locator · source kind · viewport width · zoom mode · state. | pattern | 80 |
| D10 | Cursor | Crosshair while armed. Over a portal that is not input-focused — poster or live-rendering alike — the normal board arrow, because clicks select the frame. Over the input-focused portal, Slate applies the cursor the page requests through the composition controller. Over the chrome strip: always arrow, because the strip stays a Slate target. | pattern | 78 |
| D11 | Commit | One journaled `Add` of `PortalNode { class: Host, kind: Web, web: Some(WebPortalRef { source, entry, viewport, export_mode }) }`. Poster texture, live state, scroll offset, and page history are derived and never journaled (Art. VI.3). One gesture = one undo. No style state consumed (D16). | precedent | 90 |
| D12 | Cancel | Esc peels one layer per press (P0.1): page input focus → drag draft → armed tool → selection. Releasing input focus returns the portal to ordinary live rendering; it does **not** tear the webview down, and page state (scroll position, form contents, running charts) survives. Teardown happens only on pool eviction (D29) or node delete, and aborts any in-flight load. | pattern | 82 |
| D13 | Selected presentation | Standard frame handles (resize + rotation), joining the shared handle-carousel pattern. Contents expose no grips. Resize scales the rendered page (D20 zoom `Fit`); it does not reflow unless viewport is `Auto`. Rotation is fully supported: contents composite as a texture in the scene, so a rotated portal keeps rendering and stays hit-testable through its rotated rect, exactly like an image. | pattern | 80 |
| D14 | Post-edit | Portal sidebar section when selected, mirrored by `portal.web.*` commands, each a journaled `Patch`: source (URL field / Choose file… / Choose folder + entry), viewport width and zoom mode, export mode, Reload, Recapture poster, Open in system browser, Package into workbook folder (Art. IX.4), and Allow this origin (local trust, not journaled — D32). | pattern | 80 |
| D15 | Non-goals | Cut, each a decision (Art. III): browser chrome (tabs, back/forward, address bar, bookmarks, history), devtools, downloads, popups and `window.open` (blocked, offered as open-in-browser), printing, a cookie jar shared with the user's real browser, persisted logins across sessions, notifications / geolocation / camera / mic / clipboard-read, scraping page DOM into board nodes, writing back to the source HTML file (Art. IX.5), and typed frame dimensions (D08). **Not cut, deferred to its own contract:** the declared control surface of D18 — the only channel that will ever cross the frame boundary, carrying typed parameter values from Slate into the page. Arbitrary JS bridges, host objects, and page-initiated `postMessage` into Slate stay permanently cut. Page-internal navigation while focused is allowed but never persisted: the authored locator is the home. | pattern | 82 |
| D16 | Create-style inheritance | **No** — P1.portal.style. | precedent | 90 |
| D17 | Hit-testing & pick | The frame picks on its (rotated) rect, including marquee, whenever it is not input-focused: a live-rendering portal is still an ordinary board node under the cursor. Only the single input-focused portal routes pointer and keyboard to the page and selects nothing on the board, and even then the chrome strip and a `portal.web.border_hit_px` (6 px) border band stay Slate targets so the frame can be grabbed and released. No board tool reaches the contents. | pattern | 80 |
| D18 | Portal class & authority | **Host, with one named future exception.** The page owns its own runtime and Slate owns no mutations inside it; the only journaled acts are on the frame — place, move, resize, rebind, re-parameterize, delete, bake. **The exception, deferred to its own contract:** a hosted surface may declare a *control surface* of named, typed parameters, which board wires drive from other nodes — the case being a dashboard with sliders wired to an input node so other board content moves them. Values written that way are authored journaled data on the Slate side and travel one way, Slate → page, over a declarative typed channel; the page never issues commands back, which keeps Art. VII.3 (data, not code) and VII.4 intact and makes the exception explicit rather than accidental. Determinism is not required of host portals (P1.portal.determinism). | stated | 100 |
| D19 | Source binding | `SourceUri.locator` holds exactly one of: an `http(s)` URL; a path to an `.html`/`.htm` file; or a path to a directory plus `WebPortalRef.entry` (default `index.html`) for a multi-file dashboard. Paths are relative to the workbook first, absolute as fallback (Art. IX.2). Bound by the inspector, the empty-state button, or by dropping a file/folder/tab on an unbound portal; rebinding is a journaled `Patch` that discards the cached poster. Refused: `javascript:` and `data:` URIs, non-`http(s)` schemes (`ftp:`, `about:`, custom protocol handlers), `.slate` files (diverted to open as a tab), and non-HTML files (which stay ordinary board items). | pattern | 80 |
| D20 | Query & parameters | Journaled: `viewport.width_css` (default 1280) — the CSS width the page is laid out at; `viewport.zoom` = `Fit` (scale the render to the frame width, default) \| `Fixed(f32)` \| `Auto` (frame size drives the CSS viewport, so resize reflows like a browser window); `entry` (folder sources); `interactive_allowed` (default true); `poster_capture` = `OnBind` \| `Manual` (never a timer in v1); `export_mode` (D24); `title`; frame rect and fill. Derived: scroll offset, page history, live state, poster texture, load errors. | guess | 62 |
| D21 | Regeneration & staleness | Poster recapture triggers: bind/rebind, viewport change, explicit `portal.web.recapture`, pool eviction (D29), and — for local file/folder sources only — an mtime change found by a worker-thread poll no more often than `portal.web.poll_secs` (1.0). Remote URLs never auto-recapture: a network fetch is never implicit. Capture runs off-thread, generation-tagged; a stale capture is discarded, never painted (Art. II.3). While capturing, the previous poster stays at `portal.web.stale_alpha` (0.6) with a progress strip. Source gone: `Missing`, last poster retained and dimmed, locator named (P1.portal.health). | pattern | 85 |
| D22 | Contents interaction | **Many render, one listens.** Every pooled portal (D29) renders live all the time — charts tick, animations run — while pointer and keyboard stay with the board. Hover shows the full locator; single click selects the frame; double-click (or Enter on the selection) takes **input focus**, routing pointer and keyboard to that one page and marking it (D09); Esc or clicking elsewhere releases focus without stopping its rendering. Ctrl+double-click opens the locator in the system browser instead. While input-focused the wheel scrolls the page and Ctrl+wheel still zooms the camera, so P0.5 holds — **flagged as a deviation** because the plain wheel is surrendered for that one frame. | pattern | 78 |
| D23 | Level of detail | Buckets key on the portal's on-screen height in physical pixels, so a hundred tiled pages zoomed out cost nothing. Below `portal.web.lod_strip_px` (96): chrome strip only, no texture bound. From 96 to `portal.web.live_min_px` (320), or off-screen at any size: the cached poster. At or above 320 and on screen: **eligible** for the live pool, admitted by the D29 priority; an eligible portal that misses admission paints its poster and reports `Budgeted`. Double-clicking a portal below the live threshold zooms the camera to fit it first, then takes input focus, rather than refusing. | pattern | 75 |
| D24 | Export serialization | Two modes, journaled per portal, defaulting by source kind (OQ2 = A). Local file/folder: **packaged iframe** — `build_assets` copies the entry file and its folder into `assets/` and rewrites the locator (Art. IX.4 fork, origin recorded in the manifest), and the writer emits `<iframe src="assets/…" sandbox="allow-scripts allow-same-origin" title=…>` plus a caption naming the origin. Remote URL: **poster + pointer** (Art. V.3) — `<a href=url><img poster></a>` with a caption reading "Live page, captured &lt;date&gt;", and an honesty note where the site would refuse framing. `export_mode: Iframe` is opt-in for remote and says in its caption that the frame loads live from the network. | stated | 95 |
| D25 | Bake | `portal.web.bake` emits one journaled `Add` batch: an `Image` node holding the captured poster (written into the workbook assets) plus a provenance `Text` node naming the locator, the source kind, and the capture time. The portal is **left in place** — bake copies, it does not convert. Exact repo-lens D25 precedent. | precedent | 85 |
| D26 | Collaboration & per-peer | Frame, locator, entry, viewport, and export mode sync as ordinary journal deltas. Poster texture, live/pool state, scroll offset, page history, and per-origin consent are per-peer and never transmitted — each peer runs its own pool, and only the peer who focuses a portal gives it input. A peer that cannot resolve the locator paints `Unknown` naming the locator it tried (P1.portal.health). Consent is deliberately not journaled: a `.slate` you received must never arrive carrying someone else's decision to trust a host. | pattern | 85 |
| D27 | Agent surface | **Parity, mediated by the existing autonomy grant.** Agents and humans drive the identical `board.portal.web` / `portal.web.*` commands with identical privileges (Art. VII.1); agent-issued mutations stage for acceptance by default and apply directly where the workspace has granted autonomy (Art. VII.6) — the same rule that governs every command, not a portal-specific leash. Origin consent (D32) is a command like any other and follows that same path. The remaining restrictions are **capability-level, not agent-level**: nobody, human or agent, can read page DOM, cookies, or storage, or make the page issue a command, because no such channel exists (D15). Art. IX.5's "never available to an agent" clause governs write-back to sources and does not bite here — this portal never writes to its source. The context beacon carries locator, source kind, viewport, and state. | stated | 100 |
| D28 | Determinism & provenance | `n/a` — **P1.portal.determinism**: determinism is required of *generated* portals only (Art. V.3), and Art. IV.2 governs extracted graphs; neither describes a hosted page. What this portal owes instead is provenance, shown on the frame and in every export: resolved locator, source kind, content hash (local) or capture timestamp (remote), viewport width, and the origin recorded by packaging. | stated | 100 |
| D29 | Performance envelope | **A pool, not a webview per page.** At most `portal.web.live_pool` (6) webviews exist at once, admitted from the eligible set (D23) by priority: input-focused first, then greatest on-screen area, then most recently focused. Eviction captures a poster on the way out, so a demoted portal degrades to its last frame rather than blanking. Contents composite offscreen into a texture (WebView2 visual hosting), so portals obey z-order, rotation, and opacity like any node. Frame rates are budgeted, not free: the input-focused portal renders at the board's rate, other pooled portals at `portal.web.idle_fps` (5), and texture uploads are capped at `portal.web.uploads_per_frame` (2) per frame with the remainder held in a backlog that requests a repaint — the per-frame-budget rule the scanner and watcher already follow. Poster capture is async and generation-tagged; posters cache by (locator, content hash or capture epoch, viewport width) and evict with the thumbnail tier (Art. II.2). Nothing on the frame loop touches the network or the filesystem; the local mtime poll is a worker at ≥ 1 s. The research-hub case is therefore a hundred cached textures, at most six processes, and none at all once the board zooms past the strip threshold. A missing WebView2 runtime degrades every portal to `NoRuntime`, never a stall. | pattern | 80 |
| D30 | Failure & honesty states | `Unbound` ("Choose page or file…"). `Unknown` — not yet resolved, neutral marker, never blocking (P1.portal.health). `Blocked` — remote origin not yet permitted, host named. `Loading`. `Live`. `Budgeted` — eligible but outside the live pool, showing its last frame and saying so. `Poster(age)` — stale by construction, age stated. `Missing` — file gone, DNS failure, or 404, locator named, last poster kept. `Refused(reason)` — unsupported or dangerous scheme. `NoRuntime` — WebView2 runtime absent, with what to install. `TooSmall` — below `live_min_px`. `NotFramable` — export-time note when a remote site refuses iframing. Every state names itself in the frame rather than blanking it. | pattern | 85 |
| D31 | View-state ownership | Journaled authored intent: frame rect / rotation / opacity / fill, title, locator, entry, viewport width and zoom mode, `interactive_allowed`, `poster_capture` mode, export mode. Derived per-peer: poster texture and its capture time, live/pool membership, input focus, scroll offset, page navigation history, load errors, origin consent grants, WebView2 runtime availability. | pattern | 90 |
| D32 | Trust, sandbox & consent | Remote origins are `Blocked` until the human permits them; grants key on (workbook, origin), live in local app state, and are **never journaled or written into the `.slate`**, so a workbook you receive cannot arrive pre-trusting a host. Local sources under the workbook root need no prompt. The webview runs in a per-workbook user-data folder — no cookies, storage, or logins shared with the user's real browser. Denied outright: downloads, popups, notifications, geolocation, camera, microphone, clipboard read, and any host object or `postMessage` bridge (D15). Exports sandbox what they embed: packaged local content gets `sandbox="allow-scripts allow-same-origin"`, because a dashboard must fetch its own sibling data files; remote iframes get `allow-scripts` only. | pattern | 90 |

Source values: stated (user), precedent (approved or proposed for an
overlapping portal), pattern (catalog or constitution article), research
(source app), guess (agent proposal — must be confirmed before `agreed`).

## Feel constants

| Token | Meaning | Initial value |
|-------|---------|---------------|
| `portal.web.default_size` | Click placement size | `960 x 540` |
| `portal.web.poll_secs` | Local-source mtime poll floor | `1.0` |
| `portal.web.stale_alpha` | Poster alpha while recapturing | `0.6` |
| `portal.web.lod_strip_px` | Below this on-screen height, chrome strip only | `96` |
| `portal.web.live_min_px` | On-screen height that makes a portal pool-eligible | `320` |
| `portal.web.live_pool` | Webviews alive at once | `6` |
| `portal.web.idle_fps` | Render rate for pooled portals without input focus | `5` |
| `portal.web.uploads_per_frame` | Contents textures uploaded per frame | `2` |
| `portal.web.border_hit_px` | Border band that stays a Slate target while focused | `6` |
| `portal.web.viewport_default` | Authored CSS viewport width | `1280` |

## Golden paths

1. **GP1 — place unbound:** Portals flyout → Web portal → press-drag-release →
   host portal appears selected, unbound, painting "Choose page or file…";
   tool = Select.
2. **GP2 — bind local dashboard:** Choose folder… → pick a folder containing
   `index.html` → locator stored workbook-relative, `entry = index.html`,
   poster captured off-thread, chrome strip shows file glyph + folder name +
   `Ok`.
3. **GP3 — drag a browser tab:** drag a tab out of Edge onto the board → a
   bound portal appears at the drop point in state `Blocked` naming the host →
   Allow this origin → `Loading` → `Live`. Consent appears in neither the
   journal nor the saved `.slate`.
4. **GP4 — focus and release:** double-click a pooled portal → it takes input
   focus, clicks and keys reach the page, the accent border appears → Esc →
   focus released, the portal keeps rendering, scroll position preserved.
5. **GP5 — no live below threshold:** double-click a portal painted 120 px
   tall → the camera zooms to fit it, then it takes focus; no webview is
   created while it is below `live_min_px`.
6. **GP6 — rotation keeps rendering:** rotate a pooled portal 15° → contents
   keep updating inside the rotated rect and clicks still hit the frame
   through its rotated bounds.
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
