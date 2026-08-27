use super::support::*;
use better_web_browser::engine::{ControlKind, DisplayItem};
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, FocusInput, InputModifiers, PointerButton, PointerInput, PointerPhase,
    PresentationAcknowledgement, SemanticRole, TextInput,
};
use std::collections::HashSet;
use std::time::Duration;

#[test]
fn renderer_publishes_bounded_semantics_and_incremental_focus_and_value_updates() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        120,
        r#"<!doctype html><title>Accessible document</title>
        <main aria-label="Primary content">
          <h2 id="heading">Semantic heading</h2>
          <p>Introductory text <a href="/details" aria-label="Details link">details</a></p>
          <form aria-label="Search form">
            <input aria-label="Query" value="start">
            <button id="run" type="button">Run search</button>
            <button aria-disabled="true">Disabled action</button>
          </form>
          <ul><li>First item</li></ul>
          <table>
            <tr><th>Column</th><td>Value</td></tr>
          </table>
        </main>
        <script>
          document.querySelector('#run').addEventListener('click', () => {
            document.querySelector('#heading').textContent = 'Invoked heading';
          });
        </script>"#,
    );

    let update = &initial.accessibility;
    assert!(update.full);
    assert_eq!(update.root, update.focus);
    let ids = update
        .nodes
        .iter()
        .map(|node| node.id)
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), update.nodes.len());
    assert!(update.nodes.iter().all(|node| {
        node.children.iter().all(|child| ids.contains(child))
            && node.bounds.width >= 0.0
            && node.bounds.height >= 0.0
    }));

    let root = update
        .nodes
        .iter()
        .find(|node| node.role == SemanticRole::RootWebArea)
        .expect("root web area");
    assert_eq!(root.name, "Accessible document");
    assert_eq!(root.bounds.width, 800.0);
    assert_eq!(root.bounds.height, 600.0);

    let heading = node_with_role(update, SemanticRole::Heading);
    assert_eq!(heading.name, "Semantic heading");
    assert_eq!(heading.level, Some(2));
    let link = node_with_role(update, SemanticRole::Link);
    assert_eq!(link.name, "Details link");
    assert!(link.actions.focus && link.actions.invoke);
    assert!(
        update
            .nodes
            .iter()
            .any(|node| node.role == SemanticRole::Main)
    );
    assert!(
        update
            .nodes
            .iter()
            .any(|node| node.role == SemanticRole::Form)
    );
    assert!(
        update
            .nodes
            .iter()
            .any(|node| node.role == SemanticRole::List)
    );
    assert!(
        update
            .nodes
            .iter()
            .any(|node| node.role == SemanticRole::ListItem)
    );
    assert!(
        update
            .nodes
            .iter()
            .any(|node| node.role == SemanticRole::Table)
    );
    assert!(
        update
            .nodes
            .iter()
            .any(|node| node.role == SemanticRole::ColumnHeader)
    );
    assert!(
        update
            .nodes
            .iter()
            .any(|node| node.role == SemanticRole::Cell)
    );

    let button = node_with_role(update, SemanticRole::Button);
    assert!(button.actions.invoke);
    let disabled_button = update
        .nodes
        .iter()
        .find(|node| node.name == "Disabled action")
        .expect("ARIA-disabled button");
    assert!(disabled_button.disabled);
    assert!(!disabled_button.actions.focus && !disabled_button.actions.invoke);
    let button_id = button.id;
    let button_bounds = button.bounds;

    let input = node_with_role(update, SemanticRole::TextInput);
    assert_eq!(input.name, "Query");
    assert_eq!(input.value, "start");
    assert!(input.actions.focus && input.actions.set_value);
    let input_id = input.id;

    acknowledge(&session, &initial);
    session
        .send_input(DocumentInput::Focus(FocusInput {
            document: initial.document,
            sequence: 1,
            focused: true,
            target: Some(input_id),
        }))
        .unwrap();
    let focused = wait_for_presentation(&session, initial.document, initial.revision);
    assert!(!focused.accessibility.full);
    assert_eq!(focused.accessibility.focus, input_id);

    acknowledge(&session, &focused);
    session
        .send_input(DocumentInput::Text(TextInput {
            document: initial.document,
            sequence: 2,
            target: input_id,
            value: "updated".into(),
            selection_start: 7,
            selection_end: 7,
        }))
        .unwrap();
    let edited = wait_for_presentation(&session, initial.document, focused.revision);
    assert!(!edited.accessibility.full);
    let changed = edited
        .accessibility
        .nodes
        .iter()
        .find(|node| node.id == input_id)
        .expect("changed text input semantic node");
    assert_eq!(
        changed.value, "updated",
        "runtime={:?}; semantic={changed:?}; controls={:?}",
        edited.runtime, edited.layout.forms
    );
    assert_eq!(
        changed
            .selection
            .map(|selection| (selection.start, selection.end)),
        Some((7, 7))
    );

    acknowledge(&session, &edited);
    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document: initial.document,
            sequence: 3,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            x: button_bounds.x + button_bounds.width / 2.0,
            y: button_bounds.y + button_bounds.height / 2.0,
            modifiers: InputModifiers::default(),
            target: Some(button_id),
        }))
        .unwrap();
    let invoked = wait_for_presentation(&session, initial.document, edited.revision);
    assert!(
        invoked
            .accessibility
            .nodes
            .iter()
            .any(|node| { node.role == SemanticRole::Heading && node.name == "Invoked heading" })
    );

    acknowledge(&session, &invoked);
    session.cancel_document(initial.document).unwrap();
    let plain = load_html_document(
        &session,
        121,
        r#"<!doctype html><title>No script</title>
        <input aria-label="Plain query" value="before">"#,
    );
    let plain_input = node_with_role(&plain.accessibility, SemanticRole::TextInput).id;
    acknowledge(&session, &plain);
    session
        .send_input(DocumentInput::Text(TextInput {
            document: plain.document,
            sequence: 1,
            target: plain_input,
            value: "after".into(),
            selection_start: 5,
            selection_end: 5,
        }))
        .unwrap();
    let plain_edited = wait_for_presentation(&session, plain.document, plain.revision);
    assert!(
        plain_edited
            .accessibility
            .nodes
            .iter()
            .any(|node| { node.id == plain_input && node.value == "after" })
    );

    acknowledge(&session, &plain_edited);
    session.shutdown().expect("shutdown renderer");
}

