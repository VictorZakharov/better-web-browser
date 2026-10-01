//! Recording capability reports must describe actual encoders, not decoders.

use super::*;

mod conversion;
mod interface;

fn verify(code: &str) {
    let (dom, outcome) = execute_html(&format!(
        "<body><output>pending</output><script>{code}</script></body>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome.media_device_actions.is_empty(),
        "queries cannot request devices"
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn encoding_queries_admit_only_real_audio_recorder_formats() {
    verify(
        r#"
        const query = contentType => navigator.mediaCapabilities.encodingInfo({type:'record',
            audio:{contentType}});
        const cases = [
            ['audio/flac',true], ['audio/ogg;codecs=opus',true], ['audio/webm;codecs=opus',true],
            ['Audio/WebM;CODECS=" OPUS "',true], ['audio/ogg;codecs=vorbis',false],
            ['audio/webm;codecs=vorbis',false], ['audio/aac',false], ['audio/mpeg',false],
            ['audio/unknown',false], ['audio/mp4;codecs=mp4a.40.2',false],
            ['audio/webm;codecs=speex',false]
        ];
        Promise.all(cases.map(([type]) => query(type))).then(reports => {
            for(let i=0;i<reports.length;i++) {
                const report=reports[i];
                if(report.supported!==cases[i][1] || report.smooth || report.powerEfficient ||
                    'keySystemAccess' in report || report.configuration.type!=='record' ||
                    report.configuration.audio.contentType!==cases[i][0]) throw Error('encoder report '+i);
                if(report.supported && !MediaRecorder.isTypeSupported(cases[i][0]))
                    throw Error('query has no corresponding encoder');
            }
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn queried_opus_rates_channels_and_bitrate_never_bypass_recorder_limits() {
    verify(
        r#"
        const cases=[];
        for(const contentType of ['audio/ogg;codecs=opus','audio/webm;codecs=opus']) {
            const base={contentType,channels:'2',samplerate:48000,bitrate:64000};
            cases.push([base,true]);
            for(const samplerate of [8000,12000,16000,24000,48000])
                cases.push([{...base,samplerate},true]);
            for(const samplerate of [0,44100,96000,192000])
                cases.push([{...base,samplerate},false]);
            for(const channels of ['0','3','8','stereo','5.1',''])
                cases.push([{...base,channels},false]);
            for(const bitrate of [0,499,512001,-1,Infinity])
                cases.push([{...base,bitrate},false]);
            cases.push([{...base,bitrate:500},true], [{...base,bitrate:512000},true]);
        }
        cases.push([{contentType:'audio/flac',channels:'1',samplerate:44100},true],
            [{contentType:'audio/flac',channels:'2',samplerate:48000},true],
            [{contentType:'audio/flac',channels:'2',samplerate:96000},false],
            [{contentType:'audio/flac',channels:'2',samplerate:192000},false],
            [{contentType:'audio/flac',samplerate:200001},false]);
        Promise.all(cases.map(([audio])=>navigator.mediaCapabilities.encodingInfo({type:'record',audio})))
        .then(reports=>{
            for(let i=0;i<cases.length;i++)
                if(reports[i].supported!==cases[i][1] || reports[i].smooth || reports[i].powerEfficient)
                    throw Error('bounded encoding configuration '+i);
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn video_muxed_and_webrtc_queries_resolve_unsupported_without_inventing_an_encoder() {
    verify(
        r#"
        const video={contentType:'video/webm;codecs=vp8',width:320,height:240,
            bitrate:500000,framerate:30};
        const audio={contentType:'audio/webm;codecs=opus'};
        Promise.all([
            navigator.mediaCapabilities.encodingInfo({type:'record',video}),
            navigator.mediaCapabilities.encodingInfo({type:'record',video,audio}),
            navigator.mediaCapabilities.encodingInfo({type:'webrtc',audio:{contentType:'audio/opus'}}),
            navigator.mediaCapabilities.encodingInfo({type:'webrtc',video:{...video,
                contentType:'video/VP9',scalabilityMode:'L3T3_KEY'}}),
            navigator.mediaCapabilities.encodingInfo({type:'record',video:{...video,
                contentType:'video/mp4;codecs=avc1.42e01e'}})
        ]).then(reports=>{
            if(reports.some(report=>report.supported || report.smooth || report.powerEfficient))
                throw Error('playback support is not encoding support');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}

#[test]
fn encoding_configuration_is_a_deep_snapshot_and_settles_as_an_owned_task() {
    verify(
        r#"
        const capabilities=navigator.mediaCapabilities;
        const events=[];
        const audio={contentType:'audio/webm;codecs=opus',channels:2,samplerate:48000,bitrate:64000};
        const config={type:'record',audio,ignored:{state:1},
            get keySystemConfiguration(){throw Error('encoding has no EME dictionary');}};
        const pending=capabilities.encodingInfo(config);
        events.push('returned');
        config.type='webrtc'; audio.channels=8; audio.contentType='invalid';
        Promise.resolve().then(()=>events.push('microtask'));
        pending.then(report=>{
            events.push('encoding');
            if(events.join(',')!=='returned,microtask,encoding') throw Error('encoding task order');
            if(!report.supported || report.configuration===config || report.configuration.audio===audio ||
                report.configuration.audio.channels!=='2' || report.configuration.type!=='record' ||
                report.configuration.audio.contentType!=='audio/webm;codecs=opus' ||
                Object.keys(report.configuration).join(',')!=='audio,type' ||
                'keySystemConfiguration' in report.configuration || 'ignored' in report.configuration)
                throw Error('encoding snapshot');
            report.configuration.audio.channels='1';
            return capabilities.encodingInfo({type:'record',audio:{contentType:'audio/flac'}});
        }).then(report=>{
            if(!report.supported || 'channels' in report.configuration.audio) throw Error('snapshot isolation');
            document.querySelector('output').textContent='passed';
        });
    "#,
    );
}
