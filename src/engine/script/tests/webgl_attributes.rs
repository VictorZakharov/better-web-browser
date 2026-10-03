//! Context options follow Web IDL even when the native backend rejects a hint.
use super::*;

fn check(script: &str) {
    let (dom, outcome) = execute_html(&format!("<output></output><script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pass"
    );
}

#[test]
fn webgl_attributes_read_every_dictionary_member_once_in_idl_order() {
    check(
        r#"
        const order=['alpha','antialias','depth','desynchronized','failIfMajorPerformanceCaveat',
            'powerPreference','premultipliedAlpha','preserveDrawingBuffer','stencil'];
        const seen=[], options=Object.create(null);
        for(const name of order) Object.defineProperty(options,name,{get(){seen.push(name);return undefined}});
        Object.defineProperty(options,'unused',{get(){throw Error('unknown dictionary member read')}});
        const gl=new OffscreenCanvas(1,1).getContext('webgl',options);
        const a=gl.getContextAttributes();
        if(seen.join()!==order.join() || !a.alpha || !a.depth || !a.premultipliedAlpha ||
            a.antialias || a.desynchronized || a.stencil || a.preserveDrawingBuffer ||
            a.failIfMajorPerformanceCaveat || a.powerPreference!=='default') throw Error(JSON.stringify({seen,a}));
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_attributes_use_inherited_values_and_do_not_mutate_author_options() {
    check(
        r#"
        const options=Object.freeze(Object.create({alpha:null,depth:0,stencil:1,antialias:true,
            desynchronized:true,premultipliedAlpha:'',preserveDrawingBuffer:'yes',powerPreference:'low-power'}));
        const canvas=new OffscreenCanvas(1,1), gl=canvas.getContext('experimental-webgl',options);
        const a=gl.getContextAttributes();
        if(a.alpha || a.depth || a.premultipliedAlpha || a.antialias || a.desynchronized ||
            !a.stencil || !a.preserveDrawingBuffer || a.powerPreference!=='low-power') throw Error(JSON.stringify(a));
        a.alpha=true;a.powerPreference='high-performance';
        if(gl.getContextAttributes().alpha || gl.getContextAttributes().powerPreference!=='low-power') throw Error('mutable snapshot');
        const ignored=new Proxy({}, {get(){throw Error('existing context must ignore options')}});
        if(canvas.getContext('webgl',ignored)!==gl || canvas.getContext('2d')!==null) throw Error('context mode');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_attributes_conversion_exceptions_precede_native_creation_and_error_events() {
    check(
        r#"
        const canvas=document.createElement('canvas');let events=0, converted=0;
        canvas.addEventListener('webglcontextcreationerror',()=>events++);
        const invalid=[42,'attributes',{powerPreference:'unsupported'},{powerPreference:null},
            {powerPreference:Symbol('power')}, {get depth(){throw new TypeError('getter')}}];
        for(const options of invalid) {
            let threw=false;try{canvas.getContext('webgl',options)}catch(e){threw=e instanceof TypeError}
            if(!threw)throw Error('dictionary accepted');
        }
        if(events)throw Error('conversion error dispatched native error event');
        const gl=canvas.getContext('webgl',{powerPreference:{toString(){converted++;return 'high-performance'}}});
        if(!gl || converted!==1 || gl.getContextAttributes().powerPreference!=='high-performance') throw Error('enum conversion');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_attributes_software_caveat_rejection_has_trusted_cancelable_event() {
    check(
        r#"
        const canvas=document.createElement('canvas');let event, reads=[];
        canvas.addEventListener('webglcontextcreationerror',e=>{event=e;e.preventDefault()});
        const result=canvas.getContext('webgl',{failIfMajorPerformanceCaveat:true,
            get powerPreference(){reads.push('power');return 'default'},
            get stencil(){reads.push('stencil');return false}});
        if(result!==null || !event || !event.isTrusted || !event.cancelable || event.bubbles ||
            !event.defaultPrevented || !event.statusMessage.includes('software') || reads.join()!=='power,stencil')
            throw Error('creation failure contract');
        if(!canvas.getContext('webgl',null)) throw Error('failed creation fixed canvas mode');
        const synthetic=new WebGLContextEvent('webglcontextcreationerror');
        if(synthetic.isTrusted || synthetic.cancelable) throw Error('author event gained native privileges');
        document.querySelector('output').textContent='pass';
    "#,
    );
}

#[test]
fn webgl_attributes_restore_preserves_granted_settings_without_author_getters() {
    super::webgl_lifecycle::run(
        r#"
        const canvas=document.querySelector('canvas');let reads=0;
        const options={alpha:false,depth:false,stencil:true,premultipliedAlpha:false,
            preserveDrawingBuffer:true,get powerPreference(){reads++;return 'high-performance'}};
        const gl=canvas.getContext('webgl',options), snapshot=JSON.stringify(gl.getContextAttributes());
        const loss=gl.getExtension('WEBGL_lose_context');
        canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();setTimeout(()=>loss.restoreContext(),0)});
        canvas.addEventListener('webglcontextrestored',()=>{
            if(reads!==1 || JSON.stringify(gl.getContextAttributes())!==snapshot || gl.getParameter(gl.ALPHA_BITS)!==0 ||
                gl.getParameter(gl.DEPTH_BITS)!==0 || gl.getParameter(gl.STENCIL_BITS)===0) throw Error('restore attributes');
            canvas.setAttribute('data-result','pass');
        });
        loss.loseContext();
    "#,
    );
}
