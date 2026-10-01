use super::check;

#[test]
fn rgb_channel_swizzle_and_opaque_formats_are_real_conversions() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([1,2,3,4]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const bgra=new Uint8Array(4),rgbx=new Uint8Array(4);
        frame.copyTo(bgra,{format:'BGRA'}).then(()=>assert(bgra.join(',')==='3,2,1,4','BGRA swizzle'));
        frame.copyTo(rgbx,{format:'RGBX'}).then(()=>assert(rgbx.join(',')==='1,2,3,255','opaque copy'));
    "#,
    );
}
