use super::*;

#[test]
fn font_face_set_keeps_css_members_until_their_live_owner_is_removed() {
    let (dom, outcome) = execute_html(
        r#"<style id=sheet>@font-face{font-family:Example;src:url(/font.woff)}</style>
        <output id=probe></output><script>
        const set=document.fonts,face=[...set][0];
        const scriptFace=new FontFace('Script','url(/script.woff)');
        set.add(scriptFace);
        const checks=[set.size===2,set.has(face),set.delete(face)===false];
        checks.push(set.add(face)===set);
        set.clear();checks.push(set.size===1,set.has(face),!set.has(scriptFace));
        sheet.remove();checks.push(!set.has(face),set.size===0);
        checks.push(set.add(face)===set,set.has(face),set.delete(face),set.size===0);
        probe.textContent=checks.every(Boolean);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn css_font_members_follow_edits_disable_and_reconnection() {
    let (dom, outcome) = execute_html(
        r#"<style id=sheet>@font-face{font-family:First;src:url(/a.woff)}</style>
        <output id=probe></output><script>
        const sheet=document.getElementById('sheet');
        const set=document.fonts,first=[...set][0],checks=[first.family==='First'];
        sheet.textContent='@font-face{font-family:Second;src:url(/b.woff)}';
        const second=[...set][0];checks.push(!set.has(first),second.family==='Second',set.size===1);
        sheet.disabled=true;checks.push(set.size===0);
        sheet.disabled=false;checks.push(set.size===1);
        sheet.remove();checks.push(set.size===0);
        document.head.appendChild(sheet);checks.push(set.size===1);
        probe.textContent=checks.every(Boolean);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn font_set_empty_quoted_family_is_valid_and_does_not_match_a_named_face() {
    let (dom, outcome) = execute_html(
        r#"<output id=probe>pending</output><script>
        document.fonts.add(new FontFace('Example','url(/font.woff)'));
        document.fonts.load('1px ""').then(faces=>probe.textContent=
            Array.isArray(faces)&&faces.length===0&&document.fonts.check('1px ""'));
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn css_connected_unicode_ranges_participate_in_real_font_queries() {
    let (dom, outcome) = execute_html(
        r#"
      <style>@font-face{font-family:Subset;src:url(/ahem.ttf);unicode-range:U+41}</style>
      <output id=probe></output><script>
      const face=[...document.fonts][0],set=document.fonts;
      probe.textContent=face.unicodeRange==='U+41'&&!set.check('20px Subset','A')
        &&set.check('20px Subset','B');
      </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn renderer_css_font_snapshot_preserves_range_restrictions_in_canvas() {
    let dom = dom::parse_with_scripting(
        r#"
      <style>@font-face{font-family:CssAhem;src:url(/ahem.ttf);unicode-range:U+41}</style>
      <output id=probe></output><script>
      const ctx=new OffscreenCanvas(80,60).getContext('2d');ctx.font='20px CssAhem';
      const face=[...document.fonts][0];
      probe.textContent=face.status==='loaded'&&face.unicodeRange==='U+41'
        &&ctx.measureText('A').width===20&&ctx.measureText('B').width!==20;
      </script>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let face = crate::engine::font::WebFontFace {
        family: "CssAhem".into(),
        weight: 400,
        weight_min: 400.0,
        weight_max: 400.0,
        features: Default::default(),
        italic: false,
        url: "https://example.com/ahem.ttf".into(),
        fallback_urls: Vec::new(),
        unicode_range: "U+41".into(),
    };
    let font = crate::engine::font::decode_web_font(
        &face,
        include_bytes!("../../../../tests/canvas/fonts/ahem.ttf"),
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
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}
