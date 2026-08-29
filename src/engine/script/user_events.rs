//! Trusted native-event dispatch into one retained document realm.

use super::*;

pub(super) fn dispatch(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    event: UserInputEvent,
) -> UserInputResult {
    let payload = payload(host, event);
    let invocation = format!(
        "document.__dispatchNativeInput({});",
        serde_json::to_string(&payload).unwrap_or_else(|_| "null".into())
    );
    let mut outcome = ScriptOutcome::default();
    let default_allowed = match context.eval(Source::from_bytes(&invocation)) {
        Ok(value) => value.to_boolean(),
        Err(error) => {
            outcome
                .errors
                .push(format!("dispatch trusted user input: {error}"));
            false
        }
    };
    if let Err(error) = context.run_jobs() {
        outcome
            .errors
            .push(format!("dispatch trusted user input promise jobs: {error}"));
    }
    super::module_lifecycle::drain(context, host, &mut outcome);
    UserInputResult {
        outcome,
        default_allowed,
    }
}

fn payload(host: &Rc<RefCell<HostState>>, event: UserInputEvent) -> serde_json::Value {
    let target = |node: Option<NodeRef>| {
        node.map(|node| host.borrow_mut().id_for(&node))
            .unwrap_or(0)
    };
    match event {
        UserInputEvent::Pointer {
            target: node,
            phase,
            button,
            buttons,
            x,
            y,
            activate,
            modifiers,
        } => serde_json::json!({
            "kind": "pointer", "target": target(node), "phase": phase,
            "button": button, "buttons": buttons, "x": x, "y": y,
            "activate": activate, "alt": modifiers.alt,
            "control": modifiers.control, "shift": modifiers.shift, "meta": modifiers.meta
        }),
        UserInputEvent::Keyboard {
            target: node,
            phase,
            key,
            code,
            key_code,
            repeat,
            modifiers,
        } => serde_json::json!({
            "kind": "keyboard", "target": target(node), "phase": phase,
            "key": key, "code": code, "keyCode": key_code, "repeat": repeat,
            "alt": modifiers.alt, "control": modifiers.control,
            "shift": modifiers.shift, "meta": modifiers.meta
        }),
        UserInputEvent::Text {
            target: node,
            value,
            selection_start,
            selection_end,
        } => serde_json::json!({
            "kind": "text", "target": target(Some(node)), "value": value,
            "selectionStart": selection_start, "selectionEnd": selection_end
        }),
        UserInputEvent::Focus {
            target: node,
            focused,
        } => serde_json::json!({
            "kind": "focus", "target": target(node), "focused": focused
        }),
        UserInputEvent::Simple {
            target: node,
            event_type,
            bubbles,
            cancelable,
        } => serde_json::json!({
            "kind": "simple", "target": target(Some(node)), "type": event_type,
            "bubbles": bubbles, "cancelable": cancelable
        }),
        UserInputEvent::Scroll { x, y } => {
            serde_json::json!({ "kind": "scroll", "x": x, "y": y })
        }
        UserInputEvent::Viewport {
            width,
            height,
            scale,
        } => serde_json::json!({
            "kind": "viewport", "width": width, "height": height, "scale": scale
        }),
        UserInputEvent::Lifecycle { state, previous } => serde_json::json!({
            "kind": "lifecycle", "state": state, "previous": previous
        }),
        UserInputEvent::Fullscreen {
            request_id,
            disposition,
        } => serde_json::json!({
            "kind": "fullscreen", "requestId": request_id, "disposition": disposition
        }),
    }
}
