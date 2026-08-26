//! Nonblocking ownership of one sandboxed renderer lifecycle per browser tab.

mod events;

use super::tabs::{IdentifiedTab, TabId};
use super::*;
use better_web_browser::renderer_process::{
    RendererExit, RendererLaunchOptions, RendererSession, RendererSnapshot,
};
use better_web_browser::renderer_protocol::BrowsingContextId;
use std::sync::{Arc, Mutex, mpsc};

const RENDERER_MONITOR_INTERVAL_MS: u32 = 250;
const ACTIVE_RENDERER_MONITOR_INTERVAL_MS: u32 = 16;

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
    pub(super) tab_title: String,
    pub(super) phase: RendererLifecyclePhase,
    pub(super) snapshot: Option<RendererSnapshot>,
    pub(super) restart_count: u32,
    pub(super) last_exit: Option<RendererExit>,
    pub(super) launch_error: Option<String>,
    pub(super) last_diagnostic: Option<String>,
}

impl RendererTaskStatus {
    pub(super) fn starting(context_id: u64, tab_title: String) -> Self {
        Self {
            context_id,
            tab_title,
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
            renderers: vec![RendererTaskStatus::starting(1, "New Tab".into())],
        }
    }
}

impl RendererTaskRegistry {
    fn renderer_mut(&mut self, context_id: u64, title: &str) -> &mut RendererTaskStatus {
        if let Some(index) = self
            .renderers
            .iter()
            .position(|renderer| renderer.context_id == context_id)
        {
            let renderer = &mut self.renderers[index];
            renderer.tab_title.clear();
            renderer.tab_title.push_str(title);
            renderer
        } else {
            self.renderers
                .push(RendererTaskStatus::starting(context_id, title.into()));
            self.renderers.last_mut().expect("renderer was appended")
        }
    }

    fn remove(&mut self, context_id: u64) {
        self.renderers
            .retain(|renderer| renderer.context_id != context_id);
    }
}

pub(super) type SharedRendererRegistry = Arc<Mutex<RendererTaskRegistry>>;

impl BrowserState {
    pub(super) unsafe fn ensure_renderer_monitoring(&mut self) {
        let needs_monitor = self
            .tabs
            .iter()
            .any(|tab| tab.renderer_session.is_some() || tab.renderer_launch_receiver.is_some());
        if needs_monitor {
            SetTimer(
                self.window,
                ID_RENDERER_MONITOR_TIMER,
                self.renderer_monitor_interval(),
                null(),
            );
        } else {
            KillTimer(self.window, ID_RENDERER_MONITOR_TIMER);
        }
    }

    pub(super) unsafe fn start_renderer(&mut self) {
        self.start_renderer_for(self.tabs.active_id());
    }

    pub(super) unsafe fn replace_renderer_for_navigation(&mut self, id: TabId) {
        let session = self.tabs.get_mut(id).and_then(|tab| {
            tab.renderer_launch_receiver.take();
            tab.renderer_clock_pending = false;
            tab.renderer_work_pending = false;
            tab.renderer_session.take()
        });
        if let Some(session) = session {
            session.terminate_in_background();
        }
    }

    pub(super) unsafe fn start_renderer_for(&mut self, id: TabId) {
        let title = {
            let Some(tab) = self.tabs.get_mut(id) else {
                return;
            };
            if tab.renderer_launch_receiver.is_some() || tab.renderer_session.is_some() {
                return;
            }
            tab.incidents.record("renderer", "launch requested");
            tab.title.clone()
        };
        self.update_renderer_status(id, &title, |status| {
            status.phase = RendererLifecyclePhase::Starting;
            status.snapshot = None;
            status.launch_error = None;
            status.last_diagnostic = None;
        });

        let tab_router = self.app.tab_router.clone();
        let (sender, receiver) = mpsc::channel();
        let spawn = std::thread::Builder::new()
            .name(format!("breeze-renderer-launch-{}", id.get()))
            .spawn(move || {
                let result = RendererLaunchOptions::current_executable().and_then(|mut options| {
                    options.browsing_context = BrowsingContextId::new(id.get())
                        .map_err(|error| format!("allocate browsing context: {error}"))?;
                    RendererSession::launch(options)
                });
                if sender.send(result).is_ok()
                    && let Some(window) = tab_router.destination(id)
                {
                    unsafe {
                        PostMessageW(
                            window as Hwnd,
                            WM_APP_RENDERER_LAUNCHED,
                            id.get() as usize,
                            0,
                        );
                    }
                }
            });
        match spawn {
            Ok(_) => {
                if let Some(tab) = self.tabs.get_mut(id) {
                    tab.renderer_launch_receiver = Some(receiver);
                }
            }
            Err(error) => {
                self.record_renderer_launch_failure(
                    id,
                    format!("start renderer launcher: {error}"),
                );
            }
        }
    }

