//! Genuine backing-store validation cannot be replaced by author properties.
use super::webgl2_bindings_tests::{check, document};

#[test]
fn webgl2_realm_detached_and_resizable_out_of_bounds_views_throw_before_upload() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2)), buffer=gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER,buffer); gl.bufferData(gl.ARRAY_BUFFER,new Uint8Array([7,8]),gl.STATIC_DRAW);
        const detached=new Uint8Array(2);
        structuredClone(detached.buffer,{transfer:[detached.buffer]});
        const resizable=new ArrayBuffer(8,{maxByteLength:16});
        const fixed=new Uint8Array(resizable,4,4), data=new DataView(resizable,4,4);
        resizable.resize(2);
        for (const view of [detached,fixed,data]) {
            for (const call of [()=>gl.bufferData(gl.ARRAY_BUFFER,view,gl.STATIC_DRAW),
                ()=>gl.bufferSubData(gl.ARRAY_BUFFER,0,view),
                ()=>gl.getBufferSubData(gl.ARRAY_BUFFER,0,view)]) {
                let threw=false; try { call(); } catch(e) { threw=e instanceof TypeError; }
                if (!threw) throw Error('invalid backing store accepted');
            }
        }
        const result=new Uint8Array(2); gl.getBufferSubData(gl.ARRAY_BUFFER,0,result);
        if ([...result].join()!=='7,8' || gl.getError()!==0) throw Error('failed view conversion mutated GPU storage');
        resizable.resize(8); fixed.set([1,2,3,4]);
        let rejected=false;try {gl.bufferData(gl.ARRAY_BUFFER,fixed,gl.STATIC_DRAW);}
        catch(error){rejected=error instanceof TypeError;}
        if(!rejected)throw Error('regrown resizable store must still reject');
        gl.bufferData(gl.ARRAY_BUFFER,new Uint8Array(fixed),gl.STATIC_DRAW);
        if (gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)!==4 || gl.getError()!==0) throw Error('regrown view did not recover');
    "#,
    );
}

#[test]
fn webgl2_realm_numeric_typed_lists_ignore_author_iterators_but_sequences_observe_them() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2));
        const values=new Uint32Array([1,2,3,0xffffffff]);
        values[Symbol.iterator]=()=>{throw Error('typed input iterator must not run');};
        gl.vertexAttribI4uiv(0,values);
        if ([...gl.getVertexAttrib(0,gl.CURRENT_VERTEX_ATTRIB)].join()!=='1,2,3,4294967295') throw Error('typed list read');
        const events=[];
        const sequence={*[Symbol.iterator](){events.push('iterator');for(let i=0;i<4;i++)yield {valueOf(){events.push(i);return i+5;}};}};
        gl.vertexAttribI4uiv(0,sequence);
        if (events.join()!=='iterator,0,1,2,3' || [...gl.getVertexAttrib(0,gl.CURRENT_VERTEX_ATTRIB)].join()!=='5,6,7,8')
            throw Error('sequence conversion order');
        if (gl.getError()!==0) throw Error('unexpected GPU error');
    "#,
    );
}

#[test]
fn webgl2_realm_byte_ranges_use_captured_intrinsics_after_prototype_changes() {
    let (mut context, _host) = document();
    check(
        &mut context,
        r#"
        const gl=__stageWebGl2(new OffscreenCanvas(2,2)), buffer=gl.createBuffer();
        const source=new Uint8Array([10,20,30,40]), destination=new Uint8Array(4);
        const prototype=Object.getPrototypeOf(Uint8Array.prototype);
        const originalSubarray=prototype.subarray, originalSet=prototype.set, originalIsView=ArrayBuffer.isView;
        try {
            prototype.subarray=()=>{throw Error('author subarray invoked');};
            prototype.set=()=>{throw Error('author set invoked');};
            ArrayBuffer.isView=()=>false;
            gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
            gl.bufferData(gl.ARRAY_BUFFER,source,gl.STATIC_DRAW,1,2);
            gl.getBufferSubData(gl.ARRAY_BUFFER,0,destination,1,2);
        } finally {
            prototype.subarray=originalSubarray; prototype.set=originalSet; ArrayBuffer.isView=originalIsView;
        }
        if ([...destination].join()!=='0,20,30,0' || gl.getError()!==0) throw Error('captured intrinsic range');
    "#,
    );
}
