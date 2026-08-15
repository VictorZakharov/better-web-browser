//! Nonblocking ownership of the sandboxed renderer lifecycle.

use super::*;
use better_web_browser::renderer_process::{
    RendererEvent, RendererExit, RendererLaunchOptions, RendererSession, RendererSnapshot,
    RendererState,
};
use std::sync::{Arc, Mutex, mpsc};

const RENDERER_MONITOR_INTERVAL_MS: u32 = 250;
const PRIMARY_RENDERER_CONTEXT_ID: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RendererLifecyclePhase {
    Starting,
    Running,
    Unresponsive,
    Exited,
    LaunchFailed,
}

#[derive(Clone, Debug)]
pub(super) struct RendererTaskStatus {
    pub(super) context_id: u64,
    pub(super) phase: RendererLifecyclePhase,
    pub(super) snapshot: Option<RendererSnapshot>,
    pub(super) restart_count: u32,
    pub(super) last_exit: Option<RendererExit>,
    pub(super) launch_error: Option<String>,
    pub(super) last_diagnostic: Option<String>,
}

impl RendererTaskStatus {
    fn starting(context_id: u64) -> Self {
        Self {
            context_id,
            phase: RendererLifecyclePhase::Starting,
            snapshot: None,
            restart_count: 0,
            last_exit: None,
            launch_error: None,
            last_diagnostic: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct RendererTaskRegistry {
    pub(super) renderers: Vec<RendererTaskStatus>,
}

impl Default for RendererTaskRegistry {
    fn default() -> Self {
        Self {
            renderers: vec![RendererTaskStatus::starting(PRIMARY_RENDERER_CONTEXT_ID)],
        }
    }
}

impl RendererTaskRegistry {
    fn renderer_mut(&mut self, context_id: u64) -> &mut RendererTaskStatus {
        if let Some(index) = self
            .renderers
            .iter()
            .position(|renderer| renderer.context_id == context_id)
        {
            &mut self.renderers[index]
        } else {
            self.renderers
                .push(RendererTaskStatus::starting(context_id));
            self.renderers.last_mut().expect("renderer was appended")
        }
    }
}

pub(super) type SharedRendererRegistry = Arc<Mutex<RendererTaskRegistry>>;

impl BrowserState {
    pub(super) unsafe fn start_renderer(&mut self) {
        if self.renderer_launch_pending || self.renderer_session.is_some() {
            return;
        }
        self.renderer_launch_pending = true;
        self.update_renderer_status(|status| {
            status.phase = RendererLifecyclePhase::Starting;
            status.snapshot = None;
            status.launch_error = None;
            status.last_diagnostic = None;
        });

        let window = self.window as usize;
        let (sender, receiver) = mpsc::channel();
        let spawn = std::thread::Builder::new()
            .name("breeze-renderer-launch".into())
            .spawn(move || {
                let result =
                    RendererLaunchOptions::current_executable().and_then(RendererSession::launch);
                if sender.send(result).is_ok() {
                    unsafe { PostMessageW(window as Hwnd, WM_APP_RENDERER_LAUNCHED, 0, 0) };
                }
            });
        match spawn {
            Ok(_) => self.renderer_launch_receiver = Some(receiver),
            Err(error) => {
                self.record_renderer_launch_failure(format!("start renderer launcher: {error}"));
            }
        }
    }

    pub(super) unsafe fn finish_renderer_launch(&mut self) {
        let Some(receiver) = self.renderer_launch_receiver.take() else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                self.renderer_launch_receiver = Some(receiver);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("renderer launcher exited without a result".into())
            }
        };
        self.renderer_launch_pending = false;
        let session = match result {
            Ok(session) => session,
            Err(error) => {
                self.record_renderer_launch_failure(error);
                return;
            }
        };
        let snapshot = session.snapshot();
        self.update_renderer_status(|status| {
            if self.renderer_started_once {
                status.restart_count = status.restart_count.saturating_add(1);
            }
            status.phase = RendererLifecyclePhase::Running;
            status.snapshot = Some(snapshot);
            status.launch_error = None;
        });
        self.renderer_started_once = true;
        self.renderer_session = Some(session);
        if SetTimer(
            self.window,
            ID_RENDERER_MONITOR_TIMER,
            RENDERER_MONITOR_INTERVAL_MS,
            null(),
        ) == 0
        {
            self.renderer_session.take();
            self.record_renderer_launch_failure(last_error("start renderer monitor"));
        }
    }

    pub(super) unsafe fn poll_renderer(&mut self) {
        let (snapshot, events) = {
            let Some(session) = self.renderer_session.as_ref() else {
                KillTimer(self.window, ID_RENDERER_MONITOR_TIMER);
                return;
            };
            let snapshot = session.snapshot();
            let mut events = Vec::new();
            while let Ok(Some(event)) = session.try_event() {
                events.push(event);
            }
            (snapshot, events)
        };

        self.update_renderer_status(|status| {
            status.phase = match snapshot.state {
                RendererState::Running => RendererLifecyclePhase::Running,
                RendererState::Unresponsive => RendererLifecyclePhase::Unresponsive,
                RendererState::Exited => RendererLifecyclePhase::Exited,
            };
            status.snapshot = Some(snapshot);
        });

        let mut exit = None;
        for event in events {
            match event {
                RendererEvent::Diagnostic { code, text } => {
                    self.update_renderer_status(|status| {
                        status.last_diagnostic = Some(format!("{code}: {text}"));
                    });
                }
                RendererEvent::Unresponsive => {
                    self.update_renderer_status(|status| {
                        status.phase = RendererLifecyclePhase::Unresponsive;
                    });
                }
                RendererEvent::Exited(renderer_exit) => exit = Some(renderer_exit),
            }
        }

        if let Some(exit) = exit {
            let crash_surface = exit.crash_surface();
            self.update_renderer_status(|status| {
                status.phase = RendererLifecyclePhase::Exited;
                status.last_exit = Some(exit);
            });
            KillTimer(self.window, ID_RENDERER_MONITOR_TIMER);
            self.renderer_session.take();
            if let Some(surface) = crash_surface {
                self.set_status(&format!(
                    "{}: {}. Reload to restart the renderer.",
                    surface.title, surface.detail
                ));
            }
        }
    }

    fn update_renderer_status(&self, update: impl FnOnce(&mut RendererTaskStatus)) {
        let mut registry = self
            .renderer_registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(registry.renderer_mut(PRIMARY_RENDERER_CONTEXT_ID));
    }

    unsafe fn record_renderer_launch_failure(&mut self, error: String) {
        self.renderer_launch_receiver = None;
        self.renderer_launch_pending = false;
        self.update_renderer_status(|status| {
            status.phase = RendererLifecyclePhase::LaunchFailed;
            status.snapshot = None;
            status.launch_error = Some(error.clone());
        });
        self.set_status(&format!("Renderer unavailable: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_keeps_renderer_context_rows_independent() {
        let mut registry = RendererTaskRegistry::default();
        registry.renderer_mut(2).restart_count = 3;

        assert_eq!(registry.renderers.len(), 2);
        assert_eq!(registry.renderers[0].context_id, 1);
        assert_eq!(registry.renderers[0].restart_count, 0);
        assert_eq!(registry.renderers[1].context_id, 2);
        assert_eq!(registry.renderers[1].restart_count, 3);
    }
}
