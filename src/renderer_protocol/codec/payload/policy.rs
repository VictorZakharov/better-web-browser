//! One-flag renderer policies: GPU Canvas (per document) and accessibility (per renderer).
use crate::renderer_protocol::{BrowserMessage, DocumentId, ProtocolError};

pub(super) const GPU_CANVAS: u16 = 0x0251;
pub(super) const ACCESSIBILITY: u16 = 0x0253;

pub(super) fn encode_gpu_canvas(
    document: DocumentId,
    enabled: bool,
) -> Result<(u16, Vec<u8>), ProtocolError> {
    let mut bytes = document.get().to_le_bytes().to_vec();
    bytes.push(u8::from(enabled));
    Ok((GPU_CANVAS, bytes))
}

pub(super) fn encode_accessibility(enabled: bool) -> Result<(u16, Vec<u8>), ProtocolError> {
    Ok((ACCESSIBILITY, vec![u8::from(enabled)]))
}

pub(super) fn decode_gpu_canvas(payload: &[u8]) -> Result<BrowserMessage, ProtocolError> {
    let fields: &[u8; 9] = payload
        .try_into()
        .map_err(|_| ProtocolError::InvalidPayload("GPU Canvas policy length"))?;
    let enabled = flag(fields[8], "GPU Canvas policy flag")?;
    let document = DocumentId::new(u64::from_le_bytes(fields[..8].try_into().unwrap()))?;
    Ok(BrowserMessage::GpuCanvasPolicy { document, enabled })
}

pub(super) fn decode_accessibility(payload: &[u8]) -> Result<BrowserMessage, ProtocolError> {
    let [value] = payload else {
        return Err(ProtocolError::InvalidPayload("accessibility policy length"));
    };
    let enabled = flag(*value, "accessibility policy flag")?;
    Ok(BrowserMessage::AccessibilityPolicy { enabled })
}

fn flag(value: u8, field: &'static str) -> Result<bool, ProtocolError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ProtocolError::InvalidPayload(field)),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gpu_canvas_policy_rejects_zero_document_extra_bytes_and_non_boolean_flags() {
        let mut payload = 1_u64.to_le_bytes().to_vec();
        payload.push(2);
        assert!(super::decode_gpu_canvas(&payload).is_err());
        payload[8] = 1;
        assert!(super::decode_gpu_canvas(&payload).is_ok());
        payload.push(0);
        assert!(super::decode_gpu_canvas(&payload).is_err());
        assert!(super::decode_gpu_canvas(&[0; 9]).is_err());
        assert!(super::decode_gpu_canvas(&payload[..8]).is_err());
    }

    #[test]
    fn accessibility_policy_is_one_strict_boolean() {
        use crate::renderer_protocol::BrowserMessage;
        assert!(matches!(
            super::decode_accessibility(&[1]),
            Ok(BrowserMessage::AccessibilityPolicy { enabled: true })
        ));
        assert!(matches!(
            super::decode_accessibility(&[0]),
            Ok(BrowserMessage::AccessibilityPolicy { enabled: false })
        ));
        for payload in [&[][..], &[2], &[1, 0]] {
            assert!(super::decode_accessibility(payload).is_err());
        }
    }
}
