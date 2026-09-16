use super::*;

#[test]
fn document_write_preserves_one_tokenizer_stream_and_url_serialization() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body>
        <div><a href='?notin=&notin;&not;&;& &'>Link</a><p>Text: &notin;&not;</p></div>
        <script>
            const markup = "<div><a href='?notin=&notin;&not;&;& &'>Link</a><p>Text: &notin;&not;</p></div>";
            // This helper executes a completed DOM, without a network parser. Use an
            // explicit inert stream; active insertion is covered by live_runtime.
            window.writtenDocument = document.implementation.createHTMLDocument();
            writtenDocument.open();
            for (let index = 0; index < markup.length; index++) writtenDocument.write(markup.charAt(index));
            writtenDocument.close();
        </script>
        <p id="status">no</p><script>
            const divs = writtenDocument.getElementsByTagName('div');
            const writtenHref = divs[0].firstChild.href;
            const query = writtenHref.substring(writtenHref.indexOf('?'));
            if (divs.length === 1 && divs[0].childNodes.length === 2 &&
                query === '?notin=%E2%88%89%C2%AC&;&%20&' &&
                divs[0].lastChild.textContent === 'Text: \u2209\u00AC') {
                document.getElementById('status').textContent = 'yes';
            }
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("p")
            .find(|node| node.attr("id").as_deref() == Some("status"))
            .unwrap()
            .text_content(),
        "yes"
    );
}
