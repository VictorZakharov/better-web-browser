use super::*;

#[test]
fn window_clone_preserves_intrinsic_view_kinds_offsets_and_shared_storage() {
    let (_, outcome) = execute_html(
        r#"<script>
        const constructors=[Int8Array,Uint8Array,Uint8ClampedArray,Int16Array,Uint16Array,
            Int32Array,Uint32Array,Float16Array,Float32Array,Float64Array,BigInt64Array,BigUint64Array];
        for(const C of constructors) {
            const buffer=new ArrayBuffer(64),size=C.BYTES_PER_ELEMENT;
            const view=new C(buffer,size*2,3),mirror=new C(buffer,size*2,3),data=new DataView(buffer);
            view[0]=C===BigInt64Array||C===BigUint64Array?17n:17;
            const copy=structuredClone({view,mirror,data,buffer,again:view});
            if(Object.getPrototypeOf(copy.view)!==C.prototype||copy.view.byteOffset!==size*2||
                copy.view.length!==3||copy.again!==copy.view||copy.view.buffer!==copy.buffer||
                copy.mirror.buffer!==copy.buffer||copy.data.buffer!==copy.buffer||copy.buffer===buffer)
                throw Error(`view metadata ${C.name}`);
            copy.view[0]=C===BigInt64Array||C===BigUint64Array?29n:29;
            if(copy.mirror[0]!==copy.view[0]||view[0]==copy.view[0])throw Error('view alias mutation');
        }
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn empty_offset_views_do_not_lose_their_nonempty_backing_buffer() {
    let (_, outcome) = execute_html(
        r#"<script>
        const buffer=new ArrayBuffer(16),view=new Uint8Array(buffer,16,0),data=new DataView(buffer,16,0);
        new Uint8Array(buffer)[15]=83;
        const copy=structuredClone([view,data,buffer]);
        if(copy[0].byteOffset!==16||copy[1].byteOffset!==16||copy[0].length!==0||
            copy[1].byteLength!==0||copy[0].buffer!==copy[2]||copy[1].buffer!==copy[2]||
            new Uint8Array(copy[2])[15]!==83)throw Error('empty view backing storage');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn clone_validation_does_not_detach_transfers_when_the_graph_fails() {
    let (_, outcome) = execute_html(
        r#"<script>
        const buffer=new ArrayBuffer(16),view=new Uint8Array(buffer,4,5);view[0]=41;
        let error;
        try{structuredClone({view,buffer,bad:()=>{}},{transfer:[buffer]});}catch(e){error=e;}
        if(error?.name!=='DataCloneError'||buffer.byteLength!==16||view[0]!==41)
            throw Error('graph failure detached backing storage');
        const copy=structuredClone({view,buffer},{transfer:[buffer]});
        if(buffer.byteLength!==0||view.byteLength!==0||copy.view.buffer!==copy.buffer||copy.view[0]!==41)
            throw Error('validated transfer failed');
        try{structuredClone(buffer);throw Error('detached buffer accepted');}
        catch(e){if(e.name!=='DataCloneError')throw e;}
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn forged_arraybuffer_prototypes_do_not_fabricate_clone_storage() {
    let (_, outcome) = execute_html(
        r#"<script>
        const fake=Object.create(ArrayBuffer.prototype);
        let read=false;
        Object.defineProperty(fake,'length',{get(){read=true;return 3;}});
        let error;try{structuredClone(fake);}catch(e){error=e;}
        if(error?.name!=='DataCloneError'||read)throw Error('forged backing buffer');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
