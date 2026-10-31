# Chat train reference baseline

Captured on 30 October 2026 in `America/New_York`. This package records the existing chat-train interaction and appearance before refinement. It does not approve or implement the broader agent-interaction design, new generator controls, or survey-derived changes.

## Evidence and its limits

`reference.js` records the current working-tree source and checked-in token values. Source HEAD at capture was `eede252ae6b24cd6ecb53a1a9ccf37768bb77040`; the working tree contains existing changes, so HEAD alone is not a source identity. The source hashes below pin the files actually read.

The two copied CT5 images were generated on 30 October with a **cached production renderer test executable**, last written on 25 October. They are real Slate renderer output for synthetic fixture content. They are not screenshots of the newly compiled current working tree or native desktop interaction. The cached executable’s source revision was not established. The owner’s supplied chat topbar image is separate historical reference evidence.

Cached renderer:

- Executable: `target/release/deps/slate-c10d0371c5e7de79.exe`
- Last write: `2026-10-25T22:05:43.166089-04:00` (`2026-10-26T02:05:43.166089Z`)
- Size: 75,823,104 bytes
- SHA-256: `9D5B8FC79B44770B57F3BC3C99EB3322BD882676DDE0A6C51100CAB6DAF1FA43`
- Test: `app::tests::review::chat_train::ct5_top_bar`
- Arguments: `--exact --ignored --nocapture`
- Execution reported by the parent task: 1 passed, 2,326 filtered out, 3.37 seconds.
- Original outputs: `target/review/CT5/idle_light_zoom2.png` and `idle_dark_zoom1.png`.

The browser rendition must be compared with this evidence before any fidelity acceptance. Matching the font file and design numbers does not establish identical browser/egui shaping, rasterization, clipping, or interaction. No claim of perfect current-native pixel equivalence is made here.

## Current appearance at zoom 1

- Default/canonical card width 320, initial height 200; pair initial height 280; draft minimum height 62; minimum width 176. Fitting may shrink cards around their title and content. Default horizontal gap 96; branch lane gap 72.
- A normal card has an 8-unit rounded scene outline and the theme’s card fill. No default border. The current `agent_card_outline` paint path draws a black polygon offset one unit down at opacity 0.10 dark / 0.035 light, followed by the card; it has **no blurred shadow**. The older rectangle helper `agent_card` has a 5-unit shadow blur. Cached CT5 images may reflect an earlier/helper shadow treatment; do not import that blur into the current-outline source baseline silently.
- Header status and output dots have diameter 7, inset 6, and center y10. Title is proportional size13, beginning x16. Its x-height is aligned to the top of the header dot through a measured font rise. Title and model share one line, separated by ` · `; there is no colored header band.
- Ellipsis dots are diameter4.2 at pitch5.88. Centers relative to card right edge: −28.76, −22.88, −17; y10. Idle ink opacity0.55; hover opacity0.95 and radius +22%, animated over120ms.
- Fold chevrons belong to the **header**, centered 39.7 units left of the right edge at y10. They appear on hover. Half-width2.4, rise1.35, stroke0.55, fade100ms. The bottom edge is a 14-unit resize band, not a fold toolbar.
- Transcript text is proportional size14, inset16 on both sides. Open/collapsed text begins y32; sent summary text begins y28. Collapsed summaries show the first three lines. User and assistant use the same text ink and left alignment. There are no speech bubbles, avatars, role headings, or alternating color blocks.
- Pair mode inserts a 10-unit separator row, with a one-unit secondary-ink rule at opacity0.4 inset12 within the text column. Transcript item spacing is12.
- Composer is a frameless, zero-margin multiline field at size14. Bottom padding14; preceding transcript gap18; two units of caret slack. An empty composer takes one measured font row. Enter submits. Only the current tail has a composer; sent cards do not retain independent empty input boxes.
- Header output dot continues the train; while responding a bare 7×7 Stop square takes its place. Context and generated-file return dots are separate side handles whose y is half the computed collapsed-card height, not the current card’s center.
- Derived history rails connect the header datum at y10 with horizontal Bézier tangents. Width4.5; secondary-color opacity0.42 dark /0.14 light; fork strand pitch5.5. These are conversation history rails rather than ordinary removable context wires.
- Ordinary chat currently uses its flat scrollable model menu. Provider-advertised effort/speed generator submenus are outside this baseline. The fixture model labels `Auto`, `Astra`, `Terra`, `Luna` are reference labels, not a live model availability claim.

## Token and style owners

The numbers in `reference.js` come from `crates/atlas-shell/ui-tokens.toml`, current chat painters/metrics, and `slate-doc::agent_chat`. Token values may be altered by a running UI tuner; such unsaved runtime changes are not established by this package.

Dark board/card/text/secondary: `#0e1013` / `#1c2026` / `#dde2e8` / `#878e96`. Light: `#f6f7f8` / `#ffffff` / `#1b1e22` / `#878e96`.

