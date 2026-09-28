use super::ChildConnection;
use crate::renderer_protocol::{ProtocolHandlerRequest, RendererMessage};

impl ChildConnection {
    pub(in crate::renderer_process::child) fn send_protocol_handler_request(
        &mut self,
        request: ProtocolHandlerRequest,
    ) -> Result<(), String> {
        self.writer
            .send_renderer(&RendererMessage::ProtocolHandlerRequest(request))
            .map_err(|error| error.to_string())
    }
}
