//! Explicit participant image reads use the existing bounded image-input pool.
use super::*;

pub(crate) struct ScopedImageInput {
    key: u64,
}
impl SlateApp {
    pub(crate) fn prepare_scoped_image_input(
        &self,
        id: NodeId,
        read: &str,
    ) -> Result<ScopedImageInput, String> {
        let node = self
            .doc()
            .scene
            .node(id)
            .ok_or("Image no longer exists")?
            .clone();
        let original = self
            .image_item_path(id)
            .ok_or("This image has no available result")?;
        let source = resolve_source(self.tab().path.as_deref(), &original.to_string_lossy());
        let workspace = self
            .ai
            .config
            .valid_workspace()
            .map(PathBuf::from)
            .ok_or("Establish the AI workspace before requesting image reads")?;
        let item = atlas_agent::ContextItem {
            node: id.0,
            text: String::new(),
            images: vec![source.to_string_lossy().into_owned()],
            outputs: Default::default(),
            active: None,
            depth: None,
            slot: Some(atlas_agent::InputSlot::Media),
        };
        let image = self
            .image_for_captured_input(&item, &source)?
            .ok_or("Choose a linked image")?;
        let mut pool = self.agents.image_prep.borrow_mut();
        pool.receive();
        let base_key = pool.key(
            &node,
            &image,
            &source,
            (self.scene_gen, self.doc().scene.scene_gen()),
        );
        // Each explicit attempt can recover after a file becomes available;
        // a prior cloud-only/failure result must not poison a later read.
        use std::hash::{Hash, Hasher};
        let mut identity = std::collections::hash_map::DefaultHasher::new();
        base_key.hash(&mut identity);
        read.hash(&mut identity);
        workspace.hash(&mut identity);
        let key = identity.finish();
        let bake = self.layer_bakes.readiness_for(self.tab().id, image.item);
        let mut document = self.doc().clone();
        let workbook = self.tab().path.clone();
        let original = original.clone();
        let source = source.clone();
        let read_id = read.to_owned();
        pool.schedule(
            key,
            bake,
            Box::new(move || {
                crate::app::board_image_layers::resolve_image_doc_paths(
                    &mut document,
                    workbook.as_deref(),
                );
                crate::app::image_composite::presence_wired_image_file_from_source(
                    &document, &node, &image, &original, &source, &workspace, &read_id,
                )
            }),
        )
        .map_err(|()| "Image input preparation is busy; try again shortly")?;
        Ok(ScopedImageInput { key })
    }
    pub(crate) fn poll_scoped_image_input(
        &self,
        input: &ScopedImageInput,
    ) -> Result<Option<PathBuf>, String> {
        let mut pool = self.agents.image_prep.borrow_mut();
        pool.receive();
        match pool.result(input.key) {
            Some(Some(path)) => Ok(Some(path.clone())),
            Some(None) => Err("The image could not be prepared. Sources must be available locally, outside machine-private state, and within the bounded image limits.".into()),
            None => Ok(None),
        }
    }
}
