use super::*;

#[test]
fn hidden_renderer_emitted_chunks_decode_real_stereo_with_exact_lookahead_and_eos_trim() {
    verify_stereo_chunks("audio/ogg");
}

#[test]
fn hidden_renderer_webm_chunks_decode_real_stereo_with_exact_lookahead_and_padding() {
    verify_stereo_chunks("audio/webm");
}

fn verify_stereo_chunks(mime: &str) {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session =
        RendererSession::launch(options()).expect("launch hidden Opus recorder renderer");
    let document = DocumentId::new(863).unwrap();
    let html = r#"<!doctype html><button id="control" style="position:fixed;left:0;top:0;width:160px;height:40px">request</button>
        <output id="state" style="position:fixed;top:60px">pending</output><script>
        const order = []; const parts = []; const timecodes = [];
        let stage = 0;
        navigator.mediaDevices.getUserMedia({audio:true}).then(stream => {
            window.recorder = new MediaRecorder(stream, {mimeType:'__MIME__'});
            recorder.onstart = () => {order.push('start'); state.textContent = 'ready';};
            recorder.ondataavailable = event => {
                if (event.data.type !== '__MIME__;codecs=opus' || !event.isTrusted)
                    throw Error('recorded Blob type');
                parts.push(event.data); timecodes.push(event.timecode); order.push('data');
                state.textContent = 'chunks:' + parts.length;
            };
            recorder.onerror = event => state.textContent = 'failed:' + event.error.name;
            recorder.onstop = async () => {
                try {
                    order.push('stop');
                    const encoded = await new Blob(parts, {type:'__MIME__;codecs=opus'}).arrayBuffer();
                    const context = new OfflineAudioContext(2, 128, 48000);
                    const promise = context.decodeAudioData(encoded);
                    if (encoded.byteLength !== 0) throw Error('decode ownership');
                    const pcm = await promise;
                    if (pcm.length !== 6077 || pcm.numberOfChannels !== 2 || pcm.sampleRate !== 48000 ||
                        Math.abs(pcm.duration - 6077/48000) > 1e-12) throw Error('presentation duration');
                    const left = pcm.getChannelData(0), right = pcm.getChannelData(1);
                    for (const samples of [left,right]) {
                        if (!samples.every(Number.isFinite) || !samples.some(s => s > 0.1) ||
                            !samples.some(s => s < -0.1)) throw Error('real stereo PCM');
                    }
                    if (!left.some((s,i) => Math.abs(s-right[i]) > 0.1)) throw Error('duplicated channels');
                    if (parts.length !== 3 || timecodes[0] !== 0 ||
                        order.join(',') !== 'start,data,data,data,stop') throw Error('chunk lifecycle');
                    state.textContent = 'decoded:6077:2:' + recorder.state;
                } catch (error) {state.textContent = 'failed:' + error.message;}
            };
            control.onclick = () => {
                if (stage++ === 0) recorder.requestData(); else recorder.stop();
            };
            recorder.start(100);
        }, error => state.textContent = 'failed:' + error.name);
        </script>"#.replace("__MIME__", mime);
    let request = grant(&session, document, &html);
    wait(&session, document, "ready");
    for sequence in 1..=2 {
        audio(&session, document, request, sequence, 48_000, 2, 960);
        session.ping(Duration::from_secs(2)).unwrap();
    }
    click_recording_control(&session, document, 1);
    wait(&session, document, "chunks:1");
    for sequence in 3..=5 {
        audio(&session, document, request, sequence, 48_000, 2, 960);
        session.ping(Duration::from_secs(2)).unwrap();
    }
    wait(&session, document, "chunks:2");
    audio(&session, document, request, 6, 48_000, 2, 960);
    session.ping(Duration::from_secs(2)).unwrap();
    audio(&session, document, request, 7, 48_000, 2, 317);
    session.ping(Duration::from_secs(2)).unwrap();
    click_recording_control(&session, document, 3);
    wait(&session, document, "decoded:6077:2:inactive");
    session.shutdown().unwrap();
}

#[test]
fn hidden_renderer_pause_omits_pcm_and_disabled_microphone_is_encoded_silence() {
    verify_silence("audio/ogg");
}

#[test]
fn hidden_renderer_webm_pause_omits_pcm_and_disabled_microphone_is_encoded_silence() {
    verify_silence("audio/webm");
}

fn verify_silence(mime: &str) {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session =
        RendererSession::launch(options()).expect("launch hidden Opus silence renderer");
    let document = DocumentId::new(864).unwrap();
    let html = r#"<!doctype html><button id="control" style="position:fixed;left:0;top:0;width:160px;height:40px">next</button>
        <output id="state" style="position:fixed;top:60px">pending</output><script>
        const events = [], parts = []; let stage = 0;
        navigator.mediaDevices.getUserMedia({audio:true}).then(stream => {
            const track = stream.getAudioTracks()[0];
            const recorder = new MediaRecorder(stream, {mimeType:'__MIME__;codecs=opus'});
            recorder.onstart = () => events.push('start');
            recorder.onpause = () => {events.push('pause'); state.textContent='paused';};
            recorder.onresume = () => {events.push('resume'); state.textContent='resumed';};
            recorder.ondataavailable = event => {events.push('data'); parts.push(event.data);};
            recorder.onerror = event => state.textContent='failed:'+event.error.name;
            recorder.onstop = async () => {
                try {
                    events.push('stop');
                    const pcm = await new OfflineAudioContext(1,128,48000).decodeAudioData(
                        await new Blob(parts).arrayBuffer());
                    if (pcm.length !== 1920 || pcm.numberOfChannels !== 1 ||
                        !pcm.getChannelData(0).every(s => Number.isFinite(s) && Math.abs(s)<0.0001) ||
                        events.join(',') !== 'start,pause,resume,data,stop') throw Error('silence lifecycle');
                    state.textContent='silence:1920:inactive';
                } catch (error) {state.textContent='failed:'+error.message;}
            };
            control.onclick=() => {if(stage++===0){track.enabled=false;recorder.resume();}else recorder.stop();};
            recorder.start(); recorder.pause();
        });</script>"#.replace("__MIME__", mime);
    let request = grant(&session, document, &html);
    wait(&session, document, "paused");
    audio(&session, document, request, 1, 48_000, 1, 960);
    session.ping(Duration::from_secs(2)).unwrap();
    click_recording_control(&session, document, 1);
    wait(&session, document, "resumed");
    for sequence in 2..=3 {
        audio(&session, document, request, sequence, 48_000, 1, 960);
        session.ping(Duration::from_secs(2)).unwrap();
    }
    click_recording_control(&session, document, 3);
    wait(&session, document, "silence:1920:inactive");
    session.shutdown().unwrap();
}
