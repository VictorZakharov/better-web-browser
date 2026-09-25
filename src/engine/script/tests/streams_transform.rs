use super::*;

#[test]
fn readable_cancel_rejects_transform_writer_closed_with_original_reason() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
            (async () => {
                const stream = new TransformStream();
                const reason = new Error('cancelled');
                const cancel = stream.readable.cancel(reason);
                const writer = stream.writable.getWriter();
                let observed = '';
                try { await writer.closed; }
                catch (error) { observed = error === reason ? 'same' : error.message; }
                await cancel;
                document.querySelector('output').textContent = observed;
            })();
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "same"
    );
}
