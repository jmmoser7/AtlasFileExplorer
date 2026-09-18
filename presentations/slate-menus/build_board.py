"""Build an editable Slate board of sticky notes and native wires.

Labels match shipped chrome (menubar, dock flyouts, right-click menus).
Kit-derived flyout names are user data and are marked as such.
"""

from __future__ import annotations

import json
import math
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent
WORKBOOK = ROOT.parent / "slate-menus-workflows.slate"

STICKY = 200.0
GAP = 56.0
FRAME_PAD = 96.0
CLUSTER_GAP = 160.0
COLS = 4

STICKY_FILL = [0xF4, 0xE3, 0x8C, 0xFF]
HUB_FILL = [0xE8, 0xC8, 0x5A, 0xFF]
CAVEAT_FILL = [0xE4, 0xDC, 0xB8, 0xFF]
STICKY_INK = [0x26, 0x28, 0x2C, 0xFF]
FRAME_FILL = [0xF6, 0xF7, 0xF4, 0xFF]
WIRE = [0x4A, 0x52, 0x5C, 0xFF]

HANDLE_FRAC = 0.35
HANDLE_MIN = 24.0
HANDLE_MAX = 160.0
NORMAL = {
    "top": (0.0, -1.0),
    "right": (1.0, 0.0),
    "bottom": (0.0, 1.0),
    "left": (-1.0, 0.0),
}


# (slug, title, direction, nodes, edges)
# node = (key, label, role)  role: hub | item | caveat
# edge = (src, dst, label|None, faint)

