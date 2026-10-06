use super::network::{pending_runtime, test_response};
use super::*;
use crate::engine::script::ScriptFetchEvent;

#[test]
fn font_face_set_exposes_real_setlike_membership_with_an_unavailable_local_source() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=status></div><script>
            const face = new FontFace('Example', 'local("Unavailable")');
            const set = document.fonts;
            set.add(face);
            document.getElementById('status').textContent = [
                face.status, set.size, set.has(face), [...set].length,
                set.check('12px Example'), typeof set.ready.then,
                document.fonts === set, set.delete(face), set.size
            ].join('|');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "unloaded|1|true|1|false|function|true|true|0"
    );
    assert!(
        outcome
            .font_actions
            .iter()
            .any(|action| matches!(action, ScriptFontAction::Remove { .. }))
    );
}

#[test]
fn buffer_backed_font_face_decodes_before_reporting_loaded() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=status>pending</div><script>
            const bytes = new Uint8Array(76);
            bytes.set([119,79,70,70, 0,1,0,0, 0,0,0,76, 0,1,0,0]);
            bytes.set([0,0,0,40], 16);
            bytes.set([104,101,97,100, 0,0,0,64, 0,0,0,12, 0,0,0,12], 44);
            const face = new FontFace('Fixture', bytes, {weight: '600'});
            document.fonts.add(face);
            face.loaded.then(() => {
                document.getElementById('status').textContent =
                    face.status + '|' + document.fonts.check('12px Fixture');
            });
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "loaded|true"
    );
    assert!(outcome.font_actions.iter().any(|action| matches!(
        action, ScriptFontAction::Add { font, .. }
            if font.family == "Fixture" && font.weight == 600 && font.script_source_id.is_some()
    )));
}

#[test]
fn url_backed_face_waits_for_fetch_then_registers_decoded_bytes() {
    let (dom, mut runtime, id) = pending_runtime(
        "const face = new FontFace('Remote', 'url(/font.woff)');\
         document.fonts.add(face);\
         face.load().then(() => document.querySelector('div').textContent='loaded',\
             () => document.querySelector('div').textContent='error');",
    );
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "pending"
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(test_response(b""))),
        None,
    );
    let bytes = synthetic_woff();
    runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::Chunk(bytes), None);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "pending"
    );
    let delivered = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "pending"
    );
    let outcome = runtime.advance_time(Duration::ZERO, 16);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "loaded"
    );
    assert!(outcome.font_actions.iter().any(|action| matches!(
        action, ScriptFontAction::Add { font, .. } if font.family == "Remote"
    )));
}

#[test]
fn failed_url_face_rejects_load_and_set_ready_still_settles() {
    let (dom, mut runtime, id) = pending_runtime(
        "const face = new FontFace('Broken', 'url(/broken.woff)');\
         const set = document.fonts; set.add(face);\
         let rejected = false;\
         face.load().catch(() => { rejected = face.status === 'error'; });\
         set.ready.then(() => { document.querySelector('div').textContent =\
             rejected && set.status === 'loaded' && !set.check('12px Broken')\
                 ? 'failed and settled' : 'incorrect state'; });",
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Head(Ok(test_response(b""))),
        None,
    );
    runtime.deliver_fetch_event_with_loader(
        id,
        ScriptFetchEvent::Chunk(b"not a font".to_vec()),
        None,
    );
    let delivered = runtime.deliver_fetch_event_with_loader(id, ScriptFetchEvent::End, None);
    assert!(delivered.errors.is_empty(), "{:?}", delivered.errors);
    let outcome = runtime.advance_time(Duration::ZERO, 16);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "failed and settled"
    );
    assert!(
        !outcome
            .font_actions
            .iter()
            .any(|action| matches!(action, ScriptFontAction::Add { .. }))
    );
}

