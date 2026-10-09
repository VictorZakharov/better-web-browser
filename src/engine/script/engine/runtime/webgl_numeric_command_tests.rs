//! Closed transport admission, liveness and real public Web IDL semantics.
use super::webgl_owned_copy_tests::both;
use super::webgl2_bindings_tests::check;
use super::*;
use crate::engine::script::{HostState, module_loader::WebModuleLoader};

fn raw() -> (Context, Rc<RefCell<HostState>>, u32) {
    let host = Rc::new(RefCell::new(HostState::new(
        crate::engine::dom::parse("<!doctype html><body>").document,
        "https://example.test/",
        "UTF-8",
        Rc::new(WebModuleLoader::new()),
    )));
    let context = Context::new(HostBridge::Document(Rc::downgrade(&host))).unwrap();
    let id = host.borrow_mut().webgl.create(2, 2, "{}").unwrap();
    (context, host, id)
}

#[test]
fn webgl_numeric_transport_rejects_nonprimitive_unbounded_or_reply_commands() {
    let (mut context, _host, id) = raw();
    check(&mut context, &format!("const id={id};"));
    check(
        &mut context,
        r#"
        let coercions=0;
        const object={valueOf(){coercions++;return 1}};
        const rejected=(args)=>{
            let threw=false;
            try{__hostCall('webglCommandValues',...args)}catch(e){threw=e instanceof TypeError}
            if(!threw)throw Error('invalid private command accepted');
        };
        for(const args of [
            [id,'getError',[],[]],[id,'createBuffer',[],[]],[id,'finish',[],[]],
            [id,'shaderSource',[],[]],[id,'probe',[],[]],[id,'clearSuffix',[],[]],
            [id,'clear',new Array(65).fill(0),[]],[id,'clear',[],new Array(65).fill(0)],
            [id,'clear',new Array(33).fill(0),new Array(32).fill(0)],
            [id,'clear',[1.5],[]],[id,'clear',[NaN],[]],[id,'clear',[Infinity],[]],
            [id,'clear',[2**53],[]],[id,'clear',[object],[]],[id,'clear',[],[object]],
            [id,'clear',[],[null]],[id,'clear',[],['1']],[id,'clear',[],[1n]],
            [id,'clear',new Float32Array(1),[]],[id,'clear',[],new Float64Array(1)],
            [id,'clear',[],new Int32Array(1)],[id,'clear',[],new DataView(new ArrayBuffer(4))],
            [id,'clear',[],new Float32Array(65)],
            [id,'clear',[],new Float32Array(new ArrayBuffer(8,{maxByteLength:16}))],
            [id,'clear',new Proxy([],{}),[]],[id,'clear',[],new Proxy([], {})],
            [id,'clear',{},[]],[id,'clear',[],{}],[id,'clear',null,[]],
            [0,'clear',[],[]],[NaN,'clear',[],[]],[Infinity,'clear',[],[]],
            [2**32,'clear',[],[]],[object,'clear',[],[]],
            [id,object,[],[]],[id,'x'.repeat(65),[],[]],[id,'clear',[],[],0],
            [id,'clear',[]]
        ]) rejected(args);
        if(coercions)throw Error('private transport coerced author objects');
        if(__hostCall('webglCommand',id,'{"op":"getError"}')!=='0')throw Error('rejected command changed GL errors');
    "#,
    );
}

#[test]
fn webgl_numeric_transport_freezes_lengths_before_reentrant_property_reads() {
    let (mut context, _host, id) = raw();
    check(&mut context, &format!("const id={id};"));
    check(
        &mut context,
        r#"
        const floats=[0,1,0,1], integers=[];
        Object.defineProperty(integers,0,{get(){floats.length=1000000;return 0;}});
        let threw=false;
        try{__hostCall('webglCommandValues',id,'clearColor',integers,floats)}catch(e){threw=e instanceof TypeError}
        if(!threw)throw Error('second list grew during first list read');
        let reads=0;
        const nested=[];
        Object.defineProperty(nested,0,{get(){
            reads++;
            if(__hostCall('webglCommand',id,'{"op":"getError"}')!=='0')throw Error('nested read');
            return 16384;
        }});
        if(!__hostCall('webglCommandValues',id,'clear',nested,[]) || reads!==1)throw Error('safe reentrant read');
        const retired=[];
        Object.defineProperty(retired,0,{get(){__hostCall('webglDestroy',id);return 16384;}});
        if(__hostCall('webglCommandValues',id,'clear',retired,[])!==false)throw Error('retired context remained usable');
    "#,
    );
}

