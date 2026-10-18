# Slate registered command index

Generated snapshot of all 289 command records in [commands.rs](../../apps/slate/src/app/commands.rs), captured 14 October 2026 from the current working tree. Read the [interaction inventory](01-slate-interactions.md) for context, precedence, and current behavior.

Each entry preserves the registered name, stable command ID, binding or gesture description, and repeat policy. Repeatable means eligible for idle Space or Enter repeat; Never means excluded. A record may describe a local gesture rather than a keyboard chord. Registry presence alone does not prove that an action is available in every state or still exposed by current chrome.

This file is derived reference material. Change the application registry to change a command, then regenerate this snapshot. Do not treat this file as a second command specification.

## Application

- **Check for updates** (`app.updates.check`): Advanced → Software updates [Repeat: Never; source line 51.]

- **Download update** (`app.updates.download`): Update prompt → Download update [Repeat: Never; source line 52.]

- **Update and restart** (`app.updates.install`): Update prompt → Update and restart (save all workbooks first) [Repeat: Never; source line 53.]

- **Dismiss update prompt** (`app.updates.later`): Update prompt → Later [Repeat: Never; source line 54.]

- **Refresh connections** (`app.packs.refresh`): Advanced → Connections → Refresh (probes every pack on a worker) [Repeat: Never; source line 55.]

- **Forget a trial pack** (`app.packs.forget`): Advanced → Connections → Forget [Repeat: Never; source line 56.]

- **Add OpenAI API key** (`app.packs.openai_key`): Generate chooser → OpenAI · add API key [Repeat: Never; source line 57.]

## Board

- **Fill closed shape with image** (`board.image.shape_fill`): Drag an image into a closed shape; Alt keeps the source. Double-click to pan or scale pixels behind the fixed mask [Repeat: Never; source line 50.]

- **Media: Image** (`board.media.image`): Media > Image: choose images, PDFs, or PowerPoint [Repeat: Repeatable; source line 58.]

- **Media: 3D** (`board.media.model`): Media > 3D: choose a 3D model (Rhino, OBJ, STL, glTF, or a recognized file such as Blender, DWG, or SketchUp) [Repeat: Repeatable; source line 59.]

- **Media: Video** (`board.media.video`): Media > Video: place a video. Hover across it to scrub; click to play [Repeat: Repeatable; source line 60.]

- **Media: Text** (`board.media.text`): Media > Text: choose a Word document, spreadsheet, CSV, Excel workbook, or source file. Double-click plain text and source cards to edit authored canvas words; the linked file stays intact [Repeat: Repeatable; source line 61.]

- **Media: new text document** (`board.media.text_new`): " or ' then Enter on the board (Grasshopper panel): a blank linked .txt card at the pointer with the caret ready; Esc cancels [Repeat: Never; source line 62.]

- **Media: unbundle pages** (`board.media.unbundle`): Selection strip Pages: spread every page onto the board as a selected grid [Repeat: Never; source line 64.]

