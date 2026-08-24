//! AccessKit projection of the active validated document and tree-update coordination.

mod chrome;

use super::document::AccessibilityDocument;
use crate::windows_app::tab_state::TabFocus;
use crate::windows_app::*;
use accesskit::{Action, Node, NodeId, Rect as AccessRect, Role, Tree, TreeId, TreeUpdate};
use better_web_browser::renderer_protocol::{DocumentNodeId, SemanticNode, SemanticRole};

pub(super) const WINDOW_ID: NodeId = NodeId(1);
pub(super) const TAB_LIST_ID: NodeId = NodeId(2);
pub(super) const TOOLBAR_ID: NodeId = NodeId(3);
pub(super) const STATUS_ID: NodeId = NodeId(4);
pub(super) const BACK_ID: NodeId = NodeId(10);
pub(super) const FORWARD_ID: NodeId = NodeId(11);
pub(super) const RELOAD_ID: NodeId = NodeId(12);
pub(super) const ADDRESS_ID: NodeId = NodeId(13);
pub(super) const GO_ID: NodeId = NodeId(14);
pub(super) const READER_ID: NodeId = NodeId(15);
pub(super) const TASK_MANAGER_ID: NodeId = NodeId(16);
pub(super) const SEARCH_TABS_ID: NodeId = NodeId(17);
pub(super) const NEW_TAB_ID: NodeId = NodeId(18);
const TAB_ID_BASE: u64 = 1 << 20;

pub(super) unsafe fn full_update(state: &BrowserState, initialize: bool) -> TreeUpdate {
    let mut nodes = chrome::nodes(state);
    nodes.extend(document_nodes(state, None));
    TreeUpdate {
        nodes,
        tree: initialize.then(|| {
            let mut tree = Tree::new(WINDOW_ID);
            tree.toolkit_name = Some("Breeze".into());
            tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
            tree
        }),
        tree_id: TreeId::ROOT,
        focus: focus_id(state),
    }
}

pub(super) unsafe fn chrome_update(state: &BrowserState) -> TreeUpdate {
    TreeUpdate {
        nodes: chrome::nodes(state),
        tree: None,
        tree_id: TreeId::ROOT,
        focus: focus_id(state),
    }
}

pub(super) unsafe fn document_bounds_update(state: &BrowserState) -> TreeUpdate {
    TreeUpdate {
        nodes: document_nodes(state, None),
        tree: None,
        tree_id: TreeId::ROOT,
        focus: focus_id(state),
    }
}

pub(super) unsafe fn document_update(
    state: &BrowserState,
    changed: &[DocumentNodeId],
    full: bool,
) -> TreeUpdate {
    let mut nodes = if full {
        chrome::nodes(state)
    } else {
        Vec::new()
    };
    nodes.extend(document_nodes(state, (!full).then_some(changed)));
    TreeUpdate {
        nodes,
        tree: None,
        tree_id: TreeId::ROOT,
        focus: focus_id(state),
    }
}

unsafe fn document_nodes(
    state: &BrowserState,
    changed: Option<&[DocumentNodeId]>,
) -> Vec<(NodeId, Node)> {
    if state.surface != Surface::Page {
        return Vec::new();
    }
    let document = &state.accessibility_document;
    let mut nodes = match changed {
        Some(changed) => changed
            .iter()
            .filter_map(|id| document.node(*id))
            .map(|node| document_node(state, document, node))
            .collect::<Vec<_>>(),
        None => document
            .nodes()
            .map(|node| document_node(state, document, node))
            .collect::<Vec<_>>(),
    };
    nodes.sort_by_key(|(id, _)| id.0);
    nodes
}

unsafe fn document_node(
    state: &BrowserState,
    document: &AccessibilityDocument,
    semantic: &SemanticNode,
) -> (NodeId, Node) {
    let id = NodeId(document.platform_id(semantic.id).unwrap());
    let mut node = Node::new(accesskit_role(semantic.role));
    let children = semantic
        .children
        .iter()
        .filter_map(|child| document.platform_id(*child))
        .map(NodeId)
        .collect::<Vec<_>>();
    node.set_children(children);
    if semantic.role == SemanticRole::TextRun {
        node.set_value(semantic.value.clone());
        node.set_character_lengths(
            semantic
                .value
                .chars()
                .map(|character| character.len_utf8() as u8)
                .collect::<Vec<_>>(),
        );
    } else if !semantic.name.is_empty() {
        node.set_label(semantic.name.clone());
    }
    if semantic.role != SemanticRole::TextRun && !semantic.value.is_empty() {
        node.set_value(semantic.value.clone());
    }
    if !semantic.description.is_empty() {
        node.set_description(semantic.description.clone());
    }
    if let Some(level) = semantic.level {
        node.set_level(level as usize);
    }
    if semantic.disabled {
        node.set_disabled();
    }
    if semantic.read_only {
        node.set_read_only();
    }
    if semantic.actions.focus {
        node.add_action(Action::Focus);
    }
    if semantic.actions.invoke {
        node.add_action(Action::Click);
    }
    if semantic.actions.set_value {
        node.add_action(Action::SetValue);
    }
    node.set_bounds(document_bounds(state, semantic));
    (id, node)
}

