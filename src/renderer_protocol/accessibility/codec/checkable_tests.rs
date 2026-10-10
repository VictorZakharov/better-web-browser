use super::*;

#[test]
fn checkable_accessibility_states_roundtrip_and_impossible_states_fail_closed() {
    let root = DocumentNodeId::new((1u128 << 64) | 1).unwrap();
    let mut update = AccessibilityUpdate::full_root(root, "Choice", RectF::default());
    for role in [SemanticRole::Checkbox, SemanticRole::Radio] {
        for state in [
            SemanticChecked::False,
            SemanticChecked::True,
            SemanticChecked::Mixed,
        ] {
            update.nodes[0].role = role;
            update.nodes[0].checked = Some(state);
            let mut writer = WireWriter::new();
            if role == SemanticRole::Radio && state == SemanticChecked::Mixed {
                assert!(encode(&mut writer, &update).is_err());
            } else {
                encode(&mut writer, &update).unwrap();
                let bytes = writer.finish();
                let mut reader = WireReader::new(&bytes);
                assert_eq!(decode(&mut reader).unwrap(), update);
                reader.finish().unwrap();
            }
        }
    }
    update.nodes[0].role = SemanticRole::Checkbox;
    update.nodes[0].checked = None;
    assert!(encode(&mut WireWriter::new(), &update).is_err());
    update.nodes[0].role = SemanticRole::Button;
    update.nodes[0].checked = Some(SemanticChecked::True);
    assert!(encode(&mut WireWriter::new(), &update).is_err());
}
