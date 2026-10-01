use super::check;

#[test]
fn random_access_animation_frames_preserve_pixels_timing_and_repetition() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/gif',data:gifBytes()});await decoder.tracks.ready;
        const track=decoder.tracks.selectedTrack;
        assert(track.animated&&track.frameCount===3&&track.repetitionCount===Infinity,'animation metadata');
        for(const index of [2,0,1,2]){
            const result=await decoder.decode({frameIndex:index});
            const frame=result.image;
            assert(frame.timestamp===index*30000&&frame.duration===30000,'microsecond frame timing');
            const bytes=new Uint8Array(frame.allocationSize());await frame.copyTo(bytes);
            assert(bytes[0]===index*10&&bytes[2]===200&&bytes[3]===255,'random access decoded pixels');frame.close();
        }
        let error;try{await decoder.decode({frameIndex:3});}catch(e){error=e.name;}
        assert(error==='RangeError','out of range');decoder.close();
    "#,
    );
}

#[test]
fn deselected_track_rejects_and_can_be_selected_again() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/gif',data:gifBytes()});await decoder.tracks.ready;
        const track=decoder.tracks.selectedTrack;track.selected=false;
        assert(decoder.tracks.selectedIndex===-1&&decoder.tracks.selectedTrack===null,'deselection');
        let error;try{await decoder.decode();}catch(e){error=e.name;}
        assert(error==='InvalidStateError','no selected track');
        track.selected=true;assert(decoder.tracks.selectedIndex===0,'reselection');
        (await decoder.decode()).image.close();decoder.close();
    "#,
    );
}

#[test]
fn decoded_frame_remains_owned_after_its_decoder_is_closed() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/gif',data:gifBytes()});
        const {image}=await decoder.decode({frameIndex:1});decoder.close();
        const bytes=new Uint8Array(image.allocationSize());await image.copyTo(bytes);
        assert(bytes[0]===10,'frame resource independent from decoder');image.close();
    "#,
    );
}
