use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=32 height=32></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn rectangle_double_coordinates_cover_subpixel_edges_and_negative_sizes() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    c.fillStyle='red';c.fillRect(.25,.25,1.5,1.5);
    for(const x of [0,1])for(const y of [0,1])
        if(Math.abs(alpha(x,y)-143)>1)throw Error('quarter edge '+x+','+y+': '+alpha(x,y));
    if(alpha(2,0)||alpha(0,2))throw Error('outside rectangle');
    c.clearRect(0,0,32,32);c.fillRect(1.75,1.75,-1.5,-1.5);
    for(const x of [0,1])for(const y of [0,1])
        if(Math.abs(alpha(x,y)-143)>1)throw Error('negative rectangle');
    c.clearRect(0,0,32,32);c.globalAlpha=.5;c.fillRect(.5,4,1,1);
    if(Math.abs(alpha(0,4)-64)>1||Math.abs(alpha(1,4)-64)>1)throw Error('alpha coverage');
    c.fillRect(10,10,.25,.25);
    if(Math.abs(alpha(10,10)-8)>1)throw Error('small rectangle vanished');
    "#);
}

#[test]
fn fractional_clear_erases_whole_pixels_and_ignores_paint_effects() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const rgba=(x,y)=>Array.from(c.getImageData(x,y,1,1).data);
    c.fillStyle='red';c.fillRect(0,0,32,32);
    c.globalAlpha=.1;c.globalCompositeOperation='copy';c.filter='opacity(0)';
    c.shadowColor='blue';c.shadowOffsetX=8;
    c.clearRect(.5,0,1,1);
    if(rgba(0,0).join(',')!=='255,0,0,255'||rgba(1,0).some(Boolean))throw Error('fractional clear');
    if(rgba(2,0)[3]!==255||rgba(8,0)[3]!==255)throw Error('clear paint effects');
    c.clearRect(0,0,2,1);
    if(rgba(0,0).some(Boolean)||rgba(1,0).some(Boolean))throw Error('transparent black');
    "#);
}

#[test]
fn rectangles_convert_arguments_once_in_order_and_leave_path_unchanged() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const order=[],v=(name,value)=>({valueOf(){order.push(name);return value;}});
    c.beginPath();c.rect(10,10,4,4);
    c.fillRect(v('x',.5),v('y',0),v('w',1),v('h',1));
    if(order.join(',')!=='x,y,w,h')throw Error('IDL conversions '+order);
    order.length=0;c.clearRect(v('x',.5),v('y',0),v('w',1),v('h',1));
    if(order.join(',')!=='x,y,w,h')throw Error('clear conversions '+order);
    if(!c.isPointInPath(12,12)||c.isPointInPath(.5,.5))throw Error('rectangle changed path');
    let threw=false;try{c.fillRect(0n,0,1,1);}catch(e){threw=e instanceof TypeError;}
    if(!threw)throw Error('BigInt double accepted');
    for(const values of [[NaN,0,1,1],[0,Infinity,1,1],[0,0,0,1],[0,0,1,0]])c.fillRect(...values);
    if(c.getImageData(0,0,1,1).data[3]!==128)throw Error('invalid geometry painted');
    "#);
}

#[test]
fn affine_rectangles_preserve_area_reflection_and_subpixel_translation() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    c.setTransform(-1,0,0,1,2.5,0);c.fillRect(0,0,2,1);
    if(Math.abs(alpha(0,0)-128)>1||alpha(1,0)!==255||Math.abs(alpha(2,0)-128)>1)
        throw Error('reflection coverage');
    c.resetTransform();c.clearRect(0,0,32,32);
    c.setTransform(0,1,-1,0,2.5,.5);c.fillRect(0,0,1,1);
    for(const x of [1,2])for(const y of [0,1])
        if(Math.abs(alpha(x,y)-64)>1)throw Error('quarter turn coverage');
    c.resetTransform();c.clearRect(0,0,32,32);
    c.setTransform(1,0,1,1,0,0);c.fillRect(0,0,1,1);
    if(Math.abs(alpha(0,0)-128)>1||Math.abs(alpha(1,0)-128)>1||alpha(2,0))
        throw Error('shear pixel area');
    c.setTransform(0,0,0,0,0,0);c.clearRect(0,0,32,32);
    if(Math.abs(alpha(0,0)-128)>1)throw Error('singular clear changed bitmap');
    "#);
}

#[test]
fn rectangle_coverage_respects_clip_source_layers_and_integer_image_data() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    c.fillStyle='blue';c.fillRect(0,0,32,32);
    c.save();c.beginPath();c.rect(1,0,1,32);c.clip();
    c.clearRect(.5,0,1,1);
    if(alpha(0,0)!==255||alpha(1,0))throw Error('clear clip');
    c.restore();c.globalCompositeOperation='copy';c.fillStyle='red';c.fillRect(.5,4,1,1);
    if(Math.abs(alpha(0,4)-128)>1||Math.abs(alpha(1,4)-128)>1||alpha(4,4))
        throw Error('copy source layer');
    const image=c.getImageData(.9,4.9,1.9,1.9);
    if(image.width!==1||image.height!==1||Math.abs(image.data[3]-128)>1)
        throw Error('ImageData no longer truncates');
    "#);
}

#[test]
fn opaque_rectangle_row_copy_uses_intrinsics_and_keeps_clipped_fallback() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const set=Uint8ClampedArray.prototype.set,fill=Uint8ClampedArray.prototype.fill;
    Uint8ClampedArray.prototype.set=()=>{throw Error('author set');};
    Uint8ClampedArray.prototype.fill=()=>{throw Error('author fill');};
    try {
        c.fillStyle='red';c.fillRect(-3,-2,40,40);c.clearRect(0,0,1,32);
        c.save();c.beginPath();c.rect(10,0,2,32);c.clip();c.fillStyle='blue';c.fillRect(0,0,32,32);c.restore();
    } finally {Uint8ClampedArray.prototype.set=set;Uint8ClampedArray.prototype.fill=fill;}
    const p=x=>Array.from(c.getImageData(x,1,1,1).data);
    if(p(0).some(Boolean)||p(1).join(',')!=='255,0,0,255'||p(10).join(',')!=='0,0,255,255'||
        p(12).join(',')!=='255,0,0,255')throw Error('row copy or clip leaked');
    c.setTransform(0,1,-1,0,8,0);c.clearRect(0,0,4,2);c.fillStyle='lime';c.fillRect(0,0,4,2);
    c.resetTransform();if(p(6).join(',')!=='0,255,0,255')throw Error('axis swap row copy');
    "#);
}
