//! Exercise real public realm admission through the shared Window/Worker bootstrap.
use super::*;
use crate::engine::script::{HostState, bootstrap, module_loader::WebModuleLoader};

pub(super) fn document() -> (Context, Rc<RefCell<HostState>>) {
    let host = Rc::new(RefCell::new(HostState::new(
        crate::engine::dom::parse("<!doctype html><body>").document,
        "https://example.test/",
        "UTF-8",
        Rc::new(WebModuleLoader::new()),
    )));
    let mut context = Context::new(HostBridge::Document(Rc::downgrade(&host))).unwrap();
    let source = staged_bootstrap(bootstrap::BROWSER_BOOTSTRAP);
    context.eval(Source::from_bytes(&source)).unwrap();
    (context, host)
}

pub(in crate::engine::script) fn staged_bootstrap(bootstrap: &str) -> String {
    let source = bootstrap.replace(
        "// Opaque names retain identity, realm ownership and restoration generation.",
        r#"
        globalThis.__stageWebGl2 = (canvas, requested) => {
            return canvas.getContext('webgl2',requested);
        };
        globalThis.__stageWebGl2Constructor = WebGL2RenderingContext;
        globalThis.__stageWebGl2Types = webGlObjectClasses;
        // Opaque names retain identity, realm ownership and restoration generation.
        "#,
    );
    assert_ne!(source, bootstrap);
    source
}

pub(super) fn check(context: &mut Context, code: &str) {
    context.eval(Source::from_bytes(code)).unwrap();
}

#[test]
fn webgl2_realm_core_constants_are_readonly_and_do_not_leak_into_webgl1() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2)), constructor=__stageWebGl2Constructor;
        const expected={RGBA8:0x8058,TIMEOUT_IGNORED:-1,INVALID_INDEX:4294967295,READ_FRAMEBUFFER:0x8ca8,
            DRAW_FRAMEBUFFER_BINDING:gl.FRAMEBUFFER_BINDING,RGBA32UI:0x8d70,HALF_FLOAT:0x140b,
            FLOAT_MAT2x3:0x8b65,TEXTURE_3D:0x806f,MAX_CLIENT_WAIT_TIMEOUT_WEBGL:0x9247};
        for (const [name,value] of Object.entries(expected)) {
            for (const target of [constructor,constructor.prototype]) {
                const descriptor=Object.getOwnPropertyDescriptor(target,name);
                if (!descriptor || descriptor.value!==value || descriptor.writable || descriptor.configurable || !descriptor.enumerable)
                    throw Error('wrong constant descriptor '+name);
            }
            if (gl[name]!==value) throw Error('instance constant');
        }
        if ('READ_FRAMEBUFFER' in WebGLRenderingContext.prototype || WebGL2RenderingContext!==constructor)
            throw Error('core enums leaked into WebGL1 or public interface differs');
        if (gl.getParameter(gl.MAX_CLIENT_WAIT_TIMEOUT_WEBGL)!==0 || gl.getError()!==0)
            throw Error('constant and real capability disagree');
    "#,
    );
}

#[test]
fn webgl2_realm_buffer_overloads_select_view_ranges_before_converting_arguments() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2)), buffer=gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
        gl.bufferData(gl.ARRAY_BUFFER,new Uint8Array([11,22,33]).buffer,gl.STATIC_DRAW);
        const out=new Uint8Array(3); gl.getBufferSubData(gl.ARRAY_BUFFER,0,out);
        if ([...out].join()!=='11,22,33') throw Error('three-argument BufferSource');
        for (const source of [3,null,new ArrayBuffer(3)]) {
            const events=[];
            const target={valueOf(){events.push('target');return gl.ARRAY_BUFFER;}};
            const usage={valueOf(){events.push('usage');return gl.STATIC_DRAW;}};
            let threw=false;
            try { gl.bufferData(target,source,usage,0); } catch(e) { threw=e instanceof TypeError; }
            if (!threw || events.join()!=='target') throw Error('four-argument view overload conversion order');
        }
        gl.bufferData(gl.ARRAY_BUFFER,new Uint8Array([99,7,8,98]),gl.STATIC_DRAW,1,2,Symbol());
        const range=new Uint8Array(2); gl.getBufferSubData(gl.ARRAY_BUFFER,0,range);
        if ([...range].join()!=='7,8') throw Error('view overload or ignored extra argument');
        let threw=false;
        try { gl.bufferSubData(gl.ARRAY_BUFFER,0,new ArrayBuffer(1),0); } catch(e) { threw=e instanceof TypeError; }
        if (!threw || gl.getError()!==0) throw Error('subdata view overload');
    "#,
    );
}