    pub(super) unsafe fn finish_renderer_launch(&mut self, id: TabId) {
        let receiver = self
            .tabs
            .get_mut(id)
            .and_then(|tab| tab.renderer_launch_receiver.take());
        let Some(receiver) = receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                if let Some(tab) = self.tabs.get_mut(id) {
                    tab.renderer_launch_receiver = Some(receiver);
                }
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("renderer launcher exited without a result".into())
            }
        };
        let session = match result {
            Ok(session) => session,
            Err(error) => {
                self.record_renderer_launch_failure(id, error);
                return;
            }
        };
        let snapshot = session.snapshot();
        let (title, restarted) = {
            let Some(tab) = self.tabs.get_mut(id) else {
                return;
            };
            let restarted = tab.renderer_started_once;
            tab.renderer_started_once = true;
            tab.renderer_session = Some(session);
            tab.crashed = false;
            tab.incidents
                .record("renderer", format!("ready process {}", snapshot.process_id));
            (tab.title.clone(), restarted)
        };
        self.update_renderer_status(id, &title, |status| {
            if restarted {
                status.restart_count = status.restart_count.saturating_add(1);
            }
            status.phase = RendererLifecyclePhase::Running;
            status.snapshot = Some(snapshot);
            status.launch_error = None;
        });
        if SetTimer(
            self.window,
            ID_RENDERER_MONITOR_TIMER,
            self.renderer_monitor_interval(),
            null(),
        ) == 0
        {
            if let Some(tab) = self.tabs.get_mut(id) {
                tab.renderer_session.take();
            }
            self.record_renderer_launch_failure(id, last_error("start renderer monitor"));
            return;
        }
        self.submit_pending_renderer_document_for(id);
    }

    pub(super) unsafe fn poll_renderers(&mut self) {
        let ids = self
            .tabs
            .iter()
            .map(IdentifiedTab::tab_id)
            .collect::<Vec<_>>();
        for id in ids {
            self.poll_renderer(id);
            self.enforce_first_presentation_deadline(id);
        }
        let has_live_or_pending = self
            .tabs
            .iter()
            .any(|tab| tab.renderer_session.is_some() || tab.renderer_launch_receiver.is_some());
        if !has_live_or_pending {
            KillTimer(self.window, ID_RENDERER_MONITOR_TIMER);
        } else {
            SetTimer(
                self.window,
                ID_RENDERER_MONITOR_TIMER,
                self.renderer_monitor_interval(),
                null(),
            );
        }
    }

    pub(super) unsafe fn terminate_renderer_for(&mut self, id: TabId) {
        let result = self
            .tabs
            .get_mut(id)
            .and_then(|tab| {
                tab.renderer_work_pending = true;
                tab.renderer_session.as_ref()
            })
            .ok_or_else(|| "renderer is not running".to_string())
            .and_then(RendererSession::terminate);
        match result {
            Ok(()) => {
                if self.tabs.active_id() == id {
                    self.set_status("Terminating the selected renderer process …");
                }
                self.ensure_renderer_monitoring();
            }
            Err(error) => {
                if let Some(tab) = self.tabs.get_mut(id) {
                    tab.renderer_work_pending = false;
                }
                self.set_status(&format!("Could not terminate renderer: {error}"));
            }
        }
    }

    fn renderer_monitor_interval(&self) -> u32 {
        if self.benchmark.is_some()
            || self.tabs.iter().any(|tab| {
                tab.navigation.is_loading()
                    || tab.renderer_work_pending
                    || tab.renderer_input_poll_budget > 0
                    || !tab.pending_renderer_inputs.is_empty()
                    || tab.renderer_next_timer.is_some()
            })
        {
            ACTIVE_RENDERER_MONITOR_INTERVAL_MS
        } else {
            RENDERER_MONITOR_INTERVAL_MS
        }
    }

    pub(super) fn register_renderer_tab(&self, id: TabId) {
        let title = self
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.title.as_str())
            .unwrap_or("New Tab");
        self.update_renderer_status(id, title, |_| {});
    }

    pub(super) fn remove_renderer_tab(&self, id: TabId) {
        let mut registry = self
            .renderer_registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry.remove(id.get());
    }

    pub(super) fn update_renderer_tab_title(&self, id: TabId, title: &str) {
        self.update_renderer_status(id, title, |_| {});
    }

    fn update_renderer_status(
        &self,
        id: TabId,
        title: &str,
        update: impl FnOnce(&mut RendererTaskStatus),
    ) {
        let mut registry = self
            .renderer_registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(registry.renderer_mut(id.get(), title));
    }

    unsafe fn record_renderer_launch_failure(&mut self, id: TabId, error: String) {
        let title = if let Some(tab) = self.tabs.get_mut(id) {
            tab.mark_crashed(format!(
                "Renderer unavailable: {error}. Reload to try again."
            ));
            tab.title.clone()
        } else {
            return;
        };
        self.update_renderer_status(id, &title, |status| {
            status.phase = RendererLifecyclePhase::LaunchFailed;
            status.snapshot = None;
            status.launch_error = Some(error.clone());
        });
        if self.tabs.active_id() == id {
            self.set_status(&format!("Renderer unavailable: {error}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_keeps_renderer_context_rows_independent() {
        let mut registry = RendererTaskRegistry::default();
        registry.renderer_mut(2, "Second tab").restart_count = 3;

        assert_eq!(registry.renderers.len(), 2);
        assert_eq!(registry.renderers[0].context_id, 1);
        assert_eq!(registry.renderers[0].restart_count, 0);
        assert_eq!(registry.renderers[1].context_id, 2);
        assert_eq!(registry.renderers[1].tab_title, "Second tab");
        assert_eq!(registry.renderers[1].restart_count, 3);
    }
}