fn synthetic_woff() -> Vec<u8> {
    let mut bytes = vec![0; 76];
    bytes[..4].copy_from_slice(b"wOFF");
    bytes[4..8].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
    bytes[8..12].copy_from_slice(&76_u32.to_be_bytes());
    bytes[12..14].copy_from_slice(&1_u16.to_be_bytes());
    bytes[16..20].copy_from_slice(&40_u32.to_be_bytes());
    bytes[44..48].copy_from_slice(b"head");
    bytes[48..52].copy_from_slice(&64_u32.to_be_bytes());
    bytes[52..56].copy_from_slice(&12_u32.to_be_bytes());
    bytes[56..60].copy_from_slice(&12_u32.to_be_bytes());
    bytes
}

#[test]
fn css_connected_faces_appear_in_document_fonts_but_cannot_be_deleted() {
    let (dom, outcome) = super::cssom::execute_html_with_stylesheets(
        "<link rel=stylesheet href=/assets/fonts.css><body><div id=status></div><script>\
         const set=document.fonts, face=[...set][0];\
         document.getElementById('status').textContent=[set.size,face.family,face.status,\
           set.check('12px Fixture'),set.delete(face),set.size].join('|');\
         </script>",
        vec![(
            "https://example.com/assets/fonts.css".into(),
            "@font-face{font-family:Fixture;src:url(./fixture.woff) format('woff')}".into(),
        )],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "1|Fixture|unloaded|false|false|1"
    );
}

#[test]
fn css_connected_face_reports_loaded_after_renderer_font_installation() {
    let dom = dom::parse_with_scripting(
        "<link rel=stylesheet href=/fonts.css><body><div id=status></div><script>\
         const face=[...document.fonts][0];\
         document.getElementById('status').textContent=[face.status,\
           document.fonts.check('12px Fixture')].join('|');</script>",
        true,
    );
    let source_url = "https://example.com/fixture.woff";
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    runtime.set_document_stylesheets(&[crate::engine::css::StylesheetSource::linked(
        "https://example.com/fonts.css",
        "@font-face{font-family:Fixture;src:url(fixture.woff)}".into(),
    )]);
    let font = crate::engine::font::decode_web_font(
        &crate::engine::font::WebFontFace {
            family: "Fixture".into(),
            weight: 400,
            weight_min: 400.0,
            weight_max: 400.0,
            features: Default::default(),
            italic: false,
            url: source_url.into(),
            fallback_urls: Vec::new(),
            unicode_range: "U+0-10FFFF".into(),
        },
        &synthetic_woff(),
    )
    .unwrap();
    runtime.set_loaded_font_urls(&[font]);
    let script = dom.elements_named("script").next().unwrap();
    let outcome = runtime.execute_initial(&[ScriptInput {
        node: script.clone(),
        source_url: "https://example.com/#inline".into(),
        code: script.text_content(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: true,
    }]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "loaded|true"
    );
    assert!(outcome.fetch_actions.is_empty());
    assert!(outcome.font_actions.is_empty());
}

#[test]
fn ready_settles_before_done_event_and_second_load_creates_a_new_promise() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=status>pending</div><script>
            const bytes = new Uint8Array(76);
            bytes.set([119,79,70,70, 0,1,0,0, 0,0,0,76, 0,1,0,0]);
            bytes.set([0,0,0,40], 16);
            bytes.set([104,101,97,100, 0,0,0,64, 0,0,0,12, 0,0,0,12], 44);
            const set = document.fonts;
            const first = new FontFace('First', bytes);
            set.add(first);
            first.load();
            // Buffer loading starts on a font task. Wait for the loading event
            // before capturing its pending ready promise.
            let reused = true;
            let firstReady;
            set.addEventListener('loading', () => { firstReady ??= set.ready; });
            first.loaded.then(() => {
                const second = new FontFace('Second', bytes);
                set.add(second);
                second.load();
                second.loaded.then(() => {
                    reused = set.ready === firstReady;
                    document.getElementById('status').textContent = String(
                        !reused && set.check('12px First') && set.check('12px Second'));
                });
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true"
    );
}
