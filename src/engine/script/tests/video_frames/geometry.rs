use super::check;

#[test]
fn crop_copy_reads_visible_rect_without_applying_display_rotation() {
    check(
        r#"
        const source=Uint8Array.from([10,0,0,255,20,0,0,255,30,0,0,255,40,0,0,255]);
        const frame=new VideoFrame(source,{format:'RGBA',codedWidth:2,codedHeight:2,timestamp:0,
            visibleRect:{x:1,y:0,width:1,height:2},rotation:90});
        assert(frame.visibleRect.x===1&&frame.displayWidth===2&&frame.displayHeight===1,'visible metadata');
        const output=new Uint8Array(frame.allocationSize());
        frame.copyTo(output).then(()=>assert(output.join(',')==='20,0,0,255,40,0,0,255','coded crop'));
        const canvas=new OffscreenCanvas(2,1),context=canvas.getContext('2d');
        context.drawImage(frame,0,0);
        assert([...context.getImageData(0,0,2,1).data].join(',')==='40,0,0,255,20,0,0,255','draw rotated crop');
    "#,
    );
}

#[test]
fn frame_to_frame_crop_preserves_pixel_aspect_ratio() {
    check(
        r#"
        const source=new VideoFrame(new Uint8Array(4*4*2),{format:'RGBA',codedWidth:4,codedHeight:2,
            timestamp:9,displayWidth:12,displayHeight:8});
        const cropped=new VideoFrame(source,{visibleRect:{x:1,y:0,width:2,height:1}});
        assert(cropped.displayWidth===6&&cropped.displayHeight===4,'crop scales display dimensions');
        assert(cropped.timestamp===9&&source.visibleRect.width===4,'source metadata independent');
        const rotated=new VideoFrame(source,{visibleRect:{x:1,y:0,width:2,height:1},rotation:90});
        assert(rotated.displayWidth===4&&rotated.displayHeight===6,'rotation swaps scaled dimensions');
        const explicit=new VideoFrame(source,{rotation:90,displayWidth:7,displayHeight:5});
        assert(explicit.displayWidth===7&&explicit.displayHeight===5,'explicit display size wins');
        source.close();cropped.close();rotated.close();explicit.close();
    "#,
    );
}

#[test]
fn orientation_composition_respects_existing_horizontal_flip() {
    check(
        r#"
        const source=new VideoFrame(new Uint8Array([1,0,0,255,2,0,0,255]),
            {format:'RGBA',codedWidth:2,codedHeight:1,timestamp:0,rotation:90,flip:true});
        const first=new VideoFrame(source,{rotation:90,flip:true});
        assert(first.rotation===0&&!first.flip,'flipped base subtracts rotation');
        assert(first.displayWidth===2&&first.displayHeight===1,'composed display geometry');
        const second=new VideoFrame(first,{rotation:270});
        assert(second.rotation===270&&second.displayWidth===1&&second.displayHeight===2,'unflipped base adds');
        const canvas=new OffscreenCanvas(1,2),context=canvas.getContext('2d');context.drawImage(second,0,0);
        assert([...context.getImageData(0,0,1,2).data].join(',')==='2,0,0,255,1,0,0,255','composed pixels');
        source.close();first.close();second.close();
    "#,
    );
}

#[test]
fn fractional_rectangles_are_bounds_checked_before_truncation() {
    check(
        r#"
        const source=new Uint8Array(16),base={format:'RGBA',codedWidth:2,codedHeight:2,timestamp:0};
        for(const visibleRect of [{x:1.8,y:0,width:1,height:1},{x:0,y:1.8,width:1,height:1}]) {
            let name;try{new VideoFrame(source,{...base,visibleRect});}catch(e){name=e.name;}
            assert(name==='TypeError','fractional out-of-bounds rect rejected');
        }
        const frame=new VideoFrame(source,{...base,visibleRect:{x:0.2,y:0.2,width:1.4,height:1.4}});
        assert(frame.visibleRect.x===0&&frame.visibleRect.width===1,'integer sample rectangle');
        frame.close();
    "#,
    );
}
