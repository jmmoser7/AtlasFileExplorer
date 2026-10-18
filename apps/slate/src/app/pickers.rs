//! Asynchronous file-picker completion routing.
use super::*;

mod messages;
pub use messages::PickerMsg;

impl SlateApp {
    pub(super) fn drain_pickers(&mut self, ctx: &egui::Context) {
        let Some(msg) = self.picker.poll(ctx) else {
            return;
        };
        match msg {
            PickerMsg::ImageLayer {
                tab_id,
                image,
                result,
                path: Some(path),
            } if !self.at_home
                && self.tab().id == tab_id
                && !self.tab().read_only
                && slate_doc::media_kind(&path) == slate_doc::MediaKind::Image
                && self.image_result_id(image).as_ref() == Some(&result)
                && self
                    .doc()
                    .scene
                    .node(image)
                    .is_some_and(|node| !node.locked) =>
            {
                if let Some(item) = self.item_for_path(&path) {
                    self.add_picked_image_layer(image, item);
                }
            }
            PickerMsg::OpenDoc(Some(path)) => self.open_doc_at(path),
            PickerMsg::SaveDocAs {
                tab_id,
                path: Some(path),
            } => self.save_doc_to(tab_id, path),
            PickerMsg::AddMedia {
                tab_id,
                at,
                paths: Some(paths),
            } => {
                if !self.at_home && self.tab().id == tab_id && !self.tab().read_only {
                    let items = self.add_paths(&paths);
                    self.place_items_on_board(&items, at);
                }
            }
            PickerMsg::AddFiles(Some(paths)) => {
                self.add_paths(&paths);
            }
            PickerMsg::ExportArtifact(Some(dir)) => self.do_export(dir),
            PickerMsg::WebPortalSource {
                portal,
                path: Some(path),
            } => {
                self.bind_web_path(portal, path);
            }
            PickerMsg::AgentPortalSource {
                portal,
                path: Some(path),
            } => self.bind_agent_project(portal, path),
            PickerMsg::AtlasPortalSource {
                portal,
                path: Some(path),
            } => self.bind_atlas_folder(portal, path),
            PickerMsg::SlatePortalSource {
                portal,
                path: Some(path),
            } => self.bind_slate_workbook(ctx, portal, path),
            PickerMsg::ModelScreenshotSave {
                tab_id,
                node,
                path: Some(path),
            } if !self.at_home && self.tab().id == tab_id => {
                self.finish_model_screenshot_save(node, path);
            }
            PickerMsg::PortalCaptureSave {
                tab_id,
                node,
                identity,
                video,
                path: Some(path),
            } => {
                self.finish_portal_capture_dialog(tab_id, node, identity, video, path);
            }
            PickerMsg::PortalCaptureVideoSave {
                tab_id,
                node,
                identity,
                generation,
                path: Some(path),
            } => self.finish_portal_capture_video_dialog(tab_id, node, identity, generation, path),
            _ => {}
        }
    }
}
