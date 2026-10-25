//! An object represents its content navigable only for a document response; otherwise
//! it falls back to its children.
//! https://html.spec.whatwg.org/multipage/iframe-embed-object.html#the-object-element
use super::xml_documents::serve;
use super::*;

#[test]
fn object_falls_back_for_error_status_or_unsupported_type_but_not_documents() {
    let object = "f=document.createElement('object');f.data='/doc'";
    let iframe = "f=document.createElement('iframe');f.src='/doc'";
    for (create, status, content_type, navigable) in [
        (object, 200, "text/html", true),
        (object, 404, "text/html", false),
        (object, 200, "image/png", false),
        // An iframe renders an error page as its document.
        (iframe, 404, "text/html", true),
    ] {
        let (dom, mut runtime) = serve(create, status, content_type, b"<p>child</p>");
        let container = dom
            .elements_named("object")
            .chain(dom.elements_named("iframe"))
            .next()
            .unwrap();
        assert_eq!(
            container.represents_content_navigable(),
            navigable,
            "{create} {status} {content_type}"
        );
        if navigable {
            evaluate(
                &mut runtime,
                &dom,
                "if(f.contentDocument.body.textContent!=='child') throw Error('document');",
            );
        }
    }
}

#[test]
fn object_renders_fallback_while_loading_and_never_navigates_for_non_document_types() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        for (const type of ['', 'image/png', 'image/svg+xml', 'application/x-shockwave-flash']) {
            const o = document.createElement('object');
            o.data = '/resource';
            if (type) o.type = type;
            document.body.append(o);
        }
        </script>"#,
    );
    let fetches = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .iter()
        .filter(|action| matches!(action, ScriptFetchAction::Start { .. }))
        .count();
    assert_eq!(fetches, 1, "only the untyped object fetches its resource");
    assert!(
        dom.elements_named("object")
            .all(|object| !object.represents_content_navigable())
    );
    evaluate(
        &mut runtime,
        &dom,
        "const o=document.querySelectorAll('object');
        if(!o[0].contentDocument || [1,2,3].some(i=>o[i].contentDocument!==null)) throw Error('type');",
    );
}

#[test]
fn nested_objects_fall_back_only_as_far_as_the_first_document() {
    // Acid3 test 16: a 404 object falls back to an object whose HTML document hides the
    // innermost object and its text.
    let (dom, mut runtime) = start(
        r#"<body><script>
        const c = document.createElement('object'); c.append('FAIL');
        const b = document.createElement('object'), a = document.createElement('object');
        a.data = '/a'; b.data = '/b'; b.append(c); c.data = '/c'; a.append(b);
        document.body.append(a);
        </script>"#,
    );
    let starts: Vec<_> = (0..10)
        .flat_map(|_| runtime.advance_time(Duration::ZERO, 1).fetch_actions)
        .filter_map(|action| match action {
            ScriptFetchAction::Start { id, request } => Some((id, request.url.to_string())),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 3, "{starts:?}");
    for (id, url) in starts {
        let (status, content_type) = match url.rsplit('/').next().unwrap() {
            "a" => (404, "image/png"),
            "b" => (200, "text/html"),
            _ => (200, "image/png"),
        };
        let mut head = response(&url, "");
        head.status = status;
        head.headers.append("content-type", content_type).unwrap();
        for event in [
            crate::engine::script::ScriptFetchEvent::Head(Ok(head)),
            crate::engine::script::ScriptFetchEvent::Chunk(b"<p>".to_vec()),
            crate::engine::script::ScriptFetchEvent::End,
        ] {
            runtime.deliver_fetch_event_with_loader(id, event, None);
        }
    }
    drain(&mut runtime);
    let represented: Vec<_> = dom
        .elements_named("object")
        .map(|object| object.represents_content_navigable())
        .collect();
    assert_eq!(represented, [false, true, false]);
}

#[test]
fn object_src_governs_object_requests_and_their_redirects_instead_of_frame_src() {
    let (dom, mut runtime) = start("<body><script></script>");
    let mut headers = crate::fetch::HeaderList::new();
    headers
        .append(
            "content-security-policy",
            "object-src https://allowed.test; frame-src *",
        )
        .unwrap();
    runtime.host.borrow_mut().policy = std::sync::Arc::new(
        crate::fetch::csp::PolicyContainer::from_headers(
            "https://example.com/parent/index",
            &headers,
        )
        .unwrap(),
    );
    evaluate(
        &mut runtime,
        &dom,
        "for (const url of ['https://evil.test/x.html', 'https://allowed.test/a']) {
            const o = document.createElement('object'); o.data = url; document.body.append(o);
        }",
    );
    let mut starts = Vec::new();
    let mut diagnostics = Vec::new();
    for _ in 0..10 {
        let outcome = runtime.advance_time(Duration::ZERO, 1);
        diagnostics.extend(outcome.diagnostics);
        starts.extend(
            outcome
                .fetch_actions
                .into_iter()
                .filter_map(|action| match action {
                    ScriptFetchAction::Start { id, request } => Some((id, request)),
                    _ => None,
                }),
        );
    }
    assert!(
        diagnostics
            .iter()
            .any(|message| message.contains("object-src"))
    );
    let [(id, request)] = starts.as_slice() else {
        panic!("only the allowed object may fetch: {starts:?}");
    };
    assert_eq!(
        request.destination,
        crate::fetch::RequestDestination::Object
    );
    // A redirect to a disallowed origin is refused when the response arrives.
    let mut head = response("https://allowed.test/a", "");
    head.url_list
        .push(crate::fetch::FetchUrl::parse("https://evil.test/x.html").unwrap());
    head.headers.append("content-type", "text/html").unwrap();
    runtime.deliver_fetch_event_with_loader(
        *id,
        crate::engine::script::ScriptFetchEvent::Head(Ok(head)),
        None,
    );
    drain(&mut runtime);
    assert!(
        dom.elements_named("object")
            .all(|object| !object.represents_content_navigable())
    );
}

#[test]
fn svg_document_sizes_its_object_from_the_root_width_and_height() {
    let (dom, _runtime) = serve(
        "f=document.createElement('object');f.data='/doc'",
        200,
        "image/svg+xml",
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50px"/>"#,
    );
    let object = dom.elements_named("object").next().unwrap();
    assert_eq!(object.content_navigable_size(), Some((100.0, 50.0)));
}

#[test]
fn data_or_type_changes_on_a_connected_object_rerun_its_resource_selection() {
    let (dom, mut runtime) = start(
        "<body><script>window.o=document.createElement('object');document.body.append(o);</script>",
    );
    let fetches = |runtime: &mut ScriptRuntime| {
        (0..5)
            .flat_map(|_| runtime.advance_time(Duration::ZERO, 1).fetch_actions)
            .filter(|action| matches!(action, ScriptFetchAction::Start { .. }))
            .count()
    };
    assert_eq!(fetches(&mut runtime), 0);
    evaluate(&mut runtime, &dom, "o.data='/late';");
    assert_eq!(
        fetches(&mut runtime),
        1,
        "setting data navigates the object"
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(!o.contentDocument) throw Error('navigable'); o.type='image/png';
        if(o.contentDocument!==null) throw Error('an image type has no navigable');",
    );
}
