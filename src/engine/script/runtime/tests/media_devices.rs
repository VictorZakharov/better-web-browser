use super::*;
use crate::renderer_protocol::{
    DocumentId, MediaDeviceError, MediaDeviceResult, MediaDeviceUpdate,
};

#[test]
fn secure_document_enumerates_only_after_browser_update() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-shape', String(
                navigator.mediaDevices instanceof MediaDevices &&
                typeof navigator.mediaDevices.enumerateDevices === 'function' &&
                !('getUserMedia' in navigator.mediaDevices)));
            navigator.mediaDevices.enumerateDevices().then(devices => {
                document.body.setAttribute('data-devices', JSON.stringify(devices));
                document.body.setAttribute('data-types', devices.map(d =>
                    d instanceof InputDeviceInfo).join(','));
            });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-shape").as_deref(), Some("true"));
    assert_eq!(body.attr("data-devices"), None);
    let request = initial.media_device_actions.first().unwrap();
    let result = runtime.deliver_media_device_update(MediaDeviceUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: request.request_id,
        result: MediaDeviceResult::Presence {
            microphone: true,
            camera: true,
        },
    });
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(body.attr("data-types").as_deref(), Some("true,true"));
    assert_eq!(
        body.attr("data-devices").as_deref(),
        Some(
            r#"[{"deviceId":"","kind":"audioinput","label":"","groupId":""},{"deviceId":"","kind":"videoinput","label":"","groupId":""}]"#
        )
    );
}

#[test]
fn enumeration_error_rejects_with_named_dom_exception() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.mediaDevices.enumerateDevices().catch(error =>
                document.body.setAttribute('data-error', error.name));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    let result = runtime.deliver_media_device_update(MediaDeviceUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: initial.media_device_actions[0].request_id,
        result: MediaDeviceResult::Error(MediaDeviceError::NotAllowed),
    });
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-error")
            .as_deref(),
        Some("NotAllowedError")
    );
}

#[test]
fn media_devices_is_secure_context_only_and_has_no_public_constructor() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-exposed', String('mediaDevices' in navigator));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "http://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-exposed")
            .as_deref(),
        Some("false")
    );

    let secure = dom::parse_with_scripting(
        r#"<body><script>
            try { new MediaDevices(); } catch (error) {
                document.body.setAttribute('data-constructor', error.name);
            }
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(secure.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&secure));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        secure
            .elements_named("body")
            .next()
            .unwrap()
            .attr("data-constructor")
            .as_deref(),
        Some("TypeError")
    );
}
