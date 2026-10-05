use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=64 height=64></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn native_gradient_requests_do_not_run_author_serialization_hooks() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),g=c.createLinearGradient(0,0,64,0);
    g.addColorStop(0,'red');g.addColorStop(1,'blue');c.fillStyle=g;
    let calls=0;Object.prototype.toJSON=function(){calls++;throw Error('private state leaked')};
    const original=JSON.stringify;JSON.stringify=()=>{throw Error('replaced stringify')};
    try {c.fillRect(0,0,64,64)} finally {delete Object.prototype.toJSON;JSON.stringify=original}
    if(calls)throw Error('author serialization hook ran');
    if(c.getImageData(32,32,1,1).data[3]!==255)throw Error('gradient paint missing');
    "#);
}

#[test]
fn native_gradient_regions_keep_live_stops_transform_and_destination_state() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),g=c.createLinearGradient(.5,0,32.5,0);
    const p=(x,y)=>Array.from(c.getImageData(x,y,1,1).data);
    g.addColorStop(0,'red');g.addColorStop(1,'blue');c.fillStyle=g;c.fillRect(0,0,64,64);
    if(p(0,0).join()!=='255,0,0,255'||p(40,0).join()!=='0,0,255,255')throw Error('native endpoints');
    g.addColorStop(.5,'green');c.fillRect(0,0,64,64);
    if(p(16,0).join()!=='0,128,0,255')throw Error('stale native stops');
    c.clearRect(0,0,64,64);c.translate(10,0);c.fillRect(0,0,40,40);
    if(p(10,10).join()!=='255,0,0,255'||p(0,0)[3]||p(55,55)[3])throw Error('native region transform');
    c.globalAlpha=.5;c.fillStyle='white';c.fillRect(0,0,40,40);
    if(p(10,10).join()!=='255,128,128,255')throw Error('destination lost after native paint');
    "#);
}

#[test]
fn scalar_fallback_preserves_more_than_native_stop_budget_and_degenerate_geometry() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),g=c.createLinearGradient(0,0,64,0);
    for(let i=0;i<257;i++)g.addColorStop(i/256,'red');c.fillStyle=g;c.fillRect(0,0,64,64);
    if(Array.from(c.getImageData(32,32,1,1).data).join()!=='255,0,0,255')throw Error('stop budget blanked paint');
    const zero=c.createRadialGradient(0,0,2,0,0,2);zero.addColorStop(0,'blue');c.fillStyle=zero;c.fillRect(0,0,64,64);
    if(Array.from(c.getImageData(32,32,1,1).data).join()!=='255,0,0,255')throw Error('degenerate cone');
    "#);
}
