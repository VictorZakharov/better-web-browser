//! FontFaceSet readiness must follow actual resource and renderer publication,
//! not a timer, a capability flag, or a synthetic font's successful header parse.
use super::*;

const AHEM: &[u8] = include_bytes!("../../canvas/fonts/ahem.ttf");

fn pending_font(sources: &str) -> Driver {
    Driver::new(&format!(
        r#"<!doctype html><style>
        @font-face{{font-family:Environment;src:{sources}}}
        #sample{{display:inline-block;font:20px Environment}}
        </style><span id=sample>AAAA</span><p id=status>pending</p><script>
        const set=document.fonts, trace=[];
        const record=value=>{{trace.push(value);status.textContent=trace.join('|');}};
        set.onloading=()=>record('loading:'+set.status);
        set.onloadingdone=event=>record('done:'+event.fontfaces.length+':'+event.isTrusted);
        set.onloadingerror=event=>record('error:'+event.fontfaces.length+':'+event.isTrusted);
        set.ready.then(value=>{{
            const ctx=document.createElement('canvas').getContext('2d');ctx.font='20px Environment';
            record('ready:'+(value===set)+':'+set.status+':'+ctx.measureText('AAAA').width+
                ':'+sample.getBoundingClientRect().width);
        }});
        </script>"#
    ))
}

#[test]
fn css_ready_waits_through_fallback_and_publishes_real_canvas_and_layout_metrics() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = pending_font("url(/missing.ttf),url(/corrupt.ttf),url(/ahem.ttf)");
    let loading = driver.until_text("loading:loading");
    assert!(!painted_text(&loading).contains("ready:"));
    driver.until_request("missing.ttf");
    driver.respond_bytes("missing.ttf", b"missing", "font/ttf", 404);
    driver.until_request("corrupt.ttf");
    driver.respond_bytes("corrupt.ttf", b"not a font", "font/ttf", 200);
    driver.until_request("ahem.ttf");
    // Leave the final candidate pending: the native environment must not busy
    // poll or settle simply because earlier candidates have failed.
    driver.advance();
    driver.until_idle();
    driver.respond_bytes("ahem.ttf", AHEM, "font/ttf", 200);
    let ready = driver.until_text("ready:true:loaded:80:80");
    // Promise and font tasks can be coalesced into one presentation. Do not
    // wait for a second frame if that first frame already contains the event.
    let done = if painted_text(&ready).contains("done:1:true") {
        ready
    } else {
        assert!(
            ready.next_timer_micros.is_some(),
            "font event must schedule a wakeup"
        );
        driver.advance();
        driver.until_text("done:1:true")
    };
    let text = painted_text(&done);
    assert!(
        text.contains("loading:loading|ready:true:loaded:80:80|done:1:true"),
        "{text}"
    );
    assert_eq!(driver.requests.len(), 3, "no duplicate script-side fetch");
    driver.session.shutdown().unwrap();
}

#[test]
fn failed_css_font_releases_ready_without_reporting_a_successful_face() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><style>
        @font-face{font-family:Broken;src:url(/broken.ttf)}
        #sample{font:20px Broken}
        </style><p id=sample>AAAA</p><p id=status>pending</p><script>
        const set=document.fonts,trace=[],face=[...set][0];
        const record=value=>{trace.push(value);status.textContent=trace.join('|');};
        face.loaded.then(()=>record('unexpected-success'),error=>record('face:'+error.name));
        set.ready.then(()=>record('ready:'+set.status));
        set.onloadingdone=event=>record('done:'+event.fontfaces.length);
        set.onloadingerror=event=>record('error:'+(event.fontfaces[0]===face));
        </script>"#,
    );
    driver.until_request("broken.ttf");
    driver.respond_bytes("broken.ttf", b"not a font", "font/ttf", 200);
    let presentation = driver.until_text("error:true");
    let text = painted_text(&presentation);
    assert!(
        text.contains("face:NetworkError|ready:loaded|done:0|error:true"),
        "{text}"
    );
    assert!(!text.contains("unexpected-success"));
    assert_eq!(driver.requests.len(), 1);
    driver.session.shutdown().unwrap();
}

#[test]
fn unused_css_face_does_not_block_initial_ready_or_trigger_a_download() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><style>
        @font-face{font-family:Unused;src:url(/unused.ttf)}
        </style><p id=status>pending</p><script>
        const set=document.fonts,face=[...set][0];
        set.ready.then(value=>status.textContent='ready:'+(value===set)+':'+set.status+':'+face.status);
        </script>"#,
    );
    driver.until_text("ready:true:loaded:unloaded");
    assert!(driver.requests.is_empty());
    driver.session.shutdown().unwrap();
}

