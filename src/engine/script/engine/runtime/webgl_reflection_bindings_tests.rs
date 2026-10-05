//! Native reflection is returned through readonly, branded Web IDL records.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl_reflection_attributes_live_on_prototypes_with_unforgeable_receiver_checks() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2),{antialias:false});
        const vertex=gl.createShader(gl.VERTEX_SHADER),fragment=gl.createShader(gl.FRAGMENT_SHADER);
        gl.shaderSource(vertex,'#version 300 es\nin vec2 position;uniform float scale;void main(){gl_Position=vec4(position*scale,0,1);}');
        gl.shaderSource(fragment,'#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1);}');
        const program=gl.createProgram();
        for(const shader of [vertex,fragment]) {gl.compileShader(shader);gl.attachShader(program,shader);}
        gl.linkProgram(program);
        if(!gl.getProgramParameter(program,gl.LINK_STATUS)) throw Error(gl.getProgramInfoLog(program));
        const uniform=gl.getActiveUniform(program,0),attribute=gl.getActiveAttrib(program,0);
        const precision=gl.getShaderPrecisionFormat(gl.FRAGMENT_SHADER,gl.HIGH_FLOAT);
        if(uniform.name!=='scale'||uniform.type!==gl.FLOAT||uniform.size!==1 ||
            attribute.name!=='position'||attribute.type!==gl.FLOAT_VEC2||attribute.size!==1)
            throw Error('native reflection values changed');
        const inputs=[[WebGLActiveInfo,uniform,['size','type','name']],
            [WebGLActiveInfo,attribute,['size','type','name']],
            [WebGLShaderPrecisionFormat,precision,['rangeMin','rangeMax','precision']]];
        for(const [type,record,fields] of inputs) {
            if(!(record instanceof type)||Object.keys(record).length!==0||Reflect.ownKeys(record).length!==0)
                throw Error('reflection record exposes own implementation fields');
            if(record instanceof WebGLObject) throw Error('reflection is not a GPU resource');
            for(const field of fields) {
                const descriptor=Object.getOwnPropertyDescriptor(type.prototype,field);
                if(!descriptor || descriptor.set!==undefined || !descriptor.enumerable || !descriptor.configurable ||
                    typeof descriptor.get!=='function' || descriptor.get.length!==0 || descriptor.get.name!=='get '+field)
                    throw Error('IDL readonly attribute descriptor '+field);
                const expected=record[field];
                if(descriptor.get.call(record)!==expected) throw Error('branded getter');
                for(const receiver of [{},type.prototype,Object.create(type.prototype),new Proxy(record,{})]) {
                    let threw=false;try {descriptor.get.call(receiver);}catch(error){threw=error instanceof TypeError;}
                    if(!threw) throw Error('forged reflection receiver '+field);
                }
                const other=type===WebGLActiveInfo?precision:uniform;
                let threw=false;try {descriptor.get.call(other);}catch(error){threw=error instanceof TypeError;}
                if(!threw) throw Error('cross-interface getter '+field);
                threw=false;try{(()=>{'use strict';record[field]='changed';})();}catch(error){threw=error instanceof TypeError;}
                if(!threw || record[field]!==expected) throw Error('readonly native record '+field);
            }
        }
        const nameGetter=Object.getOwnPropertyDescriptor(WebGLActiveInfo.prototype,'name').get;
        Object.defineProperty(uniform,'name',{value:'author shadow',configurable:true});
        if(uniform.name!=='author shadow'||nameGetter.call(uniform)!=='scale') throw Error('shadow modified native slot');
        delete uniform.name;Object.setPrototypeOf(uniform,null);
        if(nameGetter.call(uniform)!=='scale') throw Error('prototype mutation removed genuine brand');
        gl.deleteProgram(program);
        if(nameGetter.call(uniform)!=='scale'||attribute.name!=='position') throw Error('record depends on retired program');
        const extension=gl.getExtension('WEBGL_lose_context');extension.loseContext();
        if(nameGetter.call(uniform)!=='scale'||precision.precision<=0) throw Error('context loss invalidated reflection snapshot');
    "#,
    );
}

#[test]
fn webgl_reflection_getters_do_not_consult_author_weakmap_hooks() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=new OffscreenCanvas(2,2).getContext('webgl');
        const precision=gl.getShaderPrecisionFormat(gl.FRAGMENT_SHADER,gl.HIGH_FLOAT);
        const getter=Object.getOwnPropertyDescriptor(WebGLShaderPrecisionFormat.prototype,'precision').get;
        const expected=getter.call(precision),get=WeakMap.prototype.get;
        WeakMap.prototype.get=function(){return {type:'WebGLShaderPrecisionFormat',values:{precision:999}};};
        try {
            if(getter.call(precision)!==expected) throw Error('author hook replaced native precision');
            let threw=false;try{getter.call(Object.create(WebGLShaderPrecisionFormat.prototype));}
            catch(error){threw=error instanceof TypeError;}
            if(!threw) throw Error('author hook forged reflection brand');
        } finally {WeakMap.prototype.get=get;}
    "#,
    );
}

