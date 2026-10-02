//! Actual AppContainer AV1 decoding and native frame transfer.
use super::{audio_codecs::completed_audio_page, support::*};
use base64::Engine;
use better_web_browser::renderer_process::RendererSession;

fn packets() -> String {
    let source = include_str!("../video-codec-fixtures/eight-frames.ivf.base64");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(source.split_whitespace().collect::<String>())
        .unwrap();
    assert_eq!(&bytes[..4], b"DKIF");
    let mut offset = 32;
    let mut packets = Vec::new();
    while offset < bytes.len() {
        let count = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 12;
        packets.push(bytes[offset..offset + count].to_vec());
        offset += count;
    }
    assert_eq!(packets.len(), 8);
    serde_json::to_string(&packets).unwrap()
}

#[test]
fn persistent_av1_decode_and_canvas_paint_survive_the_contained_renderer_protocol() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let html = format!(
        r#"<!doctype html><title>pending</title><script>
    (async()=>{{
        const packets={};let outputs=0,failure;
        const decoder=new VideoDecoder({{output:frame=>{{
            if(frame.codedWidth!==16||frame.codedHeight!==16||frame.timestamp!==outputs*250000)
                throw Error('wrong native frame metadata');
            const received=structuredClone(frame,{{transfer:[frame]}});
            if(frame.format!==null)throw Error('frame ownership retained');
            const canvas=new OffscreenCanvas(16,16),context=canvas.getContext('2d');
            context.drawImage(received,0,0);
            const pixel=context.getImageData(0,0,1,1).data;
            if(pixel[0]<150||pixel[3]!==255)throw Error('no actual native pixels');
            received.close();outputs++;
        }},error:error=>{{failure=error;}}}});
        decoder.configure({{codec:'av01.0.04M.08'}});
        for(let index=0;index<packets.length;index++)decoder.decode(new EncodedVideoChunk({{
            type:index===0?'key':'delta',timestamp:index*250000,duration:250000,data:new Uint8Array(packets[index])}}));
        await decoder.flush();if(failure)throw failure;
        if(outputs!==8)throw Error('reference frames were not retained');decoder.close();document.title='audio passed';
    }})().catch(error=>document.title='failed:'+error.message);
    </script>"#,
        packets()
    );
    let page = completed_audio_page(&session, &html);
    acknowledge(&session, &page);
    session.cancel_document(page.document).unwrap();
    session.shutdown().unwrap();
}