#[test]
fn webgl2_realm_prototype_is_distinct_and_public_admission_uses_real_native_storage() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const canvas = new OffscreenCanvas(4,4);
        const gl = __stageWebGl2(canvas, {preserveDrawingBuffer:true});
        if (!gl) throw Error('real GLES3 context required');
        if (WebGL2RenderingContext!==__stageWebGl2Constructor) throw Error('public interface identity');
        if (!(new OffscreenCanvas(1,1).getContext('webgl2') instanceof WebGL2RenderingContext))
            throw Error('public native canvas admission');
        if (gl instanceof WebGLRenderingContext) throw Error('WebGL2 incorrectly subclasses WebGL1');
        if (!(gl instanceof __stageWebGl2Constructor)) throw Error('wrong interface identity');
        if (!(gl.createBuffer() instanceof WebGLObject) || !(gl.createSampler() instanceof WebGLObject))
            throw Error('resource WebGLObject hierarchy');
        if (Object.prototype.toString.call(gl) !== '[object WebGL2RenderingContext]') throw Error('wrong tag');
        if (gl.canvas !== canvas || gl.drawingBufferWidth !== 4) throw Error('wrong drawing buffer');
        let threw = false; try { new __stageWebGl2Constructor(); } catch(e) { threw = e instanceof TypeError; }
        if (!threw) throw Error('illegal constructor accepted');
        gl.clearColor(1,0,0,1); gl.clear(gl.COLOR_BUFFER_BIT);
        const pixels = new Uint8Array(4); gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if ([...pixels].join() !== '255,0,0,255' || gl.getError() !== 0) throw Error('real pixel contract');
    "#,
    );
}

#[test]
fn webgl2_realm_native_objects_are_branded_and_peer_names_do_not_alias() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl = __stageWebGl2(new OffscreenCanvas(4,4));
        const peer = __stageWebGl2(new OffscreenCanvas(4,4));
        for (const [suffix,type] of [['Sampler','WebGLSampler'], ['Query','WebGLQuery'],
            ['TransformFeedback','WebGLTransformFeedback'], ['VertexArray','WebGLVertexArrayObject']]) {
            const value = gl['create'+suffix]();
            if (!(value instanceof __stageWebGl2Types[type])) throw Error('wrong '+type+' brand');
            if (Object.keys(value).length || Reflect.ownKeys(value).length) throw Error('native name leaked');
            let threw=false; try { gl['delete'+suffix]({}); } catch(e) { threw=e instanceof TypeError; }
            if (!threw) throw Error('forged '+type+' accepted');
            peer['delete'+suffix](value);
            if (peer.getError() !== gl.INVALID_OPERATION) throw Error('peer deletion accepted');
            gl['delete'+suffix](value); gl['delete'+suffix](value);
            if (gl['is'+suffix](value) !== false || gl.getError() !== 0) throw Error('deletion not idempotent');
        }
        const sampler = gl.createSampler();
        gl.bindSampler(0,sampler);
        gl.samplerParameteri(sampler,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
        if (gl.getSamplerParameter(sampler,gl.TEXTURE_MIN_FILTER) !== gl.NEAREST) throw Error('native sampler state');
        peer.bindSampler(0,sampler);
        if (peer.getError() !== gl.INVALID_OPERATION) throw Error('peer sampler bind accepted');
        const vao = gl.createVertexArray();
        gl.bindVertexArray(vao);
        if (!gl.isVertexArray(vao) || gl.getError() !== 0) throw Error('native vertex array state');
        const one = new OffscreenCanvas(1,1).getContext('webgl');
        let threw=false; try { gl.bindVertexArray.call(one,vao); } catch(e) { threw=e instanceof TypeError; }
        if (!threw) throw Error('WebGL1 accepted a borrowed core method');
    "#,
    );
}

