//! DOM events for completed element-owned page resources.

use super::super::*;

impl DocumentRuntime {
    pub(super) fn dispatch_resource_event(
        &mut self,
        resource: &PageResource,
        event_type: &'static str,
    ) -> Result<bool, String> {
        let targets = self.page.resource_event_targets(resource);
        let dispatched = !targets.is_empty();
        if self.script_runtime.is_none() {
            if dispatched {
                self.pending_resource_events
                    .push((resource.clone(), event_type));
            }
            return Ok(dispatched);
        }
        let image_dimensions = match resource {
            PageResource::Image { url } if event_type == "load" => self
                .page
                .images
                .get(url)
                .map(|image| (image.width, image.height))
                .unwrap_or_default(),
            PageResource::Image { .. } => (0, 0),
            _ => (0, 0),
        };
        for target in targets {
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
        let events = std::mem::take(&mut self.pending_resource_events);
        for (resource, event_type) in events {
            self.dispatch_resource_event(&resource, event_type)?;
        }
        Ok(())
    }
}
