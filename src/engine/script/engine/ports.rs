//! Agent-local MessagePort endpoints, separate from transferable realm wrappers.
//! https://html.spec.whatwg.org/multipage/web-messaging.html#message-ports
use super::{frames, message_clone::Serialized, messaging::throw_named, node_wrappers};
use crate::engine::dom::NodeId;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap, VecDeque},
};
mod bindings;
pub(super) use bindings::install;

#[derive(Default)]
pub(super) struct Ports {
    endpoints: RefCell<BTreeMap<u32, Endpoint>>,
    bindings: RefCell<HashMap<NodeId, Bindings>>,
    next: Cell<u32>,
    bytes: Cell<usize>,
    last: Cell<u32>,
    retired: RefCell<Vec<u32>>,
}
#[derive(Clone)]
struct Bindings {
    create: v8::Global<v8::Function>,
    deliver: v8::Global<v8::Function>,
}
struct Endpoint {
    owner: Option<(NodeId, v8::Global<v8::Object>)>,
    peer: Option<u32>,
    enabled: bool,
    closed: bool,
    close_event: bool,
    queue: VecDeque<Serialized>,
}
impl Endpoint {
    fn ready(&self) -> bool {
        self.owner.is_some() && (self.close_event || self.enabled && !self.queue.is_empty())
    }
}
impl Ports {
    // Dropping a queued message can itself happen while an endpoint queue is borrowed.
    // Defer recursive endpoint disposal until the next native task/admission boundary.
    pub(super) fn retire(&self, ids: &[u32]) {
        self.retired.borrow_mut().extend_from_slice(ids);
    }
    fn collect_retired(&self) {
        loop {
            let ids = std::mem::take(&mut *self.retired.borrow_mut());
            if ids.is_empty() {
                break;
            }
            for id in ids {
                self.close(id);
                self.endpoints.borrow_mut().remove(&id);
            }
        }
    }
    pub(super) fn pending(&self) -> bool {
        self.collect_retired();
        self.endpoints.borrow().values().any(Endpoint::ready)
    }
    pub(super) fn valid(
        &self,
        scope: &mut v8::PinScope,
        object: v8::Local<v8::Object>,
    ) -> Option<u32> {
        let id = id(scope, object)?;
        self.endpoints
            .borrow()
            .get(&id)
            .and_then(|endpoint| endpoint.owner.as_ref())
            .filter(|(_, owner)| v8::Local::new(scope, owner) == object)
            .map(|_| id)
    }
    pub(super) fn detach(
        &self,
        scope: &mut v8::PinScope,
        object: v8::Local<v8::Object>,
    ) -> Option<()> {
        let id = self.valid(scope, object)?;
        set_id(scope, object, 0)?;
        let mut entries = self.endpoints.borrow_mut();
        let endpoint = entries.get_mut(&id)?;
        endpoint.owner = None;
        endpoint.enabled = false;
        Some(())
    }
    pub(super) fn receive<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        id: u32,
    ) -> Option<v8::Local<'s, v8::Object>> {
        let context = scope.get_current_context();
        let document = node_wrappers::host(context)?.borrow().document.id();
        let binding = self.bindings.borrow().get(&document)?.clone();
        let create = v8::Local::new(scope, binding.create);
        let receiver = v8::undefined(scope).into();
        let object = v8::Local::<v8::Object>::try_from(create.call(scope, receiver, &[])?).ok()?;
        set_id(scope, object, id)?;
        self.endpoints.borrow_mut().get_mut(&id)?.owner =
            Some((document, v8::Global::new(scope, object)));
        Some(object)
    }
    pub(super) fn close(&self, id: u32) {
        let mut entries = self.endpoints.borrow_mut();
        let Some(endpoint) = entries.get_mut(&id) else {
            return;
        };
        endpoint.closed = true;
        let peer = endpoint.peer.take();
        self.bytes.set(
            self.bytes
                .get()
                .saturating_sub(endpoint.queue.iter().map(Serialized::size).sum()),
        );
        endpoint.queue.clear();
        if let Some(peer) = peer.and_then(|id| entries.get_mut(&id)) {
            peer.peer = None;
            peer.close_event = true;
        }
    }
    pub(super) fn remove(&self, document: NodeId) {
        self.bindings.borrow_mut().remove(&document);
        let ids: Vec<_> = self
            .endpoints
            .borrow()
            .iter()
            .filter_map(|(id, e)| {
                e.owner
                    .as_ref()
                    .is_some_and(|(owner, _)| *owner == document)
                    .then_some(*id)
            })
            .collect();
        for id in ids {
            self.close(id);
            self.endpoints.borrow_mut().remove(&id);
        }
    }
    pub(super) fn deliver(&self, scope: &mut v8::PinScope) -> Option<()> {
        let (owner, binding, data, closed) = {
            let mut entries = self.endpoints.borrow_mut();
            let id = entries
                .iter()
                .filter(|(_, e)| e.ready())
                .map(|(id, _)| *id)
                .find(|id| *id > self.last.get())
                .or_else(|| entries.iter().find(|(_, e)| e.ready()).map(|(id, _)| *id))?;
            self.last.set(id);
            let endpoint = entries.get_mut(&id)?;
            let (document, owner) = endpoint.owner.as_ref()?.clone();
            let closed = std::mem::take(&mut endpoint.close_event);
            let data = if closed {
                None
            } else {
                endpoint.queue.pop_front()
            };
            (
                owner,
                self.bindings.borrow().get(&document)?.clone(),
                data,
                closed,
            )
        };
        if let Some(data) = &data {
            self.bytes.set(self.bytes.get().saturating_sub(data.size()));
        }
        let owner = v8::Local::new(scope, owner);
        let context = owner.get_creation_context(scope)?;
        if !frames::active(context) {
            return Some(());
        }
        let scope = &mut v8::ContextScope::new(scope, context);
        let (value, ports) = if let Some(data) = data {
            data.read(scope)?
        } else {
            (v8::undefined(scope).into(), v8::Array::new(scope, 0))
        };
        let closed = v8::Boolean::new(scope, closed);
        let deliver = v8::Local::new(scope, binding.deliver);
        let receiver = v8::undefined(scope).into();
        deliver.call(
            scope,
            receiver,
            &[owner.into(), value, ports.into(), closed.into()],
        )?;
        scope.perform_microtask_checkpoint();
        Some(())
    }
}

pub(super) fn id(scope: &mut v8::PinScope, object: v8::Local<v8::Object>) -> Option<u32> {
    let key = v8::String::new(scope, "Breeze.MessagePort")?;
    let key = v8::Private::for_api(scope, Some(key));
    let value = object.get_private(scope, key)?;
    value
        .is_uint32()
        .then(|| value.uint32_value(scope))
        .flatten()
}
fn set_id(scope: &mut v8::PinScope, object: v8::Local<v8::Object>, id: u32) -> Option<()> {
    let key = v8::String::new(scope, "Breeze.MessagePort")?;
    let key = v8::Private::for_api(scope, Some(key));
    let value = v8::Integer::new_from_unsigned(scope, id);
    object.set_private(scope, key, value.into())?;
    Some(())
}
