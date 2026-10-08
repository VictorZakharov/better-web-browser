//! End-to-end function ordering and state ownership, beyond native matrix tests.
use super::*;

fn check(script: &str) {
    let (dom, outcome) = execute_html(&format!(
        "<canvas width=8 height=8></canvas><output>no</output><script>{script}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn canvas_filter_color_chain_clamps_each_function_without_intermediate_byte_rounding() {
    check(
        r#"
        const ctx=document.querySelector('canvas').getContext('2d');
        ctx.filter='brightness(.5) brightness(2)';ctx.fillStyle='rgb(1 3 5)';ctx.fillRect(0,0,1,1);
        const precise=[...ctx.getImageData(0,0,1,1).data].join(',');
        ctx.filter='brightness(2) brightness(.5)';ctx.fillStyle='rgb(200 200 200)';ctx.fillRect(2,0,1,1);
        const clipped=[...ctx.getImageData(2,0,1,1).data].join(',');
        document.querySelector('output').textContent=precise==='1,3,5,255'&&clipped==='128,128,128,255'?'yes':precise+';'+clipped;
    "#,
    );
}

#[test]
fn canvas_filter_color_shadow_boundaries_keep_original_function_order() {
    check(
        r#"
        const ctx=document.querySelector('canvas').getContext('2d');ctx.fillStyle='red';
        ctx.filter='brightness(.5) drop-shadow(2px 0 blue)';ctx.fillRect(0,0,1,1);
        const before=[...ctx.getImageData(2,0,1,1).data].join(',');
        ctx.filter='drop-shadow(2px 0 blue) brightness(.5)';ctx.fillRect(0,2,1,1);
        const after=[...ctx.getImageData(2,2,1,1).data].join(',');
        document.querySelector('output').textContent=before==='0,0,255,255'&&after==='0,0,128,255'?'yes':before+';'+after;
    "#,
    );
}

#[test]
fn canvas_filter_color_chain_preserves_clip_transform_and_saved_function_state() {
    check(
        r#"
        const ctx=document.querySelector('canvas').getContext('2d');
        ctx.translate(2,1);ctx.beginPath();ctx.rect(0,0,1,3);ctx.clip();
        ctx.filter='invert(1) brightness(.5)';ctx.save();ctx.filter='none';ctx.restore();
        ctx.fillStyle='red';ctx.fillRect(0,0,4,4);
        const ink=[...ctx.getImageData(2,1,1,1).data].join(',');
        const outside=ctx.getImageData(3,1,1,1).data[3];
        document.querySelector('output').textContent=ink==='0,128,128,255'&&outside===0&&ctx.getTransform().e===2?'yes':ink;
    "#,
    );
}

#[test]
fn canvas_filter_color_wire_does_not_invoke_inherited_tojson_or_array_push() {
    check(
        r#"
        const ctx=document.querySelector('canvas').getContext('2d');
        ctx.filter='brightness(.5) contrast(2) hue-rotate(90deg)';ctx.fillStyle='red';
        const push=Array.prototype.push;let observed=0;
        Object.prototype.toJSON=function(){observed++;return this;};
        Array.prototype.push=function(...values){
            if(values[0]?.name==='brightness'||values[0]?.name==='contrast') observed++;
            return push.apply(this,values);
        };
        try{ctx.fillRect(0,0,1,1);}finally{delete Object.prototype.toJSON;Array.prototype.push=push;}
        const pixel=ctx.getImageData(0,0,1,1).data;
        document.querySelector('output').textContent=observed===0&&pixel[3]===255?'yes':String(observed);
    "#,
    );
}
