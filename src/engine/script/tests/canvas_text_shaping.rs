use super::*;

const SHAPING: &str = include_str!("../../../../tests/canvas/text-shaping.js");

#[test]
fn canvas_kerning_and_base_direction_reach_the_real_font_shaper() {
    let (_, outcome) = execute_html(&format!(
        "<script>{SHAPING}\nconst make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};testCanvasTextShaping(make);</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_kerning_and_bidi_order_match_its_platform_state() {
    let source = format!(
        "{SHAPING}\nconst make=(w,h)=>new OffscreenCanvas(w,h);testCanvasTextShaping(make);postMessage('passed');"
    );
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/canvas-text-shaping.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected worker fetch {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}

#[test]
fn canvas_text_reads_root_units_and_inherited_direction_without_author_getters() {
    let (_, outcome) = execute_html(
        r#"
        <html lang="sr" style="font-size:32px"><body dir="rtl">
        <canvas id="surface" width="420" height="100"></canvas><script>
        const canvas=document.getElementById('surface'), c=canvas.getContext('2d');
        c.font='20px Arial';c.textAlign='left';c.direction='ltr';
        const base=c.measureText('ABC').width;
        c.letterSpacing='1rem';
        if(Math.abs(c.measureText('ABC').width-base-96)>0.03)throw Error('actual root font size');
        document.documentElement.style.fontSize='48px';
        if(Math.abs(c.measureText('ABC').width-base-144)>0.03)throw Error('root invalidation');
        c.letterSpacing='0px';c.direction='inherit';
        c.fillText('ABC \u05d0\u05d1\u05d2',10,40);
        const inherited=Array.from(c.getImageData(0,0,420,100).data);
        c.clearRect(0,0,420,100);c.direction='rtl';c.fillText('ABC \u05d0\u05d1\u05d2',10,40);
        const explicit=Array.from(c.getImageData(0,0,420,100).data);
        if(inherited.some((v,i)=>v!==explicit[i]))throw Error('computed inherited direction');
        for(const name of ['ownerDocument','parentElement','getAttribute'])
            Object.defineProperty(canvas,name,{get(){throw Error('author '+name);}});
        globalThis.getComputedStyle=()=>{throw Error('author computed style');};
        Object.defineProperty(globalThis,'innerWidth',{get(){throw Error('author viewport');}});
        c.direction='inherit';c.lang='inherit';c.letterSpacing='1rem';
        if(Math.abs(c.measureText('ABC').width-base-144)>0.03)throw Error('private environment');
        </script></body></html>
    "#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
