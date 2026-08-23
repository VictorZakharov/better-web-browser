//! Renderer-side bounded Fetch request transfer and interleaved response assembly.

use super::*;
use crate::limits::{
    MAX_DEFERRED_RENDERER_MESSAGES, MAX_RENDERER_FETCH_BATCH_BODY_BYTES,
    MAX_RENDERER_FETCH_REQUESTS_PER_BATCH, MAX_RESPONSE_BODY_BYTES,
};
use crate::renderer_protocol::{
    BrowserFetchResponse, FetchResponseHead, RendererFetchRequest, StreamingTransferAssembler,
};
use std::collections::{HashMap, HashSet};

struct IncomingResponse {
    head: FetchResponseHead,
    body: StreamingTransferAssembler,
}

impl ChildConnection {
    pub(in crate::renderer_process::child) fn fetch_batch(
        &mut self,
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        if requests.len() > MAX_RENDERER_FETCH_REQUESTS_PER_BATCH
            || requests
                .iter()
                .any(|request| request.head.document != document)
        {
            return Err("renderer Fetch batch exceeded its contract".into());
        }
        let batch_id = self.next_batch_id;
        self.next_batch_id = batch_id.checked_add(1).unwrap_or(1);
        self.writer
            .send_renderer(&RendererMessage::FetchBatchStart {
                document,
                batch_id,
                request_count: requests.len() as u32,
            })
            .map_err(|error| error.to_string())?;
        let mut expected = HashSet::with_capacity(requests.len());
        for request in requests {
            request.validate().map_err(|error| error.to_string())?;
            let request_id = request.head.request_id;
            if !expected.insert(request_id) {
                return Err("renderer Fetch batch contains duplicate request identifiers".into());
            }
            self.writer
                .send_renderer(&RendererMessage::FetchRequestStart {
                    batch_id,
                    request: request.head,
                })
                .map_err(|error| error.to_string())?;
            self.send_renderer_chunks(
                request_id,
                &request.body,
                RendererMessage::FetchRequestChunk,
            )?;
            self.writer
                .send_renderer(&RendererMessage::FetchRequestEnd(request_id))
                .map_err(|error| error.to_string())?;
        }
        self.receive_fetch_responses(document, expected)
    }

    fn receive_fetch_responses(
        &mut self,
        document: DocumentId,
        expected: HashSet<u64>,
    ) -> Result<Vec<BrowserFetchResponse>, String> {
        let mut responses = HashMap::with_capacity(expected.len());
        let mut active = HashMap::<u64, IncomingResponse>::with_capacity(expected.len());
        let mut received_bytes = 0_usize;
        while responses.len() < expected.len() && !self.stopping {
            let message = self
                .reader
                .read_browser()
                .map_err(|error| error.to_string())?;
            match message {
                BrowserMessage::Ping(token) => self
                    .writer
                    .send_renderer(&RendererMessage::Pong(token))
                    .map_err(|error| error.to_string())?,
                BrowserMessage::Shutdown => {
                    self.shutdown()?;
                    break;
                }
                BrowserMessage::CancelDocument(cancelled) if cancelled == document => {
                    return Err("document was cancelled".into());
                }
                BrowserMessage::FetchResponseStart(head) => {
                    let request_id = head.request_id;
                    if !expected.contains(&request_id)
                        || responses.contains_key(&request_id)
                        || active.contains_key(&request_id)
                    {
                        return Err("unexpected browser Fetch response".into());
                    }
                    active.insert(
                        request_id,
                        IncomingResponse {
                            body: StreamingTransferAssembler::new(
                                request_id,
                                MAX_RESPONSE_BODY_BYTES,
                            )
                            .map_err(|error| error.to_string())?,
                            head,
                        },
                    );
                }
                BrowserMessage::FetchResponseChunk(chunk) => {
                    received_bytes = received_bytes
                        .checked_add(chunk.bytes.len())
                        .ok_or_else(|| "Fetch response batch length overflow".to_string())?;
                    if received_bytes > MAX_RENDERER_FETCH_BATCH_BODY_BYTES {
                        return Err("Fetch response batch exceeded its body budget".into());
                    }
                    active
                        .get_mut(&chunk.transfer_id)
                        .ok_or_else(|| "unsolicited Fetch response chunk".to_string())?
                        .body
                        .push(chunk)
                        .map_err(|error| error.to_string())?;
                }
                BrowserMessage::FetchResponseEnd(end) => {
                    self.finish_response(end, &mut active, &mut responses)?;
                }
                BrowserMessage::FetchResponseAbort(abort) => {
                    if active.remove(&abort.request_id).is_none()
                        || responses
                            .insert(
                                abort.request_id,
                                BrowserFetchResponse {
                                    head: FetchResponseHead {
                                        request_id: abort.request_id,
                                        result:
                                            crate::renderer_protocol::FetchResponseResult::Failure(
                                                abort.error,
                                            ),
                                    },
                                    body: Vec::new(),
                                },
                            )
                            .is_some()
                    {
                        return Err("unexpected browser Fetch abort".into());
                    }
                }
                BrowserMessage::ProtocolFailure(_) => {
                    return Err("browser rejected renderer IPC".into());
                }
                other => {
                    self.defer_while_fetching(other)?;
                }
            }
        }
        if self.stopping {
            return Err("renderer is shutting down".into());
        }
        expected
            .into_iter()
            .map(|request_id| {
                responses
                    .remove(&request_id)
                    .ok_or_else(|| "browser omitted a Fetch response".to_string())
            })
            .collect()
    }

