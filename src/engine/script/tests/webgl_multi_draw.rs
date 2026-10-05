use super::*;

fn run(source: &str) {
    let (_, outcome) = execute_html(&format!("<script>{source}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn multi_draw_extension_has_context_owned_idl_methods_in_both_versions() {
    run(r#"
    const assert=(v,s)=>{if(!v)throw Error(s)};
    for(const api of ['webgl','webgl2']) {
        const gl=document.createElement('canvas').getContext(api),ext=gl.getExtension('WEBGL_multi_draw');
        assert(ext&&gl.getSupportedExtensions().includes('WEBGL_multi_draw'),'real availability');
        assert(ext===gl.getExtension('webgl_MULTI_draw'),'cached case insensitive extension');
        assert(!('WEBGL_multi_draw' in globalThis),'no interface object');
        assert(Object.prototype.toString.call(ext)==='[object WEBGL_multi_draw]','brand');
        for(const [name,length] of [['multiDrawArraysWEBGL',6],['multiDrawElementsWEBGL',7],
            ['multiDrawArraysInstancedWEBGL',8],['multiDrawElementsInstancedWEBGL',9]]) {
            assert(!(name in gl),'extension only');assert(ext[name].length===length,'arity');
            const desc=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(ext),name);
            assert(desc.enumerable&&desc.writable&&desc.configurable,'IDL method descriptor');
            let bad=false;try{ext[name].call({});}catch(e){bad=e instanceof TypeError;}assert(bad,'receiver');
            bad=false;try{ext[name]();}catch(e){bad=e instanceof TypeError;}assert(bad,'minimum arguments');
        }
        if(api==='webgl')assert(gl.getExtension('ANGLE_instanced_arrays'),'implicit instancing');
    }
    "#);
}

#[test]
fn multi_draw_list_conversion_precedes_validation_and_ignores_typed_author_hooks() {
    run(r#"
    const gl=document.createElement('canvas').getContext('webgl2'),ext=gl.getExtension('WEBGL_multi_draw');
    const events=[],value=(name,result)=>({valueOf(){events.push(name);return result;}});
    const list=name=>({*[Symbol.iterator](){events.push(name);yield value(name+'0',0);}});
    ext.multiDrawArraysInstancedWEBGL(value('mode',gl.POINTS),list('first'),value('fo',0),
        list('count'),value('co',0),list('instance'),value('io',0),value('n',-1));
    if(events.join(',')!=='mode,first,first0,fo,count,count0,co,instance,instance0,io,n')throw Error('conversion order');
    if(gl.getError()!==gl.INVALID_VALUE)throw Error('negative draw count');
    const typed=new Int32Array([0]);typed[Symbol.iterator]=()=>{throw Error('typed iterator');};
    Object.defineProperty(typed,'length',{get(){throw Error('shadow length');}});
    Object.defineProperty(typed,'buffer',{get(){throw Error('shadow buffer');}});
    ext.multiDrawArraysWEBGL(gl.POINTS,typed,0,typed,0,-1);
    if(gl.getError()!==gl.INVALID_VALUE)throw Error('native typed brand');
    for(const offset of [-1,2**53,Infinity]) {
        ext.multiDrawArraysWEBGL(gl.POINTS,[0],offset,[0],0,-1);
        if(gl.getError()!==gl.INVALID_VALUE)throw Error('offset range');
    }
    let bad=false;try{ext.multiDrawArraysWEBGL(0n,[],0,[],0,0);}catch(e){bad=e instanceof TypeError;}
    if(!bad)throw Error('BigInt mode');
    bad=false;try{ext.multiDrawArraysWEBGL(gl.POINTS,[],0n,[],0,0);}catch(e){bad=e instanceof TypeError;}
    if(!bad)throw Error('BigInt offset');
    const detached=new Int32Array(1);structuredClone(detached.buffer,{transfer:[detached.buffer]});
    bad=false;try{ext.multiDrawArraysWEBGL(gl.POINTS,detached,0,[],0,0);}catch(e){bad=e instanceof TypeError;}
    if(!bad)throw Error('detached Int32Array');
    "#);
}

#[test]
fn multi_draw_real_draw_id_pixels_and_late_bad_offsets_are_atomic() {
    run(r#"
    const canvas=document.createElement('canvas');canvas.width=4;canvas.height=4;
    const gl=canvas.getContext('webgl2',{antialias:false}),ext=gl.getExtension('WEBGL_multi_draw');
    const compile=(type,source)=>{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);
        if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
    const p=gl.createProgram();gl.attachShader(p,compile(gl.VERTEX_SHADER,
        '#version 300 es\n#extension GL_ANGLE_multi_draw : require\nflat out int draw;void main(){draw=gl_DrawID;gl_Position=vec4(float(gl_DrawID)*0.5-0.75,0.25,0,1);gl_PointSize=1.0;}'));
    gl.attachShader(p,compile(gl.FRAGMENT_SHADER,
        '#version 300 es\nprecision highp float;flat in int draw;out vec4 color;void main(){color=draw==0?vec4(1,0,0,1):draw==1?vec4(0,1,0,1):vec4(0,0,1,1);}'));
    gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));gl.useProgram(p);
    const pixel=x=>{const d=new Uint8Array(4);gl.readPixels(x,2,1,1,gl.RGBA,gl.UNSIGNED_BYTE,d);return Array.from(d).join(',');};
    ext.multiDrawArraysWEBGL(gl.POINTS,[9,0,0,0],1,[9,0,1,1],1,3);
    if(pixel(0)!=='0,0,0,0'||pixel(1)!=='0,255,0,255'||pixel(2)!=='0,0,255,255')throw Error('draw ID empty slots');
    gl.drawArrays(gl.POINTS,0,1);if(pixel(0)!=='255,0,0,255')throw Error('ordinary draw ID');
    gl.clear(gl.COLOR_BUFFER_BIT);
    const largeFirsts=new Int32Array(100001),largeCounts=new Int32Array(100001);largeCounts[100000]=1;
    ext.multiDrawArraysWEBGL(gl.POINTS,largeFirsts,100000,largeCounts,100000,1);
    if(pixel(0)!=='255,0,0,255'||gl.getError()!==gl.NO_ERROR)throw Error('bounded large typed-list slice');
    gl.clear(gl.COLOR_BUFFER_BIT);
    const firsts=new Int32Array([0,0]),counts=new Int32Array([1,1]);
    // Chrome snapshots the typed union arm during argument conversion. Later
    // offset getters may mutate the source, but not the converted owned list.
    ext.multiDrawArraysWEBGL(gl.POINTS,firsts,0,counts,{valueOf(){firsts[1]=-1;return 0;}},2);
    if(gl.getError()!==gl.NO_ERROR||pixel(0)!=='255,0,0,255')throw Error('typed conversion snapshot');
    gl.clear(gl.COLOR_BUFFER_BIT);
    const b=gl.createBuffer();gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,b);gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,new Uint16Array([0]),gl.STATIC_DRAW);
    ext.multiDrawElementsWEBGL(gl.POINTS,[1,1],0,gl.UNSIGNED_SHORT,[0,1],0,2);
    if(gl.getError()!==gl.INVALID_OPERATION||pixel(0)!=='0,0,0,0')throw Error('late unaligned offset partially drew');
    ext.multiDrawElementsInstancedWEBGL(gl.POINTS,[1],0,gl.UNSIGNED_SHORT,[0],0,[1],0,1);
    if(pixel(0)!=='255,0,0,255'||gl.getError()!==gl.NO_ERROR)throw Error('indexed instanced pixels');
    "#);
}

