//! Window bindings verify actual AV1 frames and private-resource ownership.
use super::audio_codecs::check;
use crate::engine::script::video_codecs::test_packets;
mod chunks;
mod configuration;
mod fixture;
mod lifecycle;
mod workers;

fn source() -> String {
    let reference = test_packets::fixture("rgba");
    let green: Vec<_> = reference.chunks_exact(1024).map(|frame| frame[1]).collect();
    format!(
        "const videoPackets={};const videoExpectedGreen={};",
        serde_json::to_string(&test_packets::packets()).unwrap(),
        serde_json::to_string(&green).unwrap()
    )
}

#[test]
fn av1_video_decoder_delivers_eight_real_frames_with_chunk_times_and_duration() {
    check(&format!(
        r#"
        {}
        const config={{codec:'av01.0.04M.08',hardwareAcceleration:'prefer-software'}};
        assert((await VideoDecoder.isConfigSupported(config)).supported,'real AV1 backend');
        let frames=0,green=[];
        const decoder=new VideoDecoder({{output:frame=>{{
            assert(frame instanceof VideoFrame&&frame.codedWidth===16&&frame.codedHeight===16,'actual decoded image');
            assert(frame.timestamp===frames*250000&&frame.duration===250000,'packet metadata');
            assert(frame.colorSpace.matrix==='rgb'&&frame.colorSpace.fullRange,'converted RGBA color metadata');
            const pixels=new Uint8Array(frame.allocationSize());
            frame.copyTo(pixels);green.push(pixels[1]);frames++;frame.close();
        }},error:error=>{{throw error;}}}});
        decoder.configure(config);
        for(let index=0;index<videoPackets.length;index++)decoder.decode(new EncodedVideoChunk({{
            type:index===0?'key':'delta',timestamp:index*250000,duration:250000,data:new Uint8Array(videoPackets[index])}}));
        assert(decoder.decodeQueueSize===8,'synchronous queue admission');
        await decoder.flush();assert(frames===8&&decoder.decodeQueueSize===0,'all native output drained');
        for(let index=0;index<green.length;index++)assert(Math.abs(green[index]-videoExpectedGreen[index])<=3,
            'independent reference pixel '+index+': '+green[index]);decoder.close();
    "#,
        source()
    ));
}

#[test]
fn video_output_can_be_cloned_transferred_and_painted_without_codec_byte_reconstruction() {
    check(&format!(
        r#"
        {}
        let output;
        const decoder=new VideoDecoder({{output:frame=>output=frame,error:error=>{{throw error;}}}});
        decoder.configure({{codec:'av01.0.04M.08'}});
        decoder.decode(new EncodedVideoChunk({{type:'key',timestamp:-100,data:new Uint8Array(videoPackets[0])}}));
        await decoder.flush();
        const clone=output.clone(),moved=structuredClone({{frame:output,alias:output}},{{transfer:[output]}});
        assert(output.format===null&&moved.frame===moved.alias&&moved.frame.timestamp===-100,'native resource transfer');
        const canvas=new OffscreenCanvas(16,16),context=canvas.getContext('2d');context.drawImage(moved.frame,0,0);
        const first=context.getImageData(0,0,1,1).data;
        assert(first[0]>150&&first[2]<80&&first[3]===255,'real decoded pixels painted');
        moved.frame.close();
        const bytes=new Uint8Array(clone.allocationSize());await clone.copyTo(bytes);
        assert(bytes[0]>150,'clone retains independent ownership');clone.close();decoder.close();
    "#,
        source()
    ));
}

#[test]
fn aspect_ratio_increases_display_size_before_quarter_turn_and_flip() {
    check(&format!(
        r#"
        {}
        let output;
        const config={{codec:'av01.0.04M.08',displayAspectWidth:2,displayAspectHeight:1,rotation:80,flip:true}};
        const support=await VideoDecoder.isConfigSupported(config);
        assert(support.supported&&support.config.rotation===80,'support snapshot retains converted double');
        const decoder=new VideoDecoder({{output:frame=>output=frame,error:error=>{{throw error;}}}});
        decoder.configure(config);
        decoder.decode(new EncodedVideoChunk({{type:'key',timestamp:0,data:new Uint8Array(videoPackets[0])}}));
        await decoder.flush();
        assert(output.displayWidth===16&&output.displayHeight===32,'aspect expansion then rotation');
        assert(output.rotation===90&&output.flip,'frame orientation');output.close();decoder.close();
    "#,
        source()
    ));
}