#[test]
fn composed_tree_drives_accessibility_hit_testing_and_event_retargeting() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        122,
        r#"<!doctype html><title>Shadow interaction</title>
        <x-panel id="host"><h2 slot="heading">Slotted heading</h2></x-panel>
        <p id="output">waiting</p>
        <script>
          const host = document.querySelector('#host');
          const root = host.attachShadow({ mode: 'open' });
          root.innerHTML = '<section><slot name="heading"></slot><button type="button">Shadow action</button></section>';
          host.addEventListener('click', event => {
            document.querySelector('#output').textContent =
              event.target === host ? 'retargeted' : 'leaked';
          });
        </script>"#,
    );

    assert!(
        initial
            .accessibility
            .nodes
            .iter()
            .any(|node| { node.role == SemanticRole::Heading && node.name == "Slotted heading" })
    );
    assert!(
        initial
            .accessibility
            .nodes
            .iter()
            .any(|node| { node.role == SemanticRole::Button && node.name == "Shadow action" })
    );
    let button = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Button => {
                Some(control.rect)
            }
            _ => None,
        })
        .expect("shadow button layout target");

    acknowledge(&session, &initial);
    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document: initial.document,
            sequence: 1,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            x: button.x + button.width / 2.0,
            y: button.y + button.height / 2.0,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();

    let updated = wait_for_text(&session, initial.document, "retargeted");
    assert!(!presentation_text(&updated).contains("leaked"));
    session.shutdown().expect("shutdown renderer");
}

fn node_with_role(
    update: &better_web_browser::renderer_protocol::AccessibilityUpdate,
    role: SemanticRole,
) -> &better_web_browser::renderer_protocol::SemanticNode {
    update
        .nodes
        .iter()
        .find(|node| node.role == role)
        .unwrap_or_else(|| panic!("missing semantic role {role:?}"))
}

fn acknowledge(
    session: &RendererSession,
    presentation: &better_web_browser::renderer_protocol::RendererPresentation,
) {
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: presentation.document,
            revision: presentation.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
}

fn wait_for_presentation(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    after_revision: u64,
) -> better_web_browser::renderer_protocol::RendererPresentation {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation)
                if presentation.document == document && presentation.revision > after_revision =>
            {
                return *presentation;
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected accessibility renderer event: {event:?}"),
        }
    }
}

fn wait_for_text(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    expected: &str,
) -> better_web_browser::renderer_protocol::RendererPresentation {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if presentation_text(&presentation).contains(expected) {
                    return *presentation;
                }
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected shadow renderer event: {event:?}"),
        }
    }
}

fn presentation_text(
    presentation: &better_web_browser::renderer_protocol::RendererPresentation,
) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