CLUSTERS: list[tuple[str, str, str, list, list]] = [
    (
        "00-how",
        "How to use this board",
        "lr",
        [
            ("map", "This map", "hub"),
            ("stick", "Sticky notes\n(native Text + fill)", "item"),
            ("wire", "Wires\n(native connectors)", "item"),
            ("edit", "Move, reword,\nreconnect", "item"),
            ("faint", "Faint wire =\ncondition / caveat", "caveat"),
            ("kit", "Kit-derived names\nare your data", "caveat"),
        ],
        [
            ("map", "stick", None, False),
            ("map", "wire", None, False),
            ("stick", "edit", None, False),
            ("wire", "edit", None, False),
            ("map", "faint", None, True),
            ("map", "kit", None, True),
        ],
    ),
    (
        "01-surfaces",
        "Every menu surface",
        "tb",
        [
            ("you", "You", "hub"),
            ("top", "Top bar\nicon portal", "hub"),
            ("dock", "Floating\ntools dock", "hub"),
            ("board", "Board canvas", "hub"),
            ("grid", "Grid / Venn", "hub"),
            ("read", "Readout gear", "hub"),
            ("file", "File", "item"),
            ("edit", "Edit — empty", "caveat"),
            ("view", "View", "item"),
            ("prefs", "Preferences", "item"),
            ("fly", "Palette flyouts", "item"),
            ("dots", "Dot cluster", "item"),
            ("adv", "Advanced catalog", "item"),
            ("obj", "Object right-click", "item"),
            ("empty", "Empty-canvas\nright-click", "item"),
            ("frame", "Frame toolbar", "item"),
            ("thumb", "Thumbnail\nright-click", "item"),
            ("card", "Card linger /\nright-click", "item"),
            ("gear", "Visible readouts", "item"),
        ],
        [
            ("you", "top", None, False),
            ("you", "dock", None, False),
            ("you", "board", None, False),
            ("you", "grid", None, False),
            ("you", "read", None, False),
            ("top", "file", None, False),
            ("top", "edit", None, True),
            ("top", "view", None, False),
            ("top", "prefs", None, False),
            ("dock", "fly", None, False),
            ("dock", "dots", None, False),
            ("dock", "adv", None, False),
            ("dots", "adv", None, False),
            ("board", "obj", None, False),
            ("board", "empty", None, False),
            ("board", "frame", None, False),
            ("grid", "thumb", None, False),
            ("adv", "card", None, False),
            ("read", "gear", None, False),
        ],
    ),
    (
        "02-reach",
        "How you reach each menu",
        "lr",
        [
            ("hover", "Hover Slate icon", "hub"),
            ("portal", "File / View /\nPreferences", "item"),
            ("click", "Click Slate icon", "hub"),
            ("pin", "Pin portal open", "item"),
            ("gear", "Readout gear", "hub"),
            ("gmenu", "Metrics /\nLink health", "item"),
            ("f1", "F1 or\nCtrl+Shift+P", "hub"),
            ("advwin", "Advanced window", "item"),
            ("squircle", "Click palette", "hub"),
            ("volatile", "Volatile flyout", "item"),
            ("dbl", "Double-click\npalette", "hub"),
            ("pinned", "Pinned flyout", "item"),
            ("hdot", "Hover dots", "hub"),
            ("dotchip", "Minimize / layout /\nAdvanced / Drop", "item"),
            ("advdot", "Advanced dot", "hub"),
            ("catalog", "Fullscreen catalog", "item"),
            ("linger", "Linger or\nright-click card", "hub"),
            ("cardacts", "Add / Remove / Copy /\nDuplicate / Use", "item"),
            ("rcnode", "Right-click node", "hub"),
            ("objmenu", "Board object menu", "item"),
            ("rcempty", "Right-click empty", "hub"),
            ("hidden", "Show hidden /\nUnlock", "item"),
            ("rcthumb", "Right-click\nGrid / Venn", "hub"),
            ("filemenu", "File tags + place", "item"),
            ("oneframe", "One selected frame", "hub"),
            ("frametools", "Title / order /\ntags / present", "item"),
        ],
        [
            ("hover", "portal", None, False),
            ("click", "pin", None, False),
            ("gear", "gmenu", None, False),
            ("f1", "advwin", None, False),
            ("squircle", "volatile", None, False),
            ("dbl", "pinned", None, False),
            ("hdot", "dotchip", None, False),
            ("advdot", "catalog", None, False),
            ("linger", "cardacts", None, False),
            ("rcnode", "objmenu", None, False),
            ("rcempty", "hidden", None, False),
            ("rcthumb", "filemenu", None, False),
            ("oneframe", "frametools", None, False),
        ],
    ),
    (
        "03-file-view",
        "Top bar — File and View",
        "tb",
        [
            ("icon", "Slate icon", "hub"),
            ("file", "File", "hub"),
            ("view", "View", "hub"),
            ("edit", "Edit — no rows", "caveat"),
            ("home", "Home", "item"),
            ("new", "New workbook\nCtrl+N", "item"),
            ("open", "Open workbook\nCtrl+O", "item"),
            ("save", "Save\nCtrl+S", "item"),
            ("saveas", "Save as\nCtrl+Shift+S", "item"),
            ("export", "Export HTML\nartifact — Ctrl+E", "item"),
            ("add", "Add files", "item"),
            ("close", "Close tab", "item"),
            ("exit", "Exit", "item"),
            ("grid", "Grid", "item"),
            ("venn", "Venn", "item"),
            ("board", "Board", "item"),
            ("lens", "Lens", "item"),
            ("present", "Present — F5", "item"),
            ("hide", "Hide readout bar\nF11", "item"),
        ],
        [
            ("icon", "file", None, False),
            ("icon", "view", None, False),
            ("icon", "edit", None, True),
            ("file", "home", None, False),
            ("file", "new", None, False),
            ("file", "open", None, False),
            ("file", "save", None, False),
            ("file", "saveas", None, False),
            ("file", "export", None, False),
            ("file", "add", None, False),
            ("file", "close", None, False),
            ("file", "exit", None, False),
            ("view", "grid", None, False),
            ("view", "venn", None, False),
            ("view", "board", None, False),
            ("view", "lens", None, False),
            ("view", "present", None, False),
            ("view", "hide", None, False),
        ],
    ),
    (
        "04-preferences",
        "Top bar — Preferences",
        "tb",
        [
            ("prefs", "Preferences", "hub"),
            ("dark", "Dark mode", "item"),
            ("dockl", "Dock — left edge", "item"),
            ("dockb", "Dock — bottom edge", "item"),
            ("cursor", "Launch Cursor", "item"),
            ("aiws", "Set AI workspace", "item"),
            ("tags", "Show Tags dock", "item"),
            ("sel", "Selection inspector\nF3", "item"),
            ("viewd", "Show View dock", "item"),
            ("lensd", "Show Lens dock", "item"),
            ("adv", "Advanced settings", "item"),
            ("unwired", "Panel UI unwired", "caveat"),
        ],
        [
            ("prefs", "dark", None, False),
            ("prefs", "dockl", None, False),
            ("prefs", "dockb", None, False),
            ("prefs", "cursor", None, False),
            ("prefs", "aiws", None, False),
            ("prefs", "tags", None, False),
            ("prefs", "sel", None, False),
            ("prefs", "viewd", None, False),
            ("prefs", "lensd", None, False),
            ("prefs", "adv", None, False),
            ("viewd", "unwired", None, True),
            ("lensd", "unwired", None, True),
        ],
    ),
    (
        "05-dock",
        "Floating dock icons and dots",
        "tb",
        [
            ("icons", "Primary squircles", "hub"),
            ("dots", "Dot cluster", "hub"),
            ("media", "Media", "item"),
            ("frame", "Frame", "item"),
            ("portals", "Portals", "item"),
            ("shapes", "Shapes", "item"),
            ("text", "Text", "item"),
            ("actions", "Actions", "item"),
            ("props", "Object properties", "item"),
            ("doc", "Document settings", "item"),
            ("sel", "Selection", "item"),
            ("min", "Minimize / Close", "item"),
            ("layout", "Stacked view /\nIcon strip", "item"),
            ("adv", "Advanced", "item"),
            ("drop", "Drop to canvas", "item"),
            ("board", "Board view only", "caveat"),
            ("propref", "Board or Tags pref", "caveat"),
            ("selpref", "Selection pref", "caveat"),
            ("nodots", "No Advanced /\nno Drop", "caveat"),
            ("last", "Only the last palette\nalong the dock", "caveat"),
        ],
        [
            ("icons", "media", None, False),
            ("icons", "frame", None, False),
            ("icons", "portals", None, False),
            ("icons", "shapes", None, False),
            ("icons", "text", None, False),
            ("icons", "actions", None, False),
            ("icons", "props", None, False),
            ("icons", "doc", None, False),
            ("icons", "sel", None, False),
            ("dots", "min", None, False),
            ("dots", "layout", None, False),
            ("dots", "adv", None, False),
            ("dots", "drop", None, False),
            ("media", "board", None, False),
            ("frame", "board", None, False),
            ("portals", "board", None, False),
            ("shapes", "board", None, False),
            ("text", "board", None, False),
            ("actions", "board", None, False),
            ("doc", "board", None, False),
            ("props", "propref", None, False),
            ("sel", "selpref", None, False),
            ("sel", "nodots", None, True),
            ("layout", "last", None, True),
        ],
    ),
    (
        "06-place-flyouts",
        "Media, Frame, and Portals flyouts",
        "tb",
        [
            ("media", "Media", "hub"),
            ("image", "Image — picker", "item"),
            ("model", "3D — Rhino .3dm", "item"),
            ("video", "Video — poster", "item"),
            ("frame", "Frame presets", "hub"),
            ("letter", "8.5 × 11", "item"),
            ("tabloid", "11 × 17", "item"),
            ("wide", "16:9", "item"),
            ("custom", "Custom", "item"),
            ("armf", "Arm Frame tool", "item"),
            ("portals", "Portals", "hub"),
            ("repo", "Repository Lens", "item"),
            ("status", "Status Board", "item"),
            ("agent", "Agent portal", "item"),
            ("web", "Web portal", "item"),
            ("atlas", "File Atlas", "item"),
            ("kits", "Kit-derived portals", "caveat"),
            ("armp", "Arm portal tool", "item"),
        ],
        [
            ("media", "image", None, False),
            ("media", "model", None, False),
            ("media", "video", None, False),
            ("frame", "letter", None, False),
            ("frame", "tabloid", None, False),
            ("frame", "wide", None, False),
            ("frame", "custom", None, False),
            ("letter", "armf", None, False),
            ("tabloid", "armf", None, False),
            ("wide", "armf", None, False),
            ("custom", "armf", None, False),
            ("portals", "repo", None, False),
            ("portals", "status", None, False),
            ("portals", "agent", None, False),
            ("portals", "web", None, False),
            ("portals", "atlas", None, False),
            ("portals", "kits", None, True),
            ("repo", "armp", None, False),
            ("status", "armp", None, False),
            ("agent", "armp", None, False),
            ("web", "armp", None, False),
            ("atlas", "armp", None, False),
            ("kits", "armp", None, False),
        ],
    ),
    (
        "07-draw-flyouts",
        "Shapes, Text, and Actions flyouts",
        "tb",
        [
            ("shapes", "Shapes", "hub"),
            ("rect", "Rectangle — O", "item"),
            ("ellipse", "Ellipse", "item"),
            ("fold", "Curves and ink", "hub"),
            ("line", "Line — L", "item"),
            ("pen", "Pen — P", "item"),
            ("brush", "Brush — B", "item"),
            ("eraser", "Eraser — E", "item"),
            ("arc", "Arc", "item"),
            ("poly", "Polyline", "item"),
            ("bezier", "Bezier", "item"),
            ("kits", "Kit-derived shapes", "caveat"),
            ("text", "Text", "hub"),
            ("type", "Text — T", "item"),
            ("sticky", "Sticky note — N", "item"),
            ("actions", "Actions", "hub"),
            ("trim", "Trim — Ctrl+T", "item"),
            ("split", "Split — Ctrl+Shift+T", "item"),
            ("join", "Join — Ctrl+J", "item"),
            ("arm", "Arm tool", "item"),
            ("instant", "Instant command", "item"),
        ],
        [
            ("shapes", "rect", None, False),
            ("shapes", "ellipse", None, False),
            ("shapes", "fold", None, False),
            ("fold", "line", None, False),
            ("fold", "pen", None, False),
            ("fold", "brush", None, False),
            ("fold", "eraser", None, False),
            ("fold", "arc", None, False),
            ("fold", "poly", None, False),
            ("fold", "bezier", None, False),
            ("fold", "kits", None, True),
            ("text", "type", None, False),
            ("text", "sticky", None, False),
            ("actions", "trim", None, False),
            ("actions", "split", None, False),
            ("actions", "join", None, False),
            ("trim", "arm", None, False),
            ("split", "arm", None, False),
            ("join", "instant", None, False),
        ],
    ),
    (
        "08-properties-tags",
        "Object properties and Tags right-click",
        "tb",
        [
            ("props", "Object properties", "hub"),
            ("colors", "Colors fold", "hub"),
            ("tags", "Tags fold", "hub"),
            ("ink", "Ink / Paper pickers", "item"),
            ("swap", "Swap — X", "item"),
            ("reset", "Reset — D", "item"),
            ("addg", "Add tag group", "item"),
            ("addt", "Add tag", "item"),
            ("clickt", "Click tag —\nfocus in Venn", "item"),
            ("gmenu", "Right-click group", "item"),
            ("tmenu", "Right-click tag", "item"),
            ("delg", "Delete group", "item"),
            ("delt", "Remove tag", "item"),
        ],
        [
            ("props", "colors", None, False),
            ("props", "tags", None, False),
            ("colors", "ink", None, False),
            ("colors", "swap", None, False),
            ("colors", "reset", None, False),
            ("tags", "addg", None, False),
            ("tags", "addt", None, False),
            ("tags", "clickt", None, False),
            ("tags", "gmenu", None, False),
            ("tags", "tmenu", None, False),
            ("gmenu", "delg", None, False),
            ("tmenu", "delt", None, False),
        ],
    ),
    (
        "09-document-settings",
        "Document settings",
        "tb",
        [
            ("doc", "Document settings", "hub"),
            ("grid", "Show grid — G", "item"),
            ("wires", "Wire routing", "hub"),
            ("bez", "bezier", "item"),
            ("ortho", "orthogonal", "item"),
            ("osnap", "Object snaps", "hub"),
            ("master", "Enable object snaps", "item"),
            ("snapg", "Snap to grid — F9", "item"),
            ("guides", "Smart guides", "item"),
            ("reach", "tight / nearby / wide", "item"),
            ("kinds", "End Mid Center Near", "item"),
            ("kinds2", "Intersection\nQuadrant Perp Tangent", "item"),
        ],
        [
            ("doc", "grid", None, False),
            ("doc", "wires", None, False),
            ("doc", "osnap", None, False),
            ("wires", "bez", None, False),
            ("wires", "ortho", None, False),
            ("osnap", "master", None, False),
            ("osnap", "snapg", None, False),
            ("osnap", "guides", None, False),
            ("osnap", "reach", None, False),
            ("osnap", "kinds", None, False),
            ("osnap", "kinds2", None, False),
        ],
    ),
    (
        "10-advanced",
        "Advanced catalog",
        "tb",
        [
            ("dot", "Advanced dot", "hub"),
            ("cat", "Fullscreen\ncatalog canvas", "hub"),
            ("cam", "Pan / zoom / marquee", "item"),
            ("dbl", "Double-click card\nUse", "item"),
            ("drag", "Drag card onto\nhome strip", "item"),
            ("menu", "Right-click or linger", "hub"),
            ("add", "Add to toolbar", "item"),
            ("rem", "Remove from toolbar", "item"),
            ("copy", "Copy to clipboard", "item"),
            ("dup", "Duplicate —\nnew kit tool", "item"),
            ("use", "Use tool", "item"),
            ("slot", "iPhone-style slot", "item"),
            ("esc", "Escape", "item"),
            ("cancel", "Cancel in-flight drop", "caveat"),
        ],
        [
            ("dot", "cat", None, False),
            ("cat", "cam", None, False),
            ("cat", "dbl", None, False),
            ("cat", "drag", None, False),
            ("cat", "menu", None, False),
            ("menu", "add", None, False),
            ("menu", "rem", None, False),
            ("menu", "copy", None, False),
            ("menu", "dup", None, False),
            ("menu", "use", None, False),
            ("drag", "slot", None, False),
            ("esc", "cancel", None, False),
        ],
    ),
    (
        "11-board-object",
        "Board object right-click",
        "tb",
        [
            ("rc", "Right-click node", "hub"),
            ("head", "N object(s)", "hub"),
            ("dup", "Duplicate — Ctrl+D", "item"),
            ("front", "Bring to front", "item"),
            ("back", "Send to back", "item"),
            ("lock", "Lock — Ctrl+L", "item"),
            ("hide", "Hide — Ctrl+H", "item"),
            ("delete", "Delete", "item"),
            ("group", "Group — Ctrl+G", "item"),
            ("ungroup", "Ungroup\nCtrl+Shift+G", "item"),
            ("need2", "Needs 2+ targets", "caveat"),
            ("ifgrp", "If any target grouped", "caveat"),
            ("img", "Crop image /\nOpen file", "item"),
            ("pdf", "Explode PDF\ninto pages", "item"),
            ("tags", "Tags swatches", "item"),
            ("wire", "Wire: arrows /\nFaint / Edit label", "item"),
        ],
        [
            ("rc", "head", None, False),
            ("head", "dup", None, False),
            ("head", "front", None, False),
            ("head", "back", None, False),
            ("head", "lock", None, False),
            ("head", "hide", None, False),
            ("head", "delete", None, False),
            ("head", "group", None, False),
            ("head", "ungroup", None, False),
            ("group", "need2", None, True),
            ("ungroup", "ifgrp", None, True),
            ("head", "img", None, False),
            ("head", "pdf", None, False),
            ("head", "tags", None, False),
            ("head", "wire", None, False),
        ],
    ),
    (
        "12-board-portal",
        "Board portal right-click extras",
        "tb",
        [
            ("portal", "Clicked node\nis a portal", "hub"),
            ("max", "Maximize / Restore", "hub"),
            ("web", "Web", "hub"),
            ("atlas", "File Atlas", "hub"),
            ("agent", "Agent", "hub"),
            ("other", "Repo Lens /\nStatus Board\nchrome only", "caveat"),
            ("hidetab", "Hide tab / Show tab", "item"),
            ("copyurl", "Copy URL", "item"),
            ("pasteurl", "Paste URL", "item"),
            ("opena", "Open in File Atlas", "item"),
            ("refresh", "Refresh", "item"),
            ("bake", "Bake poster", "item"),
            ("entera", "Enter / Leave contents", "item"),
            ("openc", "Open in Cursor", "item"),
            ("switch", "Switch agent", "item"),
            ("unbundle", "Unbundle images", "item"),
            ("enterg", "Enter / Leave contents", "item"),
        ],
        [
            ("portal", "max", None, False),
            ("max", "web", None, False),
            ("max", "atlas", None, False),
            ("max", "agent", None, False),
            ("max", "other", None, True),
            ("web", "hidetab", None, False),
            ("web", "copyurl", None, False),
            ("web", "pasteurl", None, False),
            ("atlas", "opena", None, False),
            ("atlas", "refresh", None, False),
            ("atlas", "bake", None, False),
            ("atlas", "entera", None, False),
            ("agent", "openc", None, False),
            ("agent", "switch", None, False),
            ("agent", "unbundle", None, False),
            ("agent", "enterg", None, False),
        ],
    ),
    (
        "13-frame-empty",
        "Frame toolbar and empty-canvas menu",
        "tb",
        [
            ("bar", "One selected frame", "hub"),
            ("title", "Title field", "item"),
            ("prev", "Previous slide", "item"),
            ("next", "Next slide", "item"),
            ("addimg", "Add images", "item"),
            ("tags", "Tags submenu", "hub"),
            ("present", "Present from here", "item"),
            ("trash", "Delete frame", "item"),
            ("emptyt", "No tags yet", "caveat"),
            ("inherit", "Images inherit\nthese tags", "item"),
            ("swatches", "Per-group swatches", "item"),
            ("empty", "Right-click\nempty board", "hub"),
            ("showh", "Show all hidden\nCtrl+Shift+H", "item"),
            ("unlock", "Unlock all\nCtrl+Shift+L", "item"),
            ("onlyif", "Only if hidden or\nlocked count > 0", "caveat"),
        ],
        [
            ("bar", "title", None, False),
            ("bar", "prev", None, False),
            ("bar", "next", None, False),
            ("bar", "addimg", None, False),
            ("bar", "tags", None, False),
            ("bar", "present", None, False),
            ("bar", "trash", None, False),
            ("tags", "emptyt", None, True),
            ("tags", "inherit", None, False),
            ("tags", "swatches", None, False),
            ("empty", "showh", None, False),
            ("empty", "unlock", None, False),
            ("empty", "onlyif", None, True),
        ],
    ),
    (
        "14-grid-venn",
        "Grid / Venn thumbnail right-click",
        "tb",
        [
            ("thumb", "Right-click thumbnail", "hub"),
            ("head", "N file(s)", "hub"),
            ("tags", "Tag swatches\nper group", "item"),
            ("empty", "No tags yet —\ncreate groups in Tags", "caveat"),
            ("explode", "Explode PDF\ninto pages", "item"),
            ("place", "Place on board", "item"),
            ("remove", "Remove from workbook", "item"),
            ("board", "Switches to\nBoard view", "item"),
            ("lens", "Lens canvas", "hub"),
            ("home", "Cover Flow home", "hub"),
            ("none", "No right-click menu", "caveat"),
        ],
        [
            ("thumb", "head", None, False),
            ("head", "tags", None, False),
            ("tags", "empty", None, True),
            ("head", "explode", None, False),
            ("head", "place", None, False),
            ("head", "remove", None, False),
            ("place", "board", None, False),
            ("lens", "none", None, True),
            ("home", "none", None, True),
        ],
    ),
    (
        "15-readouts-advanced",
        "Readout gear and Advanced window",
        "tb",
        [
            ("gear", "Readout gear", "hub"),
            ("metrics", "Metrics", "item"),
            ("health", "Link health", "item"),
            ("chips", "Board readout clicks", "hub"),
            ("hidden", "N hidden — Show all", "item"),
            ("locked", "N locked — Unlock all", "item"),
            ("f1", "F1 / Ctrl+Shift+P", "hub"),
            ("adv", "Advanced window", "hub"),
            ("updates", "Updates", "item"),
            ("previews", "Canvas previews", "item"),
            ("hover", "Board hover highlights", "item"),
            ("session", "Session log", "item"),
            ("cmds", "Commands reference", "item"),
        ],
        [
            ("gear", "metrics", None, False),
            ("gear", "health", None, False),
            ("chips", "hidden", None, False),
            ("chips", "locked", None, False),
            ("f1", "adv", None, False),
            ("adv", "updates", None, False),
            ("adv", "previews", None, False),
            ("adv", "hover", None, False),
            ("adv", "session", None, False),
            ("adv", "cmds", None, False),
        ],
    ),
    (
        "16-folder-drop",
        "Folder drop chooser",
        "tb",
        [
            ("drop", "Drop a folder\non the Board", "hub"),
            ("ask", "How should it land?", "hub"),
            ("atlas", "File Atlas lens", "item"),
            ("repo", "Repository Lens", "item"),
            ("status", "Status Board", "item"),
            ("web", "Web portal", "item"),
            ("files", "Place files\non the board", "item"),
            ("cancel", "Cancel", "item"),
            ("ifgit", "Git repo detected", "caveat"),
            ("ifstate", "project-state.json\npresent", "caveat"),
            ("ifhtml", "HTML entry present", "caveat"),
            ("always", "Always offered", "caveat"),
        ],
        [
            ("drop", "ask", None, False),
            ("ask", "atlas", None, False),
            ("ask", "repo", None, False),
            ("ask", "status", None, False),
            ("ask", "web", None, False),
            ("ask", "files", None, False),
            ("ask", "cancel", None, False),
            ("repo", "ifgit", None, True),
            ("status", "ifstate", None, True),
            ("web", "ifhtml", None, True),
            ("files", "always", None, True),
        ],
    ),
    (
        "17-workflow-arm",
        "Workflow — arm a board tool",
        "lr",
        [
            ("open", "Open Board view", "hub"),
            ("click", "Click a dock palette", "item"),
            ("mode", "Flyout mode", "hub"),
            ("icon", "Click a tool icon", "item"),
            ("row", "Click a row", "item"),
            ("armed", "Tool is armed", "hub"),
            ("draw", "Draw or click\non the canvas", "item"),
            ("cmd", "Journal SceneCmd", "item"),
            ("adv", "Advanced catalog", "hub"),
            ("use", "Use tool /\ndouble-click", "item"),
            ("hotkey", "Command palette\n/ hotkey", "hub"),
        ],
        [
            ("open", "click", None, False),
            ("click", "mode", None, False),
            ("mode", "icon", "Icon strip", False),
            ("mode", "row", "Stacked list", False),
            ("icon", "armed", None, False),
            ("row", "armed", None, False),
            ("armed", "draw", None, False),
            ("draw", "cmd", None, False),
            ("adv", "use", None, False),
            ("use", "armed", None, False),
            ("hotkey", "armed", None, False),
        ],
    ),
    (
        "18-workflow-catalog",
        "Workflow — catalog onto the home strip",
        "tb",
        [
            ("open", "Open a palette", "hub"),
            ("adv", "Advanced dot", "item"),
            ("cat", "Catalog of\nevery tool", "hub"),
            ("lift", "Press a card\nand drag", "item"),
            ("fade", "Home strip\nhighlights", "item"),
            ("slide", "Icons slide aside", "item"),
            ("drop", "Drop into a slot", "item"),
            ("persist", "Order saved in\nchrome prefs", "item"),
            ("esc", "Escape or\ndrop outside", "item"),
            ("cancel", "No change", "caveat"),
            ("menu", "Linger\nAdd to toolbar", "item"),
            ("append", "Appends at end", "item"),
        ],
        [
            ("open", "adv", None, False),
            ("adv", "cat", None, False),
            ("cat", "lift", None, False),
            ("lift", "fade", None, False),
            ("fade", "slide", None, False),
            ("slide", "drop", None, False),
            ("drop", "persist", None, False),
            ("lift", "esc", None, False),
            ("esc", "cancel", None, False),
            ("menu", "append", None, False),
        ],
    ),
    (
        "19-workflow-grid-board",
        "Workflow — Grid file onto the Board",
        "lr",
        [
            ("add", "File — Add files", "hub"),
            ("grid", "Grid or Venn", "hub"),
            ("tag", "Right-click —\nassign tags", "item"),
            ("place", "Place on board", "item"),
            ("board", "Board view", "hub"),
            ("img", "Image / PDF /\nvideo / 3D node", "item"),
            ("rc", "Right-click\nobject menu", "hub"),
            ("open", "Open file", "item"),
            ("crop", "Crop image", "item"),
            ("explode", "Explode PDF", "item"),
        ],
        [
            ("add", "grid", None, False),
            ("grid", "tag", None, False),
            ("grid", "place", None, False),
            ("place", "board", None, False),
            ("board", "img", None, False),
            ("img", "rc", None, False),
            ("rc", "open", None, False),
            ("rc", "crop", None, False),
            ("rc", "explode", None, False),
        ],
    ),
    (
        "20-not-menus",
        "Surfaces that are not menus",
        "tb",
        [
            ("not", "Not dropdown menus", "hub"),
            ("palette", "Canvas command palette", "item"),
            ("align", "Align widget", "item"),
            ("insp", "Selection inspector form", "item"),
            ("adjust", "Image adjust — Ctrl+U", "item"),
            ("pdf", "PDF / PPT page strip", "item"),
            ("present", "Presentation mode", "item"),
            ("model", "3D orbit / padlock", "item"),
            ("tabs", "Workbook tab —\nno context menu", "caveat"),
            ("win", "Window minus /\nmax / close", "item"),
            ("unwired", "Toggles exist /\nbody does not", "hub"),
            ("viewd", "Show View dock", "caveat"),
            ("lensd", "Show Lens dock", "caveat"),
            ("workbook", "Workbook dock", "caveat"),
            ("ai", "AI dock", "caveat"),
        ],
        [
            ("not", "palette", None, False),
            ("not", "align", None, False),
            ("not", "insp", None, False),
            ("not", "adjust", None, False),
            ("not", "pdf", None, False),
            ("not", "present", None, False),
            ("not", "model", None, False),
            ("not", "tabs", None, True),
            ("not", "win", None, False),
            ("unwired", "viewd", None, True),
            ("unwired", "lensd", None, True),
            ("unwired", "workbook", None, True),
            ("unwired", "ai", None, True),
        ],
    ),
]


