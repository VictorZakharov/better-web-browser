//! Child viewport publication and observer delivery.
use super::*;

impl ScriptRuntime {
    /// Tell each child Window its actual CSS viewport after its iframe content box
    /// has been laid out. This also delivers the normal resize/media-query events.
    pub(crate) fn dispatch_frame_viewports(
        &mut self,
        viewports: &[(NodeId, RectF)],
    ) -> ScriptOutcome {
        let mut outcome = ScriptOutcome::default();
        let ids: Vec<_> = self
            .frames
            .as_ref()
            .into_iter()
            .flat_map(|frames| frames.children.keys().copied())
            .collect();
        for id in ids {
            let Some(rect) = viewports
                .iter()
                .find_map(|(document, rect)| (*document == id).then_some(*rect))
            else {
                continue;
            };
            let changed = self
                .frames
                .as_ref()
                .and_then(|frames| frames.children.get(&id))
                .is_some_and(|child| {
                    child.host.borrow().frame_viewport_dispatched != Some((rect.width, rect.height))
                });
            if changed {
                let child_outcome = {
                    let child = self
                        .frames
                        .as_mut()
                        .and_then(|frames| frames.children.get_mut(&id))
                        .unwrap();
                    let scale = child.host.borrow().media_environment.resolution_dppx;
                    child.host.borrow_mut().frame_viewport_dispatched =
                        Some((rect.width, rect.height));
                    child
                        .dispatch_user_input(UserInputEvent::Viewport {
                            width: rect.width,
                            height: rect.height,
                            layout_width: rect.width,
                            layout_height: rect.height,
                            scale,
                        })
                        .outcome
                };
                let child_outcome = self.collect_frame_result(id, child_outcome);
                documents::append(&mut outcome, child_outcome);
            }
            let nested = self
                .frames
                .as_mut()
                .and_then(|frames| frames.children.get_mut(&id))
                .map(|child| child.dispatch_frame_viewports(viewports));
            if let Some(nested) = nested {
                let nested = self.collect_frame_result(id, nested);
                documents::append(&mut outcome, nested);
            }
        }
        outcome
    }

    pub(crate) fn has_pending_frame_layout(&self) -> bool {
        self.frames.as_ref().is_some_and(|frames| {
            frames
                .children
                .values()
                .any(|child| child.host.borrow().frame_layout_pending)
        })
    }

    pub(crate) fn finish_frame_layout_attempt(&mut self) {
        if let Some(frames) = &mut self.frames {
            for child in frames.children.values_mut() {
                // Hidden and zero-size frames need no repeated parent rebuild. Revealing
                // the iframe invalidates its parent layout and publishes a real box.
                child.host.borrow_mut().frame_layout_pending = false;
            }
        }
    }

    pub(in crate::engine::script::runtime) fn child_observers_pending(&self, tasks: bool) -> bool {
        self.frames.as_ref().is_some_and(|frames| {
            frames.children.values().any(|child| {
                if tasks {
                    child.has_intersection_task()
                } else {
                    child.has_pending_intersection_observers()
                }
            })
        })
    }

    pub(in crate::engine::script::runtime) fn collect_child_observers(
        &mut self,
        tasks: bool,
    ) -> ScriptOutcome {
        self.sync_child_runtimes();
        let mut outcome = ScriptOutcome::default();
        let mut ids: Vec<_> = self
            .frames
            .as_ref()
            .into_iter()
            .flat_map(|frames| frames.children.keys().copied())
            .collect();
        ids.sort();
        for id in ids {
            let child_outcome = self
                .frames
                .as_mut()
                .and_then(|frames| frames.children.get_mut(&id))
                .map(|child| {
                    if tasks {
                        child.notify_intersection_observers()
                    } else {
                        child.gather_intersection_observers()
                    }
                });
            if let Some(child_outcome) = child_outcome {
                let child_outcome = self.collect_frame_result(id, child_outcome);
                documents::append(&mut outcome, child_outcome);
            }
        }
        outcome
    }
}
