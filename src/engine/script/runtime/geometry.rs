use super::*;

impl ScriptRuntime {
    pub(crate) fn set_hit_test_snapshot(&mut self, layout: &crate::engine::LayoutOutput) {
        let mut host = self.host.borrow_mut();
        host.hit_test_snapshot =
            crate::engine::layout::HitTestSnapshot::from_layout(layout, &host.document);
        host.hit_test_geometry_version = Some(host.document.subtree_mutation_version());
    }
    pub(crate) fn set_layout_fragments(
        &mut self,
        fragments: &std::sync::Arc<crate::engine::layout::FragmentGeometry>,
    ) {
        self.host.borrow_mut().layout_fragments = fragments.clone();
    }
    pub(crate) fn set_sticky_offsets(&mut self, offsets: &HashMap<NodeId, (f32, f32)>) {
        let mut host = self.host.borrow_mut();
        host.sticky_offsets.clone_from(offsets);
        host.geometry_scroll_offset = host.document.scroll_offset.get();
        host.geometry_scroll_dirty = false;
    }
    pub(crate) fn set_scroll_boxes(
        &mut self,
        boxes: &HashMap<NodeId, crate::engine::layout::ScrollBox>,
    ) {
        self.host.borrow_mut().scroll_boxes.clone_from(boxes);
    }
    pub(crate) fn set_resize_boxes(
        &mut self,
        boxes: &HashMap<NodeId, crate::engine::layout::ResizeBox>,
    ) {
        self.host.borrow_mut().resize_boxes.clone_from(boxes);
    }

    pub(crate) fn has_pending_resize_observers(&self) -> bool {
        let host = self.host.borrow();
        host.resize_observers_pending && !host.resize_observers_deferred
    }

    pub(crate) fn has_pending_intersection_observers(&self) -> bool {
        self.host.borrow().intersection_observers_pending || self.child_observers_pending(false)
    }

    pub(crate) fn has_intersection_task(&self) -> bool {
        self.host.borrow().intersection_task_pending || self.child_observers_pending(true)
    }

    pub(crate) fn gather_intersection_observers(&mut self) -> ScriptOutcome {
        let mut outcome = self.notify_observers(false, false, true);
        super::frames::documents::append(&mut outcome, self.collect_child_observers(false));
        outcome
    }
    pub(crate) fn set_layout_content_height(&mut self, height: f32) {
        self.host.borrow_mut().layout_content_height = height;
    }

    /// Publishes the renderer's latest layout snapshot to CSSOM View APIs in this realm.
    pub(crate) fn set_layout_geometry(&mut self, geometry: &HashMap<NodeId, RectF>) {
        let mut host = self.host.borrow_mut();
        host.layout_geometry.clone_from(geometry);
        host.layout_fragments = Default::default();
        host.layout_geometry_version = host.document.subtree_mutation_version();
        host.pending_layout_invalidation
            .acknowledge_published_geometry();
        host.layout_geometry_initialized = true;
    }

    pub(crate) fn set_layout_flush_callback(&mut self, callback: LayoutFlushCallback) {
        self.host.borrow_mut().layout_flush = Some(callback);
    }

    /// Runs the dedicated rendering-observer task against the latest layout snapshot.
    #[cfg(test)]
    pub(crate) fn notify_layout_changed(&mut self) -> ScriptOutcome {
        self.notify_observers(true, true, true)
    }

    pub(crate) fn notify_resize_observers(&mut self) -> ScriptOutcome {
        self.notify_observers(true, false, false)
    }

    pub(crate) fn notify_intersection_observers(&mut self) -> ScriptOutcome {
        let mut outcome = self.notify_observers(false, true, false);
        super::frames::documents::append(&mut outcome, self.collect_child_observers(true));
        outcome
    }

    fn notify_observers(
        &mut self,
        resize: bool,
        intersection: bool,
        gather: bool,
    ) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        // A skipped observation belongs to a later rendering opportunity, after that
        // opportunity's animation callbacks, not the next input/resource checkpoint.
        let resize = resize && !host.borrow().resize_observers_deferred;
        let result = catch_unwind(AssertUnwindSafe(|| {
            host.borrow_mut().begin_task();
            if resize {
                host.borrow_mut().resize_observers_pending = false;
            }
            let mut outcome = ScriptOutcome::default();
            let delivery = (|| -> JsResult<()> {
                if !resize {
                    return Ok(());
                }
                let mut depth = 0.0_f64;
                loop {
                    let depth_arg = if depth.is_infinite() {
                        "Infinity".into()
                    } else {
                        depth.to_string()
                    };
                    if !context
                        .eval(Source::from_bytes(format!(
                            "__gatherResizeObservers({depth_arg})"
                        )))?
                        .to_boolean()
                    {
                        break;
                    }
                    let mut shallowest = f64::INFINITY;
                    loop {
                        let value =
                            context.eval(Source::from_bytes("__broadcastResizeObserver()"))?;
                        let next = value.as_number().unwrap_or(-1.0);
                        if next < 0.0 {
                            break;
                        }
                        shallowest = shallowest.min(next);
                        context.run_jobs()?;
                    }
                    depth = shallowest;
                }
                context.eval(Source::from_bytes("__finishResizeObservers()"))?;
                context.run_jobs()?;
                Ok(())
            })();
            if let Err(error) = delivery {
                outcome
                    .errors
                    .push(format!("notify resize observers: {error}"));
            }
            let intersections = (|| -> JsResult<()> {
                if gather {
                    host.borrow_mut().intersection_observers_pending = false;
                    context.eval(Source::from_bytes("__gatherIntersectionObservers();"))?;
                }
                if intersection {
                    host.borrow_mut().intersection_task_pending = false;
                    context.eval(Source::from_bytes("__beginIntersectionDelivery();"))?;
                    while context
                        .eval(Source::from_bytes("__broadcastIntersectionObserver()"))?
                        .to_boolean()
                    {
                        context.run_jobs()?;
                    }
                }
                Ok(())
            })();
            if let Err(error) = intersections {
                outcome
                    .errors
                    .push(format!("notify geometry observers: {error}"));
            }
            if let Err(error) = context.run_jobs() {
                outcome
                    .errors
                    .push(format!("geometry observer promise job: {error}"));
            }
            outcome
        }));
        self.finish_guarded_run(result)
    }
}
