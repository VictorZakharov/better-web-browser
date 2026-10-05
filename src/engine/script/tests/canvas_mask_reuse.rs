use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=32 height=32></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn repeated_coverage_keeps_live_paint_alpha_and_destination_pixels() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();p.rect(2,2,10,10);
    const pixel=(x,y)=>Array.from(c.getImageData(x,y,1,1).data);
    c.fillStyle='red';c.fill(p);
    if(pixel(5,5).join()!=='255,0,0,255')throw Error('first paint');
    c.clearRect(0,0,32,32);c.fillStyle='blue';c.globalAlpha=.5;c.fill(p);
    if(pixel(5,5).join()!=='0,0,255,128')throw Error('cached paint or alpha');
    c.fillStyle='red';c.fill(p);
    if(pixel(5,5).join()!=='170,0,85,192')throw Error('cached destination');
    c.globalAlpha=1;c.globalCompositeOperation='destination-out';c.fill(p);
    if(pixel(5,5)[3])throw Error('cached operator');
    "#);
}

#[test]
fn coverage_reuse_does_not_alias_paths_or_pen_and_transform_state() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    p.moveTo(2,8);p.lineTo(20,8);c.lineWidth=2;c.stroke(p);
    if(alpha(8,5))throw Error('initial pen');
    c.clearRect(0,0,32,32);c.lineWidth=8;c.stroke(p);
    if(alpha(8,5)!==255)throw Error('pen state omitted');
    c.clearRect(0,0,32,32);c.lineWidth=2;c.translate(0,8);c.stroke(p);
    if(alpha(8,8)||alpha(8,16)!==255)throw Error('transform omitted');
    c.resetTransform();c.clearRect(0,0,32,32);p.lineTo(20,20);c.stroke(p);
    if(alpha(19,16)!==255)throw Error('retained path mutation omitted');
    "#);
}

#[test]
fn reuse_preserves_fill_rule_clip_save_restore_and_bitmap_reset() {
    run(r#"
    const canvas=document.querySelector('canvas'),c=canvas.getContext('2d'),p=new Path2D();
    p.rect(0,0,20,20);p.rect(4,4,8,8);
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    c.fill(p);if(alpha(6,6)!==255)throw Error('nonzero');
    c.clearRect(0,0,32,32);c.fill(p,'evenodd');
    if(alpha(6,6)||alpha(2,2)!==255)throw Error('fill rule omitted');
    c.clearRect(0,0,32,32);c.save();c.beginPath();c.rect(0,0,2,2);c.clip();c.fill(p);
    if(alpha(6,6)||alpha(1,1)!==255)throw Error('cached clip');
    c.restore();c.fill(p);if(alpha(6,6)!==255)throw Error('restore clip');
    canvas.width=32;c.fillStyle='blue';c.fill(p);
    if(Array.from(c.getImageData(6,6,1,1).data).join()!=='0,0,255,255')throw Error('bitmap reset');
    "#);
}
