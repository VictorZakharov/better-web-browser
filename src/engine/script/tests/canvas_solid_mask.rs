use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=64 height=64></canvas><canvas width=64 height=64></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn batched_solid_fill_and_stroke_match_scalar_clipped_arithmetic() {
    run(r#"
    const contexts=Array.from(document.querySelectorAll('canvas'),n=>n.getContext('2d'));
    const [fast,scalar]=contexts;
    scalar.beginPath();scalar.rect(0,0,64,64);scalar.clip();
    const p=new Path2D();p.moveTo(4.25,5.5);p.lineTo(55.5,16.25);p.lineTo(30.75,57.25);p.closePath();
    const assertEqual=()=>{
        const a=fast.getImageData(0,0,64,64).data,b=scalar.getImageData(0,0,64,64).data;
        for(let i=0;i<a.length;i++)if(a[i]!==b[i])throw Error('batch mismatch at '+i+': '+a[i]+' '+b[i]);
    };
    for(const opacity of [1,.5,.123,0])for(const color of ['red','#123456','rgba(10,220,80,.5)','transparent']){
        for(const c of contexts){
            c.globalAlpha=1;c.fillStyle='rgba(0,0,255,.5)';c.fillRect(0,0,64,64);
            let paint=color;
            // More than 256 stops forces the independent scalar shader path.
            if(c===scalar){const g=c.createLinearGradient(0,0,64,0);for(let i=0;i<=256;i++)g.addColorStop(i/256,color);paint=g;}
            c.globalAlpha=opacity;c.fillStyle=paint;c.strokeStyle=paint;
            c.lineWidth=3.5;c.lineCap='round';c.lineJoin='bevel';c.setLineDash([5,2,1,2]);
            c.fill(p);c.stroke(p);
        }
        assertEqual();
    }
    "#);
}

#[test]
fn batched_region_preserves_pixels_outside_shape_and_current_destination() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();
    p.rect(8,8,40,40);const pixel=(x,y)=>Array.from(c.getImageData(x,y,1,1).data).join();
    c.fillStyle='green';c.fillRect(0,0,64,64);
    c.fillStyle='red';c.globalAlpha=.5;c.fill(p);
    if(pixel(0,0)!=='0,128,0,255'||pixel(16,16)!=='128,64,0,255')throw Error('region paint');
    c.fillStyle='blue';c.fill(p);
    if(pixel(16,16)!=='64,32,128,255')throw Error('stale destination');
    c.globalAlpha=1;c.clearRect(0,0,64,64);c.setTransform(-1,0,0,1,64,0);c.fill(p);
    if(pixel(55,16)!=='0,0,255,255'||pixel(15,16)!=='0,0,0,0')throw Error('reflected region');
    "#);
}

#[test]
fn native_source_generation_still_respects_whole_surface_operators_and_effects() {
    run(r#"
    const contexts=Array.from(document.querySelectorAll('canvas'),n=>n.getContext('2d'));
    contexts[1].beginPath();contexts[1].rect(0,0,64,64);contexts[1].clip();
    const p=new Path2D();p.rect(10,10,32,32);
    const equal=()=>{
        const a=contexts[0].getImageData(0,0,64,64).data,b=contexts[1].getImageData(0,0,64,64).data;
        for(let i=0;i<a.length;i++)if(a[i]!==b[i])throw Error('effect/operator mismatch '+i);
    };
    for(const mode of ['copy','source-in','destination-in','xor','multiply']){
        for(const c of contexts){
            c.globalCompositeOperation='source-over';c.fillStyle='blue';c.fillRect(0,0,64,64);
            c.globalCompositeOperation=mode;c.fillStyle='red';c.globalAlpha=.5;c.fill(p);
        }
        equal();
    }
    for(const c of contexts){
        c.globalCompositeOperation='source-over';c.globalAlpha=1;c.clearRect(0,0,64,64);
        c.shadowColor='blue';c.shadowBlur=2;c.shadowOffsetX=3;c.fillStyle='red';c.fill(p);
    }
    equal();
    "#);
}
