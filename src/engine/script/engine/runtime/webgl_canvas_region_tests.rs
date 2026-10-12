//! Borrowing private Canvas storage is synchronous only. The texture command
//! must own the selected bytes before the next author call can repaint/reset.
use super::webgl_owned_copy_tests::both;

const SETUP: &str = r#"
    const gl=new OffscreenCanvas(2,2).getContext('webgl2',{antialias:false});
    const source=new OffscreenCanvas(16,12),c=source.getContext('2d');
    const image=c.createImageData(16,12);
    for(let y=0;y<12;y++)for(let x=0;x<16;x++)image.data.set([x*13,y*17,73,(x+y)*9],(y*16+x)*4);
    c.putImageData(image,0,0);
    const texture=gl.createTexture(),fb=gl.createFramebuffer();
    gl.bindTexture(gl.TEXTURE_2D,texture);gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
    const check=(w,h,expected)=>{
        if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw Error('incomplete region texture');
        const pixels=new Uint8Array(w*h*4);gl.readPixels(0,0,w,h,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
        if(String(pixels)!==String(expected)||gl.getError())throw Error('canvas upload '+pixels+' expected '+expected);
    };
    const expected=(flip,premultiply,firstY=3)=>{
        const result=[];
        for(let y=0;y<2;y++)for(let x=2;x<4;x++){
            const row=flip?11-firstY-y:firstY+y,alpha=(x+row)*9;
            result.push(...[x*13,row*17,73].map(v=>premultiply?Math.round(v*(alpha/255)):v),alpha);
        }
        return result;
    };
"#;

#[test]
fn cropped_canvas_uploads_preserve_premultiplication_flip_and_call_time_ownership() {
    both(&format!(
        r#"{SETUP}
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,2);gl.pixelStorei(gl.UNPACK_SKIP_ROWS,3);
        gl.pixelStorei(gl.UNPACK_ROW_LENGTH,16);gl.pixelStorei(gl.UNPACK_IMAGE_HEIGHT,12);
        for(const flip of [false,true])for(const premultiply of [false,true]){{
            c.putImageData(image,0,0);
            gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,flip);
            gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,premultiply);
            gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
            // The command is deferred, but no private source alias may escape.
            c.clearRect(0,0,16,12);source.width=16;
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
            check(2,2,expected(flip,premultiply));
        }}
    "#
    ));
}

#[test]
fn immutable_canvas_atlas_subuploads_keep_neighbor_pixels_and_unpack_state() {
    both(&format!(
        r#"{SETUP}
        gl.texStorage2D(gl.TEXTURE_2D,1,gl.RGBA8,4,3);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,2);gl.pixelStorei(gl.UNPACK_SKIP_ROWS,3);
        gl.pixelStorei(gl.UNPACK_ROW_LENGTH,16);gl.pixelStorei(gl.UNPACK_IMAGE_HEIGHT,12);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        gl.texSubImage2D(gl.TEXTURE_2D,0,1,1,2,2,gl.RGBA,gl.UNSIGNED_BYTE,source);
        c.fillStyle='red';c.fillRect(0,0,16,12);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const output=new Array(4*3*4).fill(0),region=expected(false,true);
        output.splice((4+1)*4,8,...region.slice(0,8));output.splice((8+1)*4,8,...region.slice(8));
        check(4,3,output);
        if(gl.getParameter(gl.UNPACK_SKIP_PIXELS)!==2||gl.getParameter(gl.UNPACK_SKIP_ROWS)!==3||
            gl.getParameter(gl.UNPACK_ROW_LENGTH)!==16||gl.getParameter(gl.UNPACK_IMAGE_HEIGHT)!==12)
            throw Error('upload altered public unpack state');
    "#
    ));
}

#[test]
fn borrowed_canvas_packing_uses_private_parameters_and_captured_pixel_intrinsics() {
    both(&format!(
        r#"{SETUP}
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,2);gl.pixelStorei(gl.UNPACK_SKIP_ROWS,3);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        const parameter=gl.getParameter,ctor=Uint8Array,integer=Number.isSafeInteger;
        const typed=Object.getPrototypeOf(Uint8Array.prototype),descriptors=Object.getOwnPropertyDescriptors(typed);
        let callbacks=0;
        try {{
            Object.defineProperty(Object.prototype,'borrowedCanvas',{{configurable:true,get(){{callbacks++;throw Error('private record escaped')}}}});
            gl.getParameter=()=>{{callbacks++;source.width=1;throw Error('public parameter callback')}};
            Number.isSafeInteger=()=>{{callbacks++;throw Error('public integer callback')}};
            globalThis.Uint8Array=()=>{{callbacks++;throw Error('public constructor callback')}};
            for(const name of ['buffer','byteOffset','byteLength','length'])
                Object.defineProperty(typed,name,{{configurable:true,get(){{callbacks++;throw Error('public pixel getter')}}}});
            typed.set=typed.subarray=()=>{{callbacks++;throw Error('public row copy')}};
            gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
        }}finally{{
            delete Object.prototype.borrowedCanvas;
            gl.getParameter=parameter;globalThis.Uint8Array=ctor;Number.isSafeInteger=integer;
            Object.defineProperties(typed,descriptors);
        }}
        if(callbacks)throw Error('borrowed private pixels reached author code');
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        check(2,2,expected(false,true));
    "#
    ));
}

