//! Shared JavaScript admission and view/brand conversion for owned BC payloads.
use super::webgl_instancing::check;

pub(crate) const SETUP: &str = r#"
    const gl=document.querySelector('canvas').getContext('webgl',{preserveDrawingBuffer:true});
    const assert=(value,label)=>{if(!value)throw Error(label)};
    const error=(expected,label)=>assert(gl.getError()===expected,label);
    const names=['WEBGL_compressed_texture_s3tc','WEBGL_compressed_texture_s3tc_srgb','EXT_texture_compression_rgtc'];
    assert(gl.getParameter(gl.COMPRESSED_TEXTURE_FORMATS) instanceof Uint32Array,'typed query');
    assert(gl.getParameter(gl.COMPRESSED_TEXTURE_FORMATS).length===0,'initially disabled');
    const ext=gl.getExtension(names[0]);assert(ext,'real S3TC capability');
    assert(ext===gl.getExtension(names[0].toUpperCase()),'cached identity');
    assert(ext.COMPRESSED_RGBA_S3TC_DXT1_EXT===0x83f1,'DXT1 constant');
    assert(Object.prototype.toString.call(ext)==='[object WEBGL_compressed_texture_s3tc]','extension brand');
    assert(typeof WEBGL_compressed_texture_s3tc==='undefined','LegacyNoInterfaceObject');
    const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
    const red=new Uint8Array([0,248,0,0,0,0,0,0]);
"#;

#[test]
fn webgl_compressed_extension_idl_constants_have_immutable_legacy_descriptors() {
    check(&format!(
        r#"{SETUP}
        for(const name of names) {{
            const extension=gl.getExtension(name);assert(extension,name);
            const prototype=Object.getPrototypeOf(extension);
            for(const key of Object.keys(prototype)) {{
                const descriptor=Object.getOwnPropertyDescriptor(prototype,key);
                assert(descriptor.enumerable&&!descriptor.writable&&!descriptor.configurable,'constant descriptor');
                assert(extension.constructor[key]===extension[key],'constructor constant');
            }}
            let threw=false;try{{new extension.constructor()}}catch(error){{threw=error instanceof TypeError}}
            assert(threw,'illegal constructor');
        }}
        assert(gl.getParameter(gl.COMPRESSED_TEXTURE_FORMATS).length===12,'all formats');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_compressed_uploads_honor_array_buffer_view_ranges_and_errors() {
    check(&format!(
        r#"{SETUP}
        const storage=new Uint8Array(24);storage.fill(91);storage.set(red,8);
        gl.compressedTexImage2D(gl.TEXTURE_2D,0,0x83f1,4,4,0,storage.subarray(8,16));error(0,'view range');
        gl.compressedTexImage2D(gl.TEXTURE_2D,0,0x83f1,4,4,0,new DataView(storage.buffer,8,8));error(0,'DataView range');
        for(const bytes of [new Uint8Array(7),new Uint8Array(9)]){{
            gl.compressedTexImage2D(gl.TEXTURE_2D,0,0x83f1,4,4,0,bytes);error(gl.INVALID_VALUE,'exact block count');
        }}
        for(const value of [null,{{}},red.buffer,[...red]]){{
            let threw=false;try{{gl.compressedTexImage2D(gl.TEXTURE_2D,0,0x83f1,4,4,0,value)}}catch(error){{threw=error instanceof TypeError}}
            assert(threw,'ArrayBufferView brand');
        }}
        let threw=false;try{{gl.compressedTexImage2D(gl.TEXTURE_2D,0,0x83f1,4,4,0)}}catch(error){{threw=error instanceof TypeError}}
        assert(threw,'required data argument');
        error(0,'IDL errors do not create GL errors');
        gl.compressedTexImage2D(gl.TEXTURE_2D,0,0x83f1,8,4,0,new Uint8Array([...red,...red]));error(0,'two blocks');
        gl.compressedTexSubImage2D(gl.TEXTURE_2D,0,1,0,4,4,0x83f1,red);error(gl.INVALID_OPERATION,'block offset');
        gl.generateMipmap(gl.TEXTURE_2D);error(gl.INVALID_OPERATION,'cannot generate compressed blocks');
        document.querySelector('output').textContent='pass';
    "#
    ));
}

#[test]
fn webgl_compressed_uploads_validate_only_the_requested_family() {
    check(&format!(
        r#"{SETUP}
        for(const format of [0x8c4c,0x8c4d,0x8dbb,0x8dbc]){{
            gl.compressedTexImage2D(gl.TEXTURE_2D,0,format,4,4,0,red);error(gl.INVALID_ENUM,'independent admission');
        }}
        const other=document.createElement('canvas').getContext('webgl');
        other.bindTexture(other.TEXTURE_2D,other.createTexture());
        other.compressedTexImage2D(other.TEXTURE_2D,0,0x83f1,4,4,0,red);
        assert(other.getError()===other.INVALID_ENUM,'per context permissions');
        error(0,'first context unchanged');
        document.querySelector('output').textContent='pass';
    "#
    ));
}
