//! Conversion boundaries protect against audible wrapping and channel aliasing.
use super::check;

#[test]
fn integer_narrowing_discards_low_bits_instead_of_rounding() {
    check(
        r#"
        const audio=new AudioData({format:'s32',sampleRate:48000,numberOfFrames:5,numberOfChannels:1,
            timestamp:0,data:new Int32Array([1073741823,-1,-1073741825,2147483647,-2147483648])});
        const signed=new Int16Array(5),unsigned=new Uint8Array(5);
        audio.copyTo(signed,{planeIndex:0,format:'s16'});audio.copyTo(unsigned,{planeIndex:0,format:'u8'});
        assert(signed.join(',')==='16383,-1,-16385,32767,-32768','arithmetic narrowing');
        assert(unsigned.join(',')==='191,127,63,255,0','bias after narrowing');audio.close();
    "#,
    );
}

#[test]
fn float_to_integer_saturates_and_nan_maps_to_silence() {
    check(
        r#"
        const audio=new AudioData({format:'f32',sampleRate:48000,numberOfFrames:7,numberOfChannels:1,
            timestamp:0,data:new Float32Array([-Infinity,-2,-1,NaN,1,2,Infinity])});
        for(const [format,Ctor,expected] of [['u8',Uint8Array,[0,0,0,128,255,255,255]],
            ['s16',Int16Array,[-32768,-32768,-32768,0,32767,32767,32767]],
            ['s32',Int32Array,[-2147483648,-2147483648,-2147483648,0,2147483647,2147483647,2147483647]]]) {
            const samples=new Ctor(7);audio.copyTo(samples,{planeIndex:0,format});
            assert(samples.every((sample,index)=>sample===expected[index]),'saturation '+format);
        }
        audio.close();
    "#,
    );
}

#[test]
fn float_rearrangement_preserves_payload_bits_and_negative_zero() {
    check(
        r#"
        const bits=new Uint32Array([0x80000000,0x7fc01234,0x7f800000,0xff800000]);
        const audio=new AudioData({format:'f32',sampleRate:48000,numberOfFrames:2,numberOfChannels:2,
            timestamp:0,data:bits});
        const left=new Uint32Array(2),right=new Uint32Array(2);
        audio.copyTo(left,{planeIndex:0,format:'f32-planar'});
        audio.copyTo(right,{planeIndex:1,format:'f32-planar'});
        assert(left[0]===0x80000000&&left[1]===0x7f800000,'left channel bits');
        assert(right[0]===0x7fc01234&&right[1]===0xff800000,'NaN payload and infinity');
        const clone=audio.clone();audio.close();const copy=new Uint32Array(4);
        clone.copyTo(copy,{planeIndex:0});assert(copy.every((value,index)=>value===bits[index]),'clone bit identity');
        clone.close();
    "#,
    );
}

#[test]
fn copy_destination_views_never_write_outside_their_byte_window() {
    check(
        r#"
        const audio=new AudioData({format:'s16',sampleRate:8000,numberOfFrames:3,numberOfChannels:2,
            timestamp:0,data:new Int16Array([1,10,2,20,3,30])});
        const bytes=new Uint8Array(20).fill(0xee);
        audio.copyTo(new DataView(bytes.buffer,3,8),{planeIndex:0,frameOffset:1,frameCount:2});
        assert(bytes.slice(0,3).every(value=>value===0xee)&&bytes.slice(11).every(value=>value===0xee),'sentinels');
        assert(bytes.slice(3,11).join(',')==='2,0,20,0,3,0,30,0','unaligned DataView');
        const before=bytes.slice();let error;
        try{audio.copyTo(new DataView(bytes.buffer,3,7),{planeIndex:0,frameCount:2});}catch(e){error=e;}
        assert(error instanceof RangeError&&bytes.every((value,index)=>value===before[index]),'atomic small target');
        audio.close();
    "#,
    );
}

#[test]
fn every_channel_can_be_extracted_from_either_layout() {
    check(
        r#"
        for(const planar of [false,true]) {
            const channels=7,frames=5,input=new Int16Array(channels*frames);
            for(let channel=0;channel<channels;channel++)for(let frame=0;frame<frames;frame++)
                input[planar?channel*frames+frame:frame*channels+channel]=channel*100+frame;
            const audio=new AudioData({format:planar?'s16-planar':'s16',sampleRate:48000,
                numberOfFrames:frames,numberOfChannels:channels,timestamp:0,data:input});
            for(let channel=0;channel<channels;channel++) {
                const output=new Int16Array(3);audio.copyTo(output,{planeIndex:channel,
                    frameOffset:1,frameCount:3,format:'s16-planar'});
                assert(output.every((value,index)=>value===channel*100+index+1),'channel '+channel);
            }
            const interleaved=new Int16Array(input.length);audio.copyTo(interleaved,{planeIndex:0,format:'s16'});
            assert(interleaved.every((value,index)=>value===index%channels*100+Math.floor(index/channels)),
                'all interleaved channels');audio.close();
        }
    "#,
    );
}
