//! Pure support queries shared by Window and Worker, with no device, network,
//! decoder or encoder allocation. Author dictionary conversion stays in JS.

use super::super::*;

pub(in crate::engine::script) fn dispatch(operation: &str, args: &[JsValue]) -> Option<JsValue> {
    let string = |index| match args.get(index) {
        Some(JsValue::String(value)) => value.as_str(),
        _ => "",
    };
    let channels = match args.get(2) {
        Some(JsValue::String(value)) => Some(value.as_str()),
        _ => None,
    };
    match operation {
        "mediaCapabilitiesContentType" => Some(JsValue::from(
            crate::media_type::capability_content_type(string(1), string(2), string(3)).to_string(),
        )),
        "mediaCapabilitiesAudioSupported" => Some(JsValue::from(
            crate::media_type::audio_parameters_supported(
                string(1),
                string(5),
                channels,
                args.get(3).and_then(JsValue::as_number),
                args.get(4).and_then(JsValue::as_boolean).unwrap_or(false),
            ),
        )),
        "mediaCapabilitiesEncodingAudioSupported" => {
            Some(JsValue::from(crate::media_type::encoding_audio_supported(
                string(1),
                channels,
                args.get(3).and_then(JsValue::as_number),
                args.get(4).and_then(JsValue::as_number),
            )))
        }
        _ => None,
    }
}
