use super::*;

fn request(runtime: &mut ScriptRuntime) -> (u32, Box<crate::fetch::FetchRequest>) {
    for _ in 0..100 {
        let result = runtime.advance_time(Duration::ZERO, 1);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        if let Some(request) = result
            .fetch_actions
            .into_iter()
            .find_map(|action| match action {
                ScriptFetchAction::Start { id, request } => Some((id, request)),
                _ => None,
            })
        {
            return request;
        }
    }
    panic!("expected child fetch");
}

#[test]
fn dynamically_inserted_script_executes_in_child_and_wakes_parent_scheduler() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f = document.createElement('iframe');
        f.srcdoc = '<script>const s=document.createElement("script");s.src="/dynamic.js";document.head.append(s)<\/script>';
        document.body.append(f);
    </script>"#,
    );
    let (id, fetch) = request(&mut runtime);
    assert_eq!(fetch.url.as_str(), "https://example.com/dynamic.js");
    let result = runtime.complete_fetch_with_loader(
        id,
        Ok(response(fetch.url.as_str(), "window.dynamicValue=42")),
        None,
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(runtime.next_timer_delay(), Some(Duration::ZERO));
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(f.contentWindow.dynamicValue!==42 || window.dynamicValue) throw Error('script owner');",
    );
}

#[test]
fn child_dynamic_import_checks_mime_and_keeps_module_realm() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f = document.createElement('iframe');
        f.srcdoc = '<script>import("/module.js").then(m=>window.imported=m.value)<\/script>';
        document.body.append(f);
    </script>"#,
    );
    let (id, fetch) = request(&mut runtime);
    assert_eq!(fetch.mode, crate::fetch::RequestMode::Cors);
    let mut module = response(fetch.url.as_str(), "export const value = {child: true};");
    module
        .headers
        .append("content-type", "text/javascript")
        .unwrap();
    let result = runtime.complete_fetch_with_loader(id, Ok(module), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(!f.contentWindow.imported.child || !(f.contentWindow.imported instanceof f.contentWindow.Object) || window.imported) throw Error('import owner');",
    );
}

#[test]
fn initial_blank_location_navigation_does_not_navigate_parent() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        const f = document.createElement('iframe'); document.body.append(f);
        f.contentWindow.location.href = '/child';
    </script>"#,
    );
    let (id, fetch) = request(&mut runtime);
    assert_eq!(fetch.url.as_str(), "https://example.com/child");
    let result = runtime.complete_fetch_with_loader(
        id,
        Ok(response(fetch.url.as_str(), "<p>child</p>")),
        None,
    );
    assert!(result.navigation_url.is_none());
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(f.contentDocument.body.textContent!=='child' || location.pathname!=='/parent/index') throw Error('navigation owner');",
    );
}
