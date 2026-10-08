//! Shared bitmap clipping is applied after source sampling and path coverage.
use super::*;

const SOURCE: &str = r#"
const pixel=(c,x,y)=>Array.from(c.getImageData(x,y,1,1).data).join();
for(const kind of ['linear','radial','conic','pattern']) {
    const width=71,height=45;
    const clipped=make(width,height).getContext('2d');
    const reference=make(width,height).getContext('2d');
    for(const c of [clipped,reference]) {c.fillStyle='rgb(11,22,33)';c.fillRect(0,0,width,height);}
    let paint;
    if(kind==='linear') paint=clipped.createLinearGradient(.5,0,50.5,0);
    if(kind==='radial') paint=clipped.createRadialGradient(24.5,20.5,0,24.5,20.5,30);
    if(kind==='conic') paint=clipped.createConicGradient(.25,24.5,20.5);
    if(kind==='pattern') {
        const source=make(3,2),ctx=source.getContext('2d');
        ctx.putImageData(new ImageData(new Uint8ClampedArray([
            255,0,0,255, 0,255,0,128, 0,0,255,255,
            255,255,0,255, 0,255,255,128, 255,0,255,255
        ]),3,2),0,0);
        paint=clipped.createPattern(source,'repeat');
        paint.setTransform({a:2,b:0,c:0,d:2,e:3,f:2});
    } else {
        paint.addColorStop(0,'rgba(255,0,0,.4)');
        paint.addColorStop(.5,'rgba(0,255,0,.8)');paint.addColorStop(1,'blue');
    }
    clipped.save();clipped.beginPath();clipped.rect(13,7,31,23);clipped.clip();
    for(const c of [clipped,reference]) {
        c.translate(4,3);c.globalAlpha=.6;c.fillStyle=paint;c.strokeStyle=paint;
        c.beginPath();c.ellipse(30,18,29,17,.2,0,Math.PI*2);c.fill();
        c.lineWidth=3;c.beginPath();c.moveTo(1,2);c.lineTo(62,34);c.stroke();
    }
    const actual=clipped.getImageData(0,0,width,height).data;
    const expected=reference.getImageData(0,0,width,height).data;
    for(let y=0;y<height;y++) for(let x=0;x<width;x++) for(let channel=0;channel<4;channel++) {
        const i=(y*width+x)*4+channel;
        const value=x>=13&&x<44&&y>=7&&y<30 ? expected[i] : [11,22,33,255][channel];
        if(actual[i]!==value) throw Error(kind+' clip/coverage/stride at '+x+','+y+':'+channel);
    }
    clipped.save();clipped.setTransform(1,0,0,1,0,0);
    clipped.beginPath();clipped.rect(17,11,15,8);clipped.clip();
    clipped.globalAlpha=1;clipped.fillRect(0,0,width,height);clipped.restore();
    reference.setTransform(1,0,0,1,0,0);reference.globalAlpha=1;
    reference.fillRect(0,0,width,height);
    const nested=clipped.getImageData(0,0,width,height).data;
    const repainted=reference.getImageData(0,0,width,height).data;
    for(let y=0;y<height;y++) for(let x=0;x<width;x++) for(let channel=0;channel<4;channel++) {
        const i=(y*width+x)*4+channel;
        if(nested[i] !== (x>=17&&x<32&&y>=11&&y<19 ? repainted[i] : actual[i]))
            throw Error(kind+' nested clip/intersection escaped at '+x+','+y);
    }
    clipped.restore();clipped.fillStyle='red';clipped.fillRect(0,0,width,height);
    if(pixel(clipped,0,0)!=='255,0,0,255') throw Error(kind+' restored clip retained');
    clipped.canvas.width=width;
    if(pixel(clipped,20,20)!=='0,0,0,0') throw Error(kind+' resize retained clip/pixels');
}
"#;

#[test]
fn window_gradient_and_pattern_shaders_preserve_bitmap_clip_coverage_and_reset() {
    let (_, outcome) = execute_html(&format!(
        "<script>const make=(width,height)=>{{const c=document.createElement('canvas');c.width=width;c.height=height;return c;}};{SOURCE}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_gradient_and_pattern_shaders_preserve_bitmap_clip_coverage_and_reset() {
    let source = format!(
        "const make=(width,height)=>new OffscreenCanvas(width,height);{SOURCE};postMessage('passed');"
    );
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/clipped-shaders.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}
