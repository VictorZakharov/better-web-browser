use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=64 height=64></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn native_stroke_coverage_preserves_subpixel_width_and_global_alpha() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const sample=(x,y)=>Array.from(c.getImageData(x,y,1,1).data);
    c.lineWidth=1;c.strokeStyle='red';c.beginPath();c.moveTo(8,20);c.lineTo(48,20);c.stroke();
    for(const y of [19,20]) {
        const p=sample(20,y);
        if(p[0]!==255||p[1]||p[2]||Math.abs(p[3]-128)>1)throw Error('half pixel coverage '+p);
    }
    if(sample(20,18)[3]||sample(20,21)[3])throw Error('stroke coverage leaked outside support');
    c.clearRect(0,0,64,64);c.globalAlpha=.5;c.stroke();
    for(const y of [19,20])if(Math.abs(sample(20,y)[3]-64)>1)throw Error('global alpha applied twice');
    const retained=new Path2D();retained.moveTo(8,20);retained.lineTo(48,20);
    c.clearRect(0,0,64,64);c.globalAlpha=1;c.translate(0,.5);c.stroke(retained);
    if(sample(20,20)[3]!==255||sample(20,19)[3]||sample(20,21)[3])throw Error('pixel aligned subpixel pen');
    "#);
}

#[test]
fn fractional_coverage_is_source_alpha_for_blending_and_full_surface_operators() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const source=new Path2D();source.moveTo(8,20);source.lineTo(48,20);
    const modes=['source-over','source-in','source-out','source-atop','destination-over',
        'destination-in','destination-out','destination-atop','xor','copy','lighter','multiply','screen'];
    const rgba=(x,y)=>Array.from(c.getImageData(x,y,1,1).data);
    const background=()=>{c.globalCompositeOperation='source-over';c.clearRect(0,0,64,64);
        c.fillStyle='rgba(20,80,160,.5)';c.fillRect(0,0,64,64);};
    for(const mode of modes) {
        background();c.globalCompositeOperation=mode;c.lineWidth=1;c.strokeStyle='rgba(200,40,60,.5)';c.stroke(source);
        const actual=rgba(20,20),outside=rgba(2,2);
        background();c.globalCompositeOperation=mode;c.fillStyle='rgba(200,40,60,.25)';c.fillRect(8,19,40,2);
        const expected=rgba(20,20),expectedOutside=rgba(2,2);
        if(actual.some((v,i)=>Math.abs(v-expected[i])>2))throw Error(mode+' fractional source '+actual+' / '+expected);
        if(outside.some((v,i)=>v!==expectedOutside[i]))throw Error(mode+' outside source layer');
    }
    "#);
}

#[test]
fn antialiased_stroke_source_is_generated_before_integer_clip_and_shadow() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();
    p.moveTo(8,20);p.lineTo(48,20);c.lineWidth=1;c.strokeStyle='red';
    c.save();c.beginPath();c.rect(20,0,20,64);c.clip();
    c.shadowColor='blue';c.shadowOffsetY=4;c.stroke(p);
    const pixel=(x,y)=>Array.from(c.getImageData(x,y,1,1).data);
    if(pixel(19,20)[3]||pixel(19,24)[3])throw Error('clip applied before layer generation');
    if(Math.abs(pixel(25,20)[3]-128)>1||pixel(25,20)[0]!==255)throw Error('source coverage lost');
    if(Math.abs(pixel(25,24)[3]-128)>1||pixel(25,24)[2]!==255)throw Error('shadow coverage lost');
    c.restore();c.clearRect(0,0,64,64);c.stroke(p);
    if(Math.abs(pixel(19,20)[3]-128)>1)throw Error('clip restore changed coverage');
    "#);
}
