use super::check;

#[test]
fn reset_rejects_pending_requests_but_preserves_encoded_source() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:pngBytes()});
        const pending=decoder.decode();decoder.reset();
        let error;try{await pending;}catch(e){error=e.name;}
        assert(error==='AbortError','pending reset');
        (await decoder.decode()).image.close();decoder.close();
    "#,
    );
}

#[test]
fn close_is_idempotent_and_settles_early_pending_promises() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:pngBytes()});
        const pending=decoder.decode(),ready=decoder.tracks.ready;
        decoder.close();decoder.close();
        for(const promise of [pending,ready]){
            let error;try{await promise;}catch(e){error=e.name;}
            assert(error==='AbortError','close pending promise');
        }
        await decoder.completed;
        let error;try{await decoder.decode();}catch(e){error=e.name;}
        assert(error==='InvalidStateError','closed decode');
    "#,
    );
}

#[test]
fn reset_after_metadata_keeps_tracks_and_allows_subsequent_decode() {
    check(
        r#"
        const decoder=new ImageDecoder({type:'image/png',data:pngBytes()});await decoder.tracks.ready;
        const track=decoder.tracks[0],pending=decoder.decode();decoder.reset();
        let error;try{await pending;}catch(e){error=e.name;}
        assert(error==='AbortError'&&decoder.tracks[0]===track,'reset preserves tracks');
        (await decoder.decode()).image.close();decoder.close();
    "#,
    );
}
