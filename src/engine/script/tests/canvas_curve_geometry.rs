use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=64 height=64></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn empty_curve_subpaths_start_at_the_first_control_point() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');c.lineWidth=1;
    c.quadraticCurveTo(10,10,30,10);
    if(c.isPointInStroke(0,0)||c.isPointInStroke(5,5)||!c.isPointInStroke(20,10))
        throw Error('quadratic implicit origin');
    c.beginPath();c.bezierCurveTo(10,10,20,10,30,10);
    if(c.isPointInStroke(0,0)||c.isPointInStroke(5,5)||!c.isPointInStroke(20,10))
        throw Error('cubic implicit origin');
    c.beginPath();c.translate(4,4);c.quadraticCurveTo(10,10,30,10);c.resetTransform();
    if(c.isPointInStroke(4,4)||!c.isPointInStroke(24,14))throw Error('transformed implicit origin');
    "#);
}

#[test]
fn curves_after_close_use_the_closed_contours_first_point() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');c.lineWidth=1;
    const closed=()=>{c.beginPath();c.rect(10,10,4,4);};
    closed();c.quadraticCurveTo(20,10,30,10);
    if(!c.isPointInStroke(19,10)||c.isPointInStroke(19,11))throw Error('quadratic after close');
    closed();c.bezierCurveTo(16,10,20,10,30,10);
    if(!c.isPointInStroke(19,10)||c.isPointInStroke(19,11))throw Error('cubic after close');
    closed();c.arcTo(30,10,30,30,4);
    if(!c.isPointInStroke(20,10)||c.isPointInStroke(20,11))throw Error('arcTo after close');
    "#);
}

#[test]
fn large_finite_arc_angles_terminate_without_repeated_turn_subtraction() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    for(const ccw of [true,false]) {
        c.beginPath();c.arc(20,20,4,-1e300,1e300,ccw);c.stroke();
        c.beginPath();c.ellipse(20,20,4,2,1e300,1e300,-1e300,ccw);c.stroke();
    }
    c.beginPath();c.arc(20,20,4,0,0);c.stroke();
    if(c.isPointInStroke(24,20))throw Error('zero sweep became circle');
    "#);
}

#[test]
fn adaptive_default_curve_geometry_matches_retained_transformed_geometry() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const geometry=p=>{p.moveTo(2,2);p.quadraticCurveTo(16,30,30,2);p.lineTo(30,32);p.lineTo(2,32);p.closePath();};
    c.setTransform(1.5,0,0,1.5,2,2);geometry(c);c.resetTransform();c.fill();
    const first=Array.from(c.getImageData(0,0,64,64).data);
    c.clearRect(0,0,64,64);const transformed=new Path2D();
    transformed.moveTo(5,5);transformed.quadraticCurveTo(26,47,47,5);transformed.lineTo(47,50);transformed.lineTo(5,50);transformed.closePath();
    c.fill(transformed);const second=Array.from(c.getImageData(0,0,64,64).data);
    if(first.some((v,i)=>v!==second[i]))throw Error('construction-time curve transform');
    "#);
}

#[test]
fn curve_geometry_budget_is_bounded_and_invalid_values_do_not_change_path() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');c.beginPath();c.moveTo(10,10);c.lineTo(30,10);
    for(const call of [()=>c.quadraticCurveTo(NaN,0,1,1),()=>c.bezierCurveTo(1,1,Infinity,1,2,2),
        ()=>c.ellipse(20,20,2,2,0,NaN,1)])call();
    if(!c.isPointInStroke(20,10)||c.isPointInStroke(1,1))throw Error('invalid curve changed path');
    let bounded=false;const p=new Path2D();p.moveTo(0,0);
    try{for(let i=0;i<10000;i++)p.quadraticCurveTo(20,40,40,0);}catch(e){bounded=e.name==='NotSupportedError';}
    if(!bounded)throw Error('unbounded curve points');
    "#);
}