Shared action menu: radius5, padding3.5, row height30, horizontal padding8, row gap1.5, icon16.5, icon gap8.5, hover radius4.5, text13, tracking0.35. The chat ellipsis menu sets width224–260. Current chat model rows retain their existing selectable-list implementation; shared action-row metrics must not be used to silently redesign those rows.

## Font and unmodified reference assets

`assets/Ubuntu-Light.ttf` is copied byte-for-byte from `epaint_default_fonts-0.31.1/fonts/Ubuntu-Light.ttf` in the local Cargo registry. Use it as `ChatTrainUbuntu` at weight300. This is the proportional font underlying the inspected egui setup; there is no font conversion. `assets/Ubuntu-Font-Licence.txt` is its matching `UFL.txt`, retained verbatim.

The PNGs are unmodified copies. No image-generation, crop, resize, or image-edit operation was performed.

| Asset | SHA-256 |
|---|---|
| `Ubuntu-Light.ttf` | `80307B8DA7649AA4EE4D484B232140E3CE1EC0CA093073D3C53C8F5A5CED7A70` |
| `Ubuntu-Font-Licence.txt` | `2F0015108D68627BD788D313F529C21FF4DA2C2C42A5E1F3883ACC83480F9002` |
| `cached-renderer-idle-light-zoom2.png` | `F84208D35519A7F551703BBDF5E8DBA4A6FDF0A28E67078D6CC25E5F07361E98` |
| `cached-renderer-idle-dark-zoom1.png` | `C0B9DCCB51A4FC8DAA0BE6B9616DE0FB2F51D1E693CC2FC67DF1F593B3D00522` |
| `owner-chat-topbar-reference-20261010.png` | `2A02F7B2EB4A81250E251CE0D44F44C59F18514676E4AAAA2885D802143E28C1` |

Owner image source: `docs/requests/2026-10-10/chat-topbar-reference.png`. The date in the copied filename denotes its repository reference folder, not an independently established image capture timestamp.

## Pinned working-tree source hashes

Repository-relative paths, SHA-256 at capture:

| Source | SHA-256 |
|---|---|
| `crates/atlas-shell/ui-tokens.toml` | `927B395034D9F00557242E0E3686D9A17EBB9560DBB012D42EF075A6471A269A` |
| `crates/atlas-shell/src/tokens_chat_card.rs` | `F9EFB5BE48C967AA7B3800D0E184D58FCAB79245E502424D07EBA0DAAF490B23` |
| `crates/atlas-shell/src/selection_tools.rs` | `5C0E82343D24969F1048F5FCC3FA80145B977FA1E768ECCBB5977E2EE460115B` |
| `crates/slate-doc/src/agent_chat.rs` | `0B598F80A73ACFE8A9AD0CCE8C1F53D9E0021B2ACB6FC995965DB92F8EB6D61C` |
| `crates/slate-doc/src/media.rs` | `B1B4F8668E088E1525381B84B41831BA7516F86EB4E3510DD20A9E40309A817D` |
| `apps/slate/src/app/board_agent/card_metrics.rs` | `635F265C25BA55B6C38AB4F7CBA0AACDA60627DA532BB8421BE76AE68744780F` |
| `apps/slate/src/app/board_agent/card_fit.rs` | `3167ACC8ADF5ECC53D1A2FEC99A6BA8A12DF7EB6FA395D623F37F281FD4C81AF` |
| `apps/slate/src/app/board_agent/portal_header.rs` | `A280A8282CC6B86AD519DF2A137BD2123DF59E84A1C1EF7B18DF73D3FC214353` |
| `apps/slate/src/app/board_agent/portal_bound.rs` | `6ADA3E302845C253F0E311180E3089801CCAC2C6F0C6E0CBA1F478245A5E56BE` |
| `apps/slate/src/app/board_agent/header_menu.rs` | `2891B67B3E5550A3F31569E415FF068A630382C865B8EE51D0571DB4F4E9D3E4` |
| `apps/slate/src/app/board_agent/results.rs` | `32510EA371D07191F1C945E6B6579A9FBB73D246B188CA2B0D632C110A106670` |
| `apps/slate/src/app/board_agent/chat_edge.rs` | `F1593D7F2C7BCAADB333218F3CD51E605C8B596E1A1EB15224FF5F18F1BC6C9A` |
| `apps/slate/src/app/board_agent/interactive_text.rs` | `AAA55C8BDDE3CF3B178538E238D52637766CF59378E01E6076077FFD50640064` |
| `crates/atlas-shell/src/selection_tools/generator_activity/fold.rs` | `84D27594E2A871DA8CDFF4032612928F75854BD7B46644FD26F267225CF19748` |

This package itself changes only local reference/prototype files and copies reference assets. It changes no production application code.
