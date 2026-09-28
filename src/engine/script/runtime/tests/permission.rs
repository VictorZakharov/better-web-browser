use super::*;
use crate::renderer_protocol::{DocumentId, PermissionName, PermissionState, PermissionUpdate};

#[test]
fn permission_query_is_async_browser_authoritative_and_changes_live() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-same', String(navigator.permissions === navigator.permissions));
            document.body.setAttribute('data-hook', typeof __receivePermissionUpdate);
            let descriptorReads = 0;
            navigator.permissions.query({get name() { descriptorReads++; return 'geolocation'; }}).then(status => {
                globalThis.permissionStatus = status;
                document.body.setAttribute('data-initial', status.state);
                document.body.setAttribute('data-reads', String(descriptorReads));
                status.addEventListener('change', event => {
                    document.body.setAttribute('data-listener', status.state);
                    document.body.setAttribute('data-trusted', String(event.isTrusted));
                });
                status.onchange = () => {
                    document.body.setAttribute('data-onchange', status.state);
                };
            });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.permission_actions.len(), 1);
    assert_eq!(
        initial.permission_actions[0].name,
        PermissionName::Geolocation
    );
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-same").as_deref(), Some("true"));
    assert_eq!(body.attr("data-hook").as_deref(), Some("undefined"));
    assert_eq!(body.attr("data-initial"), None);

    let id = initial.permission_actions[0].request_id;
    let update = |state| PermissionUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: id,
        name: PermissionName::Geolocation,
        state,
        rejected: false,
    };
    let settled = runtime.deliver_permission_update(update(PermissionState::Prompt));
    assert!(settled.errors.is_empty(), "{:?}", settled.errors);
    assert_eq!(body.attr("data-initial").as_deref(), Some("prompt"));
    assert_eq!(body.attr("data-reads").as_deref(), Some("2"));
    let changed = runtime.deliver_permission_update(update(PermissionState::Granted));
    assert!(changed.errors.is_empty(), "{:?}", changed.errors);
    assert_eq!(body.attr("data-listener").as_deref(), Some("granted"));
    assert_eq!(body.attr("data-trusted").as_deref(), Some("true"));
    assert_eq!(body.attr("data-onchange").as_deref(), Some("granted"));
}

#[test]
fn unsupported_and_malformed_permission_descriptors_reject_without_browser_request() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            Promise.all([
                navigator.permissions.query({name:'clipboard-read'}),
                navigator.permissions.query({name:'not-a-permission'}),
                navigator.permissions.query(null),
            ].map(promise => promise.then(() => 'accepted', error => error.name)))
                .then(names => document.body.setAttribute('data-errors', names.join(',')));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.permission_actions.is_empty());
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-errors").as_deref(),
        Some("TypeError,TypeError,TypeError")
    );
}

#[test]
fn browser_quota_rejection_settles_the_pending_query() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            navigator.permissions.query({name:'notifications'}).then(
                () => document.body.setAttribute('data-result', 'unexpected'),
                error => document.body.setAttribute('data-result', error.name));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let update = PermissionUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: initial.permission_actions[0].request_id,
        name: PermissionName::Notifications,
        state: PermissionState::Denied,
        rejected: true,
    };
    let settled = runtime.deliver_permission_update(update);
    assert!(settled.errors.is_empty(), "{:?}", settled.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-result").as_deref(),
        Some("QuotaExceededError")
    );
}

#[test]
fn detached_child_document_query_rejects_invalid_state_without_browser_request() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const frame = document.createElement('iframe');
            document.body.append(frame);
            const permissions = frame.contentWindow.navigator.permissions;
            frame.remove();
            permissions.query({name:'geolocation'}).then(
                () => document.body.setAttribute('data-result', 'unexpected'),
                error => document.body.setAttribute('data-result', error.name));
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let outcome = runtime.execute_initial(&script_inputs(&dom));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.permission_actions.is_empty());
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-result").as_deref(),
        Some("InvalidStateError")
    );
}
