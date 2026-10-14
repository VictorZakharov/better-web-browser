//! Display cadence and visibility are embedding policy, not author timer delays.
use super::*;
use crate::engine::rendering::RefreshRate;

impl ScriptRuntime {
    pub fn set_rendering_environment(&mut self, rate: RefreshRate, visible: bool) {
        self.host
            .borrow_mut()
            .set_rendering_environment(rate, visible);
        self.set_child_rendering_environment(rate, visible);
    }
}
