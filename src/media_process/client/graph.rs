use super::*;
use crate::media_protocol::{GraphPcmFormat, GraphPcmStatus};

impl MediaClient {
    pub(crate) fn queue_graph_pcm(
        &mut self,
        document_id: u64,
        context_id: u64,
        format: GraphPcmFormat,
        pcm: Vec<u8>,
    ) -> Result<GraphPcmStatus, String> {
        format
            .validate(pcm.len())
            .map_err(|error| error.to_string())?;
        let request_id = self.allocate_request()?;
        self.send(BrowserMediaMessage::QueueGraphPcm {
            request_id,
            document_id,
            context_id,
            format,
            pcm,
        })?;
        self.receive_graph_pcm_status(request_id)
    }

    pub(crate) fn close_graph_pcm(
        &mut self,
        document_id: u64,
        context_id: u64,
    ) -> Result<bool, String> {
        let request_id = self.allocate_request()?;
        self.send(BrowserMediaMessage::CloseGraphPcm {
            request_id,
            document_id,
            context_id,
        })?;
        self.receive_graph_pcm_status(request_id)
            .map(|status| status == GraphPcmStatus::Accepted)
    }

    fn receive_graph_pcm_status(&self, request_id: u64) -> Result<GraphPcmStatus, String> {
        match self.receive("graph PCM")? {
            WorkerMediaMessage::GraphPcmStatus {
                request_id: actual,
                status,
            } if actual == request_id => Ok(status),
            _ => Err("media worker returned stale graph PCM status".into()),
        }
    }
}
