use super::*;
use crate::engine::Page;

#[cfg(windows)]
#[test]
fn font_face_feature_settings_change_real_window_glyphs_and_invalidate_caches() {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .encode(crate::engine::font::test_features::font_bytes());
    let source = include_str!("../../../../tests/canvas/font-feature-settings.js");
    let (dom, outcome) = execute_html(&format!(
        "<output>pending</output><script>{source}\nconst bytes=Uint8Array.from(atob('{bytes}'),c=>c.charCodeAt(0));\
         testFontFeatureSettings(bytes,document.fonts).then(v=>document.querySelector('output').textContent=v,\
         e=>document.querySelector('output').textContent=String(e));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[cfg(windows)]
#[test]
fn font_face_feature_settings_change_real_worker_glyphs_and_invalidate_caches() {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .encode(crate::engine::font::test_features::font_bytes());
    let source = include_str!("../../../../tests/canvas/font-feature-settings.js");
    let (runtime, mut outcome) = WorkerRuntime::start(
        "https://example.test/features.js",
        &format!(
            "{source}\nconst bytes=Uint8Array.from(atob('{bytes}'),c=>c.charCodeAt(0));\
         testFontFeatureSettings(bytes,fonts).then(postMessage,e=>postMessage(String(e)));"
        ),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected request {url}"))),
    );
    let mut runtime = runtime.unwrap();
    for _ in 0..64 {
        if runtime.next_timer_delay() != Some(Duration::ZERO) {
            break;
        }
        let turn = runtime.advance_time(Duration::ZERO, 1);
        outcome.errors.extend(turn.errors);
        outcome.messages.extend(turn.messages);
    }
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}

#[test]
fn css_font_features_and_kerning_inherit_serialize_and_validate_the_same_grammar() {
    let (dom, outcome) = execute_html(
        r#"<style>
        #parent {font-feature-settings:'liga' off,'kern' on,'liga' 3; font-kerning:none}
        #child {font-feature-settings:'invalid';font-kerning:bogus}
    </style><body><div id=parent><span id=child>x</span></div><output></output><script>
        const assert=(v,m)=>{if(!v)throw Error(m)};
        const child=document.querySelector('#child'), css=getComputedStyle(child);
        for(const name of ['font-feature-settings','fontFeatureSettings','font-kerning','fontKerning',
            'font-variant-ligatures','fontVariantLigatures','font-variant-numeric','fontVariantNumeric'])
            assert(name in css,'supported computed attribute '+name);
        for(const name of ['not-a-css-property','notACssProperty','--custom',Symbol('unknown')])
            assert(!(name in css)&&css[name]===undefined,'unknown attribute remains absent');
        assert(css.getPropertyValue('font-feature-settings')==='"kern", "liga" 3','computed normalization');
        assert(css.getPropertyValue('font-kerning')==='none','inherited kerning');
        assert(CSS.supports('font-feature-settings',String.raw`'k\65rn' off`),'CSS escapes');
        assert(!CSS.supports('font-feature-settings',"'liga',invalid"),'atomic rejection');
        assert(CSS.supports('font-kerning','normal')&&!CSS.supports('font-kerning','normal none'),'kerning grammar');
        child.style.setProperty('font-feature-settings',"'liga' off");
        child.style.setProperty('font-feature-settings',"'liga' -1");
        assert(getComputedStyle(child).getPropertyValue('font-feature-settings')==='"liga" 0','invalid assignment preserves previous');
        child.style.setProperty('font-feature-settings','initial');
        child.style.setProperty('font-kerning','initial');
        assert(getComputedStyle(child).getPropertyValue('font-feature-settings')==='normal','initial features');
        assert(getComputedStyle(child).getPropertyValue('font-kerning')==='auto','initial kerning');
        child.style.setProperty('font-feature-settings','unset');
        child.style.setProperty('font-kerning','unset');
        assert(getComputedStyle(child).getPropertyValue('font-feature-settings')==='"kern", "liga" 3','unset inherits features');
        assert(getComputedStyle(child).getPropertyValue('font-kerning')==='none','unset inherits kerning');
        child.style.setProperty('font','20px Arial');
        assert(getComputedStyle(child).getPropertyValue('font-feature-settings')==='normal','font shorthand resets features');
        assert(getComputedStyle(child).getPropertyValue('font-kerning')==='auto','font shorthand resets kerning');
        document.querySelector('output').textContent='passed';
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn font_feature_mutation_invalidates_used_font_specs_not_only_cssom_strings() {
    let page = Page::parse(
        "<style>div{font-feature-settings:'liga' off;font-kerning:none}</style>\
        <div id=p>x</div>",
        "https://example.test/",
    );
    let node = page.dom.elements_named("div").next().unwrap();
    let before = page.style_for_viewport(800.0, 600.0).get(&node).clone();
    node.set_attr(
        "style",
        "font-feature-settings:'liga' on;font-kerning:normal",
    );
    let after = page.style_for_viewport(800.0, 600.0).get(&node).clone();
    assert_ne!(before.font_features, after.font_features);
    assert_ne!(before.font_kerning, after.font_kerning);
    assert!(!before.layout_equivalent(&after));
}
