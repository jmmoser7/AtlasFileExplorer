//! The argument catalogue is both provider documentation and validation data.
use serde_json::{Map, Value};

pub(super) struct Definition {
    pub command: &'static str,
    pub targets: &'static str,
    pub arguments: &'static [(&'static str, &'static str)],
}

const STYLE_ARGUMENTS: &[(&str, &str)] = &[("edits", "1–16 typed edits: FillRgb([r,g,b]), StrokeRgb([r,g,b]), StrokeWidth(0..10000), TextFamily(existing Typeface), TextSize(1..512), TextAlign(left/center/right), TextRgb([r,g,b]), TextBold(bool), TextItalic(bool), TextUnderline(bool). Serde enum objects, e.g. {\"TextSize\":24}. Unsupported target/property pairs and net no-ops are refused; no source edits or vertex scope.")];

pub(super) const DEFINITIONS: &[Definition] = &[
    Definition {
        command: "presence.read_text", targets: "Exactly one readable authored Text, shape text, or existing authored card override. No linked-source fallback or extra source authority.",
        arguments: &[("cursor", "optional UTF-8 byte cursor, default zero; must be a character boundary"), ("limit", "optional 4–4096 bytes, default 4096"), ("generation", "required for nonzero continuation cursor; echo the prior page generation, restart at zero if stale")],
    },
    Definition {
        command: "board.objects.set_text", targets: "Exactly one authored Text, eligible shape, or existing authored card override. Replaces authored words, including explicit empty text; source stays intact.",
        arguments: &[("text", "required replacement, up to 16384 characters, empty allowed")],
    },
    Definition { command: "board.shape.edit", targets: "1–128 explicit objects supporting each requested basic property", arguments: STYLE_ARGUMENTS },
    Definition { command: "board.wire.edit", targets: "1–128 explicit connectors; only supported basic stroke properties", arguments: STYLE_ARGUMENTS },
    Definition {
        command: "board.objects.create_train", targets: "Empty. Creates a fresh chat train, without sending, provider Full access, or new conversation authority. Existing human opt-in for this agent's own new conversations is required to send to it; otherwise the human must grant that endpoint.",
        arguments: &[("x", "finite world x"), ("y", "finite world y"), ("width", "positive world width"), ("height", "positive world height"), ("provider", "optional codex (default), cursor or local"), ("model", "optional model identifier, up to 256 characters"), ("title", "optional title up to 256 characters"), ("prompt", "optional authored instruction up to 16384 characters; not sent automatically")],
    },
    Definition {
        command: "board.objects.create_generator", targets: "Empty. Creates an image generator without running it. Wire explicitly readable sources, then portal.agent.run in a separate witnessed action. No provider permissions are inherited.",
        arguments: &[("x", "finite world x"), ("y", "finite world y"), ("width", "positive world width"), ("height", "positive world height"), ("provider", "optional codex-image (default), openai-image or comfy"), ("model", "optional model identifier up to 256 characters"), ("prompt", "optional authored instruction up to 16384 characters; not sent automatically")],
    },
    Definition {
        command: "board.objects.place_images", targets: "1–64 explicitly readable existing linked image or completed image-generator node IDs. Uses their registered item IDs; never imports a provider path. Requires board-file-inputs permission. Result is a centred grid with the source aspect ratios. Tagged frames require matching item assignments already present.",
        arguments: &[("x", "finite world x of the new group centre"), ("y", "finite world y of the new group centre")],
    },
    Definition {
        command: "portal.agent.run", targets: "Exactly one existing image-generator node ID; its wired input IDs must also be in the readable scope. Requires explicit autonomous board edits. Provider permission remains separate.",
        arguments: &[("prompt", "optional nonempty prompt override, up to 16384 characters; authored and wired prompt otherwise")],
    },
    Definition {
        command: "board.objects.move",
        targets: "Existing stable node IDs; a frame includes its geometric members. Use dx/dy together, or x/y together for one target.",
        arguments: &[("dx", "finite world-unit delta"), ("dy", "finite world-unit delta"), ("x", "single-target absolute world x"), ("y", "single-target absolute world y")],
    },
    Definition {
        command: "board.objects.resize", targets: "Existing stable node IDs",
        arguments: &[("width", "positive finite world width"), ("height", "positive finite world height"), ("x", "optional world x"), ("y", "optional world y")],
    },
    Definition {
        command: "board.objects.crop", targets: "Exactly one existing stable croppable image ID. Nondestructive board crop only; no source file reads or writes. Ordinary frames and fixed masks use the same human crop gesture.",
        arguments: &[("mode", "handle or pan; mutually exclusive exact argument sets"), ("handle", "handle mode only: integer 0–7, clockwise Nw N Ne E Se S Sw W"), ("x", "handle mode only: finite absolute world x"), ("y", "handle mode only: finite absolute world y"), ("dx", "pan mode only: finite world x delta from gesture start"), ("dy", "pan mode only: finite world y delta from gesture start")],
    },
    Definition {
        command: "board.objects.order", targets: "One stable node ID for z, or frame IDs for deck order. Supply z or order, never both.",
        arguments: &[("z", "nonnegative board z-index below object count"), ("order", "nonnegative integer slide order, frames only")],
    },
    Definition {
        command: "board.objects.create_frame", targets: "Empty; creates a new frame using the board kit",
        arguments: &[("x", "finite world x"), ("y", "finite world y"), ("width", "positive finite world width"), ("height", "positive finite world height"), ("title", "optional frame title, up to 256 characters")],
    },
    Definition {
        command: "board.objects.create_text", targets: "Empty; creates authored board text without writing a source file",
        arguments: &[("x", "finite world x"), ("y", "finite world y"), ("width", "positive finite world width"), ("height", "positive finite world height"), ("text", "authored text, up to 16384 characters")],
    },
    Definition {
        command: "board.objects.create_shape", targets: "Empty; creates a shape using the board kit",
        arguments: &[("x", "finite world x"), ("y", "finite world y"), ("width", "positive finite world width"), ("height", "positive finite world height"), ("shape", "optional rect (default), ellipse, or polygon")],
    },
    Definition {
        command: "board.objects.wire", targets: "Exactly [source stable node ID, destination stable node ID]",
        arguments: &[("source_side", "optional top/right/bottom/left/start/mid/end; right by default"), ("target_side", "optional top/right/bottom/left/start/mid/end; left by default"), ("source_t", "optional edge or arclength fraction 0..1; 0.5 by default"), ("target_t", "optional edge or arclength fraction 0..1; 0.5 by default")],
    },
    Definition {
        command: "board.objects.delete", targets: "Existing stable board node IDs; includes subtree and affected wires. Source files are never deleted.", arguments: &[],
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