#[test]
fn webgl_context_operations_and_attributes_have_webidl_property_descriptors() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        for(const prototype of [WebGLRenderingContext.prototype,__stageWebGl2Constructor.prototype]) {
            for(const name of Object.getOwnPropertyNames(prototype)) {
                if(name==='constructor') continue;
                const descriptor=Object.getOwnPropertyDescriptor(prototype,name);
                if(typeof descriptor.value==='number') continue;
                if(!descriptor.enumerable||!descriptor.configurable) throw Error('IDL member descriptor '+name);
                if(typeof descriptor.value==='function' && (!descriptor.writable||descriptor.value.name!==name))
                    throw Error('IDL operation descriptor '+name);
                if(descriptor.get && descriptor.set!==undefined) throw Error('readonly context attribute '+name);
            }
        }
        for(const constructor of [WebGLRenderingContext,__stageWebGl2Constructor,WebGLActiveInfo,WebGLShaderPrecisionFormat]) {
            let threw=false;try{new constructor();}catch(error){threw=error instanceof TypeError;}
            if(!threw) throw Error('illegal constructor available');
        }
        const event=new WebGLContextEvent('webglcontextlost',{statusMessage:'native failure'});
        const message=Object.getOwnPropertyDescriptor(WebGLContextEvent.prototype,'statusMessage');
        if(!message.enumerable||message.set!==undefined||message.get.call(event)!=='native failure')
            throw Error('readonly context event attribute');
    "#,
    );
}

#[test]
fn webgl_native_context_resource_and_extension_brands_ignore_author_weakmap_replacement() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2),{antialias:false});
        const buffer=gl.createBuffer(),extension=gl.getExtension('WEBGL_lose_context');
        const event=new WebGLContextEvent('native',{statusMessage:'real message'});
        const status=Object.getOwnPropertyDescriptor(WebGLContextEvent.prototype,'statusMessage').get;
        const original={get:WeakMap.prototype.get,set:WeakMap.prototype.set,
            has:WeakMap.prototype.has,delete:WeakMap.prototype.delete,constructor:WeakMap};
        let consulted=0;
        WeakMap.prototype.get=function(){consulted++;return {id:1,type:'WebGLBuffer',context:gl,epoch:0,api:'webgl2'};};
        WeakMap.prototype.set=function(){consulted++;};WeakMap.prototype.has=function(){consulted++;return true;};
        WeakMap.prototype.delete=function(){consulted++;return true;};
        globalThis.WeakMap=function(){consulted++;throw Error('author constructor');};
        try {
            for(const receiver of [{},Object.create(__stageWebGl2Constructor.prototype),new Proxy(gl,{})]) {
                let threw=false;try{__stageWebGl2Constructor.prototype.createBuffer.call(receiver);}
                catch(error){threw=error instanceof TypeError;}
                if(!threw) throw Error('forged context receiver');
            }
            for(const resource of [{},Object.create(WebGLBuffer.prototype),new Proxy(buffer,{})]) {
                let threw=false;try{gl.bindBuffer(gl.ARRAY_BUFFER,resource);}catch(error){threw=error instanceof TypeError;}
                if(!threw) throw Error('forged resource argument');
            }
            gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,16,gl.STATIC_DRAW);
            if(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)!==16) throw Error('genuine native resource');
            const next=gl.createBuffer();gl.deleteBuffer(next);
            if(gl.isBuffer(next)) throw Error('retired resource brand');
            if(status.call(event)!=='real message') throw Error('forged event status');
            let threw=false;try{status.call({});}catch(error){threw=error instanceof TypeError;}
            if(!threw) throw Error('forged event receiver');
            threw=false;try{Object.getPrototypeOf(extension).loseContext.call({});}
            catch(error){threw=error instanceof TypeError;}
            if(!threw) throw Error('forged extension receiver');
            if(consulted!==0 || gl.getError()!==0) throw Error('private brand consulted author hooks');
        } finally {
            globalThis.WeakMap=original.constructor;
            for(const name of ['get','set','has','delete']) WeakMap.prototype[name]=original[name];
        }
        extension.loseContext();
        if(!gl.isContextLost()) throw Error('genuine extension brand lost');
    "#,
    );
}
