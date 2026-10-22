//! Shared portal capture commands. Legacy 3D IDs remain callable aliases.
use super::*;

pub(super) const MENU: CommandSpec = spec(
    "portal.capture",
    "Board",
    "Capture portal",
    "Portal blister → Capture: still image or video to File or Canvas",
    None,
    Repeat::Never,
    BOARD,
    &["capture portal", "screenshot", "snapshot", "record portal"],
);
pub(super) const STILL_CANVAS: CommandSpec = spec(
    "portal.capture.still.canvas",
    "Board",
    "Capture still image to Canvas",
    "Capture → Still image to Canvas: save a durable linked image beside the source; one undo",
    None,
    Repeat::Never,
    BOARD,
    &["snapshot canvas", "capture image", "png"],
);
pub(super) const STILL_FILE: CommandSpec = spec(
    "portal.capture.still.file",
    "Board",
    "Capture still image to File",
    "Capture → Still image to File: choose PNG, JPEG, or WebP; 3D captures retain view metadata",
    None,
    Repeat::Never,
    BOARD,
    &["screenshot file", "capture file", "jpeg", "webp"],
);
pub(super) const VIDEO_CANVAS: CommandSpec = spec(
    "portal.capture.video.canvas",
    "Board",
    "Start portal video (Canvas compatibility)",
    "Compatibility alias for Start recording; Pause then choose File or Canvas",
    None,
    Repeat::Never,
    BOARD,
    &["record canvas", "video capture", "mp4"],
);
pub(super) const VIDEO_FILE: CommandSpec = spec(
    "portal.capture.video.file",
    "Board",
    "Start portal video (File compatibility)",
    "Compatibility alias for Start recording; Pause then choose File or Canvas",
    None,
    Repeat::Never,
    BOARD,
    &["record file", "record portal", "mp4"],
);
pub(super) const STOP: CommandSpec = spec(
    "portal.capture.stop",
    "Board",
    "Pause portal video (Stop compatibility)",
    "Compatibility alias for Pause: freeze the timer, then choose File or Canvas",
    None,
    Repeat::Never,
    BOARD,
    &["stop recording", "save recording"],
);
pub(super) const VIDEO_START: CommandSpec = spec(
    "portal.capture.video.start",
    "Board",
    "Start portal video",
    "Capture → Record video: show a recording timer and Pause outside the portal frame",
    None,
    Repeat::Never,
    BOARD,
    &["record portal", "start recording", "mp4"],
);
pub(super) const PAUSE: CommandSpec = spec(
    "portal.capture.pause",
    "Board",
    "Pause portal video",
    "Recording control → Pause: freeze recorded duration and show File or Canvas destinations",
    None,
    Repeat::Never,
    BOARD,
    &["pause recording", "pause capture"],
);
pub(super) const RESUME: CommandSpec = spec(
    "portal.capture.resume",
    "Board",
    "Resume portal video",
    "Paused recording → Resume: continue without including the pause in the MP4 timeline",
    None,
    Repeat::Never,
    BOARD,
    &["resume recording", "continue recording"],
);
pub(super) const SAVE_FILE: CommandSpec = spec(
    "portal.capture.save.file",
    "Board",
    "Save paused portal video to File",
    "Paused recording → Save to File: choose MP4 output and finalize asynchronously; cancel leaves recording paused",
    None,
    Repeat::Never,
    BOARD,
    &["save recording", "recording file"],
);
pub(super) const SAVE_CANVAS: CommandSpec = spec(
    "portal.capture.save.canvas",
    "Board",
    "Place paused portal video on Canvas",
    "Paused recording → Place on Canvas: save durable MP4 and add one undoable linked video",
    None,
    Repeat::Never,
    BOARD,
    &["place recording", "recording canvas"],
);
pub(super) const CANCEL: CommandSpec = spec(
    "portal.capture.cancel",
    "Board",
    "Cancel portal video",
    "Recording Capture control → Cancel: discard the recording without placing or saving media",
    None,
    Repeat::Never,
    BOARD,
    &["cancel recording", "discard recording"],
);
pub(super) const MODEL_MENU: CommandSpec = spec(
    "board.model_screenshot",
    "Board",
    "Screenshot 3D viewport",
    "Compatibility alias for Capture on the selected 3D viewport",
    None,
    Repeat::Never,
    BOARD,
    &["viewport screenshot", "xmp", "camera"],
);
pub(super) const MODEL_CANVAS: CommandSpec = spec(
    "board.model_screenshot_canvas",
    "Board",
    "Export viewport to canvas",
    "Compatibility alias for Capture → Still image to Canvas, retaining 3D view metadata",
    None,
    Repeat::Never,
    BOARD,
    &["viewport canvas", "place"],
);
pub(super) const MODEL_FILE: CommandSpec = spec(
    "board.model_screenshot_save",
    "Board",
    "Export viewport to file",
    "Compatibility alias for Capture → Still image to File, retaining 3D view metadata",
    None,
    Repeat::Never,
    BOARD,
    &["viewport save", "xmp"],
);

#[test]
fn capture_actions_are_explicit_and_have_no_keyboard_chords() {
    let registry = registry();
    for command in [
        MENU,
        STILL_CANVAS,
        STILL_FILE,
        VIDEO_CANVAS,
        VIDEO_FILE,
        VIDEO_START,
        PAUSE,
        RESUME,
        SAVE_FILE,
        SAVE_CANVAS,
        STOP,
        CANCEL,
        MODEL_MENU,
        MODEL_CANVAS,
        MODEL_FILE,
    ] {
        let registered = registry.by_id(command.id).expect(command.id.0);
        assert_eq!(registered.repeat, Repeat::Never, "{}", command.id.0);
        assert!(registered.chord.is_none(), "{}", command.id.0);
    }
}
