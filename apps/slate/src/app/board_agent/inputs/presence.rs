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
        let source = self
            .image_item_path(id)
            .ok_or("This image has no available result")?;
        let source = resolve_source(self.tab().path.as_deref(), &source.to_string_lossy());
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
        let key = identity.finish();
        let bake = self.layer_bakes.readiness_for(self.tab().id, image.item);
        let mut document = self.doc().clone();
        let workbook = self.tab().path.clone();
        pool.schedule(
            key,
            bake,
            Box::new(move || {
                if atlas_core::cloud::is_dehydrated(&source) {
                    return None;
                }
                crate::app::board_image_layers::resolve_image_doc_paths(
                    &mut document,
                    workbook.as_deref(),
                );
                crate::app::image_composite::agent_wired_image_file_from_source(
                    &document, &node, &image, &source,
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
            Some(None) => Err("The image could not be prepared. Cloud-only sources must be made available locally first.".into()),
            None => Ok(None),
        }
    }
}
