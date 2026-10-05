use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=32 height=32></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn fill_paths_have_fractional_coverage_and_union_before_opacity() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();
    p.rect(.5,0,2,2);c.fillStyle='red';c.globalAlpha=.5;c.fill(p);
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    if(Math.abs(alpha(0,0)-64)>1||Math.abs(alpha(1,0)-128)>1||alpha(3,0))throw Error('fill coverage');
    c.clearRect(0,0,32,32);p.rect(.5,0,2,2);c.fill(p);
    if(Math.abs(alpha(0,0)-64)>1||Math.abs(alpha(1,0)-128)>1)throw Error('duplicate fill darkened');
    c.clearRect(0,0,32,32);c.fill(p,'evenodd');
    if(alpha(0,0)||alpha(1,0))throw Error('evenodd duplicate contours');
    "#);
}

#[test]
fn filling_open_contours_does_not_close_them_for_later_stroking() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    c.beginPath();c.moveTo(2,2);c.lineTo(20,2);c.lineTo(20,20);c.fill();
    if(!c.getImageData(18,10,1,1).data[3])throw Error('open fill not closed');
    c.lineWidth=1;
    if(c.isPointInStroke(10,10))throw Error('fill changed open contour');
    if(!c.isPointInPath(18,10))throw Error('fill changed path geometry');
    c.clearRect(0,0,32,32);c.stroke();
    if(c.getImageData(10,10,1,1).data[3])throw Error('stroke inherited synthetic close');
    "#);
}

#[test]
fn native_fill_preserves_construction_transform_and_retained_path_transform() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();
    p.rect(0,0,1,1);c.translate(.5,4);c.fill(p);
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    if(Math.abs(alpha(0,4)-128)>1||Math.abs(alpha(1,4)-128)>1)throw Error('retained transform');
    c.beginPath();c.rect(4,0,1,1);c.resetTransform();c.fill();
    if(Math.abs(alpha(4,4)-128)>1||Math.abs(alpha(5,4)-128)>1)throw Error('construction transform');
    c.clearRect(0,0,32,32);c.setTransform(-1,0,0,1,2.5,0);c.fill(p);
    if(Math.abs(alpha(1,0)-128)>1||Math.abs(alpha(2,0)-128)>1)throw Error('reflection fill');
    "#);
}

#[test]
fn compound_fill_rules_preserve_holes_and_self_intersection() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();
    p.rect(1,1,20,20);p.rect(5,5,8,8);c.fill(p,'evenodd');
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    if(alpha(2,2)!==255||alpha(6,6))throw Error('evenodd hole');
    c.clearRect(0,0,32,32);c.fill(p);
    if(alpha(6,6)!==255)throw Error('nonzero nested contour');
    const reversed=new Path2D();reversed.rect(1,1,20,20);
    reversed.moveTo(5,5);reversed.lineTo(5,13);reversed.lineTo(13,13);reversed.lineTo(13,5);reversed.closePath();
    c.clearRect(0,0,32,32);c.fill(reversed);
    if(alpha(2,2)!==255||alpha(6,6))throw Error('opposite winding hole');
    const cross=new Path2D();cross.moveTo(2,2);cross.lineTo(18,18);cross.lineTo(2,18);cross.lineTo(18,2);
    c.clearRect(0,0,32,32);c.fill(cross,'evenodd');
    if(alpha(10,4)!==255||alpha(4,10))throw Error('self intersection');
    "#);
}

#[test]
fn fill_coverage_reaches_shadow_filter_and_full_surface_compositor_once() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();p.rect(.5,4,1,1);
    const pixel=(x,y)=>Array.from(c.getImageData(x,y,1,1).data);
    c.fillStyle='red';c.shadowColor='blue';c.shadowOffsetY=4;c.filter='opacity(50%)';c.fill(p);
    if(Math.abs(pixel(0,4)[3]-64)>1||pixel(0,4)[0]!==255||
        Math.abs(pixel(0,8)[3]-64)>1||pixel(0,8)[2]!==255)throw Error('effect source coverage');
    c.shadowColor='transparent';c.filter='none';c.clearRect(0,0,32,32);
    c.fillStyle='blue';c.fillRect(0,0,32,32);c.globalCompositeOperation='copy';c.fillStyle='red';
    c.save();c.beginPath();c.rect(1,0,1,32);c.clip();c.fill(p);c.restore();
    if(pixel(0,4).join(',')!=='0,0,255,255'||Math.abs(pixel(1,4)[3]-128)>1||pixel(1,10)[3])
        throw Error('clip or copy coverage');
    "#);
}

#[test]
fn native_fill_is_real_bounded_geometry_not_a_new_author_visible_api() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const empty=new Path2D();empty.moveTo(1,1);empty.lineTo(2,2);c.fill(empty);
    if(c.getImageData(1,1,1,1).data[3])throw Error('degenerate painted');
    // Large absolute coordinates select the bounded JS fallback, rather than
    // interpreting a rejected native mask as a successfully empty fill.
    const outside=new Path2D();outside.rect(-20000,-20000,20020,20020);c.fill(outside);
    if(c.getImageData(2,2,1,1).data[3]!==255)throw Error('fallback discarded valid fill');
    "#);
}
