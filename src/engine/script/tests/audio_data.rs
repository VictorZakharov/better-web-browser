use super::*;

mod dictionaries;
mod sample_conversion;
mod serialization;

fn check(source: &str) {
    let html = format!(
        "<script>const assert=(v,m)=>{{if(!v)throw Error(m);}};\n{source}\nconsole.log('audio samples passed');</script>"
    );
    let (_, result) = execute_html(&html);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        result
            .console
            .iter()
            .any(|line| line == "log: audio samples passed"),
        "{:?}",
        result.console
    );
}

#[test]
fn planar_samples_copy_to_interleaved_without_relabeling() {
    check(
        r#"
        const input=new Float32Array([-.5,0,.5,1,-1,.25]);
        const audio=new AudioData({format:'f32-planar',sampleRate:48000,numberOfFrames:3,
            numberOfChannels:2,timestamp:-100,data:input});
        input.fill(0);
        const output=new Float32Array(6);
        audio.copyTo(output,{planeIndex:0,format:'f32'});
        assert(output.join(',')==='-0.5,1,0,-1,0.5,0.25','interleaved PCM');
        assert(audio.duration===62&&audio.timestamp===-100,'microsecond metadata');
        const plane=new Float32Array(2);
        audio.copyTo(plane,{planeIndex:1,frameOffset:1,format:'f32-planar'});
        assert(plane.join(',')==='-1,0.25','plane slice');
    "#,
    );
}

#[test]
fn every_pcm_format_converts_to_required_float_planar() {
    check(
        r#"
        const types={u8:Uint8Array,s16:Int16Array,s32:Int32Array,f32:Float32Array};
        const inputs={u8:[0,128,192,64],s16:[-32768,0,16384,-16384],
            s32:[-2147483648,0,1073741824,-1073741824],f32:[-1,0,.5,-.5]};
        for(const planar of [false,true])for(const kind of Object.keys(types)) {
            const data=new types[kind](inputs[kind]);
            const frame=new AudioData({format:kind+(planar?'-planar':''),sampleRate:8000,
                numberOfFrames:2,numberOfChannels:2,timestamp:0,data});
            for(let channel=0;channel<2;channel++) {
                const out=new Float32Array(2);
                frame.copyTo(out,{planeIndex:channel,format:'f32-planar'});
                const expected=planar?inputs.f32.slice(channel*2,channel*2+2):
                    [inputs.f32[channel],inputs.f32[2+channel]];
                assert(out.join(',')===expected.join(','),'conversion '+kind+' '+planar+' '+channel);
            }
            frame.close();
        }
    "#,
    );
}

#[test]
fn close_releases_only_the_original_reference() {
    check(
        r#"
        const audio=new AudioData({format:'s16',sampleRate:8000,numberOfFrames:1,
            numberOfChannels:1,timestamp:123,data:new Int16Array([1234])});
        const cloned=audio.clone(); audio.close(); audio.close();
        assert(audio.format===null&&audio.sampleRate===0&&audio.numberOfFrames===0&&
            audio.numberOfChannels===0&&audio.duration===0&&audio.timestamp===123,'closed state');
        let error;try{audio.clone();}catch(e){error=e.name;}
        assert(error==='InvalidStateError','closed cloning');
        const out=new Int16Array(1);cloned.copyTo(out,{planeIndex:0});
        assert(out[0]===1234,'clone resource remains live');
    "#,
    );
}

#[test]
fn audio_resources_clone_and_transfer_atomically() {
    check(
        r#"
        const audio=new AudioData({format:'u8',sampleRate:8000,numberOfFrames:2,
            numberOfChannels:1,timestamp:0,data:new Uint8Array([1,255])});
        let error;try{structuredClone({audio,bad:()=>{}},{transfer:[audio]});}catch(e){error=e.name;}
        assert(error==='DataCloneError'&&audio.numberOfFrames===2,'failed clone is atomic');
        const copy=structuredClone({a:audio,b:audio},{transfer:[audio]});
        assert(copy.a===copy.b&&copy.a instanceof AudioData,'identity and brand');
        assert(audio.numberOfFrames===0,'sender closed');
        const out=new Uint8Array(2);copy.a.copyTo(out,{planeIndex:0});
        assert(out.join(',')==='1,255','transferred samples');
    "#,
    );
}

#[test]
fn chunks_snapshot_view_bounds_and_survive_cloning() {
    check(
        r#"
        const bytes=new Uint8Array([9,1,2,9]);
        const chunk=new EncodedAudioChunk({type:'key',timestamp:-42,duration:100,
            data:new DataView(bytes.buffer,1,2)});
        bytes.fill(0);const clone=structuredClone(chunk);const out=new Uint8Array(4).fill(7);
        clone.copyTo(out.subarray(1,3));
        assert(out.join(',')==='7,1,2,7','bounded immutable chunk');
        assert(clone.type==='key'&&clone.timestamp===-42&&clone.duration===100&&clone.byteLength===2,'metadata');
        let error;try{chunk.copyTo(new Uint8Array(1));}catch(e){error=e.name;}
        assert(error==='TypeError','small chunk destination');
    "#,
    );
}
