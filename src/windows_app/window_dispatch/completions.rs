//! Document and renderer completions follow their tab across window transfers.

use super::super::renderer_lifecycle::notifications::WM_APP_RENDERER_EVENTS;
use super::*;

pub(super) fn is_tab_completion(message: u32) -> bool {
    matches!(
        message,
        WM_APP_PAGE_LOADED
            | WM_APP_RENDERER_LAUNCHED
            | WM_APP_RENDERER_FETCH_COMPLETE
            | WM_APP_RENDERER_EVENTS
    )
}

pub(super) unsafe fn dispatch(
    state: &mut BrowserState,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    match message {
        WM_APP_PAGE_LOADED => {
            if let Some(id) = TabId::from_message(wparam) {
                if reroute_tab_message(state, id, message, wparam, lparam) {
                    return 0;
                }
                let message = Box::from_raw(lparam as *mut LoadMessage);
                if state.tabs.contains(id) {
                    state.route_navigation_message(id, *message);
                }
            } else {
                drop(Box::from_raw(lparam as *mut LoadMessage));
            }
            0
        }
        WM_APP_RENDERER_FETCH_COMPLETE => {
            if let Some(id) = TabId::from_message(wparam) {
                if reroute_tab_message(state, id, message, wparam, lparam) {
                    return 0;
                }
                let completion =
                    Box::from_raw(lparam as *mut renderer_fetch::RendererFetchCompletion);
                if state.tabs.contains(id) {
                    state.route_renderer_fetch_completion(id, *completion);
                }
            } else {
                drop(Box::from_raw(
                    lparam as *mut renderer_fetch::RendererFetchCompletion,
                ));
            }
            0
        }
        WM_APP_RENDERER_LAUNCHED => {
            if let Some(id) = TabId::from_message(wparam) {
                if reroute_tab_message(state, id, message, wparam, lparam) {
                    return 0;
                }
                if state.tabs.contains(id) {
                    state.finish_renderer_launch(id);
                }
            }
            0
        }
        WM_APP_RENDERER_EVENTS => {
            if let Some(id) = TabId::from_message(wparam)
                && !reroute_tab_message(state, id, message, wparam, lparam)
            {
                state.renderer_events_ready(id, lparam as u64);
            }
            0
        }
        _ => unreachable!("only tab completions are dispatched here"),
    }
}