def fill_for(role: str) -> list[int]:
    if role == "hub":
        return HUB_FILL
    if role == "caveat":
        return CAVEAT_FILL
    return STICKY_FILL


def layered_layout(keys: list[str], edges: list, direction: str) -> dict[str, tuple[int, int]]:
    children: dict[str, list[str]] = defaultdict(list)
    incoming: dict[str, int] = {k: 0 for k in keys}
    for src, dst, _label, _faint in edges:
        children[src].append(dst)
        incoming[dst] = incoming.get(dst, 0) + 1
        incoming.setdefault(src, incoming.get(src, 0))
    roots = [k for k in keys if incoming.get(k, 0) == 0] or list(keys)
    layer: dict[str, int] = {}
    queue = list(roots)
    for r in roots:
        layer[r] = 0
    seen = set(roots)
    while queue:
        cur = queue.pop(0)
        for nxt in children.get(cur, []):
            layer[nxt] = max(layer.get(nxt, 0), layer[cur] + 1)
            if nxt not in seen:
                seen.add(nxt)
                queue.append(nxt)
    for k in keys:
        layer.setdefault(k, 0)
    by_layer: dict[int, list[str]] = defaultdict(list)
    for k in keys:
        by_layer[layer[k]].append(k)
    # One barycenter pass to cut crossings.
    for _ in range(2):
        for ly in sorted(by_layer):
            if ly == 0:
                continue
            parents_of: dict[str, list[str]] = defaultdict(list)
            for src, dst, _l, _f in edges:
                if layer.get(dst) == ly and layer.get(src) == ly - 1:
                    parents_of[dst].append(src)
            order_prev = {k: i for i, k in enumerate(by_layer[ly - 1])}

            def bary(n: str) -> float:
                ps = parents_of.get(n, [])
                if not ps:
                    return float(by_layer[ly].index(n))
                return sum(order_prev.get(p, 0) for p in ps) / len(ps)

            by_layer[ly] = sorted(by_layer[ly], key=bary)
    pos: dict[str, tuple[int, int]] = {}
    if direction == "lr":
        for ly, row in by_layer.items():
            for rank, key in enumerate(row):
                pos[key] = (ly, rank)
    else:
        for ly, row in by_layer.items():
            for rank, key in enumerate(row):
                pos[key] = (rank, ly)
    return pos


