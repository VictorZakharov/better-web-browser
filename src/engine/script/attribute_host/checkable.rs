//! Live input state uses rendering invalidation, never fabricated attribute mutations.
use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if !matches!(
        operation,
        "inputChecked"
            | "inputSetChecked"
            | "inputIndeterminate"
            | "inputSetIndeterminate"
            | "inputRadioGroup"
            | "inputResetChecked"
    ) {
        return Ok(None);
    }
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Ok(Some(JsValue::undefined()));
    };
    let version = node.document_mutation_version();
    match operation {
        "inputChecked" => return Ok(Some(JsValue::from(node.checked()))),
        "inputIndeterminate" => return Ok(Some(JsValue::from(node.indeterminate()))),
        "inputRadioGroup" => {
            let ids = node
                .radio_group()
                .iter()
                .map(|node| state.id_for(node).to_string())
                .collect::<Vec<_>>()
                .join(",");
            return Ok(Some(js_string(ids)));
        }
        "inputSetChecked" => node.set_checked(
            args.get(2).and_then(JsValue::as_boolean).unwrap_or(false),
            true,
        ),
        "inputSetIndeterminate" => {
            node.set_indeterminate(args.get(2).and_then(JsValue::as_boolean).unwrap_or(false))
        }
        "inputResetChecked" => node.reset_checked(),
        _ => unreachable!(),
    };
    if node.document_mutation_version() != version {
        state.record_mutation(Some(&node), MutationKind::State);
        // Checking a radio unchecks its group peers; their validity flips
        // too even when they live under a different parent.
        if node.is_radio() && state.mutation_requires_render(&node) {
            let document = state.document.clone();
            for peer in node.radio_group() {
                if peer.id() != node.id() {
                    // Record the same state dependency as the activated radio:
                    // its parent includes siblings, and validation adds owners.
                    state
                        .pending_invalidation
                        .record(&document, Some(&peer), MutationKind::State);
                    state.pending_layout_invalidation.record(
                        &document,
                        Some(&peer),
                        MutationKind::State,
                    );
                }
            }
        }
    }
    Ok(Some(JsValue::undefined()))
}
