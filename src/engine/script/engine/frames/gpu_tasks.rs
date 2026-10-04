//! All live related document realms share the same HTML event-loop boundary.
use super::*;

impl FrameTree {
    pub(in crate::engine::script::engine) fn gpu_task_bridges(&self) -> Vec<Rc<HostBridge>> {
        // Rust weak slots follow the context-owned bridge lifetime, including
        // retained detached documents. Native publication must not enter V8 or
        // create local handles merely to enumerate these non-JavaScript owners.
        let mut hosts = self.gpu_hosts.borrow_mut();
        hosts.retain(|_, bridge| bridge.strong_count() != 0);
        hosts.values().filter_map(Weak::upgrade).collect()
    }
}
