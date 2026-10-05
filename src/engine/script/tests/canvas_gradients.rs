use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!(
        "<canvas width=64 height=64></canvas><script>{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn gradient_color_and_alpha_are_interpolated_without_premultiplication() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d');
    const g=c.createLinearGradient(.5,0,2.5,0);
    g.addColorStop(0,'rgba(255,0,0,0)');g.addColorStop(1,'blue');c.fillStyle=g;c.fillRect(0,0,3,1);
    const p=Array.from(c.getImageData(1,0,1,1).data);
    if(p.join()!=='128,0,128,128')throw Error('nonpremultiplied interpolation '+p);
    c.fillStyle='green';c.fillRect(0,2,3,1);c.fillStyle=g;c.fillRect(0,2,3,1);
    const q=Array.from(c.getImageData(1,2,1,1).data);
    if(q.join()!=='64,64,64,255')throw Error('gradient then compositing '+q);
    "#);
}

#[test]
fn gradient_slots_are_private_branded_and_live_despite_author_properties() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),g=c.createLinearGradient(.5,0,2.5,0);
    if(Object.keys(g).length||Object.prototype.toString.call(g)!=='[object CanvasGradient]')throw Error('opaque gradient');
    g.__kind='bad';g.__geometry=[];g.__stops=[];g.channels=[255,0,0,255];
    g.addColorStop(0,'red');g.addColorStop(1,'blue');c.fillStyle=g;
    const p=new Path2D();p.rect(0,0,32,32);c.fill(p);
    if(c.getImageData(31,16,1,1).data[2]!==255)throw Error('forged solid paint shortcut');
    g.addColorStop(.5,'green');c.fill(p);
    if(Array.from(c.getImageData(1,0,1,1).data).join()!=='0,128,0,255')throw Error('live private stops');
    const trace=[],value={valueOf(){trace.push('converted');return 0}};
    for(const receiver of [{},Object.create(CanvasGradient.prototype),new Proxy(g,{})]){
        let name='';try{CanvasGradient.prototype.addColorStop.call(receiver,value,'red')}catch(e){name=e.name}
        if(name!=='TypeError'||trace.length)throw Error('gradient brand before conversion');
    }
    const previous=c.fillStyle;c.fillStyle=Object.create(CanvasGradient.prototype);
    if(c.fillStyle!==previous)throw Error('forged gradient accepted');
    "#);
}

#[test]
fn gradient_webidl_conversions_are_ordered_and_distinct_from_range_validation() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),g=c.createLinearGradient(0,0,4,0);
    const error=(fn,name)=>{let actual='';try{fn()}catch(e){actual=e.name}if(actual!==name)throw Error(actual+' != '+name)};
    error(()=>g.addColorStop(NaN,'red'),'TypeError');error(()=>g.addColorStop(Infinity,'red'),'TypeError');
    error(()=>g.addColorStop(0n,'red'),'TypeError');error(()=>g.addColorStop(0,Symbol()),'TypeError');
    error(()=>g.addColorStop(-1,'red'),'IndexSizeError');error(()=>g.addColorStop(0,'bad color'),'SyntaxError');
    error(()=>g.addColorStop(0),'TypeError');
    const trace=[];
    error(()=>g.addColorStop({valueOf(){trace.push('offset');return 2}},
        {toString(){trace.push('color');return 'red'}}),'IndexSizeError');
    if(trace.join()!=='offset,color')throw Error('IDL arguments not converted before range validation');
    trace.length=0;
    error(()=>c.createLinearGradient({valueOf(){trace.push('first');return 0}}),'TypeError');
    if(trace.length)throw Error('required argument check after conversion');
    error(()=>c.createLinearGradient(0,0,Infinity,0),'TypeError');
    error(()=>c.createConicGradient(0n,0,0),'TypeError');
    error(()=>c.createRadialGradient(0,0,-1,0,0,2),'IndexSizeError');
    error(()=>CanvasRenderingContext2D.prototype.createLinearGradient.call({},0,0,1,0),'TypeError');
    "#);
}

#[test]
fn duplicate_stops_extend_endpoints_and_degenerate_gradient_paints_nothing() {
    run(r#"
    const c=document.querySelector('canvas').getContext('2d'),g=c.createLinearGradient(2.5,0,4.5,0);
    g.addColorStop(.5,'red');g.addColorStop(.5,'blue');c.fillStyle=g;c.fillRect(0,0,8,1);
    const p=x=>Array.from(c.getImageData(x,0,1,1).data).join();
    if(p(0)!=='255,0,0,255'||p(3)!=='0,0,255,255'||p(7)!=='0,0,255,255')throw Error('stop order or extrapolation');
    c.fillStyle=c.createLinearGradient(0,0,0,0);c.fillStyle.addColorStop(0,'green');c.fillRect(0,0,8,1);
    if(p(3)!=='0,0,255,255')throw Error('degenerate gradient');
    "#);
}
