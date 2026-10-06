use super::*;
use base64::Engine;

const AHEM: &[u8] = include_bytes!("../../../../tests/canvas/fonts/ahem.ttf");
const WEB_FONTS: &str = include_str!("../../../../tests/canvas/web-fonts.js");

#[test]
fn font_unicode_ranges_control_real_canvas_selection_and_queries() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let source = include_str!("../../../../tests/canvas/font-unicode-ranges.js");
    let (dom, outcome) = execute_html(&format!(
        "<output id=probe>pending</output><script>{source}\nconst bytes=Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0));testFontUnicodeRanges(bytes).then(result=>probe.textContent=result,error=>probe.textContent=String(error));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn font_face_set_queries_reuse_css_family_lists_and_weight_matching() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let matching = include_str!("../../../../tests/canvas/font-face-matching.js");
    let (dom, outcome) = execute_html(&format!(
        "<output id=probe>pending</output><script>{matching}\nconst bytes=Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0));testFontFaceMatching(bytes).then(result=>probe.textContent=result,error=>probe.textContent=String(error));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn font_face_binding_conversions_preserve_real_font_bytes_and_observable_order() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let bindings = include_str!("../../../../tests/canvas/font-face-bindings.js");
    let (dom, outcome) = execute_html(&format!(
        "<output id=probe>pending</output><script>{bindings}\nconst bytes=Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0));testFontFaceBindings(bytes).then(result=>probe.textContent=result,error=>probe.textContent=String(error));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn canvas_uses_loaded_document_font_bytes_for_real_advances_and_pixels() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let (dom, outcome) = execute_html(&format!(
        "<output id=probe>pending</output><script>{WEB_FONTS}\nconst bytes=Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0));testCanvasWebFonts(bytes).then(result=>probe.textContent=result,error=>probe.textContent=String(error));</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn document_canvas_fonts_do_not_leak_into_a_later_document_on_the_same_thread() {
    let encoded = base64::engine::general_purpose::STANDARD.encode(AHEM);
    let (_, outcome) = execute_html(&format!(
        "<script>const face=new FontFace('CanvasIsolationAhem',Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0)));face.load().then(()=>{{document.fonts.add(face);const c=document.createElement('canvas').getContext('2d');c.font='20px CanvasIsolationAhem';if(Math.abs(c.measureText('ABC').width-60)>0.001)throw Error('font not installed');}});</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let (_, outcome) = execute_html(
        "<script>const c=document.createElement('canvas').getContext('2d');c.font='20px CanvasIsolationAhem';if(Math.abs(c.measureText('ABC').width-60)<0.1)throw Error('foreign document font leaked');</script>",
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
