use super::*;
use crate::renderer_protocol::{
    DocumentId, GeolocationAction, GeolocationErrorCode, GeolocationEvent, GeolocationPosition,
    GeolocationUpdate,
};

#[test]
fn secure_page_gets_callback_only_from_browser_update() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-exposed', String(navigator.geolocation === navigator.geolocation));
            document.body.setAttribute('data-constructor', String((() => {
                try { new Geolocation(); return 'public'; }
                catch (error) { return error instanceof TypeError ? 'private' : 'wrong error'; }
            })()));
            navigator.geolocation.getCurrentPosition(position => {
                document.body.setAttribute('data-position', position.coords.latitude + ',' + position.timestamp);
                document.body.setAttribute('data-interface', String(
                    position instanceof GeolocationPosition &&
                    position.coords instanceof GeolocationCoordinates));
                document.body.setAttribute('data-json', JSON.stringify(position));
            }, error => document.body.setAttribute('data-error', String(error.code)), {
                maximumAge: 1200, timeout: 5000, enableHighAccuracy: true
            });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-exposed").as_deref(), Some("true"));
    assert_eq!(body.attr("data-constructor").as_deref(), Some("private"));
    assert_eq!(body.attr("data-position"), None);
    let request = initial.geolocation_actions.first().unwrap();
    assert!(matches!(
        request.action,
        GeolocationAction::Start {
            watch: false,
            high_accuracy: true,
            timeout_millis: 5000,
            maximum_age_millis: 1200,
        }
    ));
    let outcome = runtime.deliver_geolocation_update(GeolocationUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: request.request_id,
        terminal: true,
        event: GeolocationEvent::Position(GeolocationPosition {
            latitude: 43.65,
            longitude: -79.38,
            accuracy: 12.0,
            altitude: None,
            altitude_accuracy: None,
            heading: None,
            speed: None,
            timestamp_millis: 1234,
        }),
    });
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(body.attr("data-position").as_deref(), Some("43.65,1234"));
    assert_eq!(body.attr("data-interface").as_deref(), Some("true"));
    let json: serde_json::Value = serde_json::from_str(&body.attr("data-json").unwrap()).unwrap();
    assert_eq!(json["coords"]["latitude"], 43.65);
    assert_eq!(json["coords"]["altitude"], serde_json::Value::Null);
    assert_eq!(json["timestamp"], 1234);
}

#[test]
fn watch_clear_and_structured_error_are_document_scoped() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            globalThis.geoWatch = navigator.geolocation.watchPosition(
                () => document.body.setAttribute('data-position', 'yes'),
                error => {
                    document.body.setAttribute('data-error', String(error.code));
                    document.body.setAttribute('data-error-interface', String(
                        error instanceof GeolocationPositionError));
                });
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request = initial.geolocation_actions.first().unwrap();
    let id = request.request_id;
    assert!(matches!(
        request.action,
        GeolocationAction::Start { watch: true, .. }
    ));
    let update = runtime.deliver_geolocation_update(GeolocationUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: id,
        terminal: false,
        event: GeolocationEvent::Error {
            code: GeolocationErrorCode::Timeout,
            message: "Location acquisition timed out".into(),
        },
    });
    assert!(update.errors.is_empty(), "{:?}", update.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-error")
            .as_deref(),
        Some("3")
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-error-interface")
            .as_deref(),
        Some("true")
    );
    let clear = runtime.execute_additional_with_loader(
        &[input(
            &script,
            "clear.js",
            "navigator.geolocation.clearWatch(geoWatch);",
            false,
        )],
        None,
    );
    assert!(clear.errors.is_empty(), "{:?}", clear.errors);
    assert!(matches!(
        clear.geolocation_actions[0].action,
        GeolocationAction::Clear
    ));
}

#[test]
fn insecure_page_does_not_expose_secure_context_geolocation() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>document.body.setAttribute('data-exposed', String('geolocation' in navigator));</script></body>"#,
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
}