def pick_sides(ax: float, ay: float, bx: float, by: float) -> tuple[str, str]:
    dx = (bx + STICKY * 0.5) - (ax + STICKY * 0.5)
    dy = (by + STICKY * 0.5) - (ay + STICKY * 0.5)
    if abs(dx) >= abs(dy):
        return ("right", "left") if dx >= 0 else ("left", "right")
    return ("bottom", "top") if dy >= 0 else ("top", "bottom")


def anchor(rect: dict, side: str, t: float) -> tuple[float, float]:
    t = min(1.0, max(0.0, t))
    x, y, w, h = rect["x"], rect["y"], rect["w"], rect["h"]
    if side == "top":
        return x + w * t, y
    if side == "right":
        return x + w, y + h * t
    if side == "bottom":
        return x + w * t, y + h
    return x, y + h * t


def _norm(v: tuple[float, float], fallback: tuple[float, float]) -> tuple[float, float]:
    length = math.hypot(*v)
    if length > 1e-6:
        return v[0] / length, v[1] / length
    fl = math.hypot(*fallback)
    if fl > 1e-6:
        return fallback[0] / fl, fallback[1] / fl
    return 0.0, 0.0


def cubic_axis_bounds(p0: float, c1: float, c2: float, p3: float) -> tuple[float, float]:
    lo, hi = min(p0, p3), max(p0, p3)
    a = 3.0 * (p3 - 3.0 * c2 + 3.0 * c1 - p0)
    b = 6.0 * (c2 - 2.0 * c1 + p0)
    c = 3.0 * (c1 - p0)

    def consider(t: float) -> None:
        nonlocal lo, hi
        if 0.0 < t < 1.0:
            u = 1.0 - t
            v = u * u * u * p0 + 3.0 * u * u * t * c1 + 3.0 * u * t * t * c2 + t * t * t * p3
            lo = min(lo, v)
            hi = max(hi, v)

    if abs(a) < 1e-6:
        if abs(b) > 1e-6:
            consider(-c / b)
    else:
        disc = b * b - 4.0 * a * c
        if disc >= 0.0:
            sq = math.sqrt(disc)
            consider((-b + sq) / (2.0 * a))
            consider((-b - sq) / (2.0 * a))
    return lo, hi


