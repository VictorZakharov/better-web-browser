//! A private DSP agent. Its clock and output never depend on document tasks.

use super::engine::{Context, HostBridge, JsValue, Source};
use crate::media_protocol::{GraphPcmFormat, GraphPcmHealth};
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;

mod bootstrap;
#[cfg(test)]
mod tests;
mod worker;

const MAX_TRANSFER_BYTES: usize = 640 * 1024 * 1024;
const MAX_GRAPH_BYTES: usize = 16 * 1024 * 1024;

/// Bound to one contained document, including its cancellation generation.
pub(crate) trait AudioRenderOutput: Send + Sync {
    fn queue(&self, stream: u32, format: GraphPcmFormat, pcm: Vec<u8>) -> Result<bool, String>;
    fn health(&self, stream: u32) -> Option<GraphPcmHealth>;
    fn started(&self, stream: u32) -> Result<bool, String>;
    fn close(&self, stream: u32);
}

struct Asset {
    id: u32,
    description: String,
    arrays: Vec<JsValue>,
}

#[derive(Default)]
struct State {
    output: Option<Arc<dyn AudioRenderOutput>>,
    stream: Option<(u32, u32)>,
    rate: u32,
    graph: Option<String>,
    assets: Vec<Asset>,
    uncommitted_assets: Vec<Asset>,
    transfer_bytes: usize,
    packets: Vec<JsValue>,
    callbacks: Vec<Vec<JsValue>>,
    feedback_requests: HashSet<u32>,
    feedback: HashMap<u32, JsValue>,
    packet_bytes: usize,
    started: bool,
    error: Option<String>,
}

struct Control {
    state: Arc<Mutex<State>>,
    clock: Arc<AtomicU64>,
    wake: mpsc::SyncSender<()>,
}

#[derive(Clone)]
pub struct AudioRenderControl(Arc<Control>);

impl std::fmt::Debug for AudioRenderControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioRenderControl").finish_non_exhaustive()
    }
}

impl AudioRenderControl {
    pub(super) fn new() -> Result<Self, String> {
        let state = Arc::new(Mutex::new(State::default()));
        let clock = Arc::new(AtomicU64::new(0));
        let (wake, receiver) = mpsc::sync_channel(1);
        let shared = Arc::clone(&state);
        let progress = Arc::clone(&clock);
        std::thread::Builder::new()
            .name("breeze-audio-render".into())
            .spawn(move || {
                let result = worker::run(&shared, &progress, receiver);
                if let Err(error) = result {
                    shared.lock().unwrap_or_else(|p| p.into_inner()).error = Some(error);
                }
            })
            .map_err(|error| format!("start audio rendering thread: {error}"))?;
        Ok(Self(Arc::new(Control { state, clock, wake })))
    }

    pub(crate) fn attach(&self, output: Arc<dyn AudioRenderOutput>) {
        self.0
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .output = Some(output);
        self.wake();
    }

    pub(super) fn start(&self, stream: u32, rate: u32) {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        state.stream = Some((stream, rate));
        state.rate = rate;
        state.started = false;
        drop(state);
        self.wake();
    }

    pub(super) fn pause(&self) {
        self.0
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .stream = None;
        self.wake();
    }

    pub(super) fn graph(&self, graph: String) -> bool {
        if graph.len() > MAX_GRAPH_BYTES {
            return false;
        }
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        state.graph = Some(graph);
        let assets = std::mem::take(&mut state.uncommitted_assets);
        state.assets.extend(assets);
        drop(state);
        self.wake();
        true
    }

    pub(super) fn asset(&self, id: u32, description: String, arrays: Vec<JsValue>) -> bool {
        let bytes = description.len()
            + arrays
                .iter()
                .filter_map(JsValue::as_bytes)
                .map(<[u8]>::len)
                .sum::<usize>();
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if bytes > MAX_TRANSFER_BYTES.saturating_sub(state.transfer_bytes) {
            return false;
        }
        state.transfer_bytes += bytes;
        state.uncommitted_assets.push(Asset {
            id,
            description,
            arrays,
        });
        true
    }

    pub(super) fn frame(&self) -> u64 {
        self.0.clock.load(Ordering::Acquire)
    }

    pub(super) fn feedback(&self, id: u32) -> JsValue {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.feedback_requests.len() < 256 {
            state.feedback_requests.insert(id);
        }
        state.feedback.get(&id).cloned().unwrap_or(JsValue::Null)
    }

    pub(super) fn callback(&self, arguments: Vec<JsValue>) -> bool {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.callbacks.len() >= 256 {
            return false;
        }
        state.callbacks.push(arguments);
        drop(state);
        self.wake();
        true
    }

    pub(super) fn poll(&self) -> JsValue {
        let mut state = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        let mut packets = Vec::new();
        let mut arrays = Vec::new();
        for packet in std::mem::take(&mut state.packets) {
            if let JsValue::Array(mut parts) = packet {
                if let Some(JsValue::String(description)) = parts.first()
                    && let Ok(mut packet) = serde_json::from_str::<serde_json::Value>(description)
                {
                    packet["arrayOffset"] = arrays.len().into();
                    packets.push(packet);
                }
                arrays.extend(parts.drain(1..));
            }
        }
        state.packet_bytes = 0;
        let status = serde_json::json!({ "frame": self.frame(), "started": state.started,
            "error": state.error, "packets": packets });
        let mut reply = vec![JsValue::from(status.to_string())];
        reply.extend(arrays);
        JsValue::Array(reply)
    }

    fn wake(&self) {
        let _ = self.0.wake.try_send(());
    }
}
