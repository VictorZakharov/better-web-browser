//! Renderer-facing admission, cancellation, and completion of graph PCM commands.

use super::*;
use crate::renderer_protocol::DocumentId;

impl ChildConnection {
    pub(in crate::renderer_process::child) fn activate_graph_pcm(&mut self, document: DocumentId) {
        if let Some(media) = self.media.as_mut() {
            media.graph_document = document.get();
            if let Some(graph) = media.graph.as_ref() {
                graph.activate(document.get());
            }
        }
    }

    pub(in crate::renderer_process::child) fn retire_graph_pcm(&mut self, document: DocumentId) {
        if let Some(media) = self
            .media
            .as_mut()
            .filter(|media| media.graph_document == document.get())
        {
            media.graph_document = 0;
            if let Some(graph) = media.graph.as_ref() {
                graph.retire(document.get());
            }
        }
    }

    pub(in crate::renderer_process::child) fn retire_all_graph_pcm(&mut self) {
        if let Some(media) = self.media.as_mut() {
            let document_id = std::mem::take(&mut media.graph_document);
            if let Some(graph) = media.graph.as_ref() {
                graph.retire(document_id);
            }
        }
    }

    /// Admit one bounded chunk without waiting for media IPC or the audio device.
    /// False means the renderer's transport queue is full; the caller must retry
    /// the same PCM bytes or retire the stream before rendering more samples.
    pub(in crate::renderer_process::child) fn try_queue_graph_pcm(
        &mut self,
        document: DocumentId,
        context_id: u64,
        format: GraphPcmFormat,
        pcm: Vec<u8>,
    ) -> Result<bool, String> {
        let media = self
            .media
            .as_mut()
            .ok_or("contained media worker is unavailable")?;
        if media.graph_document != document.get() {
            return Ok(false);
        }
        if media.graph.is_none() {
            media.graph = Some(GraphPcmPump::new(
                Arc::clone(&media.client),
                document.get(),
            )?);
        }
        media
            .graph
            .as_ref()
            .expect("graph PCM pump initialized")
            .try_queue(document.get(), context_id, format, pcm)
    }

    pub(in crate::renderer_process::child) fn close_graph_pcm(
        &self,
        document: DocumentId,
        context_id: u64,
    ) -> Option<u64> {
        self.media
            .as_ref()
            .filter(|media| media.graph_document == document.get())
            .and_then(|media| media.graph.as_ref())
            .and_then(|graph| graph.close(document.get(), context_id))
    }

    pub(in crate::renderer_process::child) fn graph_pcm_first_chunk_status(
        &self,
        document: DocumentId,
        context_id: u64,
    ) -> Result<Option<()>, String> {
        let graph = self
            .media
            .as_ref()
            .filter(|media| media.graph_document == document.get())
            .and_then(|media| media.graph.as_ref())
            .ok_or("graph PCM transport is unavailable")?;
        graph.first_chunk_status(document.get(), context_id)
    }

    pub(in crate::renderer_process::child) fn graph_pcm_close_completed(
        &self,
        document: DocumentId,
        ticket: u64,
    ) -> bool {
        self.media
            .as_ref()
            .filter(|media| media.graph_document == document.get())
            .and_then(|media| media.graph.as_ref())
            .is_none_or(|graph| graph.close_completed(ticket))
    }
}
