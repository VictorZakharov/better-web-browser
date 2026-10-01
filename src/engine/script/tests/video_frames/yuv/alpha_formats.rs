use super::super::check;

#[test]
fn yuv_422_and_444_alpha_formats_copy_all_planes_and_render_coverage() {
    check(
        r#"
        for(const [format,chroma] of [['I422A',2],['I444A',4]]) {
            const pixels=new Uint8Array(8+chroma*2);pixels.subarray(0,4).fill(235);
            pixels.subarray(4,4+chroma*2).fill(128);pixels.set([0,64,128,255],4+chroma*2);
            const frame=new VideoFrame(pixels,{format,codedWidth:2,codedHeight:2,timestamp:5});
            const copy=new Uint8Array(frame.allocationSize());
            frame.copyTo(copy).then(layout=>{
                assert(copy.join(',')===pixels.join(','),'lossless '+format+' samples');
                assert(layout.length===4&&layout[3].offset===4+chroma*2,'four planar layouts');
            });
            const context=new OffscreenCanvas(2,2).getContext('2d');context.drawImage(frame,0,0);
            const output=context.getImageData(0,0,2,2).data;
            assert([output[3],output[7],output[11],output[15]].join(',')==='0,64,128,255','painted alpha');
            const opaque=new VideoFrame(frame,{alpha:'discard'});
            assert(opaque.format===format.slice(0,-1)&&opaque.allocationSize()===4+chroma*2,'opaque equivalent');
            frame.close();opaque.close();
        }
    "#,
    );
}

#[test]
fn planar_alpha_structured_clone_and_transfer_preserve_raw_bytes() {
    check(
        r#"
        for(const [format,pixels] of [['I422A',[16,235,128,128,1,255]],['I444A',[16,235,128,128,128,128,1,255]]]) {
            const frame=new VideoFrame(new Uint8Array(pixels),{format,codedWidth:2,codedHeight:1,timestamp:9});
            const moved=structuredClone(frame,{transfer:[frame]});
            assert(frame.format===null&&moved.format===format,'transfer '+format);
            const output=new Uint8Array(moved.allocationSize());
            moved.copyTo(output).then(()=>assert(output.join(',')===pixels.join(','),'transferred alpha planes'));
            moved.close();
        }
    "#,
    );
}

#[test]
fn alpha_formats_use_their_own_chroma_alignment_not_a_blanket_even_origin() {
    check(
        r#"
        const full=new VideoFrame(new Uint8Array(16).fill(128),{format:'I444A',codedWidth:2,codedHeight:2,timestamp:0,
            visibleRect:{x:1,y:1,width:1,height:1}});
        assert(full.allocationSize()===4,'444 alpha supports odd origin');full.close();
        const horizontal=new VideoFrame(new Uint8Array(12).fill(128),{format:'I422A',codedWidth:2,codedHeight:2,timestamp:0,
            visibleRect:{x:0,y:1,width:2,height:1}});
        assert(horizontal.allocationSize()===6,'422 alpha supports odd vertical origin');horizontal.close();
        let name;try{new VideoFrame(new Uint8Array(12),{format:'I422A',codedWidth:2,codedHeight:2,timestamp:0,
            visibleRect:{x:1,y:0,width:1,height:1}});}catch(e){name=e.name;}
        assert(name==='TypeError','422 alpha rejects odd horizontal chroma origin');
    "#,
    );
}
