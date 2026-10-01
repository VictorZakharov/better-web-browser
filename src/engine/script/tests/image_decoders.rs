//! End-to-end control/codec tests run the actual V8 bootstrap and native worker.
use super::*;
use base64::Engine;
use std::io::Cursor;
use std::time::{Duration, Instant};
mod animation;
mod cancellation;
mod composition;
mod idl;
mod options;
mod resources;
mod security;
mod streams;

fn encoded_png() -> String {
    let mut output = Cursor::new(Vec::new());
    let image = image::RgbaImage::from_raw(2, 1, vec![10, 20, 30, 255, 40, 50, 60, 128]).unwrap();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
    base64::engine::general_purpose::STANDARD.encode(output.into_inner())
}

fn encoded_gif(count: usize) -> String {
    let mut output = Vec::new();
    let mut encoder = image::codecs::gif::GifEncoder::new(&mut output);
    encoder
        .set_repeat(image::codecs::gif::Repeat::Infinite)
        .unwrap();
    for index in 0..count {
        let image =
            image::RgbaImage::from_pixel(2, 1, image::Rgba([index as u8 * 10, 0, 200, 255]));
        encoder
            .encode_frame(image::Frame::from_parts(
                image,
                0,
                0,
                image::Delay::from_numer_denom_ms(30, 1),
            ))
            .unwrap();
    }
    drop(encoder);
    base64::engine::general_purpose::STANDARD.encode(output)
}

fn check(source: &str) -> Vec<String> {
    let html = format!(
        r#"<body><script>
        const assert=(value,message)=>{{if(!value)throw Error(message);}};
        const pngBytes=()=>Uint8Array.from(atob('{}'),c=>c.charCodeAt(0));
        const gifBytes=()=>Uint8Array.from(atob('{}'),c=>c.charCodeAt(0));
        const fixtureBytes=name=>Uint8Array.from(atob(JSON.parse('{}').find(item=>item.name===name).bytes),c=>c.charCodeAt(0));
        (async()=>{{{source}}})().then(()=>console.log('image decoder passed'),error=>console.error(error.name+':'+error.message));
    </script>"#,
        encoded_png(),
        encoded_gif(3),
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../../tests/image-frame-fixtures/frames.json"
        ))
        .unwrap()
    );
    let dom = dom::parse_with_scripting(&html, true);
    let scripts = dom
        .elements_named("script")
        .map(|node| ScriptInput {
            source_url: "https://example.com/#inline".into(),
            code: node.text_content(),
            node,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        })
        .collect::<Vec<_>>();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let first = runtime.execute_initial(&scripts);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    let mut console = first.console;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !console
        .iter()
        .any(|line| line == "log: image decoder passed")
    {
        assert!(
            !console.iter().any(|line| line.starts_with("error:")),
            "{console:?}"
        );
        assert!(
            Instant::now() < deadline,
            "image decoder did not settle: {console:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
        let outcome = runtime.advance_time(Duration::from_millis(10), 64);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        console.extend(outcome.console);
    }
    assert!(
        !console.iter().any(|line| line.starts_with("error:")),
        "{console:?}"
    );
    console
}

#[test]
fn complete_buffer_decode_outputs_copyable_rgba_frame_and_stable_promises() {
    check(
        r#"
        const bytes=pngBytes();
        const decoder=new ImageDecoder({type:'image/png',data:bytes});
        assert(decoder.complete===true,'buffer complete synchronously');
        assert(decoder.completed===decoder.completed&&decoder.tracks===decoder.tracks,'SameObject promises and list');
        const ready=decoder.tracks.ready;
        await ready; await decoder.completed;
        assert(decoder.tracks.ready===ready,'stable ready');
        assert(decoder.type==='image/png'&&decoder.tracks.length===1,'decoder metadata');
        const track=decoder.tracks[0];
        assert(track===decoder.tracks.selectedTrack&&track.selected,'selected track');
        assert(!track.animated&&track.frameCount===1&&track.repetitionCount===0,'still metadata');
        const result=await decoder.decode();
        assert(result.complete===true&&result.image instanceof VideoFrame,'real output interface');
        assert(result.image.timestamp===0&&result.image.duration===null,'still timing');
        const pixels=new Uint8Array(result.image.allocationSize());await result.image.copyTo(pixels);
        assert(pixels.join(',')==='10,20,30,255,40,50,60,128','decoded actual pixels');
        result.image.close();decoder.close();
        assert(decoder.tracks.length===0&&decoder.tracks.selectedIndex===-1,'close removes tracks');
    "#,
    );
}

#[test]
fn feature_admission_reports_only_available_image_codecs() {
    check(
        r#"
        for(const mime of ['image/png','image/jpeg','image/gif','image/webp','image/bmp','image/x-icon'])
            assert(await ImageDecoder.isTypeSupported(mime),'supported '+mime);
        for(const mime of ['image/svg+xml','image/avif','image/jxl'])
            assert(!(await ImageDecoder.isTypeSupported(mime)),'unsupported '+mime);
        assert(await ImageDecoder.isTypeSupported('IMAGE/PNG'),'case insensitive MIME essence');
        assert(await ImageDecoder.isTypeSupported('image/png;foo=bar'),'MIME parameters');
    "#,
    );
}

#[test]
fn malformed_encoded_data_rejects_and_does_not_leave_a_live_session() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:new Uint8Array([1,2,3])});
        let failure;try{await decoder.decode();}catch(e){failure=e.name;}
        assert(failure==='EncodingError','malformed pixels reject');
        let state;try{await decoder.decode();}catch(e){state=e.name;}
        assert(state==='InvalidStateError','bad data closes decoder');
        decoder.close();
        const valid=new ImageDecoder({type:'image/png',data:pngBytes()});
        (await valid.decode()).image.close();valid.close();
    "#,
    );
}