#[test]
fn webgl2_realm_idl_converts_in_order_and_keeps_required_method_metadata() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl = __stageWebGl2(new OffscreenCanvas(4,4));
        const sampler = gl.createSampler(), order=[];
        const pname = {valueOf(){order.push('pname'); return gl.TEXTURE_MIN_FILTER;}};
        const value = {valueOf(){order.push('value'); return gl.NEAREST;}};
        if (gl.samplerParameteri(sampler,pname,value,Symbol()) !== undefined) throw Error('wrong return');
        if (order.join() !== 'pname,value') throw Error('conversion order');
        if (gl.samplerParameteri.length !== 3 || gl.samplerParameteri.name !== 'samplerParameteri') throw Error('IDL metadata');
        let threw=false; try { gl.samplerParameteri(sampler,gl.TEXTURE_MIN_FILTER); } catch(e) { threw=e instanceof TypeError; }
        if (!threw) throw Error('missing argument accepted');
        threw=false; try { gl.bindSampler(0n,sampler); } catch(e) { threw=e instanceof TypeError; }
        if (!threw) throw Error('BigInt enum accepted');
        if (gl.getError() !== 0) throw Error('IDL failure leaked GL error');
    "#,
    );
}

#[test]
fn webgl2_realm_buffer_ranges_and_owned_readback_preserve_view_neighbors() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl = __stageWebGl2(new OffscreenCanvas(4,4));
        const buffer = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
        const source = new Uint16Array([99,100,101,102,103,104]);
        if (gl.bufferData(gl.ARRAY_BUFFER,source,gl.STATIC_DRAW,1,4) !== undefined) throw Error('wrong bufferData return');
        if (gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE) !== 8) throw Error('element offset not honored');
        const read = new Uint16Array(8).fill(77);
        gl.getBufferSubData(gl.ARRAY_BUFFER,0,read,2,4);
        if ([...read].join() !== '77,77,100,101,102,103,77,77') throw Error('readback neighbors');
        const patch = new Uint16Array([9,10,11,12]);
        gl.bufferSubData(gl.ARRAY_BUFFER,2,patch,1,2);
        gl.getBufferSubData(gl.ARRAY_BUFFER,0,read,2,4);
        if ([...read].join() !== '77,77,100,10,11,103,77,77') throw Error('native subrange upload');
        gl.bindBufferBase(0x8a11,0,buffer);
        if (gl.getIndexedParameter(0x8a28,0) !== buffer || gl.getIndexedParameter(0x8a2a,0) !== 8) throw Error('indexed identity');
        gl.getBufferSubData(gl.ARRAY_BUFFER,0,read,9);
        if (gl.getError() !== gl.INVALID_VALUE) throw Error('invalid destination offset accepted');
        if ([...read].join() !== '77,77,100,10,11,103,77,77') throw Error('failed update touched bytes');
        if (gl.getError() !== 0) throw Error('unexpected GL error');
    "#,
    );
}

#[test]
fn webgl2_realm_buffer_views_use_intrinsic_ranges_not_author_properties() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl = __stageWebGl2(new OffscreenCanvas(4,4));
        const buffer = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
        const source = new Uint8Array([4,5,6,7]);
        for (const name of ['buffer','byteOffset','byteLength','BYTES_PER_ELEMENT'])
            Object.defineProperty(source,name,{get(){throw Error('author '+name+' getter called');}});
        gl.bufferData(gl.ARRAY_BUFFER,source,gl.STATIC_DRAW,1,2);
        const destination = new DataView(new ArrayBuffer(8),2,4);
        for (const name of ['buffer','byteOffset','byteLength'])
            Object.defineProperty(destination,name,{get(){throw Error('author '+name+' getter called');}});
        gl.getBufferSubData(gl.ARRAY_BUFFER,0,destination,1,2);
        if (destination.getUint8(0)!==0 || destination.getUint8(1)!==5 || destination.getUint8(2)!==6 || destination.getUint8(3)!==0)
            throw Error('DataView byte offsets');
        let threw=false; try { gl.bufferSubData(gl.ARRAY_BUFFER,0,null); } catch(e) { threw=e instanceof TypeError; }
        if (!threw || gl.getError()!==0) throw Error('nullable BufferSource accepted');
    "#,
    );
}

