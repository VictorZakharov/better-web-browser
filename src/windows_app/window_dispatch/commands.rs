//! Browser chrome and projected page-control command routing.
use super::*;

pub(super) unsafe fn dispatch(state: &mut BrowserState, wparam: Wparam) -> Lresult {
    let id = wparam & 0xffff;
    let notification = (wparam >> 16) & 0xffff;
    match id {
        ID_BACK => state.go_back(),
        ID_FORWARD => state.go_forward(),
        ID_RELOAD => state.reload(),
        ID_GO => state.navigate_from_address(),
        ID_TASK_MANAGER => state.open_task_manager(),
        ID_READER => state.toggle_reader(),
        ID_OPTIONS => state.open_user_agent_options(),
        ID_PAGE_CONTROL_BASE.. => state.activate_page_control(id, notification),
        _ => {}
    }
    0
}
