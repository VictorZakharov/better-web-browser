//! Nonblocking ownership of one sandboxed renderer lifecycle per browser tab.

use super::tabs::{IdentifiedTab, TabId};
use super::*;
use better_web_browser::renderer_process::{
    RendererEvent, RendererExit, RendererLaunchOptions, RendererSession, RendererSnapshot,
    RendererState,
};
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
        let needs_monitor = self.tabs.iter().any(|tab| {
            tab.renderer_session.is_some()
                || tab.renderer_launch_pending
                || tab.renderer_launch_receiver.is_some()
        });
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

    pub(super) unsafe fn start_renderer_for(&mut self, id: TabId) {
        let title = {
            let Some(tab) = self.tabs.get_mut(id) else {
                return;
            };
            if tab.renderer_launch_pending || tab.renderer_session.is_some() {
                return;
            }
            tab.renderer_launch_pending = true;
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
                let result =
                    RendererLaunchOptions::current_executable().and_then(RendererSession::launch);
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
        if let Some(tab) = self.tabs.get_mut(id) {
            tab.renderer_launch_pending = false;
        }
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
        }
        let has_live_or_pending = self.tabs.iter().any(|tab| {
            tab.renderer_session.is_some()
                || tab.renderer_launch_pending
                || tab.renderer_launch_receiver.is_some()
        });
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

    fn renderer_monitor_interval(&self) -> u32 {
        if self.benchmark.is_some()
            || self.tabs.iter().any(|tab| {
                tab.loading || tab.renderer_work_pending || tab.renderer_next_timer.is_some()
            })
        {
            ACTIVE_RENDERER_MONITOR_INTERVAL_MS
        } else {
            RENDERER_MONITOR_INTERVAL_MS
        }
    }

    unsafe fn poll_renderer(&mut self, id: TabId) {
        let snapshot_and_events = self.tabs.get_mut(id).and_then(|tab| {
            tab.renderer_session.as_ref().map(|session| {
                let snapshot = session.snapshot();
                let mut events = Vec::new();
                while let Ok(Some(event)) = session.try_event() {
                    events.push(event);
                }
                (tab.title.clone(), snapshot, events)
            })
        });
        let Some((title, snapshot, events)) = snapshot_and_events else {
            return;
        };
        self.update_renderer_status(id, &title, |status| {
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
                    self.update_renderer_status(id, &title, |status| {
                        status.last_diagnostic = Some(format!("{code}: {text}"));
                    });
                }
                RendererEvent::Unresponsive => {
                    self.update_renderer_status(id, &title, |status| {
                        status.phase = RendererLifecyclePhase::Unresponsive;
                    });
                }
                RendererEvent::FetchBatch(requests) => {
                    self.begin_renderer_fetch_batch(id, requests);
                }
                RendererEvent::Presentation(presentation) => {
                    self.process_for_tab(id, |state| {
                        state.activate_renderer_presentation(*presentation)
                    });
                }
                RendererEvent::TimeAdvanced {
                    document,
                    next_timer_micros,
                } => {
                    self.process_for_tab(id, |state| {
                        state.complete_renderer_time_advance(document, next_timer_micros)
                    });
                }
                RendererEvent::DocumentFailed { document, detail } => {
                    let current = self
                        .tabs
                        .get_mut(id)
                        .is_some_and(|tab| tab.renderer_document == Some(document));
                    if current {
                        self.contain_page_engine_failure(id, detail);
                    }
                }
                RendererEvent::NavigationRequested { document, url } => {
                    self.process_for_tab(id, |state| {
                        if state.renderer_document == Some(document)
                            && state.allow_script_navigation(&url)
                        {
                            state.begin_navigation(url, browser_navigation::HistoryMode::Script);
                        }
                    });
                }
                RendererEvent::Exited(renderer_exit) => exit = Some(renderer_exit),
            }
        }

        if let Some(exit) = exit {
            let crash_surface = exit.crash_surface();
            self.update_renderer_status(id, &title, |status| {
                status.phase = RendererLifecyclePhase::Exited;
                status.last_exit = Some(exit);
            });
            let status = crash_surface.map(|surface| {
                format!(
                    "{}: {}. Reload to restart the renderer.",
                    surface.title, surface.detail
                )
            });
            if let Some(tab) = self.tabs.get_mut(id) {
                if let Some(status) = status.as_ref() {
                    tab.mark_crashed(status.clone());
                } else {
                    tab.renderer_session.take();
                }
            }
            if self.tabs.active_id() == id
                && let Some(status) = status
            {
                self.set_status(&status);
            }
        }
    }

    unsafe fn begin_renderer_fetch_batch(
        &mut self,
        id: TabId,
        requests: Vec<better_web_browser::renderer_protocol::RendererFetchRequest>,
    ) {
        let Some(document) = requests.first().map(|request| request.head.document) else {
            self.contain_page_engine_failure(id, "renderer sent an empty Fetch batch".into());
            return;
        };
        let context = self.tabs.get_mut(id).and_then(|tab| {
            (tab.renderer_document == Some(document))
                .then(|| (tab.reader_url.clone(), tab.document_fetch.signal()))
        });
        let Some((document_url, signal)) = context else {
            return;
        };
        let result = renderer_fetch::spawn_fetch_batch(
            id,
            document,
            document_url,
            requests,
            Arc::clone(&self.http_client),
            signal,
            self.app.tab_router.clone(),
        );
        if let Err(error) = result {
            self.contain_page_engine_failure(id, error);
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
