use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=96 height=96></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn dash_end_caps_and_subpath_resets_match_stroke_hit_regions() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const assert=(value,label)=>{if(!value)throw Error(label)};
    const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
    for(const cap of ['butt','round','square']) {
        c.clearRect(0,0,96,96);c.lineWidth=4;c.lineCap=cap;c.setLineDash([8,8]);
        c.beginPath();c.moveTo(8,20);c.lineTo(18,20);c.moveTo(8,40);c.lineTo(48,40);c.stroke();
        assert(alpha(10,20)===255&&alpha(10,40)===255,'reset '+cap);
        assert(alpha(20,40)===0&&!c.isPointInStroke(20.5,40.5),'gap '+cap);
        assert((alpha(17,40)!==0)===(cap!=='butt'),'dash cap pixels '+cap);
        assert(c.isPointInStroke(17.5,40.5)===(cap!=='butt'),'dash cap hit '+cap);
        assert(c.isPointInStroke(12,42),'boundary '+cap);
    }
    "#);
}

#[test]
fn dashed_corner_joins_do_not_fill_gaps_and_hit_testing_ignores_drawing_clip() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    c.lineWidth=4;c.lineJoin='miter';c.setLineDash([8,24]);
    const p=new Path2D();p.moveTo(8,20);p.lineTo(24,20);p.lineTo(24,44);c.stroke(p);
    if(c.getImageData(25,19,1,1).data[3]||c.isPointInStroke(p,25.5,19.5))throw Error('gap retained corner');
    c.setLineDash([24,8]);c.save();c.beginPath();c.rect(0,0,1,1);c.clip();c.stroke(p);
    if(!c.isPointInStroke(p,25.5,19.5))throw Error('clip affected geometric hit');c.restore();
    "#);
}

#[test]
fn dash_offsets_all_zero_patterns_and_round_zero_runs_are_real_pixels() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');c.lineWidth=4;
    const p=new Path2D();p.moveTo(8,20);p.lineTo(48,20);
    c.setLineDash([8,8]);c.lineDashOffset=-4;c.stroke(p);
    const before=Array.from(c.getImageData(0,0,96,96).data);
    c.clearRect(0,0,96,96);c.lineDashOffset=12;c.stroke(p);
    if(Array.from(c.getImageData(0,0,96,96).data).some((v,i)=>v!==before[i]))throw Error('negative phase');
    c.clearRect(0,0,96,96);c.setLineDash([0,0]);c.stroke(p);
    if(c.getImageData(20,20,1,1).data[3]!==255)throw Error('all zero is solid');
    c.clearRect(0,0,96,96);c.lineDashOffset=0;c.lineCap='round';c.setLineDash([0,8]);c.stroke(p);
    if(c.getImageData(17,20,1,1).data[3]!==255||c.getImageData(12,20,1,1).data[3]!==0)throw Error('zero-on round caps');
    if(!c.isPointInStroke(p,17.5,20.5)||c.isPointInStroke(p,12.5,20.5))throw Error('zero-on hit regions');
    "#);
}

#[test]
fn dash_state_is_saved_copied_and_reset_without_mutating_the_path() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    c.lineWidth=4;c.lineCap='round';c.setLineDash([8]);c.lineDashOffset=-4;
    const input=c.getLineDash();input[0]=100;
    if(c.getLineDash().join(',')!=='8,8')throw Error('dash list exposed storage or odd normalization');
    c.save();c.setLineDash([2,3]);c.lineDashOffset=5;c.lineCap='butt';c.restore();
    if(c.getLineDash().join(',')!=='8,8'||c.lineDashOffset!==-4||c.lineCap!=='round')throw Error('saved dash state');
    for(const pattern of [[-1,2],[NaN,2],[Infinity,2]])c.setLineDash(pattern);
    if(c.getLineDash().join(',')!=='8,8')throw Error('invalid assignment changed pattern');
    const p=new Path2D();p.moveTo(8,20);p.lineTo(48,20);c.stroke(p);
    c.clearRect(0,0,96,96);c.setLineDash([]);c.lineDashOffset=0;c.stroke(p);
    if(c.getImageData(20,20,1,1).data[3]!==255||!c.isPointInStroke(p,20.5,20.5))throw Error('dash mutated Path2D');
    c.canvas.width=96;
    if(c.getLineDash().length||c.lineDashOffset!==0||c.lineCap!=='butt')throw Error('bitmap reset retained dash state');
    "#);
}

#[test]
fn scaled_dash_cap_geometry_survives_shear_reflection_and_empty_off_runs() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),p=new Path2D();
    p.moveTo(8,20);p.lineTo(48,20);c.lineWidth=4;c.lineCap='round';c.setLineDash([8,8]);
    for(const m of [[1,.5,.25,1,8,3],[-1,0,0,1,80,0],[2,0,0,3,0,0]]) {
        c.setTransform(...m);const [a,b,k,d,e,f]=m;
        const point=(x,y)=>[a*x+k*y+e,b*x+d*y+f];
        if(!c.isPointInStroke(p,...point(17,20))||c.isPointInStroke(p,...point(20,20)))throw Error('transformed dash cap');
    }
    c.resetTransform();c.clearRect(0,0,96,96);c.lineDashOffset=8;
    const empty=new Path2D();empty.moveTo(8,20);empty.lineTo(9,20);c.stroke(empty);
    if(c.getImageData(0,0,96,96).data.some(v=>v)||c.isPointInStroke(empty,8.5,20))throw Error('empty dash selected solid fallback');
    "#);
}
