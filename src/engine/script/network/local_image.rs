//! Local Blob image decoding has image-src policy, not author connect-src.
use super::*;

pub(super) fn allowed(args: &[JsValue], state: &HostState) -> JsResult<JsValue> {
    let source = argument_string(args, 1)?;
    // This capability does not authorize a network/file resolver. Ownership of
    // the Blob bytes is proved separately by the realm's private object-URL map.
    let valid = source.len() <= 16 * 1024
        && source.starts_with("blob:")
        && state
            .policy
            .check_request(crate::fetch::RequestDestination::Image, &source, 0)
            .is_ok();
    Ok(JsValue::Boolean(valid))
}
