use super::check;
mod alpha_formats;

#[test]
fn yuv_frames_preserve_raw_planes_and_draw_real_luma() {
    check(
        r#"
        const pixels=new Uint8Array([16,235,16,235,128,128]);
        const frame=new VideoFrame(pixels,{format:'I420',codedWidth:2,codedHeight:2,timestamp:0});
        assert(frame.allocationSize()===6,'planar size');
        const output=new Uint8Array(6);
        frame.copyTo(output).then(layout=>{
            assert(output.join(',')===pixels.join(','),'lossless planes');
            assert(layout.map(x=>x.offset).join(',')==='0,4,5','plane offsets');
        });
        const canvas=new OffscreenCanvas(2,2),context=canvas.getContext('2d');context.drawImage(frame,0,0);
        assert([...context.getImageData(0,0,2,1).data].join(',')==='0,0,0,255,255,255,255,255','limited range YUV pixels');
    "#,
    );
}

#[test]
fn odd_yuv_dimensions_round_chroma_planes_up_without_touching_destination_padding() {
    check(
        r#"
        const pixels=new Uint8Array(17).fill(128);pixels.subarray(0,9).fill(16);
        const frame=new VideoFrame(pixels,{format:'I420',codedWidth:3,codedHeight:3,timestamp:0});
        assert(frame.allocationSize()===17,'odd 3x3 Y plus two 2x2 chroma planes');
        const options={layout:[{offset:2,stride:5},{offset:20,stride:3},{offset:30,stride:3}]};
        assert(frame.allocationSize(options)===36,'plane end includes padding');
        const output=new Uint8Array(40).fill(99);
        frame.copyTo(output,options).then(layout=>{
            assert(layout.map(plane=>plane.offset).join(',')==='2,20,30','supplied plane offsets');
            assert(output.slice(2,5).every(value=>value===16),'luma first row');
            assert(output.slice(12,15).every(value=>value===16),'luma third row');
            assert(output.slice(20,22).every(value=>value===128),'first chroma row');
            for(const index of [0,1,5,6,15,16,19,22,25,29,32,35,36,39])
                assert(output[index]===99,'padding preserved at '+index);
        });frame.close();
    "#,
    );
}

#[test]
fn yuv_crop_alignment_is_checked_even_when_copying_to_rgb() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array(24).fill(128),{format:'I420',codedWidth:4,codedHeight:4,timestamp:0});
        for(const rect of [{x:1,y:0,width:2,height:2},{x:0,y:1,width:2,height:2}]) {
            let name;try{frame.allocationSize({rect,format:'RGBA'});}catch(e){name=e.name;}
            assert(name==='TypeError','RGB output does not relax chroma source alignment');
        }
        assert(frame.allocationSize({rect:{x:2,y:2,width:2,height:2}})===6,'aligned cropped plane sizes');
        let unsupported;try{frame.allocationSize({format:'I420'});}catch(e){unsupported=e.name;}
        assert(unsupported==='NotSupportedError','explicit output format is limited to RGB');
        assert(frame.allocationSize({colorSpace:'display-p3'})===24,'color target ignored for raw YUV');
        frame.close();
    "#,
    );
}

#[test]
fn alpha_yuv_keeps_raw_coverage_and_discard_clone_drops_only_alpha_plane() {
    check(
        r#"
        const bytes=new Uint8Array([235,235,235,235,128,128,0,64,128,255]);
        const frame=new VideoFrame(bytes,{format:'I420A',codedWidth:2,codedHeight:2,timestamp:0});
        const opaque=new VideoFrame(frame,{alpha:'discard'});
        assert(opaque.format==='I420'&&opaque.allocationSize()===6&&frame.allocationSize()===10,'opaque plane ownership');
        const output=new Uint8Array(10);frame.copyTo(output).then(()=>assert(output.join(',')===bytes.join(','),'raw alpha samples'));
        const context=new OffscreenCanvas(2,2).getContext('2d');context.drawImage(frame,0,0);
        const pixels=context.getImageData(0,0,2,2).data;
        assert(pixels[3]===0&&pixels[7]===64&&pixels[11]===128&&pixels[15]===255,'Canvas receives exact coverage');
        context.clearRect(0,0,2,2);context.drawImage(opaque,0,0);
        assert(context.getImageData(0,0,2,2).data.every(value=>value===255),'discard clone is opaque white');
        frame.close();opaque.close();
    "#,
    );
}

#[test]
fn unsupported_hdr_conversion_rejects_but_raw_storage_remains_lossless() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([1,2,3,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0,
            colorSpace:{primaries:'bt2020',transfer:'pq',matrix:'rgb',fullRange:true}});
        const raw=new Uint8Array(4);frame.copyTo(raw).then(()=>assert(raw.join(',')==='1,2,3,255','opaque coded data preserved'));
        let name;try{new OffscreenCanvas(1,1).getContext('2d').drawImage(frame,0,0);}catch(e){name=e.name;}
        assert(name==='NotSupportedError','HDR cannot silently paint as SDR');frame.close();
    "#,
    );
}
