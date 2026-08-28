use super::*;

#[test]
fn intersection_observer_delivers_an_asynchronous_initial_entry() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="target"></div><div id="status">waiting</div><script>
            let synchronous = true;
            const target = document.getElementById('target');
            const observer = new IntersectionObserver((entries, current) => {
                const entry = entries[0];
                const valid = !synchronous && current === observer && entry.target === target &&
                    entry.isIntersecting && entry.intersectionRatio === 1 &&
                    entry.rootBounds.width === 1792 && entry.rootBounds.height === 740 &&
                    observer.root === null && observer.rootMargin === '10px 20% 10px 20%' &&
                    observer.thresholds.join(',') === '0,0.5';
                document.getElementById('status').textContent = valid ? 'yes' : 'invalid';
            }, { rootMargin: '10px 20%', threshold: [0.5, 0] });
            observer.observe(target);
            synchronous = false;
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").nth(1).unwrap().text_content(),
        "yes"
    );
}

#[test]
fn intersection_observer_validates_inputs_and_honors_disconnect() {
    let (dom, outcome) = execute_html(
        r#"<body><div id="target"></div><div id="status">waiting</div><script>
            let invalidCallback = false;
            let invalidThreshold = false;
            try { new IntersectionObserver(null); } catch (error) { invalidCallback = error instanceof TypeError; }
            try { new IntersectionObserver(() => {}, { threshold: 2 }); }
            catch (error) { invalidThreshold = error instanceof RangeError; }
            const observer = new IntersectionObserver(() => {
                document.getElementById('status').textContent = 'callback ran';
            });
            observer.observe(document.getElementById('target'));
            observer.disconnect();
            setTimeout(() => {
                document.getElementById('status').textContent = invalidCallback && invalidThreshold ? 'yes' : 'invalid';
            }, 1);
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").nth(1).unwrap().text_content(),
        "yes"
    );
}
