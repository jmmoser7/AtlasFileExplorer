//! Typed completions keep save dialogs attached to their initiating source.
use eframe::egui;
use slate_doc::scene::NodeId;
use std::path::PathBuf;

pub enum PickerMsg {
    ImageLayer {
        tab_id: u64,
        image: NodeId,
        result: slate_doc::image_paint::ImageResultId,
        path: Option<PathBuf>,
    },
    OpenDoc(Option<PathBuf>),
    SaveDocAs {
        tab_id: u64,
        path: Option<PathBuf>,
    },
    AddFiles(Option<Vec<PathBuf>>),
    AddMedia {
        tab_id: u64,
        at: egui::Pos2,
        paths: Option<Vec<PathBuf>>,
    },
    ExportArtifact(Option<PathBuf>),
    WebPortalSource {
        portal: NodeId,
        path: Option<PathBuf>,
    },
    AgentPortalSource {
        portal: NodeId,
        path: Option<PathBuf>,
    },
    AtlasPortalSource {
        portal: NodeId,
        path: Option<PathBuf>,
    },
    SlatePortalSource {
        portal: NodeId,
        path: Option<PathBuf>,
    },
    ModelScreenshotSave {
        tab_id: u64,
        node: NodeId,
        path: Option<PathBuf>,
    },
    PortalCaptureSave {
        tab_id: u64,
        node: NodeId,
        identity: String,
        video: bool,
        path: Option<PathBuf>,
    },
    PortalCaptureVideoSave {
        tab_id: u64,
        node: NodeId,
        identity: String,
        generation: u64,
        path: Option<PathBuf>,
    },
}
