use super::*;

#[test]
fn anchor_exposes_and_updates_url_components() {
    let (dom, outcome) = execute_html(
        r#"<body><a id="link" href="/watch?v=one#player">video</a><div id="status">no</div>
        <script>
            const link = document.getElementById('link');
            const initial = [link.href, link.origin, link.protocol, link.host, link.hostname,
                link.port, link.pathname, link.search, link.hash].join('|');
            link.pathname = '/embed/video';
            link.search = '?autoplay=1';
            link.hash = '#controls';
            const updated = link.href;
            const empty = document.createElement('a');
            document.getElementById('status').textContent =
                initial === 'https://example.com/watch?v=one#player|https://example.com|https:|example.com|example.com||/watch|?v=one|#player' &&
                updated === 'https://example.com/embed/video?autoplay=1#controls' &&
                link.toString() === updated && empty.href === '' && empty.pathname === '' ? 'yes' : 'no';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "yes"
    );
}
