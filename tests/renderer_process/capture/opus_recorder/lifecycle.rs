use super::*;

#[test]
fn hidden_renderer_rejects_44100hz_without_encoded_bytes_and_recorder_recovers() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session =
        RendererSession::launch(options()).expect("launch hidden recorder rate renderer");
    let document = DocumentId::new(865).unwrap();
    let html = r#"<!doctype html><button id="control" style="position:fixed;left:0;top:0;width:160px;height:40px">next</button>
        <output id="state" style="position:fixed;top:60px">pending</output><script>
        const events = [], parts = []; let stage = 0, starts = 0;
        navigator.mediaDevices.getUserMedia({audio:true}).then(stream => {
            const recorder = new MediaRecorder(stream, {mimeType:'audio/ogg'});
            recorder.onstart = () => {events.push('start'); state.textContent='ready:'+ ++starts;};
            recorder.onerror = event => events.push(event.error.name);
            recorder.ondataavailable = event => {events.push('data'); parts.push(event.data);};
            recorder.onstop = async () => {
                try {
                    events.push('stop');
                    if (starts === 1) {
                        if (parts.length !== 1 || parts[0].size !== 0 || recorder.state !== 'inactive' ||
                            events.join(',') !== 'start,NotSupportedError,data,stop')
                            throw Error('unsupported-rate lifecycle');
                        state.textContent='rejected:44100:empty'; return;
                    }
                    const pcm = await new OfflineAudioContext(1,128,48000).decodeAudioData(
                        await new Blob(parts).arrayBuffer());
                    if (pcm.length !== 960 || pcm.sampleRate !== 48000 || pcm.numberOfChannels !== 1 ||
                        !pcm.getChannelData(0).some(s => Math.abs(s)>0.01) ||
                        events.join(',') !== 'start,NotSupportedError,data,stop,start,data,stop')
                        throw Error('recovered PCM');
                    state.textContent='recovered:48000:960';
                } catch (error) {state.textContent='failed:'+error.message;}
            };
            control.onclick=() => {if(stage++===0)recorder.start();else recorder.stop();};
            recorder.start();
        });</script>"#;
    let request = grant(&session, document, html);
    wait(&session, document, "ready:1");
    audio(&session, document, request, 1, 44_100, 1, 882);
    wait(&session, document, "rejected:44100:empty");
    click_recording_control(&session, document, 1);
    wait(&session, document, "ready:2");
    audio(&session, document, request, 2, 48_000, 1, 960);
    click_recording_control(&session, document, 3);
    wait(&session, document, "recovered:48000:960");
    session.shutdown().unwrap();
}

#[test]
fn navigating_retires_eight_active_opus_encoders_and_new_document_recovers_all_slots() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session =
        RendererSession::launch(options()).expect("launch hidden recorder retirement renderer");
    let old = DocumentId::new(866).unwrap();
    let html = r#"<!doctype html><output id="state">pending</output><script>
        navigator.mediaDevices.getUserMedia({audio:true}).then(stream => {
            window.recorders=Array.from({length:8},()=>new MediaRecorder(stream,{mimeType:'audio/ogg'}));
            let started=0;
            for(const recorder of recorders) {
                recorder.onstart=()=>{if(++started===8)state.textContent='old-ready:8';};
                recorder.ondataavailable=()=>{state.textContent='old callback';console.log('old recorder callback');};
                recorder.start(100);
            }
        });</script>"#;
    let old_request = grant(&session, old, html);
    wait(&session, old, "old-ready:8");
    audio(&session, old, old_request, 1, 48_000, 2, 960);
    session.cancel_document(old).unwrap();

    let document = DocumentId::new(867).unwrap();
    let html = r#"<!doctype html><button id="control" style="position:fixed;left:0;top:0;width:160px;height:40px">stop</button>
        <output id="state" style="position:fixed;top:60px">pending</output><script>
        navigator.mediaDevices.getUserMedia({audio:true}).then(stream => {
            const recorders=Array.from({length:8},()=>new MediaRecorder(stream,{mimeType:'audio/ogg'}));
            const chunks=Array.from({length:8},()=>[]); let started=0,stopped=0;
            const verify=async()=>{
                try {
                    for(const parts of chunks) {
                        const pcm=await new OfflineAudioContext(2,128,48000).decodeAudioData(
                            await new Blob(parts).arrayBuffer());
                        if(pcm.length!==960||pcm.numberOfChannels!==2||
                            !pcm.getChannelData(0).some(s=>Math.abs(s)>0.01)||
                            !pcm.getChannelData(1).some(s=>Math.abs(s)>0.01)) throw Error('recovered encoder');
                    }
                    state.textContent='recovered:8:960';
                }catch(error){state.textContent='failed:'+error.message;}
            };
            recorders.forEach((recorder,index)=>{
                recorder.onstart=()=>{if(++started===8)state.textContent='new-ready:8';};
                recorder.onerror=event=>state.textContent='failed:'+event.error.name;
                recorder.ondataavailable=event=>chunks[index].push(event.data);
                recorder.onstop=()=>{if(++stopped===8)verify();};
                recorder.start();
            });
            control.onclick=()=>recorders.forEach(recorder=>recorder.stop());
        });</script>"#;
    let request = grant(&session, document, html);
    wait(&session, document, "new-ready:8");
    // Keep the stale browser sink alive across retirement. Delivery can be
    // rejected by its queue or discarded by the old document fence, never reused.
    let _ = session
        .media_capture_sink(old)
        .try_send_frame(MediaCaptureFrame {
            document: old,
            request_id: old_request,
            track_id: 2,
            sequence: 2,
            timestamp_100ns: 200_000,
            kind: MediaCaptureFrameKind::AudioPcm16,
            width_or_rate: 48_000,
            height_or_frames: 960,
            stride_or_channels: 2,
            bytes: vec![0; 3_840],
        });
    audio(&session, document, request, 1, 48_000, 2, 960);
    click_recording_control(&session, document, 1);
    let text = wait(&session, document, "recovered:8:960");
    assert!(!text.contains("old callback"));
    session.shutdown().unwrap();
}
