//! HTML posted-message task source, shared by related browsing contexts.
use super::{frames, message_clone::Serialized, node_wrappers, v8_api};
use crate::{engine::dom::NodeId, fetch::Origin};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
};

#[derive(Default)]
pub(super) struct Messages {
    tasks: RefCell<VecDeque<Message>>,
    bindings: RefCell<HashMap<NodeId, Bindings>>,
    bytes: Cell<usize>,
}

#[derive(Clone)]
struct Bindings {
    prepare: v8::Global<v8::Function>,
    deliver: v8::Global<v8::Function>,
    exception: v8::Global<v8::Function>,
}
struct Message {
    document: NodeId,
    target: v8::Global<v8::Object>,
    source: v8::Global<v8::Object>,
    origin: Origin,
    expected: Option<Origin>,
    data: Serialized,
}

impl Messages {
    pub(super) fn pending(&self) -> bool {
        !self.tasks.borrow().is_empty()
    }
    pub(super) fn remove(&self, document: NodeId) {
        self.bindings.borrow_mut().remove(&document);
        self.tasks.borrow_mut().retain(|message| {
            if message.document != document {
                return true;
            }
            self.bytes
                .set(self.bytes.get().saturating_sub(message.data.size()));
            false
        });
    }

    pub(super) fn deliver(&self, scope: &mut v8::PinScope) -> Option<()> {
        let Some(message) = self.tasks.borrow_mut().pop_front() else {
            return Some(());
        };
        self.bytes
            .set(self.bytes.get().saturating_sub(message.data.size()));
        let target = v8::Local::new(scope, &message.target);
        let context = target.get_creation_context(scope)?;
        if !frames::active(context) {
            return Some(());
        }
        let host = node_wrappers::host(context)?;
        let state = host.borrow();
        if message
            .expected
            .as_ref()
            .is_some_and(|expected| !expected.is_same_origin(&state.document_origin))
        {
            return Some(());
        }
        let bindings = self.bindings.borrow().get(&state.document.id()).cloned()?;
        drop(state);
        let scope = &mut v8::ContextScope::new(scope, context);
        let (data, ports) = message.data.read(scope)?;
        let source = v8::Local::new(scope, &message.source);
        let origin = v8::String::new(scope, &message.origin.serialize())?;
        let deliver = v8::Local::new(scope, bindings.deliver);
        deliver.call(
            scope,
            target.into(),
            &[data, origin.into(), source.into(), ports.into()],
        )?;
        scope.perform_microtask_checkpoint();
        Some(())
    }
}

pub(super) fn install(scope: &mut v8::PinScope) -> Option<()> {
    super::message_clone::install(scope)?;
    super::ports::install(scope)?;
    let context = scope.get_current_context();
    let tree = frames::tree(context)?;
    let host = node_wrappers::host(context)?;
    let global = context.global(scope);
    let key = v8::String::new(scope, "__windowMessageBindings")?;
    let array = global.get(scope, key.into())?;
    let array = v8::Local::<v8::Array>::try_from(array).ok()?;
    let mut functions = Vec::new();
    for index in 0..3 {
        let value = array.get_index(scope, index)?;
        let function = v8::Local::<v8::Function>::try_from(value).ok()?;
        functions.push(v8::Global::new(scope, function));
    }
    if global.delete(scope, key.into()) != Some(true) {
        return None;
    }
    tree.messages.bindings.borrow_mut().insert(
        host.borrow().document.id(),
        Bindings {
            prepare: functions[0].clone(),
            deliver: functions[1].clone(),
            exception: functions[2].clone(),
        },
    );
    let function = v8::Function::new(scope, post_message)?;
    let name = v8::String::new(scope, "postMessage")?;
    global.set(scope, name.into(), function.into())?;
    Some(())
}

