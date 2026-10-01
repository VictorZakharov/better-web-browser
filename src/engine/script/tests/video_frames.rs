use super::*;
mod buffers;
mod canvas;
mod conversion;
mod geometry;
mod idl;
mod ownership;
mod serialization;
mod yuv;

fn check(source: &str) {
    let html = format!(
        "<body><script>const assert=(value,message)=>{{if(!value){{console.error(message);throw Error(message);}}}};\n{source}\nconsole.log('frames passed');</script>"
    );
    let (_, result) = execute_html(&html);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        !result.console.iter().any(|line| line.starts_with("error:")),
        "{:?}",
        result.console
    );
    assert!(
        result
            .console
            .iter()
            .any(|line| line == "log: frames passed"),
        "{:?}",
        result.console
    );
}

#[test]
fn rgba_frame_constructs_and_copies_actual_pixels() {
    check(
        r#"
        const source=new Uint8Array([1,2,3,255,4,5,6,128]);
        const frame=new VideoFrame(source,{format:'RGBA',codedWidth:2,codedHeight:1,timestamp:-42,duration:20});
        assert(frame.format==='RGBA'&&frame.codedWidth===2&&frame.codedHeight===1,'coded metadata');
        assert(frame.timestamp===-42&&frame.duration===20,'timestamp and duration');
        assert(frame.allocationSize()===8,'allocation');
        const destination=new Uint8Array(8);
        frame.copyTo(destination).then(layout=>{
            assert(destination.join(',')===source.join(','),'copy pixels');
            assert(layout.length===1&&layout[0].offset===0&&layout[0].stride===8,'copy layout');
            console.log('copy passed');
        });
    "#,
    );
}

#[test]
fn colorspace_is_private_and_srgb_by_default_for_rgb() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array(4),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const color=frame.colorSpace;
        assert(color instanceof VideoColorSpace,'color interface');
        assert(color.primaries==='bt709'&&color.transfer==='iec61966-2-1'&&color.matrix==='rgb'&&color.fullRange===true,'sRGB');
        const record=color.toJSON(); record.matrix='bt709';
        assert(color.matrix==='rgb','JSON detached from state');
        let illegal=false;
        try{VideoColorSpace.prototype.toJSON.call({});}catch(e){illegal=e instanceof TypeError;}
        assert(illegal,'color brand');
        assert(Object.prototype.toString.call(frame)==='[object VideoFrame]','frame tag');
    "#,
    );
}
