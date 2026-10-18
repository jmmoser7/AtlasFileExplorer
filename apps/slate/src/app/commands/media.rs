//! Local media and tag visibility commands, registered by the app's one table.

use super::*;

pub(super) const OPEN_FILE: CommandSpec = spec(
    "board.open_file",
    "Board",
    "Open image file",
    "Right-click object → Open file (double-click enters crop mode instead)",
    None,
    Repeat::Never,
    BOARD,
    &[],
);

pub(super) const OPEN_LOCATION: CommandSpec = spec(
    "board.open_location",
    "Board",
    "Open file location",
    "Right-click local media → Open file location; reveals the displayed result in its folder",
    None,
    Repeat::Never,
    BOARD_SEL,
    &["folder", "explorer", "reveal"],
);

pub(super) const TAGS_VISIBILITY: CommandSpec = spec(
    "board.tags.visibility",
    "Board",
    "Show or hide tagged objects",
    "Bottom tag palette: upper half shows, lower half hides; selected tags act together",
    None,
    Repeat::Never,
    BOARD,
    &["tags", "visibility", "show", "hide"],
);

pub(super) const TAGS_ADD: CommandSpec = spec(
    "board.tags.add",
    "Board",
    "Add workbook tag",
    "Tags → Add tag; name and RGB color, with an optional existing facet id",
    None,
    Repeat::Never,
    BOARD,
    &["tags", "create", "color"],
);

pub(super) const GENERATE: CommandSpec = spec(
    "portal.agent.run",
    "Board",
    "Generate image or interactive artifact",
    "Generator → Generate; uses the explicitly connected source and preserves artifact revisions",
    None,
    Repeat::Never,
    BOARD,
    &["generate", "image", "artifact", "dashboard", "run"],
);

pub(super) const AGENT_BIND: CommandSpec = spec(
    "portal.agent.bind",
    "Board",
    "Agent portal: select folder",
    "Empty-state Select folder or inspector; binds a project folder (journaled source)",
    None,
    Repeat::Never,
    BOARD,
    &["select folder", "bind agent folder", "agent project"],
);

pub(super) const ARTIFACT_ACCEPT: CommandSpec = spec(
    "portal.artifact.accept",
    "Board",
    "Artifact: apply revision",
    "Generator → Apply accepts the completed preview as one attributed, undoable revision",
    None,
    Repeat::Never,
    BOARD,
    &["artifact", "dashboard", "apply", "accept"],
);
pub(super) const ARTIFACT_REJECT: CommandSpec = spec(
    "portal.artifact.reject",
    "Board",
    "Artifact: discard preview",
    "Generator → Discard rejects the pending result and restores the completed revision",
    None,
    Repeat::Never,
    BOARD,
    &["artifact", "dashboard", "discard", "reject"],
);
pub(super) const ARTIFACT_FORK: CommandSpec = spec(
    "portal.artifact.fork", "Board", "Artifact: fork this revision",
    "Spawn a downstream artifact with independent content and memory trace wires from the shown revision",
    None, Repeat::Never, BOARD, &["artifact", "dashboard", "fork", "variation"],
);
pub(super) const ARTIFACT_MEMORY_DISCONNECT: CommandSpec = spec(
    "portal.artifact.memory_disconnect", "Board", "Artifact: start with fresh eyes",
    "Remove only the memory trace; content and completed revisions remain, and the next run starts a new agent session",
    None, Repeat::Never, BOARD, &["artifact", "dashboard", "memory", "fresh eyes"],
);
pub(super) const ARTIFACT_MEMORY_INSPECT: CommandSpec = spec(
    "portal.artifact.memory_inspect",
    "Board",
    "Artifact: inspect memory trace",
    "Show the pinned, readable conversation checkpoint attached by this artifact's memory wire",
    None,
    Repeat::Never,
    BOARD,
    &["artifact", "memory", "history", "trace"],
);
pub(super) const ARTIFACT_REVISION: CommandSpec = spec(
    "portal.artifact.revision", "Board", "Artifact: show preserved revision",
    "Click a history chip to show its exact immutable source; a later fork starts from that revision",
    None, Repeat::Never, BOARD, &["artifact", "dashboard", "version", "evolution"],
);
pub(super) const ARTIFACT_SOURCE_OPEN: CommandSpec = spec(
    "portal.artifact.source_open",
    "Board",
    "Artifact: open revision file",
    "Open the exact displayed local revision in its system application",
    None,
    Repeat::Never,
    BOARD,
    &["artifact", "dashboard", "source", "html"],
);

pub(super) const IMAGE_COUNT: CommandSpec = spec(
    "portal.agent.image_count",
    "Board",
    "Set generated image count",
    "Hover Generate → image count; one journaled setting change",
    None,
    Repeat::Never,
    BOARD,
    &["images", "count"],
);
