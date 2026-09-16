//! No author code executes while a parser or HostState borrow is held.
use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    host: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "parserDefineCustomElement" {
        host.document
            .register_parser_custom_element(argument_string(args, 1)?);
        return Ok(Some(JsValue::undefined()));
    }
    if !matches!(
        operation,
        "parserElementFailed"
            | "parserElementResult"
            | "parserElementAttributes"
            | "parserElementInsert"
    ) {
        return Ok(None);
    }
    let constructed = host.node(argument_id(args, 2));
    let Some(session) = host.write_session(argument_id(args, 1)) else {
        return Ok(Some(JsValue::Null));
    };
    let target = session.target.clone();
    let node = match operation {
        "parserElementResult" => {
            if let Some(node) = &constructed {
                session.parser.dom().replace_parser_element(node.clone());
            }
            constructed
        }
        "parserElementFailed" => session.parser.dom().fail_parser_element(),
        "parserElementAttributes" => session.parser.dom().apply_parser_attributes(),
        _ => session.parser.dom().insert_parser_element(),
    };
    session.mutated = true;
    let records = session.parser.dom().take_parser_mutations();
    let ids = host.register_parser_changes_in(&target);
    host.record_mutation(Some(&target), MutationKind::Stylesheet);
    Ok(Some(JsValue::Object(vec![
        (
            "node".into(),
            JsValue::from(node.as_ref().map_or(0, |n| host.id_for(n))),
        ),
        ("ids".into(), JsValue::from(ids)),
        ("mutations".into(), parser_mutation_records(host, records)),
    ])))
}
