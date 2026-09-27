use super::*;
use crate::renderer_protocol::{
    DocumentId, NotificationAction, NotificationEvent, NotificationPermission, NotificationUpdate,
};

#[test]
fn notification_permission_is_seeded_before_author_script_and_resolved_by_browser_update() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            document.body.setAttribute('data-initial', Notification.permission);
            Notification.requestPermission().then(value => {
                document.body.setAttribute('data-decision', value);
            });
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_notification_permission(NotificationPermission::Default);
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request = initial.notification_actions.first().unwrap();
    assert!(matches!(
        request.action,
        NotificationAction::RequestPermission
    ));
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-initial")
            .as_deref(),
        Some("default")
    );
    let updated = runtime.deliver_notification_update(NotificationUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id: request.request_id,
        event: NotificationEvent::Permission(NotificationPermission::Granted),
    });
    assert!(updated.errors.is_empty(), "{:?}", updated.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-decision")
            .as_deref(),
        Some("granted")
    );
}

#[test]
fn notification_show_and_close_are_browser_commands_with_dom_events() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            const notice = new Notification('Report ready', { body: 'Open the report', tag: 'report' });
            notice.onshow = () => document.body.setAttribute('data-shown', 'yes');
            notice.onclose = () => document.body.setAttribute('data-closed', 'yes');
            globalThis.notice = notice;
        </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_notification_permission(NotificationPermission::Granted);
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request = initial.notification_actions.first().unwrap();
    assert!(
        matches!(&request.action, NotificationAction::Show { title, body, tag }
        if title == "Report ready" && body == "Open the report" && tag == "report")
    );
    for event in [NotificationEvent::Shown, NotificationEvent::Closed] {
        let updated = runtime.deliver_notification_update(NotificationUpdate {
            document: DocumentId::new(1).unwrap(),
            request_id: request.request_id,
            event,
        });
        assert!(updated.errors.is_empty(), "{:?}", updated.errors);
    }
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-shown").as_deref(), Some("yes"));
    assert_eq!(body.attr("data-closed").as_deref(), Some("yes"));
}

#[test]
fn notification_task_commands_are_bounded_and_recover_after_drain() {
    let dom = dom::parse_with_scripting(
        r#"<body><script>
            for (let i = 0; i < 64; i++) {
                const notice = new Notification('notice ' + i);
                if (i === 0) globalThis.firstNotice = notice;
            }
            try { new Notification('overflow'); }
            catch (error) { document.body.setAttribute('data-show-limit', error.name); }
        </script></body>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_notification_permission(NotificationPermission::Granted);

    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(initial.notification_actions.len(), 64);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-show-limit")
            .as_deref(),
        Some("RangeError")
    );

    let close = runtime.execute_additional_with_loader(
        &[input(
            &script,
            "close.js",
            r#"for (let i = 0; i < 64; i++) firstNotice.close();
               try { firstNotice.close(); }
               catch (error) { document.body.setAttribute('data-close-limit', error.name); }"#,
            false,
        )],
        None,
    );
    assert!(close.errors.is_empty(), "{:?}", close.errors);
    assert_eq!(close.notification_actions.len(), 64);
    assert!(
        close
            .notification_actions
            .iter()
            .all(|action| matches!(action.action, NotificationAction::Close))
    );
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-close-limit")
            .as_deref(),
        Some("RangeError")
    );

    let recovered = runtime.execute_additional_with_loader(
        &[input(
            &script,
            "next.js",
            "new Notification('after drain');",
            false,
        )],
        None,
    );
    assert!(recovered.errors.is_empty(), "{:?}", recovered.errors);
    assert_eq!(recovered.notification_actions.len(), 1);
}
