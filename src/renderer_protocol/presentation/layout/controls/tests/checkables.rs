use super::*;
use crate::engine::layout::test_support::FixedMeasurer;
use crate::engine::{Page, layout_page};

fn spec(kind: &str) -> ControlSpec {
    let page = Page::parse(
        &format!("<input type={kind} checked style='accent-color:red'>"),
        "https://example.test/",
    );
    layout_page(&page, 300.0, 200.0, &mut FixedMeasurer)
        .items
        .into_iter()
        .find_map(|item| {
            if let DisplayItem::Control(spec) = item {
                Some(*spec)
            } else {
                None
            }
        })
        .unwrap()
}

#[test]
fn checkable_wire_roundtrips_state_and_rejects_truncated_or_mismatched_metadata() {
    for kind in ["checkbox", "radio"] {
        let spec = spec(kind);
        let mut writer = WireWriter::new();
        encode_control(&mut writer, &spec).unwrap();
        let bytes = writer.finish();
        let mut reader = WireReader::new(&bytes);
        assert_eq!(decode_control(&mut reader).unwrap(), spec);
        reader.finish().unwrap();
        for end in 0..bytes.len() {
            assert!(decode_control(&mut WireReader::new(&bytes[..end])).is_err());
        }
        let mut invalid = spec.clone();
        invalid.checkable = None;
        assert!(encode_control(&mut WireWriter::new(), &invalid).is_err());
        invalid = spec.clone();
        invalid.kind = ControlKind::Text;
        assert!(encode_control(&mut WireWriter::new(), &invalid).is_err());
    }
    let mut radio = spec("radio");
    radio.checkable.as_mut().unwrap().indeterminate = true;
    assert!(encode_control(&mut WireWriter::new(), &radio).is_err());
}