def connector_rect(ra: dict, side_a: str, ta: float, rb: dict, side_b: str, tb: float) -> dict:
    p0 = anchor(ra, side_a, ta)
    p3 = anchor(rb, side_b, tb)
    chord = (p3[0] - p0[0], p3[1] - p0[1])
    dist = math.hypot(*chord)
    length = min(HANDLE_MAX, max(HANDLE_MIN, HANDLE_FRAC * dist))
    da = _norm(NORMAL[side_a], chord)
    db = _norm(NORMAL[side_b], (-chord[0], -chord[1]))
    c1 = (p0[0] + da[0] * length, p0[1] + da[1] * length)
    c2 = (p3[0] + db[0] * length, p3[1] + db[1] * length)
    min_x, max_x = cubic_axis_bounds(p0[0], c1[0], c2[0], p3[0])
    min_y, max_y = cubic_axis_bounds(p0[1], c1[1], c2[1], p3[1])
    return {"x": min_x, "y": min_y, "w": max(1.0, max_x - min_x), "h": max(1.0, max_y - min_y)}


def node_base(nid: int, rect: dict, kind: dict) -> dict:
    return {
        "id": nid,
        "rect": rect,
        "rotation_deg": 0.0,
        "opacity": 1.0,
        "kind": kind,
    }


