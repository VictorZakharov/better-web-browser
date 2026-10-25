//! HTML "navigate to an XML document": XML responses become XML Documents in the frame.
use super::*;
use crate::engine::script::ScriptFetchEvent;

/// Navigates a new iframe to an XML response with `content_type` and `body`.
fn load_xml(content_type: &str, body: &[u8]) -> (dom::Dom, ScriptRuntime) {
    load_into(
        "f=document.createElement('iframe');f.src='/doc'",
        content_type,
        body,
    )
}

fn load_into(create: &str, content_type: &str, body: &[u8]) -> (dom::Dom, ScriptRuntime) {
    serve(create, 200, content_type, body)
}

/// Runs `create` to assign the navigable container `f`, connects it and serves `/doc`.
pub(super) fn serve(
    create: &str,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> (dom::Dom, ScriptRuntime) {
    let (dom, mut runtime) = start(&format!(
        "<body><script>window.events=[];window.{create};
        f.onload=()=>events.push('frame-load');document.body.append(f);</script>"
    ));
    let id = runtime
        .advance_time(Duration::ZERO, 1)
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    let mut head = response("https://example.com/doc", "");
    head.status = status;
    head.headers.append("content-type", content_type).unwrap();
    for event in [
        ScriptFetchEvent::Head(Ok(head)),
        ScriptFetchEvent::Chunk(body.to_vec()),
        ScriptFetchEvent::End,
    ] {
        let result = runtime.deliver_fetch_event_with_loader(id, event, None);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        drain(&mut runtime);
    }
    (dom, runtime)
}

#[test]
fn svg_response_becomes_an_svg_document_and_fires_load() {
    let (dom, mut runtime) = load_xml(
        "image/svg+xml",
        br#"<svg xmlns="http://www.w3.org/2000/svg"><text>X</text></svg>"#,
    );
    evaluate(
        &mut runtime,
        &dom,
        r#"const d=f.contentDocument;
        if(events.join()!=='frame-load' || d.contentType!=='image/svg+xml'
            || d.documentElement.namespaceURI!=='http://www.w3.org/2000/svg'
            || d.documentElement.localName!=='svg' || d.getElementsByTagName('text').length!==1
            || d.createElement('Text').localName!=='Text')
            throw Error([events,d.contentType,d.documentElement.localName]);"#,
    );
}

#[test]
fn object_data_document_is_a_content_navigable_with_get_svg_document() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#;
    let (dom, mut runtime) = load_into(
        "f=document.createElement('object');f.data='/doc'",
        "image/svg+xml",
        svg,
    );
    evaluate(
        &mut runtime,
        &dom,
        r#"const d=f.contentDocument;
        if(events.join()!=='frame-load' || !d || f.getSVGDocument()!==d
            || f.contentWindow.document!==d || d.documentElement.localName!=='svg'
            || document.createElement('object').contentDocument!==null)
            throw Error([events,d]);"#,
    );
    // getSVGDocument() exposes only documents loaded from image/svg+xml.
    let (dom, mut runtime) = load_xml("application/xml", svg);
    evaluate(
        &mut runtime,
        &dom,
        "if(!f.contentDocument || f.getSVGDocument()!==null) throw Error('svg type');",
    );
}

#[test]
fn only_well_formed_html_namespace_scripts_run_in_xml_documents() {
    let script =
        br#"<script type="text/javascript">parent.events.push(document.body.localName)</script>"#;
    let wrap = |namespace: &str, tail: &[u8]| {
        let mut body = format!(r#"<html xmlns="{namespace}"><body>"#).into_bytes();
        body.extend_from_slice(script);
        body.extend_from_slice(tail);
        body
    };
    let xhtml = "http://www.w3.org/1999/xhtml";
    for (body, expected) in [
        (wrap(xhtml, b"</body></html>"), "body,frame-load"),
        // A well-formedness error yields a parser-error document whose scripts never run.
        (
            wrap(xhtml, b"<strong/></strong></body></html>"),
            "frame-load",
        ),
        (
            wrap("http://www.w3.org/1999/xhtml#", b"</body></html>"),
            "frame-load",
        ),
    ] {
        let (dom, mut runtime) = load_xml("text/xml", &body);
        evaluate(
            &mut runtime,
            &dom,
            &format!("if(events.join()!=='{expected}') throw Error(events);"),
        );
    }
    // Invalid UTF-8 is a fatal XML error, not a replacement character.
    let (dom, mut runtime) = load_xml("application/xml", b"<root>\xe9<test/></root>");
    evaluate(
        &mut runtime,
        &dom,
        "if(f.contentDocument.documentElement.localName!=='parsererror') throw Error('encoding');",
    );
}
