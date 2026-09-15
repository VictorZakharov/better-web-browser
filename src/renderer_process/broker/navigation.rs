//! Bounded, replayable navigation bytes shared by a network producer and renderer attempts.
//! Unlike a script response, a main response may need replay after an encoding/renderer restart.
use super::wake::BrokerWake;
use crate::limits::{MAX_FETCH_STREAM_CHUNK_BYTES, MAX_RESPONSE_BODY_BYTES};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct NavigationBody(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    bytes: Vec<u8>,
    result: Option<Result<(), String>>,
    wake: Option<BrokerWake>,
}

pub(super) enum Input {
    Chunk(Vec<u8>),
    Pending,
    End,
    Failed(String),
}

impl NavigationBody {
    pub fn append(&self, bytes: &[u8]) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|error| error.to_string())?;
        if state.result.is_some() {
            return Err("navigation input arrived after completion".into());
        }
        if bytes.len() > MAX_FETCH_STREAM_CHUNK_BYTES
            || state.bytes.len().saturating_add(bytes.len()) > MAX_RESPONSE_BODY_BYTES
        {
            return Err("navigation response exceeded its byte limit".into());
        }
        state.bytes.extend_from_slice(bytes);
        if let Some(wake) = &state.wake {
            wake.notify();
        }
        Ok(())
    }

    pub fn finish(&self, result: Result<(), String>) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.result.is_none() {
            state.result = Some(result);
        }
        if let Some(wake) = &state.wake {
            wake.notify();
        }
    }

    pub(super) fn subscribe(&self, wake: BrokerWake) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .wake = Some(wake);
    }

    pub(super) fn read(&self, offset: usize) -> Input {
        let state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(Err(error)) = &state.result {
            return Input::Failed(error.clone());
        }
        if offset < state.bytes.len() {
            let end = (offset + MAX_FETCH_STREAM_CHUNK_BYTES).min(state.bytes.len());
            return Input::Chunk(state.bytes[offset..end].to_vec());
        }
        if state.result.is_some() {
            Input::End
        } else {
            Input::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_replayable_and_completion_is_terminal() {
        let body = NavigationBody::default();
        assert!(matches!(body.read(0), Input::Pending));
        body.append(b"prefix").unwrap();
        assert!(matches!(body.read(0), Input::Chunk(bytes) if bytes == b"prefix"));
        assert!(matches!(body.read(6), Input::Pending));
        body.finish(Ok(()));
        body.finish(Err("late failure".into()));
        assert!(matches!(body.read(0), Input::Chunk(bytes) if bytes == b"prefix"));
        assert!(matches!(body.read(6), Input::End));
        assert!(body.append(b"late").is_err());
    }

    #[test]
    fn failure_retires_buffered_bytes_and_limits_apply_before_append() {
        let body = NavigationBody::default();
        assert!(
            body.append(&vec![0; MAX_FETCH_STREAM_CHUNK_BYTES + 1])
                .is_err()
        );
        let chunk = vec![0; MAX_FETCH_STREAM_CHUNK_BYTES];
        for _ in 0..MAX_RESPONSE_BODY_BYTES / chunk.len() {
            body.append(&chunk).unwrap();
        }
        assert!(body.append(b"overflow").is_err());
        body.finish(Err("connection interrupted".into()));
        assert!(matches!(body.read(0), Input::Failed(error) if error == "connection interrupted"));
    }
}
