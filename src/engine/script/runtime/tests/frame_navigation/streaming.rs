use super::*;
use crate::engine::script::ScriptFetchEvent;

fn begin() -> (dom::Dom, ScriptRuntime, u32) {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.events=[];const f=document.createElement('iframe');f.src='/stream';
        f.onload=()=>events.push('frame-load');document.body.append(f);
    </script>"#,
    );
    let result = runtime.advance_time(Duration::ZERO, 1);
    let id = result
        .fetch_actions
        .into_iter()
        .find_map(|action| match action {
            ScriptFetchAction::Start { id, .. } => Some(id),
            _ => None,
        })
        .unwrap();
    (dom, runtime, id)
}
fn deliver(runtime: &mut ScriptRuntime, id: u32, event: ScriptFetchEvent) {
    let result = runtime.deliver_fetch_event_with_loader(id, event, None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
}
fn chunk(runtime: &mut ScriptRuntime, id: u32, bytes: &[u8]) {
    deliver(runtime, id, ScriptFetchEvent::Chunk(bytes.to_vec()));
    drain(runtime);
}

#[test]
fn child_parser_runs_before_body_eof_without_finishing_load() {
    let (dom, mut runtime, id) = begin();
    deliver(
        &mut runtime,
        id,
        ScriptFetchEvent::Head(Ok(response("https://example.com/stream", ""))),
    );
    chunk(
        &mut runtime,
        id,
        b"<body><script>parent.events.push('early')</script><p>first",
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(events.join(',')!=='early' || f.contentDocument.readyState!=='loading') throw Error(events);",
    );
    chunk(
        &mut runtime,
        id,
        b"</p><script>parent.events.push('late')</script>",
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(events.join(',')!=='early,late') throw Error(events);",
    );
    deliver(&mut runtime, id, ScriptFetchEvent::End);
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(events.join(',')!=='early,late,frame-load') throw Error(events);",
    );
}

#[test]
fn child_encoding_restart_preserves_window_proxy_and_continues_stream() {
    let (dom, mut runtime, id) = begin();
    deliver(
        &mut runtime,
        id,
        ScriptFetchEvent::Head(Ok(response("https://example.com/stream", ""))),
    );
    evaluate(
        &mut runtime,
        &dom,
        "window.proxy=f.contentWindow;window.oldDocument=f.contentDocument;",
    );
    chunk(
        &mut runtime,
        id,
        b"<meta charset=windows-1252><body><p id=a>caf\xe9</p>",
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(f.contentWindow!==proxy || f.contentDocument===oldDocument || f.contentDocument.characterSet!=='windows-1252' || f.contentDocument.getElementById('a').textContent!=='caf\u{00e9}') throw Error('restart');",
    );
    chunk(
        &mut runtime,
        id,
        b"<script>parent.events.push(document.getElementById('a').textContent)</script>",
    );
    deliver(&mut runtime, id, ScriptFetchEvent::End);
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if(events.join(',')!=='caf\u{00e9},frame-load') throw Error(events);",
    );
}

#[test]
fn child_transport_charset_wins_over_meta_and_removal_aborts_live_stream() {
    let (dom, mut runtime, id) = begin();
    let mut head = response("https://example.com/stream", "");
    head.headers
        .append("content-type", "text/html; charset=windows-1252")
        .unwrap();
    deliver(&mut runtime, id, ScriptFetchEvent::Head(Ok(head)));
    chunk(
        &mut runtime,
        id,
        b"<meta charset=utf-8><body><p id=a>caf\xe9</p>",
    );
    evaluate(
        &mut runtime,
        &dom,
        "if(f.contentDocument.characterSet!=='windows-1252' || f.contentDocument.getElementById('a').textContent!=='caf\u{00e9}') throw Error('transport encoding');",
    );
    let removed = evaluate(&mut runtime, &dom, "f.remove();");
    assert!(removed.fetch_actions.iter().any(
        |action| matches!(action, ScriptFetchAction::Abort { id: cancelled } if *cancelled == id)
    ));
    chunk(
        &mut runtime,
        id,
        b"<script>parent.events.push('BAD')</script>",
    );
    evaluate(&mut runtime, &dom, "if(events.length) throw Error(events);");
}
