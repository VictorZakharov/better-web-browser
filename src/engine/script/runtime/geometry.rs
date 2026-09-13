use super::*;

impl ScriptRuntime {
    pub(crate) fn set_resize_boxes(
        &mut self,
        boxes: &HashMap<NodeId, crate::engine::layout::ResizeBox>,
    ) {
        self.host.borrow_mut().resize_boxes.clone_from(boxes);
    }

    pub(crate) fn has_pending_resize_observers(&self) -> bool {
        self.host.borrow().resize_observers_pending
    }
    pub(crate) fn set_layout_content_height(&mut self, height: f32) {
        self.host.borrow_mut().layout_content_height = height;
    }

    /// Publishes the renderer's latest layout snapshot to CSSOM View APIs in this realm.
    pub(crate) fn set_layout_geometry(&mut self, geometry: &HashMap<NodeId, RectF>) {
        let mut host = self.host.borrow_mut();
        host.layout_geometry.clone_from(geometry);
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
        self.notify_observers(true, true)
    }

    pub(crate) fn notify_resize_observers(&mut self) -> ScriptOutcome {
        self.notify_observers(true, false)
    }

    pub(crate) fn notify_intersection_observers(&mut self) -> ScriptOutcome {
        self.notify_observers(false, true)
    }

    fn notify_observers(&mut self, resize: bool, intersection: bool) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
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
            if intersection
                && let Err(error) =
                    context.eval(Source::from_bytes("__notifyIntersectionObservers();"))
            {
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
