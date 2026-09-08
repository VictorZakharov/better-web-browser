//! DOM events for completed element-owned page resources.

use super::super::*;
mod state;
pub(in crate::renderer_process::child::document) use state::ResourceEvents;

impl DocumentRuntime {
    pub(super) fn dispatch_resource_event(
        &mut self,
        resource: &PageResource,
        event_type: &'static str,
    ) -> Result<bool, String> {
        self.resource_events.complete(resource, event_type);
        self.dispatch_cached_resource_events()
    }

    pub(in crate::renderer_process::child::document) fn dispatch_cached_resource_events(
        &mut self,
    ) -> Result<bool, String> {
        if self.script_runtime.is_none() {
            return Ok(false);
        }
        let targets = self.resource_events.pending(&self.page);
        let dispatched = !targets.is_empty();
        if targets.iter().any(|(resource, event, _)| {
            *event == "load" && matches!(resource, PageResource::Stylesheet { .. })
        }) {
            // A load handler must observe the sheet that has just joined the cascade,
            // through both computed-style reads and synchronous geometry queries.
            self.sync_script_layout_page();
            if let Some(runtime) = self.script_runtime.as_mut() {
                self.page.synchronize_script_stylesheets(runtime);
            }
        }
        for (resource, event_type, target) in targets {
            // An earlier completion callback may detach or retarget a later owner.
            if crate::engine::dom::Node::shadow_including_root(&target).id()
                != self.page.dom.document.id()
                || self.page.resource_event_key(&target).as_ref() != Some(&resource)
            {
                continue;
            }
            let image_dimensions = match &resource {
                PageResource::Image { url } if event_type == "load" => self
                    .page
                    .images
                    .get(url)
                    .map(|image| (image.width, image.height))
                    .unwrap_or_default(),
                PageResource::Image { .. } => (0, 0),
                _ => (0, 0),
            };
            let event = if matches!(resource, PageResource::Image { .. }) {
                crate::engine::UserInputEvent::ImageResource {
                    target,
                    event_type,
                    natural_width: image_dimensions.0,
                    natural_height: image_dimensions.1,
                }
            } else {
                crate::engine::UserInputEvent::Simple {
                    target,
                    event_type,
                    bubbles: false,
                    cancelable: false,
                }
            };
            let response = self.dispatch_user_input(event)?;
            merge_outcome(
                &mut self.pending_async_outcome,
                response.outcome,
                self.page.dom.document.id(),
            );
        }
        Ok(dispatched)
    }

    pub(in crate::renderer_process::child::document) fn flush_pending_resource_events(
        &mut self,
    ) -> Result<(), String> {
        self.dispatch_cached_resource_events().map(|_| ())
    }
}
