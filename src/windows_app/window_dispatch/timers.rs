//! Native timer routing; GPU retries never advance the document clock.
use super::*;

pub(super) unsafe fn dispatch(state: &mut BrowserState, id: usize) -> bool {
    match id {
        ID_RENDERER_RUNTIME_TIMER => state.service_due_script_runtime(),
        ID_RENDERER_MONITOR_TIMER => {
            state.poll_renderers();
            state.tick_geolocation();
            state.tick_media_devices();
            state.tick_capture();
        }
        ID_PERFORMANCE_MONITOR_TIMER => state.refresh_performance_monitor(),
        ID_SCROLL_ANIMATION_TIMER => state.tick_scroll_animation(),
        gpu_composition::RETRY_TIMER => state.gpu_retry_timer(),
        _ => return false,
    }
    true
}
