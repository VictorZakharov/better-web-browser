//! Realm registry, leases and ordered submission; native objects remain on the owner.
use super::*;
use crate::engine::webgl::{ApiVersion, Options, command_batch, sync_reply_cache};

#[derive(Default)]
pub(crate) struct Contexts {
    live: HashSet<u32>,
    leases: HashMap<u32, Arc<()>>,
    asynchronous: HashSet<u32>,
    sync_replies: sync_reply_cache::Cache,
    pending: command_batch::Pending,
}
impl Contexts {
    pub(crate) fn resource_diagnostics(&mut self) -> Vec<String> {
        if self.live.is_empty() {
            return Vec::new();
        }
        self.flush_pending();
        let mut ids: Vec<_> = self.live.iter().copied().collect();
        ids.sort_unstable();
        match request(Operation::ResourceDiagnostics(ids)) {
            Some(Reply::ResourceDiagnostics(report)) => report,
            _ => vec!["WebGL resource diagnostics unavailable: native owner did not reply".into()],
        }
    }

    pub(crate) fn create(&mut self, width: u32, height: u32, options: &str) -> Option<u32> {
        self.flush_pending();
        if self.live.len() >= MAX_CONTEXTS || options.len() > 1024 {
            return None;
        }
        let api = serde_json::from_str::<Options>(options).ok()?.api;
        let lease = Arc::new(());
        let Reply::Created(Some(id)) = request(Operation::Create(
            width,
            height,
            options.into(),
            Arc::downgrade(&lease),
        ))?
        else {
            return None;
        };
        self.live.insert(id);
        self.leases.insert(id, lease);
        if api == ApiVersion::Two {
            self.asynchronous.insert(id);
        }
        Some(id)
    }
    pub(crate) fn remove(&mut self, id: u32) {
        self.pending.remove(id);
        self.sync_replies.remove(id);
        if self.live.remove(&id) {
            self.leases.remove(&id);
            self.asynchronous.remove(&id);
            retire(std::iter::once(id));
        }
    }
    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.sync_replies.clear();
        if self.live.is_empty() {
            return;
        }
        retire(self.live.drain());
        self.leases.clear();
        self.asynchronous.clear();
    }
    /// Called by the embedder only after an HTML task and its microtask checkpoint.
    /// Ordinary commands, presentation and nested checkpoints never publish results.
    pub(crate) fn complete_task(&mut self) {
        self.flush_pending();
        self.sync_replies.clear();
        if self.asynchronous.is_empty() {
            return;
        }
        let ids: Vec<_> = self.asynchronous.iter().copied().collect();
        let lost = match request(Operation::TaskBoundary(ids.clone())) {
            Some(Reply::TaskBoundary(lost)) => lost,
            // A stopped/timed-out owner cannot leave usable cached GPU handles.
            _ => ids,
        };
        for id in lost {
            self.remove(id);
        }
    }
    pub(crate) fn execute(&mut self, id: u32, command: &str, bytes: Option<&[u8]>) -> Value {
        self.execute_data(id, command, bytes.map(std::borrow::Cow::Borrowed))
    }
    pub(crate) fn execute_owned(
        &mut self,
        id: u32,
        command: &str,
        bytes: Option<Vec<u8>>,
    ) -> Value {
        self.execute_data(id, command, bytes.map(std::borrow::Cow::Owned))
    }
    fn execute_data(
        &mut self,
        id: u32,
        command: &str,
        bytes: Option<std::borrow::Cow<'_, [u8]>>,
    ) -> Value {
        if !self.live.contains(&id) {
            return json!({"lost":true});
        }
        if bytes.is_none() && command_batch::Pending::candidate(command) {
            self.sync_replies.remove(id);
            self.pending.push(id, command);
            if self.pending.full() {
                self.flush_pending();
            }
            return if self.live.contains(&id) {
                Value::Null
            } else {
                json!({"lost":true})
            };
        }
        self.flush_pending();
        if !self.live.contains(&id) {
            return json!({"lost":true});
        }
        // Reject before copying data into the bounded submission queue.
        let (serialized, bytes) = if command.len() > MAX_COMMAND_BYTES
            || bytes.as_ref().is_some_and(|b| b.len() > MAX_UPLOAD_BYTES)
        {
            (
                json!({"op":"bridgeError","i":[gl::OUT_OF_MEMORY]}).to_string(),
                None,
            )
        } else {
            if let Some(value) = self.sync_replies.get(id, command) {
                return value;
            }
            (command.into(), bytes.map(std::borrow::Cow::into_owned))
        };
        match request(Operation::Command(id, serialized, bytes)) {
            Some(Reply::Command(value)) => {
                self.sync_replies.record(id, command, &value);
                value
            }
            _ => {
                self.sync_replies.remove(id);
                json!({"lost":true})
            }
        }
    }
    pub(crate) fn read_pixels(
        &mut self,
        id: u32,
        command: &str,
        bytes: Option<&[u8]>,
    ) -> PixelReply {
        self.read_pixels_data(id, command, bytes.map(std::borrow::Cow::Borrowed))
    }

    pub(crate) fn read_pixels_owned(
        &mut self,
        id: u32,
        command: &str,
        bytes: Option<Vec<u8>>,
    ) -> PixelReply {
        self.read_pixels_data(id, command, bytes.map(std::borrow::Cow::Owned))
    }

    fn read_pixels_data(
        &mut self,
        id: u32,
        command: &str,
        bytes: Option<std::borrow::Cow<'_, [u8]>>,
    ) -> PixelReply {
        self.flush_pending();
        if !self.live.contains(&id) {
            return PixelReply::Lost;
        }
        if command.len() > 1024 || bytes.as_ref().is_some_and(|b| b.len() > MAX_UPLOAD_BYTES) {
            self.execute(id, r#"{"op":"bridgeError","i":[1285]}"#, None);
            return PixelReply::Error;
        }
        match request(Operation::Pixels(
            id,
            command.into(),
            bytes.map(std::borrow::Cow::into_owned),
        )) {
            Some(Reply::Pixels(value)) => {
                if matches!(value, PixelReply::Lost) {
                    self.sync_replies.remove(id);
                }
                value
            }
            _ => {
                self.sync_replies.remove(id);
                PixelReply::Lost
            }
        }
    }
    pub(crate) fn snapshot(&mut self, id: u32) -> Option<Bitmap> {
        self.flush_pending();
        if !self.live.contains(&id) {
            return None;
        }
        match request(Operation::Snapshot(id)) {
            Some(Reply::Snapshot(bitmap)) => bitmap,
            _ => None,
        }
    }

    fn flush_pending(&mut self) {
        let commands = self.pending.take();
        if commands.is_empty() {
            return;
        }
        let ids: HashSet<_> = commands.iter().map(|(id, _)| *id).collect();
        let lost = match request(Operation::Batch(commands)) {
            Some(Reply::Batch(lost)) => lost,
            _ => ids.into_iter().collect(),
        };
        for id in lost {
            self.remove(id);
        }
    }
}
impl Drop for Contexts {
    fn drop(&mut self) {
        self.clear();
    }
}
