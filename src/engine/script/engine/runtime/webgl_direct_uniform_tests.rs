//! A copy can move later only when no later argument conversion runs author code.
use super::webgl_numeric_command_tests::PROGRAM;
use super::webgl_owned_copy_tests::both;

#[test]
fn webgl_small_uniform_views_keep_byte_offsets_ranges_and_owned_queue_values() {
    both(&format!(
        r#"{PROGRAM}
        const backing=new Float32Array([99,.25,.5,.75,1,98]);
        const source=new Float32Array(backing.buffer,4,4);
        gl.uniform4fv(color,source);
        backing.fill(17);
        assert(String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','two-argument nonzero byte offset');
        const selected=new Float32Array([91,.125,.25,.5,1,92]);
        gl.uniform4fv(color,selected,1,4);selected.fill(18);
        assert(String(gl.getUniform(p,color))==='0.125,0.25,0.5,1','numeric selected range');
        const matrices=new Float32Array([91,1,2,3,4,5,6,92]);
        const view=new Float32Array(matrices.buffer,4,6);
        gl.uniformMatrix2x3fv(matrix,false,view);matrices.fill(19);
        assert(String(gl.getUniform(p,matrix))==='1,2,3,4,5,6','matrix implicit range');
        const selectedMatrix=new Float32Array([91,6,5,4,3,2,1,92]);
        gl.uniformMatrix2x3fv(matrix,false,selectedMatrix,1,6);selectedMatrix.fill(20);
        assert(String(gl.getUniform(p,matrix))==='6,5,4,3,2,1','matrix explicit numeric range');
        assert(gl.getError()===0,'owned native upload errors');
    "#
    ));
}

#[test]
fn webgl_observable_uniform_tails_keep_the_early_snapshot_and_conversion_order() {
    both(&format!(
        r#"{PROGRAM}
        const floats=new Float32Array([.25,.5,.75,1]),order=[];
        gl.uniform4fv(color,floats,{{valueOf(){{order.push('offset');floats.fill(0);return 0}}}},
            {{valueOf(){{order.push('length');floats.buffer.transfer();return 4}}}});
        assert(String(order)==='offset,length','tail conversion order');
        assert(String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','snapshot before mutating/detaching tail');
        const data=new Float32Array([1,2,3,4,5,6]);
        gl.uniformMatrix2x3fv(matrix,false,data,0,{{valueOf(){{data.fill(0);return 6}}}});
        assert(String(gl.getUniform(p,matrix))==='1,2,3,4,5,6','numeric offset does not authorize object length');
        let entries=0,iterators=0;
        const sequence={{[Symbol.iterator](){{iterators++;let n=0;return {{next(){{
            return n===4?{{done:true}}:{{done:false,value:{{valueOf(){{entries++;return ++n}}}}}};
        }}}};}}}};
        gl.uniform4fv(color,sequence,0,4);
        assert(iterators===1&&entries===4&&String(gl.getUniform(p,color))==='1,2,3,4','sequence remains observable once');
        const unchanged=String(gl.getUniform(p,color)),thrown={{token:17}};
        let caught=false;
        try{{gl.uniform4fv(color,new Float32Array([9,9,9,9]),{{valueOf(){{throw thrown}}}},4)}}
        catch(error){{caught=error===thrown}}
        assert(caught&&String(gl.getUniform(p,color))===unchanged,'throwing conversion queued no upload');
        assert(gl.getError()===0,'observable tail errors');
    "#
    ));
}

#[test]
fn webgl_intrinsic_scalar_conversion_cannot_mutate_a_uniform_through_author_helpers() {
    both(&format!(
        r#"{PROGRAM}
        const source=new Float32Array([.25,.5,.75,1]),data=new Float32Array([1,2,3,4,5,6]);
        const number=Number,boolean=Boolean,bigint=BigInt,finite=Number.isFinite,
            trunc=Math.trunc,fround=Math.fround,uintN=BigInt.asUintN,intN=BigInt.asIntN,
            includes=String.prototype.includes;
        let hooks=0;
        const fail=()=>{{hooks++;source.fill(0);data.fill(0);throw Error('author intrinsic')}};
        Number.isFinite=BigInt.asUintN=BigInt.asIntN=Math.trunc=Math.fround=fail;
        globalThis.Number=globalThis.Boolean=globalThis.BigInt=fail;
        String.prototype.includes=fail;
        try {{
            gl.uniform4fv(color,source,0,4);
            gl.uniformMatrix2x3fv(matrix,false,data,0,6);
            gl.viewport(0,0,2,2);
        }} finally {{
            globalThis.Number=number;globalThis.Boolean=boolean;globalThis.BigInt=bigint;
            Number.isFinite=finite;Math.trunc=trunc;Math.fround=fround;
            BigInt.asUintN=uintN;BigInt.asIntN=intN;
            String.prototype.includes=includes;
        }}
        assert(!hooks&&String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','captured uniform scalar conversions');
        assert(String(gl.getUniform(p,matrix))==='1,2,3,4,5,6','transpose conversion is intrinsic');
        assert(gl.getError()===0,'intrinsic conversion errors');
    "#
    ));
}

#[test]
fn webgl_optional_argument_vectors_ignore_inherited_array_slots() {
    both(&format!(
        r#"{PROGRAM}
        const source=new Float32Array([.25,.5,.75,1]),data=new Float32Array([1,2,3,4,5,6]);
        const old3=Object.getOwnPropertyDescriptor(Array.prototype,'3'),
            old4=Object.getOwnPropertyDescriptor(Array.prototype,'4');
        let hooks=0;
        const fail=()=>{{hooks++;source.fill(0);throw Error('inherited optional argument')}};
        Object.defineProperty(Array.prototype,'3',{{configurable:true,get:fail,set:fail}});
        Object.defineProperty(Array.prototype,'4',{{configurable:true,get:fail,set:fail}});
        try {{
            gl.uniform4fv(color,source);
            gl.uniformMatrix2x3fv(matrix,false,data);
        }} finally {{
            if(old3)Object.defineProperty(Array.prototype,'3',old3);else delete Array.prototype[3];
            if(old4)Object.defineProperty(Array.prototype,'4',old4);else delete Array.prototype[4];
        }}
        assert(!hooks&&String(gl.getUniform(p,color))==='0.25,0.5,0.75,1','missing offset/length are undefined');
        assert(String(gl.getUniform(p,matrix))==='1,2,3,4,5,6','matrix private argument vector');
        assert(gl.getError()===0,'private vector errors');
    "#
    ));
}
