//! Publish immutable decoded resources, before load handlers or initial scripts run.
use super::*;
use crate::engine::DecodedImage;

impl ScriptRuntime {
    pub(crate) fn set_document_images(
        &mut self,
        images: &HashMap<String, DecodedImage>,
        origins: &HashMap<String, bool>,
    ) {
        let mut host = self.host.borrow_mut();
        host.element_images.synchronize(images, origins);
    }
}
