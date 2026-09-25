//! CSSOM View's document point queries use the native paint-order snapshot.
//! https://drafts.csswg.org/cssom-view/#dom-document-elementsfrompoint

use super::*;

pub(super) fn elements_at(args: &[JsValue], state: &mut HostState) -> JsValue {
    let document_id = argument_id(args, 1);
    let (Some(x), Some(y)) = (
        args.get(2).and_then(JsValue::as_number),
        args.get(3).and_then(JsValue::as_number),
    ) else {
        return JsValue::Array(Vec::new());
    };
    if document_id != state.id_for(&state.document.clone()) || !x.is_finite() || !y.is_finite() {
        return JsValue::Array(Vec::new());
    }
    state.flush_hit_test_if_needed();
    JsValue::Array(
        state
            .hit_test_snapshot
            .elements_at(x as f32, y as f32)
            .iter()
            .map(|node| JsValue::from(state.id_for(node)))
            .collect(),
    )
}
