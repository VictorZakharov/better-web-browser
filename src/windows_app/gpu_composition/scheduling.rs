use super::*;

impl BrowserState {
    pub(in crate::windows_app) unsafe fn suspend_gpu_canvas(&mut self) {
        if self.gpu_policy_sent && !self.gpu_canvas_disabled {
            self.gpu_compositor = None;
            self.gpu_repair_pending = !self.gpu_canvases.is_empty();
            if let (Some(document), Some(session)) = (
                self.navigation.active_document(),
                self.renderer_session.as_ref(),
            ) {
                // Retire the HWND consumer before requesting a CPU repair. The
                // renderer retains its latest immutable slot until repair completes.
                let _ = session.set_gpu_canvas_policy(document, false);
                self.gpu_policy_sent = false;
            }
        }
        self.gpu_compositor = None;
        self.gpu_composition_pending = false;
        self.gpu_composition_wait_since = None;
        KillTimer(self.window, RETRY_TIMER);
    }

    pub(in crate::windows_app) unsafe fn request_gpu_composition(&mut self) {
        if self.processing_background_tab
            || (self.gpu_canvases.is_empty() && self.gpu_compositor.is_none())
        {
            return;
        }
        self.gpu_composition_pending = true;
        self.gpu_composition_wait_since
            .get_or_insert(Instant::now());
        self.paint_opportunity.request(Instant::now());
        SetTimer(self.window, RETRY_TIMER, 8, null());
    }

    pub(in crate::windows_app) unsafe fn flush_gpu_composition(&mut self) {
        if !self.gpu_composition_pending {
            self.gpu_composition_wait_since = None;
            return;
        }
        self.compose_gpu_content();
        if !self.gpu_composition_pending {
            self.gpu_composition_wait_since = None;
            KillTimer(self.window, RETRY_TIMER);
        }
    }

    pub(in crate::windows_app) unsafe fn gpu_retry_timer(&mut self) {
        if self.gpu_composition_pending {
            self.paint_opportunity.request(Instant::now());
        } else {
            KillTimer(self.window, RETRY_TIMER);
        }
    }

    pub(in crate::windows_app) unsafe fn flush_gpu_capture(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.gpu_composition_pending {
            self.flush_gpu_composition();
            if self.gpu_composition_pending {
                if Instant::now() >= deadline {
                    return Err("GPU composition did not settle before capture".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(())
    }
}
