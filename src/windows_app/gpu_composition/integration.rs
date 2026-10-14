use super::*;

impl BrowserState {
    pub(in crate::windows_app) fn gpu_content_active(&self) -> bool {
        self.gpu_compositor
            .as_ref()
            .is_some_and(|compositor| compositor.is_active(self.navigation.active_document()))
    }

    pub(in crate::windows_app) unsafe fn compose_gpu_content(&mut self) -> bool {
        let started = Instant::now();
        if self.processing_background_tab {
            return false;
        }
        self.gpu_composition_pending = false;
        let Some(document) = self.navigation.active_document() else {
            self.gpu_compositor = None;
            return false;
        };
        if self.gpu_canvases.is_empty()
            || self.gpu_canvas_disabled
            || self.gpu_repair_pending
            || self.crashed
            || self.surface != Surface::Page
        {
            self.gpu_compositor = None;
            return false;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let top = self.toolbar_height();
        let width = client.width().max(1) as u32;
        let height = (client.height() - top - self.status_height()).max(1) as u32;
        let first = self.gpu_canvases[0].clone();
        let replace = self.gpu_compositor.as_ref().is_none_or(|compositor| {
            compositor.document != document
                || (
                    compositor.device.adapter_low,
                    compositor.device.adapter_high,
                ) != (first.frame.adapter_low, first.frame.adapter_high)
        });
        let result = (|| {
            if self
                .gpu_composition_wait_since
                .is_some_and(|since| since.elapsed() >= Duration::from_secs(2))
            {
                return Err("GPU Canvas handoff did not become ready within two seconds".into());
            }
            for canvas in &self.gpu_canvases {
                if self.presented_images.get(&canvas.url).is_none_or(|image| {
                    (image.width, image.height) != (canvas.frame.width, canvas.frame.height)
                }) {
                    return Err("GPU Canvas has no matching document image".into());
                }
            }
            if replace {
                self.gpu_compositor = None;
                self.gpu_compositor = Some(Compositor::new(
                    self.window,
                    document,
                    &first,
                    width,
                    height,
                    top,
                    self.app.active_memory_budget,
                )?);
            }
            let scale = self.page_scale();
            let tab = self.tabs.active();
            let compositor = self.gpu_compositor.as_mut().unwrap();
            if !compositor.queue_available(width, height)? {
                return Ok(false);
            }
            compositor.resize(width, height, top)?;
            compositor
                .frames
                .update(&compositor.device, &tab.gpu_canvases)?;
            if !compositor.frames.ready(&tab.gpu_canvases) {
                return Ok(false);
            }
            compositor.paint(
                &tab.page_layout,
                &tab.presented_images,
                &tab.presented_glyphs,
                scale,
                tab.scroll_y,
            )
        })();
        if let Err(error) = &result {
            self.gpu_compositor = None;
            self.gpu_canvas_disabled = true;
            if let Some(session) = self.renderer_session.as_ref() {
                let _ = session.set_gpu_canvas_policy(document, false);
            }
            self.gpu_fallback_reason = Some(error.clone());
            return false;
        }
        if result == Ok(false) {
            self.gpu_composition_pending = true;
            return self.gpu_content_active();
        }
        self.gpu_frames_composed = self.gpu_frames_composed.saturating_add(1);
        self.gpu_last_submit_ms = started.elapsed().as_secs_f64() * 1000.0;
        true
    }

    pub(in crate::windows_app) fn gpu_composition_report_json(&self) -> String {
        serde_json::json!({"active":self.gpu_content_active(),"frames_composed":self.gpu_frames_composed,
            "last_submit_ms":self.gpu_last_submit_ms,"fallback_reason":self.gpu_fallback_reason,
            "bitmap_cache":self.gpu_compositor.as_ref().map(|compositor|compositor.images.report()),
            "coverage_cache":self.gpu_compositor.as_ref().map(|compositor|compositor.coverage.report()),
            "scope":"native GPU submission; excludes GPU completion and DWM presentation"}).to_string()
    }

    pub(in crate::windows_app) fn enable_gpu_canvas_policy(&mut self) {
        if self.processing_background_tab
            || self.surface != Surface::Page
            || self.crashed
            || self.gpu_policy_sent
            || self.gpu_canvas_disabled
            || self.gpu_repair_pending
        {
            return;
        }
        let Some(document) = self.navigation.active_document() else {
            return;
        };
        if !self
            .presented_images
            .keys()
            .any(|url| url.starts_with("breeze-internal:canvas:"))
        {
            return;
        }
        if let Some(session) = self.renderer_session.as_ref()
            && session.set_gpu_canvas_policy(document, true).is_ok()
        {
            self.gpu_policy_sent = true;
        }
    }
}
