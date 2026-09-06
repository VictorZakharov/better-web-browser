use super::*;

#[test]
fn media_lifecycle_diagnostics_are_bounded_and_do_not_include_resource_urls() {
    let (_, outcome) = execute_html(
        r#"<video id="movie" src="https://example.com/private-media?token=secret"></video>
        <script>
            const movie = document.getElementById('movie');
            for (let i = 0; i < 200; i++) movie.volume = i % 2 ? 0.5 : 1;
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let events = outcome
        .diagnostics
        .iter()
        .filter(|message| message.starts_with("media lifecycle:"))
        .collect::<Vec<_>>();
    assert!(!events.is_empty());
    assert!(
        events.len() <= 64,
        "host diagnostic batch exceeded its bound"
    );
    for event in events {
        assert!(event.contains("request:configure"));
        assert!(event.contains("ready=0"));
        assert!(!event.contains("private-media"));
        assert!(!event.contains("secret"));
        assert!(event.len() <= 1041);
    }
}