#[test]
fn a_font_discovered_after_load_starts_a_new_period_without_retracting_old_ready() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><span id=sample>AAAA</span><p id=status>pending</p><script>
        const set=document.fonts,first=set.ready,trace=[];
        const record=value=>{trace.push(value);status.textContent=trace.join('|');};
        first.then(()=>{
            record('initial-ready');
            const link=document.createElement('link');link.rel='stylesheet';link.href='/late-font.css';
            document.head.appendChild(link);
        });
        set.onloading=()=>{
            record('loading:'+(set.ready!==first));
            first.then(()=>record('old-still-ready'));
            set.ready.then(()=>{
                const ctx=document.createElement('canvas').getContext('2d');ctx.font='20px Late';
                record('late-ready:'+ctx.measureText('AAAA').width);
            });
        };
        set.onloadingdone=event=>record('done:'+event.fontfaces.length);
        </script>"#,
    );
    driver.until_text("initial-ready");
    driver.until_request("late-font.css");
    driver.respond_bytes(
        "late-font.css",
        b"@font-face{font-family:Late;src:url(/late-ahem.ttf)}#sample{font:20px Late}",
        "text/css",
        200,
    );
    driver.until_request("late-ahem.ttf");
    let loading = driver.until_text("old-still-ready");
    assert!(!painted_text(&loading).contains("late-ready:"));
    driver.respond_bytes("late-ahem.ttf", AHEM, "font/ttf", 200);
    let done = driver.until_text("done:1");
    let text = painted_text(&done);
    assert!(
        text.contains("initial-ready|loading:true|old-still-ready|late-ready:80|done:1"),
        "{text}"
    );
    assert_eq!(driver.requests.len(), 2);
    driver.session.shutdown().unwrap();
}

#[test]
fn canvas_only_measurement_and_drawing_load_fonts_but_assignment_alone_does_not() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><style>
        @font-face{font-family:Measured;src:url(/measured.ttf)}
        @font-face{font-family:Filled;src:url(/filled.ttf)}
        @font-face{font-family:Stroked;src:url(/stroked.ttf)}
        @font-face{font-family:Assigned;src:url(/assigned.ttf)}
        </style><canvas id=canvas width=180 height=100></canvas><p id=status>pending</p><script>
        const ctx=canvas.getContext('2d'),set=document.fonts,trace=[];
        const states=()=>Array.from(set,face=>face.status).join(',');
        const record=value=>{trace.push(value);status.textContent=trace.join('|');};
        ctx.font='20px Measured';ctx.measureText('AAAA');ctx.measureText('AAAA');
        ctx.font='20px Filled';ctx.fillText('AAAA',0,20);
        ctx.font='20px Stroked';ctx.strokeText('AAAA',0,60);
        ctx.font='20px Assigned';record('sync:'+states());
        set.ready.then(()=>{
            const widths=['Measured','Filled','Stroked'].map(name=>{
                ctx.font='20px '+name;return ctx.measureText('AAAA').width;
            });
            record('ready:'+states()+':'+widths.join(','));
        });
        set.onloadingdone=event=>record('done:'+event.fontfaces.length);
        </script>"#,
    );
    let initial = driver.until_text("sync:loading,loading,loading,unloaded");
    assert!(!painted_text(&initial).contains("ready:"));
    if initial.next_timer_micros.is_some() {
        driver.advance();
    }
    for url in ["measured.ttf", "filled.ttf", "stroked.ttf"] {
        driver.until_request(url);
        driver.respond_bytes(url, AHEM, "font/ttf", 200);
    }
    let ready = driver.until_text("ready:loaded,loaded,loaded,unloaded:80,80,80");
    let done = if painted_text(&ready).contains("done:3") {
        ready
    } else {
        assert!(ready.next_timer_micros.is_some());
        driver.advance();
        driver.until_text("done:3")
    };
    assert!(painted_text(&done).contains("done:3"));
    assert_eq!(
        driver.requests.len(),
        3,
        "no repeated or assignment-only fetch"
    );
    driver.session.shutdown().unwrap();
}

#[test]
fn shared_font_url_does_not_mark_an_unused_css_face_as_loading_or_loaded() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><style>
        @font-face{font-family:Used;src:url(/shared.ttf)}
        @font-face{font-family:Unused;src:url(/shared.ttf)}
        #sample{font:20px Used}
        </style><span id=sample>AAAA</span><p id=status>pending</p><script>
        const set=document.fonts,trace=[];
        const states=()=>Array.from(set,face=>face.family+':'+face.status).join(',');
        const record=value=>{trace.push(value);status.textContent=trace.join('|');};
        record('initial:'+states());
        set.onloading=()=>record('loading:'+states());
        set.ready.then(()=>record('ready:'+states()+':'+sample.getBoundingClientRect().width));
        set.onloadingdone=event=>record('done:'+event.fontfaces.map(face=>face.family).join(','));
        </script>"#,
    );
    driver.until_request("shared.ttf");
    let initial = driver.until_text("loading:Used:loading,Unused:unloaded");
    assert!(!painted_text(&initial).contains("ready:"));
    driver.respond_bytes("shared.ttf", AHEM, "font/ttf", 200);
    let ready = driver.until_text("ready:Used:loaded,Unused:unloaded:80");
    let done = if painted_text(&ready).contains("done:Used") {
        ready
    } else {
        assert!(ready.next_timer_micros.is_some());
        driver.advance();
        driver.until_text("done:Used")
    };
    assert!(painted_text(&done).contains("done:Used"));
    assert_eq!(driver.requests.len(), 1);
    driver.session.shutdown().unwrap();
}