#[test]
fn rgba8_premultiply_matches_existing_scalar_rounding_for_all_byte_products() {
    both(
        r#"
        const gl=new OffscreenCanvas(1,1).getContext('webgl2',{antialias:false});
        const source=new ImageData(256,256);
        for(let a=0;a<256;a++)for(let v=0;v<256;v++)source.data.set([v,255-v,73,a],(a*256+v)*4);
        const texture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,texture);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,source);
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0);
        const result=new Uint8Array(256*256*4);gl.readPixels(0,0,256,256,gl.RGBA,gl.UNSIGNED_BYTE,result);
        for(let a=0;a<256;a++)for(let v=0;v<256;v++){
            const index=(a*256+v)*4;
            for(let channel=0;channel<3;channel++)if(result[index+channel]!==Math.round(source.data[index+channel]*(a/255)))
                throw Error('premultiply rounding '+a+'/'+v+'/'+channel);
            if(result[index+3]!==a)throw Error('premultiply alpha');
        }
        if(gl.getError())throw Error('premultiply GL error');
    "#,
    );
}

#[test]
fn canvas_volume_uploads_select_layer_gaps_before_premultiply_and_source_reset() {
    both(&format!(
        r#"{SETUP}
        const volumeTexture=gl.createTexture();gl.bindTexture(gl.TEXTURE_3D,volumeTexture);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS,2);gl.pixelStorei(gl.UNPACK_SKIP_ROWS,3);
        gl.pixelStorei(gl.UNPACK_SKIP_IMAGES,1);gl.pixelStorei(gl.UNPACK_IMAGE_HEIGHT,4);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,true);
        gl.texImage3D(gl.TEXTURE_3D,0,gl.RGBA8,2,1,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
        source.width=16;
        for(let layer=0;layer<2;layer++){{
            gl.framebufferTextureLayer(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,volumeTexture,0,layer);
            check(2,1,expected(false,true,7+layer*4).slice(0,8));
        }}
    "#
    ));
}

#[test]
fn canvas_region_upload_keeps_security_errors_and_pixel_buffer_error_precedence() {
    both(&format!(
        r#"{SETUP}
        c.filter='drop-shadow(1px 0 currentColor)';c.fillRect(0,0,4,4);
        let denied=false;
        try{{gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);}}
        catch(error){{denied=error.name==='SecurityError';}}
        if(!denied)throw Error('tainted region exposed');
        const buffer=gl.createBuffer();gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,buffer);
        gl.bufferData(gl.PIXEL_UNPACK_BUFFER,16,gl.STATIC_DRAW);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
        if(gl.getError()!==gl.INVALID_OPERATION)throw Error('PBO validation read tainted source');
        gl.bindBuffer(gl.PIXEL_UNPACK_BUFFER,null);
        source.width=0;
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,2,2,0,gl.RGBA,gl.UNSIGNED_BYTE,source);
        if(gl.getError()!==gl.INVALID_VALUE)throw Error('unavailable source error changed');
    "#
    ));
}

#[test]
fn full_owned_snapshots_keep_contiguous_views_but_borrowed_canvas_rows_are_independent() {
    let intrinsics = include_str!("../../bootstrap/canvas_private_intrinsics.js");
    let rows = include_str!("../../bootstrap/webgl2_image_rows.js");
    both(&format!(
        r#"(()=>{{
        {intrinsics}
        const webGl2ByteArray=Uint8Array,webGl2Apply=Reflect.apply;
        const webGl2TypedName=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Uint8Array.prototype),Symbol.toStringTag).get;
        {rows}
        const pixels=new Uint8ClampedArray([120,60,30,128,7,11,13,255]),snapshot={{width:2,height:1,pixels}};
        const owned=webGl2Rgba8Rows(snapshot,2,1,1,0,0,1,0,false,false,false);
        if(owned.buffer!==pixels.buffer)throw Error('redundant full snapshot copy');
        const borrowed=webGl2Rgba8Rows(snapshot,2,1,1,0,0,1,0,false,false,true);
        const multiplied=webGl2Rgba8Rows(snapshot,2,1,1,0,0,1,0,false,true,true);
        if(borrowed.buffer===pixels.buffer||multiplied.buffer===pixels.buffer)throw Error('borrow escaped');
        if(String(multiplied)!=='60,30,15,128,7,11,13,255')throw Error('private premultiply');
        pixels.fill(0);
        if(String(borrowed)!=='120,60,30,128,7,11,13,255'||String(multiplied)!=='60,30,15,128,7,11,13,255')
            throw Error('post-pack source mutation');
        }})();"#
    ));
}
