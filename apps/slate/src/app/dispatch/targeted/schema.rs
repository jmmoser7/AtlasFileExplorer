//! The argument catalogue is both provider documentation and validation data.
use serde_json::{Map, Value};

pub(super) struct Definition {
    pub command: &'static str,
    pub targets: &'static str,
    pub arguments: &'static [(&'static str, &'static str)],
}

const STYLE_ARGUMENTS: &[(&str, &str)] = &[(
    "edits",
    "1–16 typed edits: FillRgb([r,g,b]), StrokeRgb([r,g,b]), StrokeWidth(0..10000), TextFamily(existing Typeface), TextSize(1..512), TextAlign(left/center/right), TextRgb([r,g,b]), TextBold(bool), TextItalic(bool), TextUnderline(bool). Serde enum objects, e.g. {\"TextSize\":24}. Unsupported target/property pairs and net no-ops are refused; no source edits or vertex scope.",
)];

pub(super) const DEFINITIONS: &[Definition] = &[
    Definition {
        command: "board.tags.create",
        targets: "Exactly one existing linked image or semantic frame as the visible organizer target; creates one workbook facet/tag through the reversible tag journal.",
        arguments: &[("name", "nonempty tag name, up to 128 characters"), ("color", "three RGB bytes"), ("group_id", "optional existing facet id; omitted creates a one-tag facet")],
    },
    Definition {
        command: "board.tags.assign",
        targets: "1–64 existing linked image or semantic frame IDs. Generic shapes, text and portals are not tag targets. A frame groups its geometric contents.",
        arguments: &[("tag_id", "existing workbook tag id")],
    },
    Definition {
        command: "board.tags.unassign",
        targets: "1–64 existing linked image or semantic frame IDs with an assignment in the requested facet.",
        arguments: &[("group_id", "existing facet id to clear")],
    },
    Definition {
        command: "board.paths.draw",
        targets: "Empty. Draws 1–64 real strokes through the existing Pen/Brush/path result owners. One witnessed, attributed undo group; temporary execution helpers may trace independent strokes within the human helper limit.",
        arguments: &[(
            "strokes",
            "1–64 objects: tool pen|brush|polyline|bezier|line|arc|rect|ellipse|polygon, points [[world_x,world_y],...], optional closed,width,color [r,g,b,a],opacity,softness; polygon also accepts sides 3..128. All coordinates are finite and bounded. Bezier uses a cubic control chain (3n+1 points). Arc points are start,end,through and must define a circle. Rect/ellipse/polygon points are opposite bounding corners and create the existing parametric kit shape, retaining its corners and other recipe style. Pen/Brush trace continuously; polyline/line visit actual vertices.",
        )],
    },
    Definition {
        command: "board.eraser.stroke",
        targets: "Explicit unlocked stroke node IDs only. Same brush-mark erasure and whole-vector-stroke removal as the human Eraser; never source files, images, text, frames or wires.",
        arguments: &[
            ("points", "bounded sweep points [[world_x,world_y],...]"),
            ("radius", "optional positive world radius"),
            ("softness", "optional 0..1"),
        ],
    },
    Definition {
        command: "board.path.join",
        targets: "Explicit unlocked paths/shapes. Uses the human Join owner: nearest open ends or closed-region union, first target style wins.",
        arguments: &[],
    },
    Definition {
        command: "board.smooth.stroke",
        targets: "1–64 explicit unlocked Path/Line strokes. Shared human vector smoother and stamped-brush blur, visibly swept and committed once; no source edits.",
        arguments: &[
            (
                "points",
                "2–4096 finite world sweep points, bounded by the geometry-work budget",
            ),
            (
                "width",
                "optional world brush width, default current Smooth width",
            ),
            (
                "strength",
                "optional 0.05–1, default current Smooth strength",
            ),
        ],
    },
    Definition {
        command: "board.objects.create_portal",
        targets: "Empty. Journaled placement using the existing Web/File Atlas portal constructors; requires explicit source-input permission. Does not grant new source, origin, filesystem-write or browser authority.",
        arguments: &[
            ("kind", "atlas|web|application"),
            ("source", "explicit folder locator or web URL"),
            ("x", "finite world x"),
            ("y", "finite world y"),
            ("width", "positive world width"),
            ("height", "positive world height"),
        ],
    },
    Definition {
        command: "portal.atlas.list",
        targets: "Exactly one readable File Atlas portal and source grant. Already-discovered metadata only; never starts a parallel scan.",
        arguments: &[
            ("offset", "optional bounded page offset"),
            ("limit", "optional bounded page length"),
        ],
    },
    Definition {
        command: "portal.atlas.highlight",
        targets: "Exactly one readable File Atlas portal. Ephemeral attributed analysis highlight independent of human selection and camera.",
        arguments: &[("locator", "exact enumerated root-relative file locator")],
    },
    Definition {
        command: "portal.atlas.read_file",
        targets: "Exactly one readable File Atlas portal and explicit source grant. Bounded content for an exact enumerated file; cloud-only files are not hydrated.",
        arguments: &[
            ("locator", "exact enumerated root-relative file locator"),
            ("mode", "optional text (default) or bounded image pixels"),
            ("cursor", "optional UTF-8 page cursor"),
            (
                "generation",
                "required returned scan generation for a continuation cursor",
            ),
            ("limit", "optional bounded byte page length"),
        ],
    },
    Definition {
        command: "portal.web.read_text",
        targets: "Exactly one readable, consented Web portal. Visible page text only; no cookies, storage, private fields or arbitrary scripts.",
        arguments: &[],
    },
    Definition {
        command: "portal.web.focus",
        targets: "Exactly one consented Web portal. Acquire the existing host for this request without stealing human interaction, camera or native cursor.",
        arguments: &[],
    },
    Definition {
        command: "portal.web.capture",
        targets: "Exactly one readable, consented Web portal. Captures its actual current visible page asynchronously.",
        arguments: &[],
    },
    Definition {
        command: "portal.web.navigate",
        targets: "Exactly one consented Web portal; derived navigation through the existing host. New origins retain human consent rules.",
        arguments: &[("url", "bounded valid destination URL")],
    },
    Definition {
        command: "portal.web.click",
        targets: "Exactly one consented Web portal; visible witnessed input, without moving the human cursor.",
        arguments: &[("x", "bounded page CSS x"), ("y", "bounded page CSS y")],
    },
    Definition {
        command: "portal.web.scroll",
        targets: "Exactly one consented Web portal; derived input through the existing host.",
        arguments: &[
            ("x", "bounded page CSS x"),
            ("y", "bounded page CSS y"),
            ("delta", "bounded scroll delta"),
            ("horizontal", "optional boolean"),
        ],
    },
    Definition {
        command: "portal.web.type",
        targets: "Exactly one consented Web portal; type into its focused page control through the existing host.",
        arguments: &[("text", "bounded literal text; no script channel")],
    },
    Definition {
        command: "portal.web.key",
        targets: "Exactly one consented Web portal; supported typed key input through the existing host.",
        arguments: &[
            ("key", "supported key name"),
            ("pressed", "boolean key state"),
        ],
    },
    Definition {
        command: "board.model.connector.inspect",
        targets: "Exactly one existing Rhino model node with an already active native binding, a readable target and an explicit linked/wired source grant. Returns validated binding and advertised command information; never activates, launches or rebinds a model.",
        arguments: &[(
            "command_id",
            "optional exact advertised native command ID for its descriptor and closed parameter schema; omitted inspects the binding and command catalogue",
        )],
    },
    Definition {
        command: "board.model.connector.capture",
        targets: "Exactly one existing Rhino model node with an already active native binding, a readable target and an explicit linked/wired source grant. Captures the actual native view into a host-owned AI workspace PNG artifact; never activates or focuses a model. No output path or destination argument is accepted.",
        arguments: &[],
    },
    Definition {
        command: "board.model.connector.invoke",
        targets: "Exactly one existing Rhino model node with an already active native binding and explicit linked/wired source grant. Use an advertised closed command only. Draft edits require a human-opened isolated Rhino copy, current workspace autonomy and an exact collaborator/document/command grant. No scripts, lifecycle changes or source write-back.",
        arguments: &[
            ("command_id", "required exact advertised native command ID obtained from inspect"),
            ("parameters", "required object matching that command's closed parameter schema, including an empty object for parameterless commands"),
        ],
    },
    Definition {
        command: "presence.read_text",
        targets: "Exactly one readable authored Text, shape text, or existing authored card override. No linked-source fallback or extra source authority.",
        arguments: &[
            (
                "cursor",
                "optional UTF-8 byte cursor, default zero; must be a character boundary",
            ),
            ("limit", "optional 4–4096 bytes, default 4096"),
            (
                "generation",
                "required for nonzero continuation cursor; echo the prior page generation, restart at zero if stale",
            ),
        ],
    },
    Definition {
        command: "board.objects.set_text",
        targets: "Exactly one authored Text, eligible shape, or existing authored card override. Replaces authored words, including explicit empty text; source stays intact.",
        arguments: &[(
            "text",
            "required replacement, up to 16384 characters, empty allowed",
        )],
    },
    Definition {
        command: "board.shape.edit",
        targets: "1–128 explicit objects supporting each requested basic property",
        arguments: STYLE_ARGUMENTS,
    },
    Definition {
        command: "board.wire.edit",
        targets: "1–128 explicit connectors; only supported basic stroke properties",
        arguments: STYLE_ARGUMENTS,
    },
    Definition {
        command: "board.objects.create_train",
        targets: "Empty. Creates a fresh chat train, without sending, provider Full access, or new conversation authority. Existing human opt-in for this agent's own new conversations is required to send to it; otherwise the human must grant that endpoint.",
        arguments: &[
            ("x", "finite world x"),
            ("y", "finite world y"),
            ("width", "positive world width"),
            ("height", "positive world height"),
            ("provider", "optional codex (default), cursor or local"),
            ("model", "optional model identifier, up to 256 characters"),
            ("title", "optional title up to 256 characters"),
            (
                "prompt",
                "optional authored instruction up to 16384 characters; not sent automatically",
            ),
        ],
    },
    Definition {
        command: "board.objects.create_generator",
        targets: "Empty. Creates an image generator without running it. Wire explicitly readable sources, then portal.agent.run in a separate witnessed action. No provider permissions are inherited.",
        arguments: &[
            ("x", "finite world x"),
            ("y", "finite world y"),
            ("width", "positive world width"),
            ("height", "positive world height"),
            (
                "provider",
                "optional codex-image (default), openai-image or comfy",
            ),
            ("model", "optional model identifier up to 256 characters"),
            (
                "prompt",
                "optional authored instruction up to 16384 characters; not sent automatically",
            ),
            (
                "aspect",
                "optional source, square, landscape, portrait, or wide",
            ),
            ("count", "optional integer number of images, 1–8"),
        ],
    },
    Definition {
        command: "board.objects.place_images",
        targets: "1–64 explicitly readable existing linked image or completed image-generator node IDs. Uses their registered item IDs; never imports a provider path. Requires board-file-inputs permission. Result is a centred grid with the source aspect ratios. Tagged frames require matching item assignments already present.",
        arguments: &[
            ("x", "finite world x of the new group centre"),
            ("y", "finite world y of the new group centre"),
        ],
    },
    Definition {
        command: "portal.agent.run",
        targets: "Exactly one existing image-generator node ID; its wired input IDs must also be in the readable scope. Requires explicit autonomous board edits. Provider permission remains separate.",
        arguments: &[
            (
                "prompt",
                "optional nonempty prompt override, up to 16384 characters; authored and wired prompt otherwise",
            ),
            (
                "aspect",
                "optional source, square, landscape, portrait, or wide",
            ),
            ("count", "optional integer number of images, 1–8"),
        ],
    },
    Definition {
        command: "board.objects.move",
        targets: "Existing stable node IDs; a frame includes its geometric members. Use dx/dy together, or x/y together for one target.",
        arguments: &[
            ("dx", "finite world-unit delta"),
            ("dy", "finite world-unit delta"),
            ("x", "single-target absolute world x"),
            ("y", "single-target absolute world y"),
        ],
    },
    Definition {
        command: "board.objects.resize",
        targets: "Existing stable node IDs",
        arguments: &[
            ("width", "positive finite world width"),
            ("height", "positive finite world height"),
            ("x", "optional world x"),
            ("y", "optional world y"),
        ],
    },
    Definition {
        command: "board.objects.crop",
        targets: "Exactly one existing stable croppable image ID. Nondestructive board crop only; no source file reads or writes. Ordinary frames and fixed masks use the same human crop gesture.",
        arguments: &[
            (
                "mode",
                "handle or pan; mutually exclusive exact argument sets",
            ),
            (
                "handle",
                "handle mode only: integer 0–7, clockwise Nw N Ne E Se S Sw W",
            ),
            ("x", "handle mode only: finite absolute world x"),
            ("y", "handle mode only: finite absolute world y"),
            (
                "dx",
                "pan mode only: finite world x delta from gesture start",
            ),
            (
                "dy",
                "pan mode only: finite world y delta from gesture start",
            ),
        ],
    },
    Definition {
        command: "board.objects.order",
        targets: "One stable node ID for z, or frame IDs for deck order. Supply z or order, never both.",
        arguments: &[
            ("z", "nonnegative board z-index below object count"),
            ("order", "nonnegative integer slide order, frames only"),
        ],
    },
    Definition {
        command: "board.objects.create_frame",
        targets: "Empty; creates a new frame using the board kit",
        arguments: &[
            ("x", "finite world x"),
            ("y", "finite world y"),
            ("width", "positive finite world width"),
            ("height", "positive finite world height"),
            ("title", "optional frame title, up to 256 characters"),
        ],
    },
    Definition {
        command: "board.objects.create_text",
        targets: "Empty; creates authored board text without writing a source file",
        arguments: &[
            ("x", "finite world x"),
            ("y", "finite world y"),
            ("width", "positive finite world width"),
            ("height", "positive finite world height"),
            ("text", "authored text, up to 16384 characters"),
        ],
    },
    Definition {
        command: "board.objects.create_shape",
        targets: "Empty; creates a shape using the board kit",
        arguments: &[
            ("x", "finite world x"),
            ("y", "finite world y"),
            ("width", "positive finite world width"),
            ("height", "positive finite world height"),
            ("shape", "optional rect (default), ellipse, or polygon"),
        ],
    },
    Definition {
        command: "board.objects.wire",
        targets: "Exactly [source stable node ID, destination stable node ID]",
        arguments: &[
            (
                "source_side",
                "optional top/right/bottom/left/start/mid/end; right by default",
            ),
            (
                "target_side",
                "optional top/right/bottom/left/start/mid/end; left by default",
            ),
            (
                "source_t",
                "optional edge or arclength fraction 0..1; 0.5 by default",
            ),
            (
                "target_t",
                "optional edge or arclength fraction 0..1; 0.5 by default",
            ),
        ],
    },
    Definition {
        command: "board.objects.delete",
        targets: "Existing stable board node IDs; includes subtree and affected wires. Source files are never deleted.",
        arguments: &[],
    },
    Definition {
        command: "presence.preview",
        targets: "One granted frame ID, or empty targets for the granted board. Read-only production board pixels, bounded to 512 nodes and a 1200px edge. No camera/selection changes. Linked image content requires board-file-inputs; incomplete host content is reported explicitly.",
        arguments: &[],
    },
];

pub(super) fn arguments(command: &str) -> Result<&'static [(&'static str, &'static str)], String> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.command == command)
        .map(|definition| definition.arguments)
        .ok_or_else(|| "This command has no targeted invocation".into())
}

pub(super) fn value() -> Value {
    Value::Object(
        DEFINITIONS
            .iter()
            .map(|definition| {
                let arguments: Map<String, Value> = definition
                    .arguments
                    .iter()
                    .map(|(name, description)| {
                        ((*name).into(), Value::String((*description).into()))
                    })
                    .collect();
                (
                    definition.command.to_owned(),
                    serde_json::json!({"targets": definition.targets, "arguments": arguments}),
                )
            })
            .collect(),
    )
}
