//! Scratch storage is private and never a cache of another canvas's pixels.
use super::*;

const PRIMITIVES: &str = include_str!("../bootstrap/canvas_image_data.js");
const REGION: &str = include_str!("../bootstrap/canvas_region.js");

fn check_private(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<script>(()=>{{{PRIMITIVES}\n{REGION}\n{source}}})();</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn native_region_reuses_only_storage_and_obeys_both_row_strides() {
    check_private(
        r#"
        let allocation;
        for (const [width,height,left,top,right,bottom] of [
            [8,8,0,2,8,6], [16,8,4,1,12,5], [8,16,0,10,8,14]
        ]) {
            const pixels = new Uint8ClampedArray(width*height*4);
            for(let i=0;i<pixels.length;i++) pixels[i]=(i*17+width)%256;
            const expected = new Uint8ClampedArray(pixels);
            const state = {width,height,pixels};
            const success = paintCanvasRegion(state,left,top,right,bottom,region=>{
                if(allocation && allocation!==region.buffer) throw Error('storage was not reused');
                allocation=region.buffer;
                const columns=right-left;
                for(let row=0;row<bottom-top;row++) for(let column=0;column<columns*4;column++) {
                    const source=((row+top)*width+left)*4+column;
                    if(region[row*columns*4+column]!==pixels[source]) throw Error('stale region/stride');
                    expected[source]=region[row*columns*4+column]^127;
                }
                return Uint8Array.from(region,value=>value^127);
            });
            if(!success || !pixels.every((value,index)=>value===expected[index])) throw Error('copyback bounds');
        }
    "#,
    );
}

#[test]
fn rejected_and_throwing_region_paints_leave_the_bitmap_unchanged() {
    check_private(
        r#"
        const state={width:12,height:8,pixels:new Uint8ClampedArray(12*8*4).fill(31)};
        for(const result of [null,new Uint8Array(1)]) {
            if(paintCanvasRegion(state,2,1,10,7,region=>{region.fill(222);return result;})) throw Error('invalid reply');
            if(!state.pixels.every(value=>value===31)) throw Error('partial copyback');
        }
        let threw=false;
        try { paintCanvasRegion(state,2,1,10,7,region=>{region.fill(89);throw Error('expected');}); }
        catch(error) { threw=error.message==='expected'; }
        if(!threw || !state.pixels.every(value=>value===31)) throw Error('exception atomicity');
        if(!paintCanvasRegion(state,2,1,10,7,region=>{
            if(!region.every(value=>value===31)) throw Error('stale failed paint');
            return region;
        })) throw Error('lost scratch after exception');
    "#,
    );
}

#[test]
fn nested_region_paints_cannot_alias_active_storage() {
    check_private(
        r#"
        const state=value=>({width:8,height:8,pixels:new Uint8ClampedArray(256).fill(value)});
        const outer=state(11),inner=state(29);
        paintCanvasRegion(outer,0,0,8,8,active=>{
            active[0]=77;
            paintCanvasRegion(inner,0,0,8,8,nested=>{
                if(nested.buffer===active.buffer || !nested.every(value=>value===29)) throw Error('nested alias');
                nested.fill(31);return nested;
            });
            if(active[0]!==77 || active[1]!==11) throw Error('active overwritten');
            return active;
        });
        if(outer.pixels[0]!==77 || outer.pixels[1]!==11 || !inner.pixels.every(value=>value===31))
            throw Error('nested destination');
    "#,
    );
}

#[test]
fn region_retention_is_bounded_and_does_not_expose_author_buffers() {
    check_private(
        r#"
        const small=takeCanvasRegion(256);releaseCanvasRegion(small);
        if(takeCanvasRegion(256)!==small) throw Error('allocation lost');
        releaseCanvasRegion(new canvasPixelArray(MAX_CANVAS_REGION_SCRATCH_BYTES+4));
        if(canvasRegionScratch!==null) throw Error('oversized allocation retained');
        const state={width:8,height:8,pixels:new Uint8ClampedArray(256)};
        paintCanvasRegion(state,0,0,8,8,region=>{
            if(region.buffer===state.pixels.buffer) throw Error('author buffer retained');
            return region;
        });
    "#,
    );
}

const PUBLIC: &str = r#"
    for(const [width,height,color,expected] of [[64,32,'red','255,0,0,255'],[32,64,'blue','0,0,255,255']]) {
        const c=make(width,height),ctx=c.getContext('2d');
        ctx.fillStyle=color;ctx.fillRect(0,0,width,height);
        if([...ctx.getImageData(0,height-1,1,1).data].join(',')!==expected) throw Error('cross-canvas storage');
        ctx.save();ctx.beginPath();ctx.rect(8,8,16,16);ctx.clip();
        ctx.fillStyle='lime';ctx.beginPath();ctx.rect(0,0,width,height);ctx.fill();ctx.restore();
        if([...ctx.getImageData(12,12,1,1).data].join(',')!=='0,255,0,255') throw Error('clipped fill');
        if([...ctx.getImageData(1,1,1,1).data].join(',')!==expected) throw Error('clip escaped');
        ctx.strokeStyle='black';ctx.lineWidth=2;ctx.beginPath();ctx.rect(4,4,width-8,height-8);ctx.stroke();
        if([...ctx.getImageData(4,16,1,1).data].join(',')!=='0,0,0,255') throw Error('stroke readback');
        c.width=width;
        if(ctx.getImageData(4,16,1,1).data.some(value=>value!==0)) throw Error('resize reused pixels');
    }
"#;

#[test]
fn window_canvas_region_reuse_preserves_immediate_readback_clip_and_resize() {
    let (_, outcome) = execute_html(&format!(
        "<script>const make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};{PUBLIC}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn worker_canvas_region_reuse_preserves_immediate_readback_clip_and_resize() {
    let source = format!("const make=(w,h)=>new OffscreenCanvas(w,h);{PUBLIC};postMessage(true);");
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/region-worker.js",
        &source,
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["true"]);
}
