//! Raw values enter the trusted serializer before the generic string/JSON bridge.
use super::*;
use crate::engine::script::workers::ScriptWorkerAction;

pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    operation: &str,
    arguments: v8::FunctionCallbackArguments,
    mut output: v8::ReturnValue,
) {
    let Some(bridge) = scope.get_current_context().get_slot::<HostBridge>() else {
        throw_error(scope, inactive_host());
        return;
    };
    match (&*bridge, operation) {
        (HostBridge::Document(host), "workerPostMessageValue") => {
            let Some(host) = host.upgrade() else {
                throw_error(scope, inactive_host());
                return;
            };
            let id = arguments.get(1).uint32_value(scope).unwrap_or(0);
            if !owns(&host, id) {
                output.set(v8::undefined(scope).into());
                return;
            }
            // Serialization invokes getters. No HostState/identifier borrow may span it.
            let Some(message) =
                super::super::worker_packets::serialize(scope, arguments.get(2), arguments.get(3))
            else {
                return;
            };
            if owns(&host, id) {
                host.borrow_mut()
                    .pending_worker_actions
                    .push(ScriptWorkerAction::PostMessage {
                        id,
                        serialized: message,
                    });
            }
            output.set(v8::undefined(scope).into());
        }
        (HostBridge::Worker(host), "workerPostValue") => {
            let Some(host) = host.upgrade() else {
                throw_error(scope, inactive_host());
                return;
            };
            if host.borrow().closed {
                output.set(v8::undefined(scope).into());
                return;
            }
            let Some(message) =
                super::super::worker_packets::serialize(scope, arguments.get(1), arguments.get(2))
            else {
                return;
            };
            if !host.borrow().closed {
                host.borrow_mut().messages.push(message);
            }
            output.set(v8::undefined(scope).into());
        }
        _ => throw_error(
            scope,
            JsNativeError::typ()
                .with_message("Worker posting operation is unavailable in this realm")
                .into(),
        ),
    }
}
fn owns(host: &std::rc::Rc<RefCell<HostState>>, id: u32) -> bool {
    let host = host.borrow();
    host.worker_identifiers.borrow().owner(id) == Some(host.document.id())
}