unsafe fn focus_id(state: &BrowserState) -> NodeId {
    let focused = GetFocus();
    for (window, id) in [
        (state.controls.back, BACK_ID),
        (state.controls.forward, FORWARD_ID),
        (state.controls.reload, RELOAD_ID),
        (state.controls.address, ADDRESS_ID),
        (state.controls.go, GO_ID),
        (state.controls.reader, READER_ID),
        (state.controls.task_manager, TASK_MANAGER_ID),
    ] {
        if focused == window {
            return id;
        }
    }
    if let Some(control) = state
        .page_controls
        .iter()
        .find(|control| control.window == focused)
        && let Ok(id) = better_web_browser::renderer_protocol::DocumentNodeId::new(
            control.spec.node_id.to_wire(),
        )
        && let Some(platform) = state.accessibility_document.platform_id(id)
    {
        return NodeId(platform);
    }
    match state.focus {
        TabFocus::Address => ADDRESS_ID,
        TabFocus::PageControl(node) => {
            better_web_browser::renderer_protocol::DocumentNodeId::new(node.to_wire())
                .ok()
                .and_then(|id| state.accessibility_document.platform_id(id))
                .map(NodeId)
                .unwrap_or(WINDOW_ID)
        }
        TabFocus::Content => state
            .accessibility_document
            .focus()
            .and_then(|id| state.accessibility_document.platform_id(id))
            .map(NodeId)
            .unwrap_or(WINDOW_ID),
    }
}

unsafe fn document_bounds(state: &BrowserState, semantic: &SemanticNode) -> AccessRect {
    if semantic.role == SemanticRole::RootWebArea {
        return AccessRect {
            x0: 0.0,
            y0: state.toolbar_height() as f64,
            x1: state.chrome.status.right as f64,
            y1: (state.toolbar_height() + state.viewport_height()) as f64,
        };
    }
    let scale = f64::from(state.page_scale());
    let x0 = f64::from(semantic.bounds.x) * scale;
    let y0 =
        f64::from(semantic.bounds.y) * scale + f64::from(state.toolbar_height() - state.scroll_y);
    AccessRect {
        x0,
        y0,
        x1: x0 + f64::from(semantic.bounds.width) * scale,
        y1: y0 + f64::from(semantic.bounds.height) * scale,
    }
}

fn accesskit_role(role: SemanticRole) -> Role {
    match role {
        SemanticRole::RootWebArea => Role::RootWebArea,
        SemanticRole::TextRun => Role::TextRun,
        SemanticRole::Paragraph => Role::Paragraph,
        SemanticRole::Heading => Role::Heading,
        SemanticRole::Link => Role::Link,
        SemanticRole::Button => Role::Button,
        SemanticRole::TextInput => Role::TextInput,
        SemanticRole::MultilineTextInput => Role::MultilineTextInput,
        SemanticRole::PasswordInput => Role::PasswordInput,
        SemanticRole::SearchInput => Role::SearchInput,
        SemanticRole::ComboBox => Role::ComboBox,
        SemanticRole::List => Role::List,
        SemanticRole::ListItem => Role::ListItem,
        SemanticRole::Table => Role::Table,
        SemanticRole::RowGroup => Role::RowGroup,
        SemanticRole::Row => Role::Row,
        SemanticRole::Cell => Role::Cell,
        SemanticRole::RowHeader => Role::RowHeader,
        SemanticRole::ColumnHeader => Role::ColumnHeader,
        SemanticRole::Image => Role::Image,
        SemanticRole::Form => Role::Form,
        SemanticRole::Main => Role::Main,
        SemanticRole::Navigation => Role::Navigation,
        SemanticRole::Header => Role::Header,
        SemanticRole::Footer => Role::Footer,
        SemanticRole::Article => Role::Article,
        SemanticRole::Section => Role::Section,
    }
}

fn tab_id(id: tabs::TabId) -> NodeId {
    NodeId(TAB_ID_BASE + id.get())
}

pub(super) fn tab_from_node(id: NodeId) -> Option<tabs::TabId> {
    id.0.checked_sub(TAB_ID_BASE)
        .and_then(|id| usize::try_from(id).ok())
        .and_then(tabs::TabId::from_message)
}
