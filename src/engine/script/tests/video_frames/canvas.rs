use super::check;

#[test]
fn canvas_and_imagebitmap_sources_create_independent_frame_resources() {
    check(
        r#"
        const canvas=new OffscreenCanvas(2,1),context=canvas.getContext('2d');
        context.fillStyle='#ff0000';context.fillRect(0,0,2,1);
        const frame=new VideoFrame(canvas,{timestamp:1});
        context.fillStyle='#00ff00';context.fillRect(0,0,2,1);
        const output=new Uint8Array(8);
        frame.copyTo(output).then(()=>assert(output.join(',')==='255,0,0,255,255,0,0,255','canvas snapshot'));
        const bitmap=canvas.transferToImageBitmap();
        const second=new VideoFrame(bitmap,{timestamp:2});bitmap.close();
        const result=new Uint8Array(8);
        second.copyTo(result).then(()=>assert(result.join(',')==='0,255,0,255,0,255,0,255','bitmap snapshot'));
        frame.close();second.close();
    "#,
    );
}

#[test]
fn drawimage_applies_frame_display_size_and_bitmap_owns_its_snapshot() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([255,0,0,255]),{format:'RGBA',codedWidth:1,codedHeight:1,
            displayWidth:2,displayHeight:2,timestamp:0});
        const canvas=new OffscreenCanvas(2,2),context=canvas.getContext('2d');context.drawImage(frame,0,0);
        assert([...context.getImageData(0,0,2,2).data].join(',')===Array(4).fill('255,0,0,255').join(','),'display scaling');
        createImageBitmap(frame).then(bitmap=>{
            frame.close();assert(bitmap.width===2&&bitmap.height===2,'bitmap display geometry');
            const other=new OffscreenCanvas(2,2).getContext('2d');other.drawImage(bitmap,0,0);
            assert(other.getImageData(1,1,1,1).data[0]===255,'bitmap survives source close');bitmap.close();
        });
    "#,
    );
}

#[test]
fn frame_pattern_uses_a_snapshot_and_survives_frame_close() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([255,0,0,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const context=new OffscreenCanvas(2,2).getContext('2d'),pattern=context.createPattern(frame,'repeat');
        frame.close();context.fillStyle=pattern;context.fillRect(0,0,2,2);
        assert(context.getImageData(1,1,1,1).data[0]===255,'pattern owns coded pixel snapshot');
    "#,
    );
}

#[test]
fn closed_imagebitmap_and_canvas_without_pixels_cannot_be_frame_sources() {
    check(
        r#"
        const canvas=new OffscreenCanvas(1,1);canvas.getContext('2d');const bitmap=canvas.transferToImageBitmap();bitmap.close();
        let name;try{new VideoFrame(bitmap,{timestamp:0});}catch(error){name=error.name;}
        assert(name==='InvalidStateError','closed image source');
        const zero=new OffscreenCanvas(0,1);let rejected=false;
        try{new VideoFrame(zero,{timestamp:0});}catch(error){rejected=true;}
        assert(rejected,'empty image source');
    "#,
    );
}

#[test]
fn discard_alpha_changes_visual_coverage_without_touching_original_frame() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([50,100,200,0]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const opaque=new VideoFrame(frame,{alpha:'discard'});
        assert(opaque.format==='RGBX'&&frame.format==='RGBA','opaque resource interpretation');
        const context=new OffscreenCanvas(1,1).getContext('2d');context.drawImage(opaque,0,0);
        assert([...context.getImageData(0,0,1,1).data].join(',')==='50,100,200,255','discard alpha renders RGB');
        frame.close();opaque.close();
    "#,
    );
}
