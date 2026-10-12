use super::*;
const CASES: &str = include_str!("../../../../tests/canvas/text-shadow-regions.js");
const OPERATORS: [&str; 10] = [
    "source-over",
    "destination-over",
    "source-atop",
    "xor",
    "copy",
    "source-in",
    "destination-in",
    "multiply",
    "screen",
    "lighter",
];
const WINDOW_MAKE: &str =
    "(w,h)=>{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}";
const WORKER_MAKE: &str = "(w,h)=>new OffscreenCanvas(w,h)";

#[test]
fn shadow_text_regions_match_dense_pixels_and_survive_repeated_atlas_repaints() {
    for operators in OPERATORS.chunks(2) {
        let encoded = serde_json::to_string(operators).unwrap();
        check_window(&format!(
            "if(testCanvasTextShadowRegions({WINDOW_MAKE},{encoded})!==36)throw Error('missing cases');"
        ));
    }
}

fn check_window(code: &str) {
    let (_, outcome) = execute_html(&format!("<script>{CASES}\n{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn workers_share_shadow_text_regions_and_bitmap_reset_contract() {
    for operators in OPERATORS.chunks(2) {
        let encoded = serde_json::to_string(operators).unwrap();
        check_worker(&format!(
            "if(testCanvasTextShadowRegions({WORKER_MAKE},{encoded})!==36)throw Error('missing cases');"
        ));
    }
}

fn check_worker(code: &str) {
    let (runtime, initial) = WorkerRuntime::start(
        "https://example.test/text-shadow.js",
        &format!("{CASES}\n{code}postMessage('passed');"),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert!(runtime.is_some());
    assert_eq!(initial.messages, ["\"passed\""]);
}

#[test]
fn shadow_text_large_atlas_repaint_and_reset_keep_pixels_in_window_and_worker() {
    check_window(&format!(
        "if(testCanvasTextShadowAtlas({WINDOW_MAKE})!==2)throw Error('atlas cases');"
    ));
    check_worker(&format!(
        "if(testCanvasTextShadowAtlas({WORKER_MAKE})!==2)throw Error('atlas cases');"
    ));
}

#[test]
fn compact_decline_does_not_repeat_text_processing_or_idl_conversions() {
    let (_, outcome) = execute_html(
        r#"<script>
        const canvas=document.createElement('canvas');canvas.width=96;canvas.height=64;
        const c=canvas.getContext('2d');c.font='18px Arial';
        c.shadowColor='black';c.shadowBlur=4;
        const replace=String.prototype.replace,counts=[];
        let processed=0,converted=0;
        String.prototype.replace=function(...args){if(String(this)==='counter')processed++;return replace.apply(this,args)};
        try{
            for(const mode of ['compact','gradient','singular']){
                c.resetTransform();c.fillStyle='red';
                if(mode==='gradient'){
                    const g=c.createLinearGradient(0,0,96,0);g.addColorStop(0,'red');g.addColorStop(1,'red');c.fillStyle=g;
                }
                if(mode==='singular')c.scale(0,1);
                processed=0;converted=0;
                c.fillText({toString(){converted++;return 'counter'}},5,30);
                if(converted!==1)throw Error('repeated IDL string conversion');
                counts.push(processed);
            }
        }finally{String.prototype.replace=replace;}
        if(counts[0]!==counts[1]||counts[0]!==counts[2])throw Error('fallback repeated text processing '+counts);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
