//! Broker-side ordering, quotas, and stale-document filtering for Fetch streams.

use super::*;
use crate::limits::{
    MAX_FETCH_STREAM_CHUNK_BYTES, MAX_RENDERER_FETCH_STREAM_BYTES, MAX_RESPONSE_BODY_BYTES,
};
use crate::renderer_protocol::FetchResponseResult;
use crate::renderer_protocol::{FetchInitiator, RendererFetchRequest};

pub(super) struct OutgoingFetch {
    offset: usize,
    allows_body: bool,
    streaming: bool,
}

impl Broker {
    pub(super) fn register_fetch_response_policies(
        &mut self,
        requests: &[RendererFetchRequest],
    ) -> Result<(), ProtocolError> {
        for request in requests {
            if self
                .fetch_response_streaming
                .insert(
                    request.head.request_id,
                    request.head.initiator == FetchInitiator::ScriptApi,
                )
                .is_some()
            {
                return Err(ProtocolError::InvalidPayload(
                    "renderer reused a pending Fetch response identifier",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn process_fetch_stream(&mut self) {
        for _ in 0..crate::limits::MAX_QUEUED_FETCH_STREAM_CHUNKS {
            if !self.writer().has_page_command_capacity() {
                break;
            }
            let event = match self.resources().fetch_stream.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            };
            if let Err(error) = self.process_fetch_stream_event(event) {
                self.protocol_failure(error);
                break;
            }
        }
    }

    fn process_fetch_stream_event(&mut self, event: FetchStreamEvent) -> Result<(), String> {
        match event {
            FetchStreamEvent::Start { document, head } => {
                if self.active_document != Some(document) {
                    return Ok(());
                }
                let request_id = head.request_id;
                if request_id == 0 || self.outgoing_fetch.contains_key(&request_id) {
                    return Err("duplicate Fetch response stream".into());
                }
                let streaming = self
                    .fetch_response_streaming
                    .remove(&request_id)
                    .ok_or_else(|| "browser returned an unrequested Fetch response".to_string())?;
                let allows_body = matches!(head.result, FetchResponseResult::Success { .. });
                self.writer()
                    .send_browser(&BrowserMessage::FetchResponseStart(head))
                    .map_err(|error| error.to_string())?;
                self.outgoing_fetch.insert(
                    request_id,
                    OutgoingFetch {
                        offset: 0,
                        allows_body,
                        streaming,
                    },
                );
            }
            FetchStreamEvent::Chunk { document, chunk } => {
                if self.active_document != Some(document) {
                    return Ok(());
                }
                let outgoing = self
                    .outgoing_fetch
                    .get_mut(&chunk.transfer_id)
                    .ok_or_else(|| "unsolicited Fetch response stream chunk".to_string())?;
                let next = outgoing
                    .offset
                    .checked_add(chunk.bytes.len())
                    .ok_or_else(|| "Fetch response stream length overflow".to_string())?;
                if !outgoing.allows_body
                    || chunk.offset as usize != outgoing.offset
                    || chunk.bytes.is_empty()
                    || chunk.bytes.len() > MAX_FETCH_STREAM_CHUNK_BYTES
                    || next
                        > if outgoing.streaming {
                            MAX_RENDERER_FETCH_STREAM_BYTES
                        } else {
                            MAX_RESPONSE_BODY_BYTES
                        }
                {
                    return Err("Fetch response stream chunk violated its contract".into());
                }
                outgoing.offset = next;
                self.writer()
                    .send_browser(&BrowserMessage::FetchResponseChunk(chunk))
                    .map_err(|error| error.to_string())?;
            }
            FetchStreamEvent::End { document, end } => {
                if self.active_document != Some(document) {
                    return Ok(());
                }
                let outgoing = self
                    .outgoing_fetch
                    .remove(&end.request_id)
                    .ok_or_else(|| "unsolicited Fetch response stream end".to_string())?;
                if outgoing.offset != end.total_length as usize {
                    return Err("Fetch response stream total length mismatch".into());
                }
                self.writer()
                    .send_browser(&BrowserMessage::FetchResponseEnd(end))
                    .map_err(|error| error.to_string())?;
            }
            FetchStreamEvent::Abort { document, abort } => {
                if self.active_document != Some(document) {
                    return Ok(());
                }
                if self.outgoing_fetch.remove(&abort.request_id).is_none() {
                    return Err("unsolicited Fetch response stream abort".into());
                }
                self.writer()
                    .send_browser(&BrowserMessage::FetchResponseAbort(abort))
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }
}