- **Adjust corner radius** (`board.shape.fillet`): Selected rectangle, polygon, polyline, frame, portal, or image: drag the square grip along its edge (on the border at the corner's tangent point); Shift snaps to whole units; Esc cancels; one undo step on release. Click the grip to type the amount; Enter applies, Esc cancels. With several selected, each shows its grip and one drag or typed amount sets them all, clamped per node. A single polyline shows a grip per corner that sets that corner alone [Repeat: Never; source line 65.]

- **Add or remove a polygon side** (`board.shape.sides`): Selected polygon: hover a vertex, then click the + outside it to add a side while that vertex stays put and the others re-space around it, or the − inside it to remove one and keep a vertex there (3 to 12 sides); one undo step each [Repeat: Never; source line 66.]

- **Edit shape properties** (`board.shape.edit`): Selection strip: Fill / Stroke / Corners / Filters / Text / Bumper; RGB, opacity, stroke details, fillet/chamfer, photo-filter intensity, typeface, justification, font size, bold/italic/underline, font color, bumper buffer and friction [Repeat: Never; source line 67.]

- **Edit shape text** (`board.shape.text`): Double-click anywhere on a closed shape to open the text editor and a blinking caret. New text is center-justified [Repeat: Never; source line 68.]

- **Edit spreadsheet cell** (`board.sheet.edit`): Double-click a CSV or Excel card to open it. Click a cell, type, and press Enter. Save writes the file; clicking off asks when there are unsaved cells [Repeat: Never; source line 69.]

- **Scroll spreadsheet** (`board.sheet.scroll`): After a double-click into a CSV or Excel card, scroll the wheel to move through rows. Shift+scroll moves across columns. Drag a grid line to resize that row or column [Repeat: Never; source line 70.]

- **Add spreadsheet column** (`board.sheet.column`): With a CSV or Excel card open, hover and click + beside the header to add a column, then type its name [Repeat: Never; source line 71.]

- **Edit wire properties** (`board.wire.edit`): Wire selection palette: color, weight, Square / Bezier, Solid / Dashed, None / Arrows [Repeat: Never; source line 72.]

- **Canvas wire settings** (`board.wire.canvas`): Right-click empty canvas: set color, width, and Orthogonal / Bézier routing for all unlocked wires in one undo step [Repeat: Never; source line 73.]

- **Edit shape dimension** (`board.shape.dimension`): Click an external dimension stringer; Enter applies around the center, Escape cancels [Repeat: Never; source line 74.]

- **Sample desktop color** (`board.color.desktop`): Eyedropper in any color editor: click it, then click any desktop pixel; or press it, drag to any desktop pixel and release. Escape cancels [Repeat: Never; source line 75.]

- **Polyline tool** (`board.tool.polyline`): Shapes: click vertices; Enter or Esc commits placed points; snap to the start to close. Shift = 45° from the last vertex; Tab locks the direction [Repeat: Repeatable; source line 76.]

- **Arc tool** (`board.tool.arc`): Shapes: pick three points for an arc. Shift = 45° from the last point; Tab locks the direction [Repeat: Repeatable; source line 77.]

- **Bezier span tool** (`board.tool.bezier`): Shapes: place anchors and drag tangent handles. Shift = 45° from the last anchor; Tab locks the direction [Repeat: Repeatable; source line 78.]

- **Lock segment direction** (`board.draft.direction_lock`): Tab while drawing a segment (Line, Polyline, Arc, Bézier span, the Brush or Eraser Shift line): locks the direction from the last point toward the pointer, which then changes only the length. Tab again releases; placing the point or Esc ends it. Hold Shift for 45° steps [Repeat: Never; source line 173.]

- **Select tool** (`board.tool.select`): V (Board view), or the combined Select/Pan bottom-dock button [Repeat: Repeatable; source line 320.]

- **Pan tool (hand)** (`board.tool.pan`): H, or middle-drag / Space + drag (Board view) [Repeat: Repeatable; source line 330.]

- **Toggle Select ⇄ Pan** (`board.tool_swap`): Click the combined Select/Pan bottom-dock button while it is active [Repeat: Never; source line 340.]

- **Frame tool (slides)** (`board.tool.frame`): F — a small frame ghost follows the pointer; click or drag (Board view) [Repeat: Repeatable; source line 350.]

- **Deck** (`board.tool.deck`): Deck squircle on a selected frame — click frames, or draw a stroke through them [Repeat: Repeatable; source line 360.]

- **Shapes / Curve / Text tools** (`board.tools_row`): R / O / L / T (Board view); click or hover a bottom-dock button to open its submenu [Repeat: Never; source line 370.]

- **Rectangle tool** (`board.tool.rect`): R — a small rectangle ghost follows the pointer; click or drag (Board view) [Repeat: Repeatable; source line 380.]

- **Ellipse tool** (`board.tool.ellipse`): O — a small ellipse ghost follows the pointer; click or drag (Board view) [Repeat: Repeatable; source line 390.]

- **Polygon tool** (`board.tool.polygon`): Y — regular polygon (default hexagon); click or drag; Shift locks aspect (Board view) [Repeat: Repeatable; source line 400.]

- **Line tool** (`board.tool.line`): L (Board view). Shift = 45° from the first point; Tab locks the direction [Repeat: Repeatable; source line 410.]

- **Agent portal** (`board.portal.agent`): Palette: agent portal; a small portal ghost follows the pointer; click or drag (Board view) [Repeat: Never; source line 420.]

- **Web portal** (`board.portal.web`): Palette: web portal; click or drag places a Google start page; drop a page link or URL text to create/rebind a portal, paste a URL, or drop an .html file / index.html folder. Browser pictures paste as images (Board view) [Repeat: Never; source line 430.]

- **File Atlas lens portal** (`board.portal.atlas`): Palette: file atlas; click or drag places an unbound folder map; drop a folder to choose a lens (Board view) [Repeat: Never; source line 448.]

- **Slate board portal** (`board.portal.slate`): Palette: slate board; click or drag places an unbound frame; drop a .slate file on a board that already has content to open or insert it [Repeat: Never; source line 458.]

- **Slate board: choose workbook** (`portal.slate.source`): Portal menu or empty-state button; binds a local .slate file [Repeat: Never; source line 468.]

- **Slate board: refresh** (`portal.slate.refresh`): Reload the bound workbook from disk [Repeat: Never; source line 478.]

- **Slate board: open workbook** (`portal.slate.open`): Open the bound workbook as a tab [Repeat: Never; source line 488.]

- **File Atlas: choose folder** (`portal.atlas.source`): Portal sidebar or empty-state button; binds a local folder [Repeat: Never; source line 498.]

- **File Atlas: refresh** (`portal.atlas.refresh`): Portal sidebar; restarts the folder scan [Repeat: Repeatable; source line 508.]

- **File Atlas: bake poster** (`portal.atlas.bake`): Portal sidebar; writes a poster of the current view as an authored image; the portal stays live [Repeat: Never; source line 518.]

- **File Atlas: enter or leave contents** (`portal.atlas.focus`): Double-click the frame or this command; Esc leaves [Repeat: Never; source line 528.]

- **File Atlas: open in File Atlas** (`portal.atlas.open`): Opens the existing Slate-hosted File Atlas viewport on this folder [Repeat: Never; source line 538.]

- **File Atlas: drag files to the board or another application** (`portal.atlas.drag_out`): In contents-focus, left-drag a file onto the Slate board to link and place it. Leave the window for a File Explorer-compatible shell drop (copy/link, never move). Esc cancels [Repeat: Never; source line 548.]

- **File Atlas: expand or collapse folder** (`portal.atlas.collapse`): In contents-focus, click a folder card to toggle it; the incremental grip opens one level, the full grip opens the subtree. Same grips as standalone File Atlas [Repeat: Never; source line 558.]

- **File Atlas: select files** (`portal.atlas.select`): In contents-focus, click a file to select it; Ctrl+click toggles it. Drag a box on empty canvas (or Shift-drag) to select multiple files. Drag a selected file to carry the selection [Repeat: Never; source line 568.]

- **File Atlas: zoom to fit** (`portal.atlas.fit`): Fit the folder map to the portal window. Formatting squircle on a selected File Atlas portal [Repeat: Never; source line 578.]

- **Web portal: choose page or file** (`portal.web.source`): Portal sidebar or empty-state button; binds a URL, an .html file, or a folder with an entry file [Repeat: Never; source line 588.]

- **Web portal: allow this origin** (`portal.web.allow_origin`): Portal sidebar; permits one remote origin for this workbook. Local trust only — never journaled or saved [Repeat: Never; source line 598.]

- **Web portal: home** (`portal.web.home`): Portal sidebar; returns the live page to the authored locator without changing the workbook [Repeat: Never; source line 608.]

- **Web portal: reload** (`portal.web.reload`): Portal sidebar; reloads the bound page in place [Repeat: Repeatable; source line 618.]

- **Web portal: back** (`portal.web.back`): In-page history for the focused portal. Following links changes what the frame shows, never what the workbook says [Repeat: Repeatable; source line 628.]

- **Web portal: forward** (`portal.web.forward`): In-page history for the focused portal, forward [Repeat: Repeatable; source line 638.]

- **Web portal: recapture poster** (`portal.web.recapture`): Portal sidebar; retakes the cached still the frame shows when it is not live [Repeat: Repeatable; source line 648.]

- **Web portal: take input focus** (`portal.web.focus`): Double-click a pooled portal, or Enter on the selection; Esc releases without stopping it [Repeat: Never; source line 658.]

- **Web portal: open in system browser** (`portal.web.open_external`): Ctrl+double-click the frame, or the Portal sidebar button [Repeat: Never; source line 668.]

- **Web portal: package into workbook folder** (`portal.web.package`): Portal sidebar; copies a local page and its folder beside the workbook and rewrites the locator (a fork that names its origin) [Repeat: Never; source line 678.]

- **Web portal: bake poster** (`portal.web.bake`): Portal sidebar; adds the captured poster plus a provenance note as authored nodes, leaving the portal in place [Repeat: Never; source line 688.]

- **Web portal: copy URL** (`portal.web.copy_url`): Right-click the portal or its identity tab; copies the displayed locator [Repeat: Never; source line 698.]

- **Web portal: paste URL** (`portal.web.paste_url`): Right-click the portal or its identity tab; rebinds that portal to the clipboard locator (journaled) [Repeat: Never; source line 708.]

- **Portal: maximize** (`portal.maximize`): Four-corner maximize square (upper-right, or the web tab's window-control slot), or right-click → Maximize. Fills the window at the screen aspect and covers Slate chrome. Esc restores [Repeat: Never; source line 718.]

- **Portal: hide or show tab** (`portal.chrome.toggle`): Right-click Hide tab or use this command to fold the web identity tab; hover the top interior to restore [Repeat: Never; source line 728.]

- **Agent portal: select folder** (`portal.agent.bind`): Empty-state Select folder or inspector; binds a project folder (journaled source) [Repeat: Never; source line 738.]

- **Agent portal: send prompt** (`portal.agent.send`): Enter in the agent composer sends; Shift+Enter inserts a new line [Repeat: Never; source line 748.]

- **Agent portal: choose provider** (`portal.agent.provider`): Agent portal inspector; toggles the provider binding [Repeat: Never; source line 758.]

- **Image wire: toggle whole bundle** (`portal.agent.wire_output`): Toggle between all images and a pinned current image on the selected wire [Repeat: Never; source line 768.]

- **Agent portal: unbundle images** (`portal.agent.unbundle`): Split completed images into independent generator portals, in one undo step [Repeat: Never; source line 769.]

- **Agent portal: stop response** (`portal.agent.stop`): Interrupt this portal's Codex, Cursor, or ComfyUI run; on a streaming chat card, click Stop on its top output circle [Repeat: Never; source line 770.]

- **Agent: show chat train** (`portal.agent.train`): Present linked messages as top-aligned cards [Repeat: Never; source line 771.]

- **Agent: message pairs** (`portal.agent.pairs`): One user message and its reply on each card [Repeat: Never; source line 772.]

- **Agent: choose model** (`portal.agent.model`): Click the model name to choose the next Codex or Ollama model, or a ComfyUI checkpoint (Auto picks one for the wired inputs) [Repeat: Never; source line 773.]

- **Generator: live render** (`portal.agent.live`): Hover a ComfyUI generator and press Live. It re-renders when the wired model's camera, the prompt, or the checkpoint changes; double-click the card to fly the model from it [Repeat: Never; source line 774.]

- **Generator: keep live frame** (`portal.agent.keep`): Press Keep on a live generator. The shown frame stays in the album and rendering continues [Repeat: Never; source line 775.]

- **Agent: rename conversation** (`portal.agent.rename`): Click the conversation name in the header to rename the connected branch [Repeat: Never; source line 776.]

- **Agent: place continuation** (`portal.agent.continue`): Click or drag the terminal output handle to place a message [Repeat: Never; source line 777.]

- **Agent: text, image or agent from this media** (`portal.agent.spawn`): Select a picture, video, 3D model, text or generated frame, open its Agent squircle, choose Text or Image, write the prompt and Submit; or click a generator's output port. A new frame appears downstream, wired to the matching input port, and runs. The output-port menu and a wire dropped on empty board also offer Agent: a local chat train bound to the workbook's assets/agent folder, with a new chat and the source wired in as its first input [Repeat: Never; source line 778.]

- **Agent: answer permission request** (`portal.agent.approval`): Allow once or deny a provider tool request [Repeat: Never; source line 779.]

- **Agent: show artifacts** (`portal.agent.artifacts`): Click a coding-agent reference or changed-document handle [Repeat: Never; source line 780.]

- **Agent: open artifact portal** (`portal.agent.open_artifact`): Choose an artifact from its handle list [Repeat: Never; source line 781.]

- **Agent: spawn or retract an output** (`portal.agent.spawn_output`): Click a capsule in a chat card's output stack, or drag it onto the board to place it at the drop point [Repeat: Never; source line 782.]

- **Agent: spawn outputs as a group** (`portal.agent.spawn_outputs`): Click Spawn all (or Spawn N after Shift-clicking capsules) in a chat card's output stack. A second press retracts the group [Repeat: Never; source line 783.]

- **Agent: show a file's evolution** (`portal.agent.evolution`): Click a capsule's vN badge in a chat card's output stack. A second click retracts the frame [Repeat: Never; source line 784.]

- **Agent: single chat window** (`portal.agent.chat`): Collapse the conversation into one window per branch [Repeat: Never; source line 785.]

- **Agent: bundle messages** (`portal.agent.bundle_chat`): Collapse a consecutive selection without changing history [Repeat: Never; source line 786.]

- **Agent: expand messages** (`portal.agent.expand_chat`): Restore the original bundled message cards [Repeat: Never; source line 787.]

- **Agent: fork at checkpoint** (`portal.agent.fork`): Ellipsis → Fork chat here, or drag the top output circle. Creates one forked chat at that checkpoint, journaled as one command and joined by the history rail. Each drag is one fork. A click without a drag still continues. A coding-agent branch gets its own session, seeded with the transcript up to that card; continuing the unbranched tail keeps the session [Repeat: Never; source line 788.]

- **Agent: collapse or expand card** (`portal.agent.collapse`): Click a chevron left of a chat card's ellipsis. A single chevron steps one level; a double chevron steps two. From maximized that is partial, then collapsed. From collapsed that is partial, then maximized. From partial both directions work, one single chevron each way. Hover highlights the mark and shows no tooltip. Detail is step-expand, jump-expand, step-collapse, or jump-collapse. With no detail, selected cards toggle between full and the three-line capsule. The tail keeps its composer [Repeat: Never; source line 789.]

- **Agent: fit card to text** (`portal.agent.fit`): Ellipsis menu → Fit to text, on a chat card you resized [Repeat: Never; source line 790.]

- **Agent: repeat a message** (`portal.agent.schedule`): Ellipsis menu → Schedule… opens a dialog: message, date and time (tonight, Oct 1, 4:55 am), Once / Every hour / Every day / Every week, and a required end (until next month, until next year, or a date). Windows Task Scheduler stores that end and will not register a task without one. A schedule saved without an end stays until you set one. A pending schedule stays in this menu, not on the card's top bar [Repeat: Never; source line 791.]

- **Agent: full access** (`portal.agent.full_access`): Ellipsis menu → Full access. This conversation runs commands and edits files without asking, from its next message. Stored for this user on this machine, never in the workbook [Repeat: Never; source line 792.]

- **Agent: show or pocket used context** (`portal.agent.pocket`): Click a chat card's lower context handle. Deleting context a card already sent pockets it into that card [Repeat: Never; source line 793.]

- **Start crosstalk** (`portal.agent.crosstalk.link`): Drag from a Cursor or Codex chat card's top or bottom port onto another conversation's card; or select two chat cards and run this [Repeat: Never; source line 794.]

- **Crosstalk: start** (`portal.agent.crosstalk.start`): Start in the capsule on a new crosstalk wire. Your grant to relay between these two conversations until the stop rule fires [Repeat: Never; source line 795.]

- **Crosstalk: open or fold a wire's capsule** (`portal.agent.crosstalk.capsule`): Click the blister at a crosstalk wire's mid-span; click away or Esc folds it [Repeat: Never; source line 796.]

- **Crosstalk: pause** (`portal.agent.crosstalk.pause`): Pause in the wire's capsule. Nothing relays until Resume [Repeat: Never; source line 797.]

- **Crosstalk: resume** (`portal.agent.crosstalk.resume`): Resume in the wire's capsule, including after reopening a board [Repeat: Never; source line 798.]

- **Crosstalk: stop** (`portal.agent.crosstalk.stop`): Stop in the wire's capsule. Journaled; Undo brings the crosstalk back [Repeat: Never; source line 799.]

- **Crosstalk: edit roles or stop rule** (`portal.agent.crosstalk.edit`): Edit… in a wire's capsule: turns, minutes and goal, from that wire on; roles while paused [Repeat: Never; source line 800.]

- **Crosstalk: send the reply** (`portal.agent.crosstalk.send`): Send to … beside the newest wire's blister when a reply is ready (Step mode): it becomes the other agent's next message [Repeat: Never; source line 801.]

- **Crosstalk: skip and pause** (`portal.agent.crosstalk.skip`): Skip and pause in the capsule: the reply stays on its card, unrelayed [Repeat: Never; source line 802.]

- **Crosstalk: 5 more turns** (`portal.agent.crosstalk.more`): In the capsule of a crosstalk that stopped at its turn limit [Repeat: Never; source line 803.]

- **Crosstalk: let the builder act on the other agent's messages** (`portal.agent.crosstalk.trust`): Tick in the Start capsule when the builder has Full access. Stored for this user, never in the workbook [Repeat: Never; source line 804.]

- **Agent: identity detail** (`portal.agent.identity`): Name and top history anchors [Repeat: Never; source line 805.]

- **Agent: summary detail** (`portal.agent.summary`): Name and conversation excerpt [Repeat: Never; source line 806.]

- **Agent: full detail** (`portal.agent.full`): Full conversation and composer [Repeat: Never; source line 807.]

- **Agent portal: reveal link folder** (`portal.agent.reveal`): Agent portal inspector; opens the session folder under the AI workspace [Repeat: Never; source line 808.]

- **Agent portal: launch provider** (`portal.agent.launch`): Opens the bound folder in Cursor (or the configured provider). Right-click or inspector [Repeat: Never; source line 818.]

- **Agent portal: take contents focus** (`portal.agent.focus`): Double-click the frame, or Enter on the selection; Esc releases. Selection alone does not hand the wheel to Cover Flow [Repeat: Never; source line 828.]

- **Agent portal: get Cursor API key** (`portal.agent.get_key`): Opens the Cursor dashboard page where a user API key is minted, then paste it in the portal [Repeat: Never; source line 838.]

- **Agent portal: switch agent** (`portal.agent.switch_chat`): After binding a project, pick an existing Cursor agent for that folder (or Switch agent later). Not a live IDE-thread attach. [Repeat: Never; source line 848.]

- **Accept staged agent proposal** (`stage.accept`): Agent portal inspector; commits the proposal as one attributed undo group [Repeat: Never; source line 858.]

- **Reject staged agent proposal** (`stage.reject`): Agent portal inspector; writes a rejection result and leaves the board untouched [Repeat: Never; source line 868.]

- **Text tool** (`board.tool.text`): T — a small text-box ghost follows the pointer; click to place (Board view); double-click text to edit [Repeat: Repeatable; source line 878.]

- **Pen tool (freehand path)** (`board.tool.pen`): P (Board view) [Repeat: Repeatable; source line 888.]

- **Duplicate** (`board.duplicate`): Alt + drag selection, Alt + drag a scale handle (original stays), or Ctrl + D [Repeat: Repeatable; source line 898.]

- **Resize (free aspect / distort)** (`board.resize_free`): Hover any edge or corner (no prior selection); Shift + drag corner (corners scale proportionally by default); Shift + drag edge locks aspect instead [Repeat: Never; source line 908.]

- **Resize from center** (`board.resize_center`): Ctrl + drag an edge or corner (Board view) [Repeat: Never; source line 920.]

- **Draw square / circle** (`board.draw_square`): Shift + drag with Frame, Rectangle, or Ellipse tool [Repeat: Never; source line 930.]

- **Draw rectangle or ellipse from center** (`board.draw_from_center`): Ctrl + drag with the Rectangle or Ellipse tool. Shift still locks a square or circle. The press point stays the center. [Repeat: Never; source line 940.]

- **Smart guides (align to objects)** (`board.smart_guides`): Preferences → Snaps. On by default; Alt suspends. Only nearby objects in the same row or column participate. [Repeat: Repeatable; source line 950.]

- **Smart-guide reach (cycle)** (`board.snap_reach`): Preferences → Snaps. Cycles Tight / Nearby / Wide. [Repeat: Repeatable; source line 961.]

- **Smart-guide reach: Tight** (`board.snap_reach.tight`): Only objects within ~160 px, same row or column [Repeat: Never; source line 971.]

- **Smart-guide reach: Nearby** (`board.snap_reach.nearby`): Default. Neighbors within ~360 px, same row or column [Repeat: Never; source line 981.]

- **Smart-guide reach: Wide** (`board.snap_reach.wide`): Anything in view that shares your row or column [Repeat: Never; source line 991.]

- **Rotate object** (`board.rotate`): Hover outside a rotatable node's corner (any polygon vertex, clear of its +/− and point pick); drag to rotate (snaps at 45°; Shift locks a polygon's gripped vertex to cardinal directions; Alt suspends). With 2+ selected, rotates the group about its center; portals in the group stay axis-aligned [Repeat: Never; source line 1001.]

- **Group resize (multi-selection)** (`board.group_resize`): With 2+ selected, drag a group bounding-box edge or corner — scales all members about the opposite corner/edge; corners scale proportionally, Shift distorts, Ctrl scales from center. Ctrl+Alt+Shift repositions members without changing their size [Repeat: Never; source line 1013.]

- **Crop image (enter crop mode)** (`board.crop_enter`): Crop on the image selection strip, double-click an image, or right-click → Crop image (images, PDF pages, video posters — not 3D viewports or text snippets) [Repeat: Never; source line 1026.]

- **Crop selected image** (`board.crop`): C with one or more croppable images selected (Board view) [Repeat: Repeatable; source line 1038.]

- **Crop: move the window / pan the content** (`board.crop_window`): In crop mode, drag a side blister to mask the image in place. Every other selected croppable image takes the same crop as you drag [Repeat: Never; source line 1048.]

- **Finish cropping** (`board.crop_finish`): Enter, Escape, or click outside the image (crop mode) [Repeat: Never; source line 1059.]

- **Open image file** (`board.open_file`): Right-click object → Open file (double-click enters crop mode instead) [Repeat: Never; source line 1069.]

- **Finish text editing** (`board.text_finish`): Escape, or click anywhere outside the text box [Repeat: Never; source line 1079.]

- **Board grid** (`board.grid`): G or F7, or Preferences → Grid [Repeat: Repeatable; source line 1089.]

- **Snap to grid** (`board.snap_grid`): F9, or Preferences → Snaps → Snap to grid [Repeat: Repeatable; source line 1099.]

- **Object snaps (master)** (`board.osnap`): Preferences → Snaps, or the command palette [Repeat: Repeatable; source line 1109.]

- **Drop toolbar to canvas** (`board.dock.drop`): Primary-circle context menu → Drop to canvas (left dock: two-dot column) [Repeat: Never; source line 1119.]

- **Toolbar Advanced (on-strip tags)** (`board.dock.advanced`): Retired from palette chrome; command surface keeps the id [Repeat: Never; source line 1129.]

- **Collapse / expand the entire toolbar** (`dock.bar.toggle`): Bottom readout blister, or hover beside / below the toolbar and click [Repeat: Never; source line 1139.]

- **Expand / collapse a tool family** (`dock.palette.toggle`): Bottom dock: click the primary circle; secondary icons expand right in the same capsule. Click again to collapse [Repeat: Never; source line 1140.]

- **Object snap: End** (`board.osnap.end`): Preferences → Snaps [Repeat: Repeatable; source line 1141.]

- **Object snap: Mid** (`board.osnap.mid`): Preferences → Snaps [Repeat: Repeatable; source line 1151.]

- **Object snap: Center** (`board.osnap.center`): Preferences → Snaps [Repeat: Repeatable; source line 1161.]

- **Object snap: Near** (`board.osnap.near`): Preferences → Snaps [Repeat: Repeatable; source line 1171.]

- **Object snap: Intersection** (`board.osnap.int`): Preferences → Snaps [Repeat: Repeatable; source line 1181.]

- **Object snap: Quadrant** (`board.osnap.quad`): Preferences → Snaps [Repeat: Repeatable; source line 1191.]

- **Object snap: Perpendicular** (`board.osnap.perp`): Preferences → Snaps [Repeat: Repeatable; source line 1201.]

- **Object snap: Tangent** (`board.osnap.tan`): Preferences → Snaps [Repeat: Repeatable; source line 1211.]

- **Cycle wire routing** (`board.wire.routing`): Wire selection palette — cycle Bezier / Square; no selection sets creation default [Repeat: Repeatable; source line 1221.]

- **Bezier wires** (`board.wire.bezier`): Wire selection palette; no selection sets creation default [Repeat: Repeatable; source line 1231.]

- **Orthogonal wires** (`board.wire.orthogonal`): Wire selection palette; no selection sets creation default [Repeat: Repeatable; source line 1241.]

- **Ortho (45° constraint)** (`board.ortho`): F8 — the constraint applies to drags in a later wave; Shift inverts while held, except on Brush and Eraser, where Shift always takes 45° steps [Repeat: Repeatable; source line 1251.]

- **Align left** (`board.align.left`): Align widget on a 2+ selection — left-edge cluster, or command palette [Repeat: Never; source line 1261.]

- **Align horizontal centers** (`board.align.center_h`): Align widget on a 2+ selection — center of a horizontal cluster [Repeat: Never; source line 1271.]

- **Align right** (`board.align.right`): Align widget on a 2+ selection — right-edge cluster [Repeat: Never; source line 1281.]

- **Align top** (`board.align.top`): Align widget on a 2+ selection — top of a vertical cluster [Repeat: Never; source line 1291.]

- **Align vertical centers** (`board.align.middle_v`): Align widget on a 2+ selection — center of a vertical cluster [Repeat: Never; source line 1301.]

- **Align bottom** (`board.align.bottom`): Align widget on a 2+ selection — bottom of a vertical cluster [Repeat: Never; source line 1311.]

- **Distribute horizontally** (`board.distribute.horizontal`): Align widget on a 3+ selection — fourth icon on a horizontal edge [Repeat: Never; source line 1321.]

- **Distribute vertically** (`board.distribute.vertical`): Align widget on a 3+ selection — fourth icon on a vertical edge [Repeat: Never; source line 1331.]

- **Delete objects** (`board.delete`): Delete or Backspace — with curve vertices picked (Direct Select anchors or Select grips), removes those vertices and rejoins the neighbors; too few left removes the curve. Picks on a rectangle or regular polygon do nothing [Repeat: Never; source line 1341.]

- **Nudge objects** (`board.nudge`): Arrow keys (Shift = ×10) [Repeat: Never; source line 1354.]

- **Marquee select** (`board.marquee`): Left-drag on empty board. Left to right selects only what is fully inside; right to left selects anything touched [Repeat: Never; source line 1364.]

- **Pan board** (`board.pan_gestures`): H (hand tool), middle-drag, right-drag, or Space + left-drag [Repeat: Never; source line 1374.]

- **Fit board content** (`board.fit`): Home (Board view) [Repeat: Never; source line 1384.]

- **Object menu (z-order, tags, delete, image Layers)** (`board.menu`): Right-click object [Repeat: Never; source line 1394.]

- **Undo board edit** (`app.undo`): Ctrl + Z [Repeat: Never; source line 1404.]

- **Redo board edit** (`app.redo`): Ctrl + Y (or Ctrl + Shift + Z) [Repeat: Never; source line 1414.]

- **Unlock / lock 3D viewport** (`board.model_lock`): Double-click a locked 3D model to enter it; Esc or a click outside locks it again, and live viewports auto-lock after 30 s idle (the frozen view becomes the slide image). Blender, DWG, SketchUp, and FBX use the same card and say when no preview exists. An Enscape standalone opens inside the card on double-click. A computer that cannot run it — not Windows, a rotated card, or a file built for a different Windows system — says so on the card. Slate never starts it on load [Repeat: Never; source line 1424.]

- **3D display mode** (`board.model_display`): Selection strip → Viewport display: Shaded, Arctic (white clay), Material mask, or Z-buffer. One undo step, live or frozen; the frozen poster uses that mode. From the palette, cycles the selected model. The image generator still receives a shaded view and a depth pass [Repeat: Never; source line 1440.]

- **Measure 3D model** (`board.model_measure`): Selection strip → Measure: enters the viewport if frozen, then click two points on the model (Rhino Distance). One measurement returns to navigate; Esc peels the pending point, then Measure, then the shown measurements. Measurements are not saved [Repeat: Never; source line 1450.]

- **Screenshot 3D viewport** (`board.model_screenshot`): Selection strip → Screenshot: export the current camera to the board or to PNG/JPEG/WebP with embedded Slate view metadata [Repeat: Never; source line 1460.]

- **Export viewport to canvas** (`board.model_screenshot_canvas`): From the screenshot menu: render the live or frozen pose, save under slate-outputs, and place a linked image beside the model [Repeat: Never; source line 1470.]

- **Export viewport to file** (`board.model_screenshot_save`): From the screenshot menu: save PNG, JPEG, or WebP with embedded camera metadata [Repeat: Never; source line 1480.]

- **Restore viewport from screenshot** (`board.model_view_restore`): Drop a Slate-tagged screenshot onto a 3D model (file or image node) to restore the saved camera as one undo step [Repeat: Never; source line 1490.]

- **Orbit 3D model** (`board.model_orbit`): Drag inside an unlocked viewport (Rhino-style Z-up orbit) [Repeat: Never; source line 1500.]

- **Pan 3D model** (`board.model_pan`): Shift + drag inside an unlocked viewport [Repeat: Never; source line 1510.]

- **Zoom 3D model** (`board.model_zoom`): Scroll inside an unlocked viewport [Repeat: Never; source line 1520.]

- **Finish path** (`board.path_finish`): Enter or double-click (Polyline / Bezier tools) [Repeat: Never; source line 1530.]

- **Cancel path** (`board.path_cancel`): Escape while drawing a path [Repeat: Never; source line 1540.]

- **Bring to front** (`board.to_front`): PageUp (Board view) [Repeat: Repeatable; source line 1550.]

- **Send to back** (`board.to_back`): PageDown or Ctrl + B (Board view) [Repeat: Repeatable; source line 1560.]

- **Copy objects** (`board.copy`): Ctrl + C (Board view; connectors between copied nodes come along) [Repeat: Repeatable; source line 1570.]

- **Cut objects** (`board.cut`): Ctrl + X (Board view) [Repeat: Repeatable; source line 1580.]

- **Paste objects** (`board.paste`): Ctrl + V — at the pointer when over the canvas, else at the view center; repeated pastes step +24, +24. A copied image or files on the clipboard land on the board. [Repeat: Repeatable; source line 1590.]

- **Paste in place** (`board.paste_in_place`): Ctrl + Shift + V — paste at the source coordinates [Repeat: Repeatable; source line 1600.]

- **Image adjustments (popover)** (`board.image.adjust`): Ctrl + U with image(s) selected (Board view) [Repeat: Repeatable; source line 1610.]

- **Invert image colors** (`board.image.invert`): Ctrl + I with image(s) selected (Board view) [Repeat: Repeatable; source line 1620.]

- **Pin image object selection** (`board.image.segment.commit`): Hover an image object, click to pin, then click to include or Alt-click to exclude [Repeat: Never; source line 1630.]

- **Polygonal image selection** (`board.image.selection.lasso`): Image editing palette: click vertices, Shift for 45°, Delete for the previous vertex, Enter to close, Esc to cancel [Repeat: Never; source line 1640.]

- **Create layer from image selection** (`board.image.selection.layer`): Right-click a pinned image selection and choose Create layer from selection [Repeat: Never; source line 1645.]

- **Add image paint layer** (`board.image.layer.add`): Layer palette + below an image being painted [Repeat: Repeatable; source line 1650.]

- **Copy image selection to canvas** (`board.image.selection.copy`): Drag a highlighted image object onto the canvas [Repeat: Never; source line 1660.]

- **Rename image layer** (`board.image.layer.rename`): Right-click a layer; type its name, then Enter or click away [Repeat: Never; source line 1662.]

- **Toggle image layer visibility** (`board.image.layer.visibility`): Right-click a layer and toggle Visible [Repeat: Never; source line 1664.]

- **Duplicate image layer** (`board.image.layer.duplicate`): Right-click Duplicate layer, or Alt-drag a layer [Repeat: Never; source line 1666.]

- **Reorder image layer** (`board.image.layer.reorder`): Drag layer previews; leftmost is topmost [Repeat: Never; source line 1668.]

- **Delete image paint layer** (`board.image.layer.delete`): Delete or Backspace while painting an image (active layer) [Repeat: Repeatable; source line 1670.]

- **Image paint layer opacity** (`board.image.layer.opacity`): Layer palette slider (active layer) [Repeat: Repeatable; source line 1680.]

- **Add image as paint layer** (`board.image.layer.add_image`): Replace / Add as layer capsule when dropping onto an image [Repeat: Never; source line 1690.]

- **Replace image pixels** (`board.image.replace`): Replace capsule when dropping one image onto another [Repeat: Never; source line 1700.]

- **Canvas palette** (`board.palette`): Double-click empty board — type to search commands, Enter places/executes [Repeat: Never; source line 1710.]

- **Brush tool (expressive ink)** (`board.tool.brush`): B — freehand stroke in the foreground color. [ ] size, Shift+[ ] softness, Shift+drag straight line from the last stroke's end (45° steps, Tab locks its direction), Shift+click connects to the last stroke, Shift+right-drag scrubs opacity, Alt+right-drag scrubs size and softness from the press point, Ctrl+right-drag opens the color wheel. Ctrl and Alt beat Shift. [Repeat: Repeatable; source line 1721.]

- **Brush stroke** (`board.brush.stroke`): Drag with the Brush tool [Repeat: Never; source line 1736.]

- **Pen straight line** (`board.pen.line`): Shift+drag with the Pen (45° steps, Tab locks); Shift+click connects [Repeat: Never; source line 1746.]

- **Eraser tool** (`board.tool.eraser`): E — drag or click over painted brush strokes to erase just that ink (soft, partial, and Shift-straight like the Brush); vector strokes the eraser crosses are removed whole. Images, text, frames, and wires are never erased. Esc cancels [Repeat: Repeatable; source line 1756.]

- **Eraser stroke** (`board.eraser.stroke`): Drag with the Eraser tool [Repeat: Never; source line 1769.]

- **Smoothing tool** (`board.tool.smooth`): S — drag a circular brush over vector strokes (Laplacian smooth + refit) or painted brush strokes (whole-stroke Gaussian blur). [ ] size; Shift+right-drag strength; Shift+drag straight pass. Esc cancels [Repeat: Repeatable; source line 1779.]

- **Smoothing pass** (`board.smooth.stroke`): Drag with the Smoothing tool [Repeat: Never; source line 1791.]

- **Eyedropper** (`board.tool.eyedropper`): I — click samples the node color under the cursor into the foreground; Alt+click samples into the background [Repeat: Repeatable; source line 1801.]

- **Sticky note** (`board.tool.sticky`): N — a small sticky ghost follows the pointer; click places one note, returns to Select, and opens a centered black caret. The fill strip stays hidden while typing — Tab/Shift+Tab while editing spawns an adjacent sticky [Repeat: Repeatable; source line 1812.]

- **Direct select (anchors)** (`board.tool.direct_select`): A — click a path to edit anchors: drag anchors/segments/handles (Alt breaks handle symmetry, Shift keeps a handle's direction, Ctrl scales both handles of the anchor; they snap to other anchors on the curve and to board snaps; a handle snaps its tip, never to its own anchor), double-click toggles corner/smooth, Delete removes the selected anchors and rejoins the neighbors, arrows nudge (Shift ×10). Alt/Ctrl/Shift+right-drag edit width, color, and opacity of the selected anchors only, or the hovered anchor. With no grip picked and no point hovered, the HUD edits the whole selected curve under the Select tool or Direct Select, relatively: width scales every tip, color shifts every tip's hue, saturation, and value, and opacity scales. The property strip sits beside the selected anchors. [Repeat: Repeatable; source line 1825.]

- **Edit curve points** (`board.curve.grips`): Select one line, polyline, arc, Bézier curve, or Pen path (Select tool): drag a vertex, end point, arc through point, or handle to move only that point (object snaps apply; one undo step per drag). Click a point to pick it; Shift+click (no travel) adds or removes it. Drag a picked edge (Ctrl+Shift+click a segment) to move its vertices. One selected rectangle or regular polygon: click a vertex to pick it (Shift+click toggles); a corner drag still resizes [Repeat: Never; source line 1846.]

- **Lock or scale Bézier handles** (`board.curve.handle_scale`): Drag a Bézier or pen handle (Select grips, Direct Select, or while drawing a span): Shift+drag keeps its direction and changes only its length; Ctrl+drag scales the anchor's other handle by the same ratio; Ctrl+Shift+drag does both; Alt+drag moves only the dragged handle. One undo step per drag; Esc restores [Repeat: Never; source line 1862.]

- **Delete anchor** (`board.direct.delete_anchor`): Delete with vertices picked, under any tool (Direct Select anchors or Select grips): the curve rebuilds through the remaining neighbors. With edges picked (Ctrl+Shift+click a segment), Delete removes those segments: an open curve splits, a closed one opens [Repeat: Never; source line 1876.]

- **Default colors** (`board.colors.default`): D — reset foreground/background to the theme ink/paper [Repeat: Repeatable; source line 1889.]

- **Swap colors** (`board.colors.swap`): X — swap foreground ⇄ background [Repeat: Repeatable; source line 1899.]

- **Brush / eraser width −** (`board.brush.width_down`): [ — steps by the Photoshop screen-px tiers (<10:1 · 10–50:5 · 50–100:10 · >100:25); adjusts the Eraser or Smooth tip while it is armed [Repeat: Repeatable; source line 1909.]

- **Brush / eraser width +** (`board.brush.width_up`): ] — same tiers as [ [Repeat: Repeatable; source line 1920.]

- **Brush softness +** (`board.brush.softness_up`): Shift+[ — softer edge, in 25% steps, while Brush, Eraser, or Smooth is armed [Repeat: Repeatable; source line 1930.]

- **Brush softness −** (`board.brush.softness_down`): Shift+] — harder edge, in 25% steps, while Brush, Eraser, or Smooth is armed [Repeat: Repeatable; source line 1940.]

- **Scrub brush size** (`board.brush.size_hud`): Alt+right-drag while Brush, Eraser, Smooth, or a curve tool is armed, or on a committed curve: its picked vertices (Direct Select anchors or Select-tool grips), else the vertex under the pointer, else the whole selected curve under the Select tool or Direct Select, every tip scaled by one factor so tapers keep (a painted stroke's widest tip lands on the value shown). The circle stays centered on the press point. Horizontal travel from the press changes diameter by two screen pixels per pixel. Vertical changes softness (up softer; Brush, Eraser, and Smooth only). Dragging down into the style row under the circle reaches hardest softness, and anywhere in that band size and softness hold while a swatch picks the texture or curve style. The row edits the whole curve, except the end conditions: with an end grip of an open curve picked, it sets that end only (flat, round, arrow, or narrow). Mid-draw on a curve tool it sets the width of the point being placed, and the curve tweens from the points already placed; mid-stroke the Pen blends into the new width. Esc restores the values from the press. [Repeat: Never; source line 1950.]

- **Brush color wheel** (`board.brush.color_wheel`): Ctrl+right-drag while Brush or a curve tool is armed, or on a committed curve (the same picked, hovered, or whole-curve target as the size HUD). Eraser and Smooth have no wheel; elsewhere Ctrl+right-drag is turbo pan. The wheel opens with the pointer on the current color (a whole curve's first tip). On a whole curve the change from that color to the pick shifts every tip's hue, saturation, and value from where it was, so a gradient stays a gradient. The disk sets saturation and value, with snaps at pure white and pure black; the gap before the hue ring picks nothing. 24 recent colors fill equal slots clockwise from 6 o'clock; a repeat moves to 6 o'clock. Entering a dot keeps the wheel still and moves the pointer to that dot's center. Past the wheel, the hold samples the screen. [Repeat: Never; source line 1976.]

- **Sample brush color** (`board.brush.sample`): Alt+left-click while Brush is armed samples the screen color under the cursor, as Photoshop's eyedropper does. It becomes the brush foreground (opacity kept) and a recent color. Releasing Alt paints again. [Repeat: Never; source line 1998.]

- **Scrub brush opacity** (`board.brush.opacity_hud`): Shift+right-drag while Brush, Eraser (strength), Smooth (strength), or a curve tool is armed, or on a committed curve (the same target as the size HUD; a vertex's opacity is its color alpha; a whole curve's opacity scales, so each vertex keeps its share), with neither Ctrl nor Alt. The circle stays on the press point. Dragging up raises opacity and dragging down lowers it, 0 to 100 percent across 100 screen pixels. The chord does not pan or open the context menu. Esc restores the press. [Repeat: Never; source line 2011.]

- **Type tip values** (`board.brush.tip_entry`): Alt, Shift, or Ctrl + right-click (released within 4 px and 0.8 s) wherever that right-drag HUD opens: a small panel beside the HUD types its values. Alt: size in board units, and softness in percent on Brush, Eraser, Smooth, and brush strokes. Shift: opacity, or Eraser / Smooth strength, in percent. Ctrl: hue in degrees, saturation and value in percent (not on Eraser or Smooth). Tab and Shift+Tab apply the field and move; Enter applies and closes as one step; a click away applies and closes; Esc restores. Digits go to the panel, not to type-to-command. [Repeat: Never; source line 2027.]

- **Trim** (`board.tool.trim`): Ctrl + T — pick cutters (Ctrl+click removes), Enter, then click the part to delete. Each click is one undo. Shift+click near a curve end extends it to the cutter. Line cutters extend infinitely; curved and polyline cutters cut along their whole path, open and closed shapes alike. [Repeat: Repeatable; source line 2045.]

- **Split** (`board.tool.split`): Ctrl + Shift + T — pick cutters (Ctrl+click removes), Enter, then click an object to divide. Every piece stays. Each click is one undo. Line cutters extend infinitely; curved and polyline cutters cut along their whole path, open and closed shapes alike. [Repeat: Repeatable; source line 2059.]

- **Join paths** (`board.path.join`): Ctrl + J — open paths join at nearest endpoints; closed shapes boolean-union; an open curve in that set is treated as a ribbon of its stroke weight. First selected style wins. [Repeat: Repeatable; source line 2072.]

- **Mirror horizontal** (`board.mirror.horizontal`): Actions → Mirror horizontal: flips selected pictures, paths, and lines left to right about their centers. Rectangles, ellipses, polygons, and text are left as they are. [Repeat: Repeatable; source line 2084.]

- **Mirror vertical** (`board.mirror.vertical`): Actions → Mirror vertical: flips selected pictures, paths, and lines top to bottom about their centers. [Repeat: Repeatable; source line 2096.]

- **Group selection** (`board.group`): Ctrl + G (2+ objects) — click any member selects the whole group; Ctrl+Shift+click selects a single member [Repeat: Repeatable; source line 2108.]

- **Ungroup selection** (`board.ungroup`): Ctrl + Shift + G [Repeat: Repeatable; source line 2119.]

- **Hide selection** (`board.hide`): Ctrl + H — hidden objects leave paint, hit-testing, cycling, present mode, and the export [Repeat: Repeatable; source line 2129.]

- **Show all hidden** (`board.show_all`): Ctrl + Shift + H (also in the right-click menu on empty canvas) [Repeat: Repeatable; source line 2140.]

- **Lock selection** (`board.lock`): Ctrl + L — locked objects paint normally and stay snap targets, but leave selection and edits; Ctrl+Shift+click force-selects one [Repeat: Repeatable; source line 2150.]

- **Unlock all** (`board.unlock_all`): Ctrl + Shift + L (also in the right-click menu on empty canvas) [Repeat: Repeatable; source line 2161.]

- **Sub-object select** (`board.subselect`): Ctrl + Shift + click — a single member inside a group, or a locked object (grayed handles, one-off edit). On a curve segment (Select tool) it also picks that edge and its two vertices; more clicks add or toggle edges. Ctrl + Shift + sweep adds contained segments left-to-right or crossed segments right-to-left [Repeat: Never; source line 2171.]

- **Draw connector (wire)** (`board.wire.add`): Hover a node edge (Select tool) → drag from a side grip; snaps to grips/edges within 14 px; release on empty canvas keeps the wire with a free end there; Shift also adds; Esc cancels [Repeat: Never; source line 2185.]

- **Detach / rewire a wire** (`board.wire.detach`): Ctrl + drag from a grip with wires (or drag a selected connector's endpoint dot) — release on a grip/edge rewires, on empty canvas frees the end [Repeat: Never; source line 2197.]

- **Move all wires on a grip** (`board.wire.move_all`): Ctrl + Shift + drag from a grip — every wire end follows; release on a target grip re-anchors all (release on empty cancels) [Repeat: Never; source line 2209.]

- **Connector label / options** (`board.wire.label`): Double-click a connector to edit its label; right-click for arrowheads, faint display, and delete [Repeat: Never; source line 2220.]

- **Hover highlight preferences** (`board.hover_highlight`): Preferences > Advanced settings > Board hover highlights (per primary node type) [Repeat: Never; source line 2323.]

- **Bumper cars (optional tool)** (`app.optional.bumper_cars`): Preferences > Configure > Tools > Bumper cars. Off by default; while off, nothing collides and the Bumper squircle is hidden [Repeat: Never; source line 2326.]

- **Bumper cars: push while dragging** (`board.bumper.push`): Drag a node whose Bumper squircle is On: it pushes other bumper nodes and they glide by their friction after release. Alt passes through; Esc mid-drag puts everything back; one Ctrl+Z undoes the whole shove [Repeat: Never; source line 2329.]

## Commands

- **Repeat last command** (`app.repeat_last`): Space (tap) or Enter (idle) [Repeat: Never; source line 2273.]

- **Suggestion box** (`app.feedback.open`): Cherry feedback circle at the far left of the Board toolstrip [Repeat: Never; source line 2283.]

- **Finish step recording** (`app.feedback.finish_recording`): Enter while Reproduce recording is active [Repeat: Never; source line 2293.]

- **Commands & shortcuts** (`app.help`): F1 — opens the Advanced window on the command reference [Repeat: Never; source line 2303.]

- **Command history window** (`app.history`): F2 [Repeat: Never; source line 2313.]

- **Advanced settings** (`app.preferences`): Ctrl + Shift + P, or Preferences → Advanced settings… [Repeat: Never; source line 2332.]

- **Selection properties** (`app.properties`): Selection properties live on the object strip above the selection [Repeat: Repeatable; source line 2342.]

- **Mark this moment in the session log** (`app.session.mark`): F4 [Repeat: Never; source line 2352.]

- **Open session log folder** (`app.session.reveal`): Advanced → Session log → Open log folder [Repeat: Never; source line 2362.]

## Files

- **Open host document** (`files.open_host`): Double-click thumbnail [Repeat: Never; source line 197.]

- **Item context menu (assign tags)** (`files.item_menu`): Right-click thumbnail (without dragging) [Repeat: Never; source line 207.]

- **Add files to workbook** (`app.add_files`): Drop files onto the window, or Workbook → Add files… [Repeat: Repeatable; source line 217.]

## Media

- **Media: show page** (`board.media.page`): Selection strip Pages: browse the deck album over the selected PDF / PowerPoint [Repeat: Never; source line 63.]

## Navigation

- **Pan (precise)** (`nav.pan`): Left-drag or right-drag on canvas background [Repeat: Never; source line 80.]

- **Turbo pan** (`nav.turbo_pan`): Ctrl + right-drag on canvas — pull away from the click point; speed scales with distance; returns to zero at the origin; locked to horizontal or vertical [Repeat: Never; source line 81.]

- **Zoom** (`canvas.zoom`): Scroll wheel (pinch on trackpad) [Repeat: Never; source line 92.]

- **Pan (scroll)** (`canvas.pan_scroll`): Shift + scroll wheel [Repeat: Never; source line 102.]

- **Hide / show the bottom readout bar** (`app.fullscreen`): F11, or the lower-left chevron, or View → Hide readout bar [Repeat: Repeatable; source line 112.]

- **Zoom in / out** (`canvas.zoom_in`): + / − [Repeat: Never; source line 122.]

- **Minimap** (`canvas.minimap`): M [Repeat: Repeatable; source line 132.]

- **Zoom tool** (`canvas.tool.zoom`): Z — click = zoom in, Alt + click = out, drag = zoom window; Esc or Z disarms (Board) [Repeat: Never; source line 142.]

- **Search canvas** (`canvas.search`): Ctrl + F — Enter / Shift+Enter cycle matches, Esc closes [Repeat: Repeatable; source line 153.]

- **Cycle objects (reading order)** (`canvas.cycle_next`): Tab / Shift+Tab (while drawing a segment, Tab locks its direction instead) [Repeat: Repeatable; source line 163.]

- **Pan with arrows (nothing selected)** (`canvas.pan_arrows`): Arrow keys with no selection (Shift = faster, Board view) [Repeat: Never; source line 186.]

## Presentation

- **Present frames as slides** (`app.present`): F5, or View → Present [Repeat: Repeatable; source line 2232.]

- **Navigate slides** (`present.navigate`): ← → / Space / PageUp / PageDown / Home / End; click sides [Repeat: Never; source line 2242.]

- **Exit presentation** (`present.exit`): Escape [Repeat: Never; source line 2252.]

- **Export HTML artifact** (`app.export`): Ctrl + E, or File → Export HTML artifact… [Repeat: Never; source line 2262.]

## Selection

- **Toggle in selection** (`select.toggle`): Ctrl + click thumbnail [Repeat: Never; source line 228.]

- **Select all visible** (`app.select_all`): Ctrl + A [Repeat: Never; source line 238.]

- **Clear selection / dismiss** (`app.cancel`): Escape (pops one layer: draft → tool → selection → menus) [Repeat: Never; source line 248.]

## Workbook

- **Home** (`app.home`): Icon portal → File → Home [Repeat: Never; source line 259.]

- **New workbook tab** (`app.new_tab`): Ctrl + N [Repeat: Never; source line 269.]

- **Open workbook** (`app.open`): Ctrl + O [Repeat: Never; source line 279.]

- **Save workbook** (`app.save`): Ctrl + S [Repeat: Never; source line 289.]

- **Save workbook as…** (`app.save_as`): Ctrl + Shift + S [Repeat: Never; source line 299.]

- **Collect assets into workbook folder** (`board.assets.collect`): Copies pasted and generated images out of app data into assets/ beside this workbook. Undo restores the old links; the copies stay. [Repeat: Never; source line 309.]

## Secondary keyboard aliases

- Ctrl+Shift+Z also invokes `app.redo`.
- Ctrl+B also invokes `board.to_back`.
- F7 also invokes `board.grid`.
- Backspace also invokes `board.delete`.

## Compatibility and stale descriptions

- `board.dock.advanced` explicitly remains as a retired command ID. It does not describe a current palette control.
- `portal.agent.identity` and `portal.agent.summary` remain registered, but current agent documentation removes name-only and summary presentation controls. Use the current conversation modes in the main inventory.
- Generic descriptions such as double-click thumbnail, background primary-drag pan, or double-click portal must be read with the content and tool-specific rules in the main inventory. Media, spreadsheets, web bodies, and Slate portals have specialized behavior.
- This index retains registry wording such as old sidebar locations where present. It does not override the current object-local selection strip, portal chrome, or context menu recipes.