#[test]
fn webgl_numeric_transport_keeps_host_liveness() {
    let (mut context, host, id) = raw();
    drop(host);
    let result = context
        .eval(Source::from_bytes(format!(
            "__hostCall('webglCommandValues',{id},'clearColor',[],[0,1,0,1]);"
        )))
        .unwrap_err();
    assert!(result.message.contains("not active"));
}

pub(super) const PROGRAM: &str = r#"
    const gl=new OffscreenCanvas(2,2).getContext('webgl2',{antialias:false,preserveDrawingBuffer:true});
    const assert=(value,label)=>{if(!value)throw Error(label)};
    const p=gl.createProgram();
    for(const [kind,source] of [
        [gl.VERTEX_SHADER,'#version 300 es\nuniform mat2x3 m;void main(){gl_Position=vec4(m[0].x+.5,m[1].y+.5,0,1);gl_PointSize=1.;}'],
        [gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;uniform vec4 c;uniform highp uvec4 u;out vec4 outColor;void main(){outColor=c+vec4(u)*0.000001;}']
    ]) {
        const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
        assert(gl.getShaderParameter(s,gl.COMPILE_STATUS),gl.getShaderInfoLog(s));gl.attachShader(p,s);
    }
    gl.linkProgram(p);assert(gl.getProgramParameter(p,gl.LINK_STATUS),gl.getProgramInfoLog(p));gl.useProgram(p);
    const color=gl.getUniformLocation(p,'c'),matrix=gl.getUniformLocation(p,'m'),unsigned=gl.getUniformLocation(p,'u');
    assert(color&&matrix&&unsigned,'active uniform locations');
"#;

#[test]
fn webgl_numeric_private_float_view_copies_only_its_owned_selected_range() {
    let (mut context, _host, id) = raw();
    check(&mut context, &format!("const id={id};"));
    check(
        &mut context,
        r#"
        const backing=new Float32Array([99,0.25,0.5,0.75,1,98]);
        const view=new Float32Array(backing.buffer,4,4);
        Object.defineProperties(view,{length:{get(){throw Error('author length')}},
            byteOffset:{get(){throw Error('author offset')}},buffer:{get(){throw Error('author buffer')}}});
        if(!__hostCall('webglCommandValues',id,'clearColor',[],view))throw Error('fixed view rejected');
        backing.fill(0);
        if(__hostCall('webglCommand',id,'{"op":"getParameter","i":[3106]}')!=='[0.25,0.5,0.75,1.0]')
            throw Error('queued clear color was not an independent offset copy');
        if(__hostCall('webglCommand',id,'{"op":"getError"}')!=='0')throw Error('typed command errors');
    "#,
    );
}

#[test]
fn webgl_numeric_private_view_rejects_detached_shared_and_reentrant_detach() {
    let (mut context, _host, id) = raw();
    check(&mut context, &format!("const id={id};"));
    check(
        &mut context,
        r#"
        const rejects=(integers,floats)=>{
            let threw=false;
            try{__hostCall('webglCommandValues',id,'clearColor',integers,floats)}catch(e){threw=e instanceof TypeError}
            if(!threw)throw Error('unsafe float view accepted');
        };
        const detached=new Float32Array(4);detached.buffer.transfer();rejects([],detached);
        rejects([],new Float32Array(new SharedArrayBuffer(16)));
        rejects([],new Proxy(new Float32Array(4),{}));
        const floats=new Float32Array([0,1,0,1]),integers=[];
        Object.defineProperty(integers,0,{get(){floats.buffer.transfer();return 0;}});
        rejects(integers,floats);
        if(__hostCall('webglCommand',id,'{"op":"getError"}')!=='0')throw Error('rejected typed command reached GL');
    "#,
    );
}

#[test]
fn webgl_numeric_uniform_snapshot_never_exposes_private_lists_to_author_intrinsics() {
    both(&format!(
        r#"{PROGRAM}
        const floats=new Float32Array([99,0.25,0.5,0.75,1,98]);
        const matrixValues=new Float32Array([99,1,2,3,4,5,6,98]);
        const apply=Reflect.apply,push=Array.prototype.push,find=Array.prototype.find;
        const typed=Object.getPrototypeOf(Float32Array.prototype);
        const lengthDescriptor=Object.getOwnPropertyDescriptor(typed,'length');
        const species=Object.getOwnPropertyDescriptor(Float32Array,Symbol.species);
        let hooks=0,actual,matrixActual;
        const fail=()=>{{hooks++;throw Error('private upload exposed to author hook')}};
        Reflect.apply=Array.prototype.push=Array.prototype.find=fail;
        Object.defineProperty(typed,'length',{{configurable:true,get:fail}});
        Object.defineProperty(Float32Array,Symbol.species,{{configurable:true,get:fail}});
        try {{
            gl.uniform4fv(color,floats,1,4);gl.uniformMatrix2x3fv(matrix,false,matrixValues,1,6);
            actual=gl.getUniform(p,color);matrixActual=gl.getUniform(p,matrix);
        }} finally {{
            Reflect.apply=apply;Array.prototype.push=push;Array.prototype.find=find;
            Object.defineProperty(typed,'length',lengthDescriptor);
            if(species)Object.defineProperty(Float32Array,Symbol.species,species);
            else delete Float32Array[Symbol.species];
        }}
        assert(!hooks&&String(actual)==='0.25,0.5,0.75,1'&&String(matrixActual)==='1,2,3,4,5,6','closed uniform snapshot');
        assert(gl.getError()===0,'closed upload errors');
    "#
    ));
}

#[test]
fn webgl_numeric_public_uploads_snapshot_typed_ranges_and_preserve_special_values() {
    both(&format!(
        r#"{PROGRAM}
        const floats=new Float32Array([99,0.25,0.5,0.75,1,98]);
        const integers=new Uint32Array([99,0xffffffff,0x80000000,7,3,98]);
        const values=new Float32Array([99,1,2,3,4,5,6,98]);
        gl.uniform4fv(color,floats,1,4);gl.uniform4uiv(unsigned,integers,1,4);
        gl.uniformMatrix2x3fv(matrix,false,values,1,6);
        floats.fill(17);integers.fill(17);values.fill(17);
        assert(String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','independent float snapshot');
        assert(String(gl.getUniform(p,unsigned))==='4294967295,2147483648,7,3','all unsigned bits');
        assert(String(gl.getUniform(p,matrix))==='1,2,3,4,5,6','nonsquare column-major matrix');
        gl.uniform4fv(color,new Float32Array([-0,NaN,Infinity,-Infinity]));
        const special=gl.getUniform(p,color);
        assert(Object.is(special[0],-0)&&Number.isNaN(special[1])&&special[2]===Infinity&&special[3]===-Infinity,'unrestricted float wire');
        gl.uniform4fv(color,new Float32Array([0,1,0,1]));gl.uniform4uiv(unsigned,new Uint32Array(4));
        gl.uniformMatrix2x3fv(matrix,false,new Float32Array(6));
        gl.drawArrays(gl.POINTS,0,1);
        const pixels=new Uint8Array(4);gl.readPixels(1,1,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        assert(String(pixels)==='0,255,0,255','draw observes queued uniforms');
        assert(gl.getError()===0,'numeric public upload errors');
    "#
    ));
}

#[test]
fn webgl_numeric_public_conversion_runs_once_before_later_argument_mutation() {
    both(&format!(
        r#"{PROGRAM}
        const source=new Float32Array([0.25,0.5,0.75,1]);let calls=0;
        gl.uniform4fv(color,source,{{valueOf(){{calls++;source.fill(0);return 0;}}}},4);
        assert(calls===1&&String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','snapshot before offset conversion');
        let iterators=0,entries=0;
        const list={{[Symbol.iterator](){{iterators++;let n=0;return {{next(){{return n===4?{{done:true}}:
            {{done:false,value:{{valueOf(){{entries++;return ++n;}}}}}};}}}};}}}};
        gl.uniform4fv(color,list);
        assert(iterators===1&&entries===4&&String(gl.getUniform(p,color))==='1,2,3,4','one sequence conversion');
        assert(gl.getError()===0,'ordered conversion errors');
    "#
    ));
}

#[test]
fn webgl_numeric_whole_and_offset_views_copy_internal_ranges_without_author_reads() {
    both(&format!(
        r#"{PROGRAM}
        const backing=new Float32Array([99,.25,.5,.75,1,98]);
        const source=new Float32Array(backing.buffer,4,4);
        let reads=0;
        const fail=()=>{{reads++;throw Error('typed range consulted author hook')}};
        Object.defineProperties(source,{{length:{{get:fail}},byteOffset:{{get:fail}},
            byteLength:{{get:fail}},buffer:{{get:fail}},[Symbol.iterator]:{{value:fail}}}});
        gl.uniform4fv(color,source);
        backing.fill(17);
        assert(!reads&&String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','whole owned view');
        const floats=new Float32Array([99,.125,.25,.5,1,98]);
        gl.uniform4fv(color,floats,1,4);floats.fill(3);
        assert(String(gl.getUniform(p,color))==='0.125,0.25,0.5,1','selected owned view');
        const shared=new Float32Array(new SharedArrayBuffer(16));
        shared.set([.5,.75,.875,1]);gl.uniform4fv(color,shared);shared.fill(0);
        assert(String(gl.getUniform(p,color))==='0.5,0.75,0.875,1','shared source copied before queueing');
        gl.uniform4fv(color,new Float32Array(0));
        assert(gl.getError()===gl.INVALID_VALUE,'live empty list is invalid WebGL data');
        gl.uniform4uiv(unsigned,new Uint32Array(0));
        assert(gl.getError()===gl.INVALID_VALUE,'empty unsigned vector');
        gl.uniformMatrix2x3fv(matrix,false,new Float32Array(0));
        assert(gl.getError()===gl.INVALID_VALUE,'empty nonsquare matrix');
        gl.uniform4fv(null,new Float32Array(0));
        assert(gl.getError()===0,'null location still ignores empty data');
        const detached=new Float32Array(4);detached.buffer.transfer();
        gl.uniform4fv(color,detached);
        assert(gl.getError()===gl.INVALID_VALUE,'detached typed list remains a GL error');
    "#
    ));
}

#[test]
fn webgl_numeric_large_uniform_lists_use_the_bounded_ordinary_transport() {
    both(
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        const p=gl.createProgram();
        for(const [kind,source] of [
            [gl.VERTEX_SHADER,'#version 300 es\nuniform mat4 matrices[5];void main(){gl_Position=(matrices[0]+matrices[1]+matrices[2]+matrices[3]+matrices[4])*vec4(1.);}'],
            [gl.FRAGMENT_SHADER,'#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1.);}']
        ]) {
            const s=gl.createShader(kind);gl.shaderSource(s,source);gl.compileShader(s);
            if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));gl.attachShader(p,s);
        }
        gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));gl.useProgram(p);
        const location=gl.getUniformLocation(p,'matrices[0]');
        const data=Float32Array.from({length:80},(_,i)=>i/4);
        gl.uniformMatrix4fv(location,false,data);data.fill(99);
        for(let i=0;i<5;i++) {
            const actual=gl.getUniform(p,gl.getUniformLocation(p,'matrices['+i+']'));
            for(let j=0;j<16;j++)if(actual[j]!==(i*16+j)/4)throw Error('fallback list component');
        }
        if(gl.getError()!==0)throw Error('large valid numeric fallback');
    "#,
    );
}