def sticky_node(nid: int, x: float, y: float, text: str, fill: list[int]) -> dict:
    return node_base(
        nid,
        {"x": x, "y": y, "w": STICKY, "h": STICKY},
        {
            "text": {
                "text": text,
                "family": "sans",
                "size": 18.0,
                "color": STICKY_INK,
                "align": "left",
                "fill": fill,
            }
        },
    )


def frame_node(nid: int, x: float, y: float, w: float, h: float, title: str, order: int) -> dict:
    return node_base(
        nid,
        {"x": x, "y": y, "w": w, "h": h},
        {
            "frame": {
                "title": title,
                "order": order,
                "fill": FRAME_FILL,
                "assignments": {},
            }
        },
    )


def connector_node(
    nid: int,
    src_id: int,
    dst_id: int,
    ra: dict,
    rb: dict,
    side_a: str,
    side_b: str,
    ta: float,
    tb: float,
    label: str | None,
    faint: bool,
) -> dict:
    conn = {
        "a": {"anchored": {"node": src_id, "side": side_a, "t": ta}},
        "b": {"anchored": {"node": dst_id, "side": side_b, "t": tb}},
        "stroke": {
            "width": 2.0,
            "color": WIRE,
            "dash": "solid",
            "cap": "Butt",
            "join": "Miter",
            "profile": "Uniform",
        },
        "arrow_b": True,
        "display": "faint" if faint else "default",
    }
    if label:
        conn["label"] = label
    return node_base(nid, connector_rect(ra, side_a, ta, rb, side_b, tb), {"connector": conn})


