//! Bulk RGBA8 DOM uploads preserve source selection, orientation and ownership.
use super::webgl_owned_copy_tests::both;

const SETUP: &str = r#"
    const gl=new OffscreenCanvas(2,2).getContext('webgl2',{antialias:false});
    const assert=(ok,label)=>{if(!ok)throw Error(label)};
    const texture=gl.createTexture(),fb=gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
    const image=(width,height)=>{
        const source=new ImageData(width,height);
        for(let y=0;y<height;y++)for(let x=0;x<width;x++)source.data.set([x*31,y*23,7,255],(y*width+x)*4);
        return source;
    };
    const expect=(width,height,expected)=>{
        assert(gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE,'complete texture');
        const actual=new Uint8Array(width*height*4);gl.readPixels(0,0,width,height,gl.RGBA,gl.UNSIGNED_BYTE,actual);
        assert(String(actual)===String(expected),'RGBA8 rows '+actual+' expected '+expected);
        assert(gl.getError()===0,'native upload validation');
    };
"#;

#[test]
fn rgba8_full_image_upload_is_owned_before_source_mutation_and_transfer() {
    both(&format!(
        r#"{SETUP}
        gl.bindTexture(gl.TEXTURE_2D,texture);
        const source=image(2,2),expected=Array.from(source.data);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,source);
        source.data.fill(0);source.data.buffer.transfer();
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        expect(2,2,expected);
    "#
    ));
}

#[test]
fn rgba8_cropped_and_flipped_rows_preserve_unpack_selection_in_both_realms() {
    both(&format!(
        r#"{SETUP}
        gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,1);gl.pixelStorei(gl.UNPACK_SKIP_ROWS,1);
        for(const flip of [false,true]) {{
            gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,flip);
            const source=image(4,4),expected=[];
            for(let row=0;row<2;row++)for(let x=1;x<3;x++)expected.push(x*31,(flip?2-row:1+row)*23,7,255);
            gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
            source.data.fill(0);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            expect(2,2,expected);
        }}
    "#
    ));
}

#[test]
fn rgba8_volume_rows_keep_layer_gaps_and_flipped_source_coordinates() {
    both(&format!(
        r#"{SETUP}
        gl.bindTexture(gl.TEXTURE_3D,texture);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,1);gl.pixelStorei(gl.UNPACK_SKIP_ROWS,1);
        gl.pixelStorei(gl.UNPACK_SKIP_IMAGES,1);gl.pixelStorei(gl.UNPACK_IMAGE_HEIGHT,3);
        for(const flip of [false,true]) {{
            const source=image(4,9);gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,flip);
            gl.texImage3D(gl.TEXTURE_3D,0,gl.RGBA8,2,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
            source.data.fill(0);
            for(let layer=0;layer<2;layer++) {{
                const expected=[];
                for(let row=0;row<2;row++)for(let x=1;x<3;x++) {{
                    const y=4+layer*3+row;expected.push(x*31,(flip?8-y:y)*23,7,255);
                }}
                gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,texture,0,layer);
                expect(2,2,expected);
            }}
        }}
    "#
    ));
}

#[test]
fn rgba8_row_copy_uses_captured_intrinsics_and_keeps_premultiply_conversion() {
    both(&format!(
        r#"{SETUP}
        gl.bindTexture(gl.TEXTURE_2D,texture);
        const source=image(4,4);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,1);gl.pixelStorei(gl.UNPACK_SKIP_ROWS,1);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,true);
        const prototype=Object.getPrototypeOf(Uint8Array.prototype),old=Uint8Array;
        const descriptors=Object.getOwnPropertyDescriptors(prototype);
        try {{
            for(const name of ['buffer','byteOffset','byteLength','length'])
                Object.defineProperty(prototype,name,{{configurable:true,get(){{throw Error('author '+name)}}}});
            prototype.set=prototype.subarray=()=>{{throw Error('author copy')}};
            globalThis.Uint8Array=function(){{throw Error('author constructor')}};
            gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
        }} finally {{
            globalThis.Uint8Array=old;Object.defineProperties(prototype,descriptors);
        }}
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        expect(2,2,[31,46,7,255,62,46,7,255,31,23,7,255,62,23,7,255]);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,0);gl.pixelStorei(gl.UNPACK_SKIP_ROWS,0);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        const partial=new ImageData(new Uint8ClampedArray([120,60,30,128]),1,1);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,partial);
        expect(1,1,[60,30,15,128]);
    "#
    ));
}
