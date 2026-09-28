use super::super::DocumentRuntime;
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::ProtocolHandlerRequest;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn start_pending_protocol_handler_requests(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        for action in std::mem::take(&mut self.pending_protocol_handler_requests) {
            connection.send_protocol_handler_request(ProtocolHandlerRequest {
                document: self.id,
                client: action.client,
                action: action.action,
                scheme: action.scheme,
                template: action.template,
            })?;
        }
        Ok(())
    }
}
