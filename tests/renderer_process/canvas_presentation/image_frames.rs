//! Real frame ownership, decoding and painted pixels cross isolated renderer IPC.
use super::super::support::*;
use super::modern_images::completed;
use better_web_browser::renderer_process::RendererSession;

fn fixtures() -> serde_json::Value {
    serde_json::from_str(include_str!("../../image-frame-fixtures/frames.json")).unwrap()
}

#[test]
fn image_decoder_outputs_real_animation_pixels_in_the_contained_renderer() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    for (index, (name, mime, frame_index, expected)) in [
        (
            "rgba.png",
            "image/png",
            0,
            vec![30, 20, 10, 255, 30, 25, 20, 128],
        ),
        (
            "disposal.gif",
            "image/gif",
            2,
            vec![
                0, 0, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255,
            ],
        ),
        (
            "composition.png",
            "image/png",
            2,
            vec![
                0, 0, 255, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 0, 255, 255,
            ],
        ),
        (
            "animation.webp",
            "image/webp",
            2,
            vec![
                255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
            ],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let session = RendererSession::launch(options()).expect("hidden frame renderer");
        let html = format!(
            r#"<!doctype html><canvas width=2 height=2></canvas><p>pending</p><script>
            const fixtures={};
            (async()=>{{
                const bytes=Uint8Array.from(atob(fixtures.find(item=>item.name==='{}').bytes),c=>c.charCodeAt(0));
                const decoder=new ImageDecoder({{type:'{}',data:bytes}});
                const {{image}}=await decoder.decode({{frameIndex:{}}});
                const cloned=structuredClone(image);image.close();decoder.close();
                const canvas=document.querySelector('canvas');canvas.width=cloned.displayWidth;canvas.height=cloned.displayHeight;
                canvas.getContext('2d').drawImage(cloned,0,0);cloned.close();
                document.querySelector('p').textContent='decoded';
            }})().catch(error=>document.querySelector('p').textContent='failed:'+error);
        </script>"#,
            fixtures(),
            name,
            mime,
            frame_index
        );
        let initial = load_html_document(&session, 350 + index as u64, &html);
        let presentation = completed(&session, initial, "decoded");
        let image = presentation
            .images
            .iter()
            .find(|image| image.url.starts_with("breeze-internal:canvas:"))
            .expect("painted frame bitmap");
        assert_eq!(image.image.bgra.as_ref(), expected, "{name}");
        acknowledge(&session, &presentation);
    }
}

#[test]
fn renderer_stream_decode_and_frame_transfer_keep_pixels_after_close() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let session = RendererSession::launch(options()).expect("hidden streamed-frame renderer");
    let html = format!(
        r#"<!doctype html><canvas width=2 height=1></canvas><p>pending</p><script>
        (async()=>{{
            const bytes=Uint8Array.from(atob(({}).find(item=>item.name==='rgba.png').bytes),c=>c.charCodeAt(0));
            const input=new ReadableStream({{start(c){{c.enqueue(bytes.slice(0,12));c.enqueue(bytes.slice(12));c.close();}}}});
            const decoder=new ImageDecoder({{type:'image/png',data:input}});
            const {{image}}=await decoder.decode();
            const transferred=structuredClone(image,{{transfer:[image]}});
            if(image.codedWidth!==0||input.locked)throw Error('ownership failed');decoder.close();
            document.querySelector('canvas').getContext('2d').drawImage(transferred,0,0);transferred.close();
            document.querySelector('p').textContent='decoded';
        }})().catch(error=>document.querySelector('p').textContent='failed:'+error);
    </script>"#,
        fixtures()
    );
    let initial = load_html_document(&session, 354, &html);
    let presentation = completed(&session, initial, "decoded");
    let image = presentation
        .images
        .iter()
        .find(|image| image.url.starts_with("breeze-internal:canvas:"))
        .unwrap();
    assert_eq!(
        image.image.bgra.as_ref(),
        [30, 20, 10, 255, 30, 25, 20, 128]
    );
    acknowledge(&session, &presentation);
}

#[test]
fn malformed_image_decoder_rejects_but_the_renderer_remains_usable() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let session = RendererSession::launch(options()).expect("hidden rejected-frame renderer");
    let initial = load_html_document(
        &session,
        355,
        r#"<!doctype html><canvas width=1 height=1></canvas><p>pending</p><script>
        (async()=>{
            const decoder=new ImageDecoder({type:'image/png',data:new Uint8Array([1,2,3])});
            let name;try{await decoder.decode();}catch(e){name=e.name;}decoder.close();
            if(name!=='EncodingError')throw Error('wrong decode error');
            const frame=new VideoFrame(new Uint8Array([9,8,7,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
            document.querySelector('canvas').getContext('2d').drawImage(frame,0,0);frame.close();
            document.querySelector('p').textContent='decoded';
        })().catch(error=>document.querySelector('p').textContent='failed:'+error);
    </script>"#,
    );
    let presentation = completed(&session, initial, "decoded");
    let image = presentation
        .images
        .iter()
        .find(|image| image.url.starts_with("breeze-internal:canvas:"))
        .unwrap();
    assert_eq!(image.image.bgra.as_ref(), [7, 8, 9, 255]);
    acknowledge(&session, &presentation);
}