def layout_cluster(
    nodes: list, edges: list, direction: str, origin: tuple[float, float]
) -> tuple[dict[str, dict], float, float]:
    keys = [k for k, _label, _role in nodes]
    cells = layered_layout(keys, edges, direction)
    ox, oy = origin
    placed: dict[str, dict] = {}
    max_cx = max((c for c, _r in cells.values()), default=0)
    max_ry = max((r for _c, r in cells.values()), default=0)
    for key, _label, _role in nodes:
        col, row = cells[key]
        placed[key] = {
            "x": ox + FRAME_PAD + col * (STICKY + GAP),
            "y": oy + FRAME_PAD + row * (STICKY + GAP),
            "w": STICKY,
            "h": STICKY,
        }
    width = FRAME_PAD * 2 + (max_cx + 1) * STICKY + max_cx * GAP
    height = FRAME_PAD * 2 + (max_ry + 1) * STICKY + max_ry * GAP
    return placed, width, height


def build_slate() -> None:
    frames: list[dict] = []
    stickies: list[dict] = []
    wires: list[dict] = []
    next_id = 1
    sizes: list[tuple[float, float]] = []
    layouts: list[dict] = []

    for _slug, _title, direction, nodes, edges in CLUSTERS:
        placed, w, h = layout_cluster(nodes, edges, direction, (0.0, 0.0))
        layouts.append(placed)
        sizes.append((w, h))

    origins: list[tuple[float, float]] = []
    cursor_x = 0.0
    cursor_y = 0.0
    row_h = 0.0
    for i, (w, h) in enumerate(sizes):
        if i and i % COLS == 0:
            cursor_x = 0.0
            cursor_y += row_h + CLUSTER_GAP
            row_h = 0.0
        origins.append((cursor_x, cursor_y))
        cursor_x += w + CLUSTER_GAP
        row_h = max(row_h, h)

    for order, ((slug, title, direction, nodes, edges), placed, (w, h), (ox, oy)) in enumerate(
        zip(CLUSTERS, layouts, sizes, origins), start=1
    ):
        world: dict[str, dict] = {}
        ids: dict[str, int] = {}
        frames.append(frame_node(next_id, ox, oy, w, h, f"{order:02d}  {title}", order))
        next_id += 1
        for key, label, role in nodes:
            rect = {
                "x": placed[key]["x"] + ox,
                "y": placed[key]["y"] + oy,
                "w": STICKY,
                "h": STICKY,
            }
            world[key] = rect
            ids[key] = next_id
            stickies.append(sticky_node(next_id, rect["x"], rect["y"], label, fill_for(role)))
            next_id += 1

        side_hits: dict[tuple[int, str], list] = defaultdict(list)
        pending = []
        for src, dst, label, faint in edges:
            sa, sb = pick_sides(world[src]["x"], world[src]["y"], world[dst]["x"], world[dst]["y"])
            pending.append((src, dst, label, faint, sa, sb))
            side_hits[(ids[src], sa)].append("out")
            side_hits[(ids[dst], sb)].append("in")
        used: dict[tuple[int, str], int] = defaultdict(int)
        for src, dst, label, faint, sa, sb in pending:
            na = len(side_hits[(ids[src], sa)])
            nb = len(side_hits[(ids[dst], sb)])
            ia = used[(ids[src], sa)]
            ib = used[(ids[dst], sb)]
            used[(ids[src], sa)] += 1
            used[(ids[dst], sb)] += 1
            ta = (ia + 1) / (na + 1)
            tb = (ib + 1) / (nb + 1)
            wires.append(
                connector_node(
                    next_id,
                    ids[src],
                    ids[dst],
                    world[src],
                    world[dst],
                    sa,
                    sb,
                    ta,
                    tb,
                    label,
                    faint,
                )
            )
            next_id += 1
        _ = slug  # kept in CLUSTERS for the mermaid filenames

    cam_x = origins[0][0] + sizes[0][0] * 0.5
    cam_y = origins[0][1] + sizes[0][1] * 0.45
    doc = {
        "format_version": 2,
        "name": "Slate menus and workflows",
        "groups": [],
        "items": [],
        "view": {
            "active_view": "board",
            "cam_x": cam_x,
            "cam_y": cam_y,
            "zoom": 0.22,
        },
        "scene": {
            "nodes": frames + stickies + wires,
            "next_node_id": next_id,
            "next_group_key": 0,
        },
        "lens_root": None,
        "next_group_id": 1,
        "next_tag_id": 1,
        "next_item_id": 1,
    }
    WORKBOOK.write_text(json.dumps(doc, indent=2) + "\n", encoding="utf-8")
    print(
        f"wrote {WORKBOOK} "
        f"({len(frames)} frames, {len(stickies)} stickies, {len(wires)} wires)"
    )


if __name__ == "__main__":
    build_slate()
