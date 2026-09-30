//! Trusted native-event dispatch into one retained document realm.

use super::*;

pub(super) fn dispatch(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    event: UserInputEvent,
) -> UserInputResult {
    host.borrow_mut().begin_task();
    let user_initiated = matches!(
        &event,
        UserInputEvent::Pointer {
            phase: "down" | "up" | "activate",
            ..
        } | UserInputEvent::Keyboard { phase: "down", .. }
    );
    let starts_activation = matches!(
        &event,
        UserInputEvent::Pointer {
            phase: "down" | "activate",
            ..
        } | UserInputEvent::Keyboard { phase: "down", .. }
    );
    {
        let mut state = host.borrow_mut();
        state.user_input_active = user_initiated;
        // A pointer-up click shares the pointer-down activation, even though
        // native down/up are delivered as separate renderer input messages.
        if starts_activation {
            state.file_picker_activation_consumed = false;
        }
    }
    let notify_audio = {
        let mut state = host.borrow_mut();
        if state.audio_activated && !state.audio_activation_notified {
            state.audio_activation_notified = true;
            true
        } else {
            false
        }
    };
    let mut outcome = ScriptOutcome::default();
    if notify_audio && let Err(error) = context.call_global("__notifyAudioActivation", &[]) {
        outcome
            .errors
            .push(format!("Web Audio activation callback: {error}"));
    }
    let native_text = matches!(event, UserInputEvent::NativeText { .. });
    let payload = payload(host, event);
    let native_text_target = native_text
        .then(|| payload.get("target").and_then(serde_json::Value::as_u64))
        .flatten();
    let call = format!(
        "document.__dispatchNativeInput({})",
        serde_json::to_string(&payload).unwrap_or_else(|_| "null".into())
    );
    let invocation = if native_text {
        format!("JSON.stringify({call})")
    } else {
        call
    };
    let mut rejected_native_text = false;
    let default_allowed = match context.eval(Source::from_bytes(&invocation)) {
        Ok(value) if native_text => {
            let serialized = value
                .to_string(context)
                .map(|text| text.to_std_string_escaped());
            match serialized
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            {
                Some(serde_json::Value::Array(fields)) if fields.len() == 1 => {
                    match fields[0].as_bool() {
                        Some(allowed) => {
                            rejected_native_text = !allowed;
                            allowed
                        }
                        None => {
                            outcome
                                .errors
                                .push("invalid native text dispatch verdict".into());
                            false
                        }
                    }
                }
                _ => {
                    outcome
                        .errors
                        .push("invalid native text dispatch verdict".into());
                    false
                }
            }
        }
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
    // A canceled beforeinput may queue Promise jobs that write a different
    // value/selection. Snapshot only after that checkpoint so the generation-
    // scoped Win32 rollback uses the final renderer-owned DOM state.
    let rejected_text = if rejected_native_text {
        let snapshot = native_text_target
            .ok_or_else(|| "native text target is missing".to_string())
            .and_then(|target| native_text_rollback(context, target));
        match snapshot {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                outcome
                    .errors
                    .push(format!("native text rollback snapshot: {error}"));
                None
            }
        }
    } else {
        None
    };
    if user_initiated && host.borrow().navigation_url.is_some() {
        host.borrow_mut().navigation_options.user_initiated = true;
    }
    host.borrow_mut().user_input_active = false;
    UserInputResult {
        outcome,
        default_allowed,
        rejected_text,
    }
}

fn native_text_rollback(
    context: &mut Context,
    target: u64,
) -> Result<super::types::RejectedTextEdit, String> {
    let call = format!("JSON.stringify(document.__nativeTextSnapshot({target}))");
    let serialized = context
        .eval(Source::from_bytes(&call))
        .map_err(|error| error.to_string())?
        .to_string(context)
        .map_err(|error| error.to_string())?
        .to_std_string_escaped();
    let fields: serde_json::Value =
        serde_json::from_str(&serialized).map_err(|error| error.to_string())?;
    let fields = fields.as_array().ok_or("snapshot is not an array")?;
    if fields.len() != 3 {
        return Err("snapshot field count is invalid".into());
    }
    let value = fields[0].as_str().ok_or("snapshot value is invalid")?;
    let start = fields[1].as_u64().ok_or("snapshot start is invalid")?;
    let end = fields[2].as_u64().ok_or("snapshot end is invalid")?;
    let start = u32::try_from(start).map_err(|error| error.to_string())?;
    let end = u32::try_from(end).map_err(|error| error.to_string())?;
    if value.len() > crate::limits::MAX_RENDERER_TEXT_INPUT_BYTES
        || start > end
        || usize::try_from(end).unwrap_or(usize::MAX) > value.encode_utf16().count()
    {
        return Err("snapshot exceeds native text IPC bounds".into());
    }
    Ok(super::types::RejectedTextEdit {
        value: value.to_owned(),
        selection_start: start,
        selection_end: end,
    })
}

