//! Browser-chrome nodes in the root AccessKit tree.

use super::*;

pub(super) unsafe fn nodes(state: &BrowserState) -> Vec<(NodeId, Node)> {
    let mut client: Rect = std::mem::zeroed();
    GetClientRect(state.window, &mut client);
    let mut window = Node::new(Role::Window);
    let mut window_children = vec![TAB_LIST_ID, TOOLBAR_ID];
    if let Some(document) = visible_document_root(state) {
        window_children.push(document);
    }
    window_children.push(STATUS_ID);
    window.set_children(window_children);
    window.set_label(PRODUCT_NAME);
    window.set_bounds(rect(client));

    let tab_layout = state.tab_strip_layout(client.right);
    let mut tab_list = Node::new(Role::TabList);
    let mut tab_children = vec![SEARCH_TABS_ID];
    tab_children.extend(state.tabs.iter().map(|tab| tab_id(tab.id)));
    tab_children.push(NEW_TAB_ID);
    tab_list.set_children(tab_children);
    tab_list.set_label("Tabs");
    tab_list.set_bounds(AccessRect {
        x0: 0.0,
        y0: 0.0,
        x1: client.right as f64,
        y1: state.scale(TAB_STRIP_HEIGHT_DIP) as f64,
    });

    let mut nodes = vec![(WINDOW_ID, window), (TAB_LIST_ID, tab_list)];
    nodes.push(chrome_button(
        SEARCH_TABS_ID,
        Role::Button,
        "Search tabs",
        tab_layout.search_tabs,
        false,
    ));
    for region in &tab_layout.tabs {
        let tab = state.tabs.iter().find(|tab| tab.id == region.id).unwrap();
        let mut node = Node::new(Role::Tab);
        node.set_label(tab.title.clone());
        node.set_bounds(rect(region.bounds));
        node.set_selected(region.id == state.tabs.active_id());
        node.add_action(Action::Click);
        node.add_action(Action::Focus);
        nodes.push((tab_id(region.id), node));
    }
    nodes.push(chrome_button(
        NEW_TAB_ID,
        Role::Button,
        "New tab",
        tab_layout.new_tab,
        false,
    ));

    let mut toolbar = Node::new(Role::Toolbar);
    toolbar.set_label("Navigation");
    toolbar.set_children(vec![
        BACK_ID,
        FORWARD_ID,
        RELOAD_ID,
        ADDRESS_ID,
        GO_ID,
        READER_ID,
        TASK_MANAGER_ID,
    ]);
    toolbar.set_bounds(AccessRect {
        x0: 0.0,
        y0: state.scale(TAB_STRIP_HEIGHT_DIP) as f64,
        x1: client.right as f64,
        y1: state.toolbar_height() as f64,
    });
    nodes.push((TOOLBAR_ID, toolbar));
    nodes.push(chrome_button(
        BACK_ID,
        Role::Button,
        "Back",
        child_rect(state.window, state.controls.back),
        state.history_index == 0,
    ));
    nodes.push(chrome_button(
        FORWARD_ID,
        Role::Button,
        "Forward",
        child_rect(state.window, state.controls.forward),
        state.history_index + 1 >= state.history.len(),
    ));
    nodes.push(chrome_button(
        RELOAD_ID,
        Role::Button,
        "Reload",
        child_rect(state.window, state.controls.reload),
        false,
    ));
    let mut address = Node::new(Role::TextInput);
    address.set_label("Address");
    address.set_value(state.omnibox_text.clone());
    address.set_placeholder("Search or enter an address");
    address.set_bounds(rect(child_rect(state.window, state.controls.address)));
    address.add_action(Action::Focus);
    address.add_action(Action::SetValue);
    nodes.push((ADDRESS_ID, address));
    nodes.push(chrome_button(
        GO_ID,
        Role::Button,
        "Go",
        child_rect(state.window, state.controls.go),
        false,
    ));
    nodes.push(chrome_button(
        READER_ID,
        Role::Button,
        "Reader",
        child_rect(state.window, state.controls.reader),
        false,
    ));
    nodes.push(chrome_button(
        TASK_MANAGER_ID,
        Role::Button,
        "Task manager",
        child_rect(state.window, state.controls.task_manager),
        false,
    ));
    let mut status = Node::new(Role::Status);
    status.set_label("Page status");
    status.set_value(state.status_text.clone());
    status.set_bounds(rect(state.chrome.status));
    nodes.push((STATUS_ID, status));
    nodes
}

fn chrome_button(
    id: NodeId,
    role: Role,
    label: &str,
    bounds: Rect,
    disabled: bool,
) -> (NodeId, Node) {
    let mut node = Node::new(role);
    node.set_label(label);
    node.set_bounds(rect(bounds));
    if disabled {
        node.set_disabled();
    }
    if !disabled {
        node.add_action(Action::Click);
        node.add_action(Action::Focus);
    }
    (id, node)
}

unsafe fn child_rect(parent: Hwnd, child: Hwnd) -> Rect {
    let mut bounds: Rect = std::mem::zeroed();
    if child.is_null() || GetWindowRect(child, &mut bounds) == 0 {
        return bounds;
    }
    let mut top_left = Point {
        x: bounds.left,
        y: bounds.top,
    };
    let mut bottom_right = Point {
        x: bounds.right,
        y: bounds.bottom,
    };
    ScreenToClient(parent, &mut top_left);
    ScreenToClient(parent, &mut bottom_right);
    Rect {
        left: top_left.x,
        top: top_left.y,
        right: bottom_right.x,
        bottom: bottom_right.y,
    }
}

fn visible_document_root(state: &BrowserState) -> Option<NodeId> {
    (state.surface == Surface::Page)
        .then(|| state.accessibility_document.root())
        .flatten()
        .and_then(|root| state.accessibility_document.platform_id(root))
        .map(NodeId)
}

fn rect(value: Rect) -> AccessRect {
    AccessRect {
        x0: value.left as f64,
        y0: value.top as f64,
        x1: value.right as f64,
        y1: value.bottom as f64,
    }
}