pub(super) fn throw_named(scope: &mut v8::PinScope, name: &str, text: &str) {
    let context = scope.get_current_context();
    let binding = frames::tree(context).and_then(|tree| {
        let id = node_wrappers::host(context)?.borrow().document.id();
        tree.messages.bindings.borrow().get(&id).cloned()
    });
    let Some(text) = v8::String::new(scope, text) else {
        return;
    };
    if let Some(binding) = binding {
        let function = v8::Local::new(scope, binding.exception);
        let Some(name) = v8::String::new(scope, name) else {
            return;
        };
        let receiver = v8::undefined(scope).into();
        if let Some(error) = function.call(scope, receiver, &[text.into(), name.into()]) {
            scope.throw_exception(error);
        }
    } else {
        let error = v8::Exception::error(scope, text);
        scope.throw_exception(error);
    }
}

fn post_message(scope: &mut v8::PinScope, args: v8::FunctionCallbackArguments, _: v8::ReturnValue) {
    let target = args.this();
    post_message_to(scope, args, target);
}

pub(super) fn post_message_to(
    scope: &mut v8::PinScope,
    args: v8::FunctionCallbackArguments,
    target: v8::Local<v8::Object>,
) {
    if args.length() == 0 {
        let text = v8::String::new(scope, "postMessage requires a message").unwrap();
        let error = v8::Exception::type_error(scope, text);
        scope.throw_exception(error);
        return;
    }
    let Some(source) = v8_api::incumbent(scope) else {
        return;
    };
    let Some(receiver) = target.get_creation_context(scope) else {
        return;
    };
    if target != receiver.global(scope) {
        return;
    }
    let Some(tree) = frames::tree(receiver) else {
        return;
    };
    let Some(source_host) = node_wrappers::host(source) else {
        return;
    };
    let Some(target_host) = node_wrappers::host(receiver) else {
        return;
    };
    let origin = source_host.borrow().document_origin.clone();
    let bindings = tree
        .messages
        .bindings
        .borrow()
        .get(&target_host.borrow().document.id())
        .cloned();
    let Some(bindings) = bindings else { return };
    let prepare = v8::Local::new(scope, bindings.prepare);
    let Some(prepared) = prepare.call(scope, target.into(), &[args.get(1), args.get(2)]) else {
        return;
    };
    let Ok(prepared) = v8::Local::<v8::Array>::try_from(prepared) else {
        return;
    };
    let Some(expected) = prepared.get_index(scope, 0) else {
        return;
    };
    let expected = expected.to_rust_string_lossy(scope);
    let expected = match expected.as_str() {
        "*" => None,
        "/" => Some(origin.clone()),
        value => match url::Url::parse(value) {
            Ok(url) => Some(Origin::parse(url.as_str()).unwrap_or_else(|_| Origin::opaque())),
            Err(_) => {
                throw_named(scope, "SyntaxError", "Invalid postMessage target origin");
                return;
            }
        },
    };
    let Some(transfers) = prepared.get_index(scope, 1) else {
        return;
    };
    let Ok(transfers) = v8::Local::<v8::Array>::try_from(transfers) else {
        return;
    };
    if tree.messages.tasks.borrow().len() >= 1024 {
        throw_named(
            scope,
            "QuotaExceededError",
            "The posted-message task queue is full",
        );
        return;
    }
    let value = v8::Local::new(scope, args.get(0));
    let Some(mut data) = Serialized::write(scope, value, transfers) else {
        return;
    };
    // Serialization can invoke getters which enqueue more messages. Recheck the shared
    // quota after serialization and before detaching any transferable buffers.
    let bytes = tree.messages.bytes.get().saturating_add(data.size());
    if bytes > 32 * 1024 * 1024 || tree.messages.tasks.borrow().len() >= 1024 {
        throw_named(
            scope,
            "QuotaExceededError",
            "The posted-message queue exceeds its limit",
        );
        return;
    }
    if !data.commit_transfers(scope) {
        return;
    }
    tree.messages.bytes.set(bytes);
    let source_global = source.global(scope);
    tree.messages.tasks.borrow_mut().push_back(Message {
        document: target_host.borrow().document.id(),
        target: v8::Global::new(scope, target),
        source: v8::Global::new(scope, source_global),
        origin,
        expected,
        data,
    });
}