#[test]
fn webgl2_public_canvas_admission_locks_version_and_exposes_illegal_interface_constructors() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        for(const create of [()=>new OffscreenCanvas(2,2),()=>{
            const canvas=document.createElement('canvas');canvas.width=2;canvas.height=2;return canvas;
        }]) {
            const offscreen=create() instanceof OffscreenCanvas;
            for(const requested of offscreen?['webgl','webgl2']:['webgl','experimental-webgl','webgl2']) {
                const canvas=create(),gl=canvas.getContext(requested,{antialias:false});
                const two=requested==='webgl2',type=two?WebGL2RenderingContext:WebGLRenderingContext;
                if(!(gl instanceof type) || gl.canvas!==canvas) throw Error('wrong public interface '+requested);
                if(canvas.getContext(requested)!==gl) throw Error('context identity changed');
                if(canvas.getContext(two?'webgl':'webgl2')!==null || canvas.getContext('2d')!==null ||
                    canvas.getContext('bitmaprenderer')!==null) throw Error('context mode unlocked');
                if(offscreen) {
                    let error='';try{canvas.getContext('experimental-webgl2');}catch(value){error=value.name;}
                    if(error!=='TypeError')throw Error('Offscreen context identifier must be an enum');
                } else if(canvas.getContext('experimental-webgl2')!==null) throw Error('unsupported experimental alias');
                if(!offscreen && !two && canvas.getContext(requested==='webgl'?'experimental-webgl':'webgl')!==gl)
                    throw Error('WebGL1 aliases differ');
                if(two && gl instanceof WebGLRenderingContext) throw Error('WebGL2 is not a WebGL1 subclass');
                gl.getExtension('WEBGL_lose_context').loseContext();
                if(canvas.getContext(requested)!==gl || canvas.getContext(two?'webgl':'webgl2')!==null)
                    throw Error('context loss unlocked canvas mode');
            }
            const locked=create();locked.getContext('2d');
            if(locked.getContext('webgl2')!==null) throw Error('2D mode unlocked');
            const failed=create();
            if(failed.getContext('webgl2',{failIfMajorPerformanceCaveat:true})!==null)
                throw Error('software backend ignored performance caveat');
            if(!(failed.getContext('webgl') instanceof WebGLRenderingContext))
                throw Error('failed creation consumed canvas context mode');
        }
        for(const name of ['WebGLRenderingContext','WebGL2RenderingContext','WebGLObject','WebGLBuffer',
            'WebGLShader','WebGLProgram','WebGLTexture','WebGLFramebuffer','WebGLRenderbuffer',
            'WebGLUniformLocation','WebGLActiveInfo','WebGLShaderPrecisionFormat','WebGLQuery',
            'WebGLSampler','WebGLSync','WebGLTransformFeedback','WebGLVertexArrayObject']) {
            const type=globalThis[name],descriptor=Object.getOwnPropertyDescriptor(globalThis,name);
            if(type.name!==name || type.length!==0 || !descriptor.writable || !descriptor.configurable || descriptor.enumerable)
                throw Error('interface object descriptor '+name);
            let threw=false;try{new type();}catch(error){threw=error instanceof TypeError;}
            if(!threw) throw Error('illegal constructor '+name);
            threw=false;try{type();}catch(error){threw=error instanceof TypeError;}
            if(!threw) throw Error('illegal function call '+name);
        }
    "#,
    );
}
