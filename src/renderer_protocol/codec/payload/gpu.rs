use crate::renderer_protocol::{BrowserMessage, DocumentId, ProtocolError};

pub(super) fn encode(document: DocumentId, enabled: bool) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut bytes = document.get().to_le_bytes().to_vec();
    bytes.push(u8::from(enabled));
    Ok((0x0251, bytes))
}

pub(super) fn decode(payload: &[u8]) -> Result<BrowserMessage, ProtocolError> {
    let fields: &[u8; 9] = payload
        .try_into()
        .map_err(|_| ProtocolError::InvalidPayload("GPU Canvas policy length"))?;
    let enabled = match fields[8] {
        0 => false,
        1 => true,
        _ => return Err(ProtocolError::InvalidPayload("GPU Canvas policy flag")),
    };
    let document = DocumentId::new(u64::from_le_bytes(fields[..8].try_into().unwrap()))?;
    Ok(BrowserMessage::GpuCanvasPolicy { document, enabled })
}

#[cfg(test)]
mod tests {
    #[test]
    fn gpu_canvas_policy_rejects_zero_document_extra_bytes_and_non_boolean_flags() {
        let mut payload = 1_u64.to_le_bytes().to_vec();
        payload.push(2);
        assert!(super::decode(&payload).is_err());
        payload[8] = 1;
        assert!(super::decode(&payload).is_ok());
        payload.push(0);
        assert!(super::decode(&payload).is_err());
        assert!(super::decode(&[0; 9]).is_err());
        assert!(super::decode(&payload[..8]).is_err());
    }
}