fn pointer_boundary(state: &mut HostState, target: Option<NodeRef>) -> serde_json::Value {
    let boundary = Node::update_hover_path(&mut state.pointer_path, target);
    for node in boundary.leaving.iter().chain(&boundary.entering) {
        if state.mutation_requires_render(node) {
            state.pending_invalidation.record(
                &state.document,
                Some(node),
                MutationKind::PointerDesignation,
            );
            state.pending_layout_invalidation.record(
                &state.document,
                Some(node),
                MutationKind::PointerDesignation,
            );
            state.timers.request_render();
        }
    }
    serde_json::json!({
        "previous": boundary.previous.as_ref().map(|node| state.id_for(node)).unwrap_or(0),
        "next": boundary.next.as_ref().map(|node| state.id_for(node)).unwrap_or(0),
        "leave": boundary.leaving.iter().map(|node| state.id_for(node)).collect::<Vec<_>>(),
        "enter": boundary.entering.iter().rev().map(|node| state.id_for(node)).collect::<Vec<_>>()
    })
}

fn payload(host: &Rc<RefCell<HostState>>, event: UserInputEvent) -> serde_json::Value {
    let target = |node: Option<NodeRef>| {
        node.map(|node| host.borrow_mut().id_for(&node))
            .unwrap_or(0)
    };
    match event {
        UserInputEvent::FragmentNavigation { url } => {
            serde_json::json!({ "kind": "fragmentNavigation", "url": url })
        }
        UserInputEvent::Wheel {
            target: node,
            x,
            y,
            delta_x,
            delta_y,
            modifiers,
        } => serde_json::json!({
            "kind": "wheel", "target": target(node), "x": x, "y": y, "deltaX": delta_x, "deltaY": delta_y,
            "alt": modifiers.alt, "control": modifiers.control, "shift": modifiers.shift, "meta": modifiers.meta
        }),
        UserInputEvent::ElementScroll { target: node } => {
            serde_json::json!({ "kind": "elementScroll", "target": target(Some(node)) })
        }
        UserInputEvent::Pointer {
            target: node,
            phase,
            button,
            buttons,
            x,
            y,
            activate,
            modifiers,
        } => {
            let boundary = pointer_boundary(&mut host.borrow_mut(), node.clone());
            serde_json::json!({
                "kind": "pointer", "target": target(node), "phase": phase,
                "boundary": boundary,
                "button": button, "buttons": buttons, "x": x, "y": y,
                "activate": activate, "alt": modifiers.alt,
                "control": modifiers.control, "shift": modifiers.shift, "meta": modifiers.meta
            })
        }
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
        UserInputEvent::NativeText {
            target: node,
            value,
            selection_start,
            selection_end,
            input_type,
            pre_selection,
        } => serde_json::json!({
            "kind": "nativeText", "target": target(Some(node)), "value": value,
            "selectionStart": selection_start, "selectionEnd": selection_end,
            "inputType": input_type,
            "beforeSelectionStart": pre_selection.map(|(start, _)| start),
            "beforeSelectionEnd": pre_selection.map(|(_, end)| end)
        }),
        UserInputEvent::Selection {
            target: node,
            selection_start,
            selection_end,
            direction,
        } => serde_json::json!({
            "kind": "selection", "target": target(Some(node)),
            "selectionStart": selection_start, "selectionEnd": selection_end,
            "direction": direction.as_str()
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
        UserInputEvent::ImageResource {
            target: node,
            event_type,
            natural_width,
            natural_height,
        } => serde_json::json!({
            "kind": "imageResource", "target": target(Some(node)), "type": event_type,
            "naturalWidth": natural_width, "naturalHeight": natural_height
        }),
        UserInputEvent::Scroll { x, y } => {
            host.borrow().document.scroll_offset.set((x, y));
            serde_json::json!({ "kind": "scroll", "x": x, "y": y })
        }
        UserInputEvent::Viewport {
            width,
            height,
            layout_width,
            layout_height,
            scale,
        } => serde_json::json!({
            "kind": "viewport", "width": width, "height": height,
            "layoutWidth": layout_width, "layoutHeight": layout_height, "scale": scale
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
        UserInputEvent::WakeLock {
            request_id,
            disposition,
        } => serde_json::json!({
            "kind": "wakeLock", "requestId": request_id, "disposition": disposition
        }),
        UserInputEvent::PointerLock {
            request_id,
            disposition,
        } => serde_json::json!({
            "kind": "pointerLock", "requestId": request_id, "disposition": disposition
        }),
        UserInputEvent::Media {
            target: node,
            request_id,
            disposition,
            current_time,
            duration,
            width,
            height,
            buffered,
        } => serde_json::json!({
            "kind": "media", "target": target(Some(node)), "requestId": request_id,
            "disposition": disposition, "currentTime": current_time, "duration": duration,
            "width": width, "height": height, "buffered": buffered
        }),
        UserInputEvent::MediaSource {
            target: node,
            disposition,
            source_url,
            reason,
        } => serde_json::json!({
            "kind": "mediaSource", "target": target(Some(node)),
            "disposition": disposition, "sourceUrl": source_url,
            "reason": reason
        }),
    }
}
