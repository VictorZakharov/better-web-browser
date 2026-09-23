use super::*;

#[test]
fn window_name_starts_empty_and_follows_the_child_browsing_context() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        if (window.name !== '') throw Error('top-level Window.name default');
        window.name = 42;
        if (window.name !== '42') throw Error('Window.name string conversion');
        window.frame = document.createElement('iframe');
        frame.setAttribute('name', 'initial-frame-name');
        frame.srcdoc = '<p>first document</p>';
        document.body.append(frame);
        if (frame.contentWindow.name !== 'initial-frame-name') throw Error('iframe name');
        if (frames !== window || frames.length !== 1 ||
            frames['initial-frame-name'] !== frame.contentWindow)
            throw Error('Window.frames named child');
        frame.contentWindow.name = 'updated-name';
        </script>"#,
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if (name !== '42' || frame.contentWindow.name !== 'updated-name') throw Error('name after navigation');",
    );
    evaluate(
        &mut runtime,
        &dom,
        "frame.srcdoc = '<p>second document</p>';",
    );
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if (frame.contentWindow.name !== 'updated-name') throw Error('name after second navigation');",
    );
}
