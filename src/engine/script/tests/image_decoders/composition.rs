//! Exercise the installed interface, not just the native animation decoder.
use super::check;

#[test]
fn jpeg_orientation_applies_to_canvas_but_not_coded_copy_geometry() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/jpeg',data:fixtureBytes('oriented.jpg')});
        const {image}=await decoder.decode();
        assert(image.codedWidth===2&&image.codedHeight===1,'coded JPEG geometry');
        assert(image.rotation===90&&image.displayWidth===1&&image.displayHeight===2,'display orientation');
        const raw=new Uint8Array(image.allocationSize());await image.copyTo(raw);
        assert(raw.length===8,'copyTo preserves coded sample dimensions');
        const context=new OffscreenCanvas(1,2).getContext('2d');context.drawImage(image,0,0);
        assert(context.getImageData(0,0,1,2).data.join(',')===raw.join(','),'rotation paints vertical coded sequence');
        image.close();decoder.close();
    "#,
    );
}

#[test]
fn apng_disposal_and_timing_survive_the_host_chunk_protocol() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:fixtureBytes('composition.png')});
        await decoder.tracks.ready;
        assert(decoder.tracks.length===1&&decoder.tracks[0].animated,'animation track');
        assert(decoder.tracks[0].repetitionCount===1,'two plays means one repetition');
        for(const index of [2,0,1,2]) {
            const {image}=await decoder.decode({frameIndex:index,completeFramesOnly:false});
            const bytes=new Uint8Array(image.allocationSize());await image.copyTo(bytes);
            assert(image.timestamp===index*40000&&image.duration===40000,'APNG microseconds');
            if(index===2) {
                assert(bytes.slice(4,8).join(',')==='255,0,0,255','previous disposal restores red');
                assert(bytes.slice(8,12).join(',')==='0,0,255,255','third blue rectangle');
            }
            if(index===1) assert(bytes.slice(4,8).join(',')==='0,255,0,255','green overlay');
            image.close();
        }
        decoder.close();
    "#,
    );
}

#[test]
fn apng_poster_selection_is_independent_of_animation_frame_index() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:fixtureBytes('poster.png'),preferAnimation:false});
        await decoder.tracks.ready;
        assert(decoder.tracks.length===2,'distinct default image creates still track');
        const still=decoder.tracks.selectedTrack;
        assert(!still.animated&&still.frameCount===1&&still.repetitionCount===0,'poster metadata');
        const {image}=await decoder.decode();const bytes=new Uint8Array(image.allocationSize());
        await image.copyTo(bytes);image.close();
        assert(bytes.every((value,index)=>value===[90,80,70,255][index%4]),'actual poster pixels');
        let range;try{await decoder.decode({frameIndex:1});}catch(e){range=e.name;}
        assert(range==='RangeError','poster has only one frame');
        const animation=[decoder.tracks[0],decoder.tracks[1]].find(track=>track.animated);
        animation.selected=true;
        assert(!still.selected&&animation.selected&&decoder.tracks.selectedTrack===animation,'exclusive selection');
        const frame=await decoder.decode({frameIndex:2});
        assert(frame.image.timestamp===80000,'animated selection uses animation timing');
        frame.image.close();decoder.close();
        assert(!animation.selected&&!still.selected,'retained tracks deselected on close');
    "#,
    );
}

#[test]
fn default_animation_preference_and_track_change_abort_only_pending_requests() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:fixtureBytes('poster.png')});
        await decoder.tracks.ready;
        const animation=decoder.tracks.selectedTrack;
        assert(animation.animated,'animation selected by default');
        const existing=(await decoder.decode()).image;
        const pending=decoder.decode({frameIndex:2});
        const still=[decoder.tracks[0],decoder.tracks[1]].find(track=>!track.animated);
        still.selected=true;
        let name;try{await pending;}catch(e){name=e.name;}
        assert(name==='AbortError','track change aborts pending output');
        assert(existing.codedWidth===2,'already returned resource remains valid');
        const poster=(await decoder.decode()).image;
        assert(poster.duration===null&&poster.timestamp===0,'poster is a still image');
        existing.close();poster.close();decoder.close();
    "#,
    );
}

#[test]
fn gif_previous_disposal_and_webp_frames_are_real_composed_resources() {
    check(
        r#"
        for(const [name,type] of [['disposal.gif','image/gif'],['animation.webp','image/webp']]) {
            const decoder=new ImageDecoder({type,data:fixtureBytes(name)});
            const {image}=await decoder.decode({frameIndex:2});
            const bytes=new Uint8Array(image.allocationSize());await image.copyTo(bytes);
            assert(decoder.tracks.selectedTrack.frameCount===3,'three encoded frames');
            if(type==='image/gif') {
                assert(image.timestamp===90000&&image.duration===60000,'GIF timing');
                assert(bytes.slice(4,8).join(',')==='255,0,0,255','GIF restores previous base');
            } else {
                assert(image.timestamp===80000&&image.duration===40000,'WebP timing');
                assert(bytes.every((value,index)=>value===[0,0,255,255][index%4]),'WebP third blue frame');
            }
            image.close();decoder.close();
        }
    "#,
    );
}
