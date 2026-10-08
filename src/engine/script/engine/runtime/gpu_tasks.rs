//! Task completion is explicit: draining a nested microtask checkpoint is insufficient.
use super::*;

impl Context {
    fn gpu_bridges(&self) -> Vec<Rc<HostBridge>> {
        let mut bridges = self._frames.gpu_task_bridges();
        if let Some(bridge) = self.gpu_host.upgrade()
            && !bridges.iter().any(|other| Rc::ptr_eq(other, &bridge))
        {
            bridges.push(bridge);
        }
        bridges
    }

    pub(in crate::engine::script) fn gpu_resource_diagnostics(&mut self) -> Vec<String> {
        let mut reports = Vec::new();
        for bridge in self.gpu_bridges() {
            match &*bridge {
                HostBridge::Document(host) => {
                    if let Some(host) = host.upgrade() {
                        reports.extend(host.borrow_mut().webgl.resource_diagnostics());
                    }
                }
                HostBridge::Worker(host) => {
                    if let Some(host) = host.upgrade() {
                        reports.extend(host.borrow_mut().webgl.resource_diagnostics());
                    }
                }
            }
        }
        reports
    }

    pub(in crate::engine::script) fn complete_gpu_task(&mut self) -> JsResult<()> {
        // No isolate entry, page code or event dispatch at the native boundary.
        for bridge in self.gpu_bridges() {
            match &*bridge {
                HostBridge::Document(host) => {
                    if let Some(host) = host.upgrade() {
                        host.borrow_mut().webgl.complete_task();
                    }
                }
                HostBridge::Worker(host) => {
                    if let Some(host) = host.upgrade() {
                        host.borrow_mut().webgl.complete_task();
                    }
                }
            }
        }
        Ok(())
    }
}
