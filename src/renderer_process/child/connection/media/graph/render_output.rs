use super::*;
use crate::engine::script::audio_render::AudioRenderOutput;
use crate::renderer_protocol::DocumentId;

struct RenderOutput {
    pump: GraphPcmPump,
    document: u64,
}

impl AudioRenderOutput for RenderOutput {
    fn queue(&self, stream: u32, format: GraphPcmFormat, pcm: Vec<u8>) -> Result<bool, String> {
        self.pump
            .try_queue(self.document, u64::from(stream), format, pcm)
    }
    fn health(&self, stream: u32) -> Option<GraphPcmHealth> {
        self.pump.health(self.document, u64::from(stream))
    }
    fn started(&self, stream: u32) -> Result<bool, String> {
        self.pump
            .first_chunk_status(self.document, u64::from(stream))
            .map(|status| status.is_some())
    }
    fn close(&self, stream: u32) {
        self.pump.close(self.document, u64::from(stream));
    }
}

impl ChildConnection {
    pub(in crate::renderer_process::child) fn audio_render_output(
        &mut self,
        document: DocumentId,
    ) -> Result<Arc<dyn AudioRenderOutput>, String> {
        let media = self
            .media
            .as_mut()
            .ok_or("contained media worker is unavailable")?;
        if media.graph_document != document.get() {
            return Err("audio rendering document was retired".into());
        }
        if media.graph.is_none() {
            media.graph = Some(GraphPcmPump::new(
                Arc::clone(&media.client),
                document.get(),
            )?);
        }
        Ok(Arc::new(RenderOutput {
            pump: media.graph.as_ref().expect("graph initialized").clone(),
            document: document.get(),
        }))
    }
}
