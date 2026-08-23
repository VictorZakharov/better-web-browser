//! Broker-side ordering, quotas, and stale-document filtering for Fetch streams.

use super::*;
use crate::limits::{MAX_FETCH_STREAM_CHUNK_BYTES, MAX_RESPONSE_BODY_BYTES};
use crate::renderer_protocol::FetchResponseResult;

pub(super) struct OutgoingFetch {
    offset: usize,
    allows_body: bool,
}

impl Broker {
    pub(super) fn process_fetch_stream(&mut self) {
        for _ in 0..crate::limits::MAX_QUEUED_FETCH_STREAM_CHUNKS {
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
                let allows_body = matches!(head.result, FetchResponseResult::Success { .. });
                self.writer()
                    .send_browser(&BrowserMessage::FetchResponseStart(head))
                    .map_err(|error| error.to_string())?;
                self.outgoing_fetch.insert(
                    request_id,
                    OutgoingFetch {
                        offset: 0,
                        allows_body,
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
                    || next > MAX_RESPONSE_BODY_BYTES
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