    fn defer_while_fetching(&mut self, message: BrowserMessage) -> Result<(), String> {
        if self
            .pending
            .back()
            .is_some_and(|pending| can_replace_deferred(pending, &message))
        {
            *self.pending.back_mut().expect("checked deferred message") = message;
            return Ok(());
        }
        if self.pending.len() >= MAX_DEFERRED_RENDERER_MESSAGES {
            return Err("renderer deferred-message queue exhausted".into());
        }
        self.pending.push_back(message);
        Ok(())
    }

    fn finish_response(
        &self,
        end: crate::renderer_protocol::FetchResponseEnd,
        active: &mut HashMap<u64, IncomingResponse>,
        responses: &mut HashMap<u64, BrowserFetchResponse>,
    ) -> Result<(), String> {
        let response = active
            .remove(&end.request_id)
            .ok_or_else(|| "unsolicited Fetch response completion".to_string())?;
        let body = response
            .body
            .finish(end.request_id, end.total_length as usize)
            .map_err(|error| error.to_string())?;
        if responses
            .insert(
                end.request_id,
                BrowserFetchResponse {
                    head: response.head,
                    body,
                },
            )
            .is_some()
        {
            return Err("duplicate browser Fetch response".into());
        }
        Ok(())
    }
}

fn can_replace_deferred(pending: &BrowserMessage, next: &BrowserMessage) -> bool {
    match (pending, next) {
        (
            BrowserMessage::Input(DocumentInput::Scroll(previous)),
            BrowserMessage::Input(DocumentInput::Scroll(next)),
        ) => previous.document == next.document,
        (
            BrowserMessage::Input(DocumentInput::Pointer(previous)),
            BrowserMessage::Input(DocumentInput::Pointer(next)),
        ) => {
            previous.document == next.document
                && previous.phase == crate::renderer_protocol::PointerPhase::Move
                && next.phase == crate::renderer_protocol::PointerPhase::Move
        }
        (
            BrowserMessage::ViewportChanged {
                document: previous, ..
            },
            BrowserMessage::ViewportChanged { document: next, .. },
        ) => previous == next,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer_protocol::{DocumentId, ScrollInput};

    #[test]
    fn consecutive_scroll_updates_coalesce_while_fetching() {
        let document = DocumentId::new(1).unwrap();
        let message = |sequence, y| {
            BrowserMessage::Input(DocumentInput::Scroll(ScrollInput {
                document,
                sequence,
                x: 0.0,
                y,
            }))
        };
        assert!(can_replace_deferred(&message(1, 10.0), &message(2, 20.0)));
    }

    #[test]
    fn ordered_input_is_never_coalesced() {
        let document = DocumentId::new(1).unwrap();
        let scroll = BrowserMessage::Input(DocumentInput::Scroll(ScrollInput {
            document,
            sequence: 1,
            x: 0.0,
            y: 10.0,
        }));
        let lifecycle = BrowserMessage::Input(DocumentInput::Lifecycle(
            crate::renderer_protocol::LifecycleInput {
                document,
                sequence: 2,
                state: crate::renderer_protocol::DocumentLifecycle::Hidden,
            },
        ));
        assert!(!can_replace_deferred(&scroll, &lifecycle));
    }
}