#[test]
fn multi_draw_webgl1_shader_and_implicit_instancing_use_real_driver_ids() {
    run(r#"
    const canvas=document.createElement('canvas');canvas.width=4;canvas.height=4;
    const gl=canvas.getContext('webgl',{antialias:false}),ext=gl.getExtension('WEBGL_multi_draw');
    const compile=(type,source)=>{const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);
        if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;};
    const p=gl.createProgram();gl.attachShader(p,compile(gl.VERTEX_SHADER,
        '#extension GL_ANGLE_multi_draw : require\nattribute vec2 position;varying vec4 color;void main(){gl_Position=vec4(position.x+float(gl_DrawID)*0.5,position.y,0,1);gl_PointSize=1.0;color=gl_DrawID==0?vec4(1,0,0,1):vec4(0,1,0,1);}'));
    gl.attachShader(p,compile(gl.FRAGMENT_SHADER,
        'precision highp float;varying vec4 color;void main(){gl_FragColor=color;}'));
    gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));gl.useProgram(p);
    const b=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,b);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-.75,.25]),gl.STATIC_DRAW);
    const location=gl.getAttribLocation(p,'position');if(location<0)throw Error('original public attribute name');
    gl.enableVertexAttribArray(location);gl.vertexAttribPointer(location,2,gl.FLOAT,false,0,0);
    const pixel=x=>{const d=new Uint8Array(4);gl.readPixels(x,2,1,1,gl.RGBA,gl.UNSIGNED_BYTE,d);return Array.from(d).join(',');};
    ext.multiDrawArraysInstancedWEBGL(gl.POINTS,[0,0],0,[1,1],0,[1,1],0,2);
    if(pixel(0)!=='255,0,0,255'||pixel(1)!=='0,255,0,255')throw Error('WebGL1 instanced IDs');
    gl.clear(gl.COLOR_BUFFER_BIT);gl.getExtension('ANGLE_instanced_arrays').drawArraysInstancedANGLE(gl.POINTS,0,1,1);
    if(pixel(0)!=='255,0,0,255'||gl.getError()!==gl.NO_ERROR)throw Error('ordinary instanced ID reset');
    "#);
}

#[test]
fn multi_draw_loss_converts_arguments_and_stale_extension_is_inert_after_restore() {
    super::webgl_lifecycle::run(
        r#"
    const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl2');
    const ext=gl.getExtension('WEBGL_multi_draw'),loss=gl.getExtension('WEBGL_lose_context');
    canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();setTimeout(()=>loss.restoreContext(),0);});
    canvas.addEventListener('webglcontextrestored',()=>{
        const fresh=gl.getExtension('WEBGL_multi_draw');if(!fresh||fresh===ext)throw Error('new admission');
        ext.multiDrawArraysWEBGL(gl.POINTS,[],0,[],0,0);
        if(gl.getError()!==gl.NO_ERROR)throw Error('stale epoch used restored context');
        canvas.dataset.result='pass';
    });
    loss.loseContext();let count=0;const value={valueOf(){count++;return 0;}};
    ext.multiDrawArraysWEBGL(value,[],value,[],value,value);
    if(count!==4)throw Error('lost conversion');
    "#,
    );
}
