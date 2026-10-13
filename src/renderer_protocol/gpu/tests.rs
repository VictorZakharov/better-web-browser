use super::*;

fn canvas() -> PresentedGpuCanvas {
    PresentedGpuCanvas {
        url: "breeze-internal:canvas:18446744073709551617:8x4".into(),
        frame: GpuFrame {
            surface: 1,
            handle: 42,
            key: 7,
            width: 8,
            height: 4,
            adapter_low: 31,
            adapter_high: -1,
        },
    }
}
#[test]
fn gpu_canvas_wire_round_trip_retains_wide_identities_and_signed_adapter() {
    let mut value = canvas();
    value.frame.surface = u64::MAX;
    value.frame.key = u64::MAX;
    let mut writer = WireWriter::new();
    encode(&mut writer, &[value.clone()]).unwrap();
    let bytes = writer.finish();
    let mut reader = WireReader::new(&bytes);
    assert_eq!(decode(&mut reader).unwrap(), vec![value]);
    reader.finish().unwrap();
}
#[test]
fn gpu_canvas_wire_rejects_bad_sizes_names_duplicates_and_counts() {
    let good = canvas();
    let mut values = vec![good.clone(), good.clone()];
    assert!(validate(&values).is_err());
    values.truncate(1);
    values[0].frame.width = u32::MAX;
    assert!(validate(&values).is_err());
    values[0] = good.clone();
    values[0].frame.key = 0;
    assert!(validate(&values).is_err());
    values[0] = good.clone();
    values[0].url = "https://example.test/image".into();
    assert!(validate(&values).is_err());
    let mut writer = WireWriter::new();
    writer.u32(65);
    let bytes = writer.finish();
    assert!(decode(&mut WireReader::new(&bytes)).is_err());
    let mut writer = WireWriter::new();
    encode(&mut writer, &[good]).unwrap();
    let mut bytes = writer.finish();
    bytes.pop();
    assert!(decode(&mut WireReader::new(&bytes)).is_err());
}
#[test]
fn gpu_canvas_policy_is_document_scoped_and_strictly_boolean() {
    use crate::renderer_protocol::{
        BrowserMessage, DocumentId, FrameReader, FrameWriter, RendererSessionId,
    };
    let session = RendererSessionId::new(1).unwrap();
    let document = DocumentId::new(2).unwrap();
    for enabled in [false, true] {
        let message = BrowserMessage::GpuCanvasPolicy { document, enabled };
        let mut writer = FrameWriter::new(Vec::new(), session);
        writer.send_browser(&message).unwrap();
        let bytes = writer.into_inner();
        assert_eq!(
            FrameReader::new(std::io::Cursor::new(bytes), session)
                .read_browser()
                .unwrap(),
            message
        );
    }
}
