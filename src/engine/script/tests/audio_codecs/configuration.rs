//! Invalid dictionaries and unsupported codec combinations are distinct outcomes.
use super::check;

#[test]
fn configuration_range_enforcement_never_wraps_into_a_supported_option() {
    check(
        r#"
        const base={codec:'opus',sampleRate:48000,numberOfChannels:1};
        for(const Ctor of [AudioDecoder,AudioEncoder]) {
            for(const member of ['sampleRate','numberOfChannels']) {
                for(const value of [-1,NaN,Infinity,4294967296,1n,Symbol()]) {
                    const config={...base,[member]:value};let failure;
                    try{await Ctor.isConfigSupported(config);}catch(error){failure=error;}
                    assert(failure instanceof TypeError,'support range '+member);
                    const codec=new Ctor({output:()=>{},error:()=>{}});failure=undefined;
                    try{codec.configure(config);}catch(error){failure=error;}
                    assert(failure instanceof TypeError&&codec.state==='unconfigured','configure range');codec.close();
                }
            }
        }
        for(const member of ['complexity','packetlossperc','frameDuration']) {
            for(const value of [-1,NaN,Infinity,2**64,1n,Symbol()]) {
                let failure;try{await AudioEncoder.isConfigSupported({...base,opus:{[member]:value}});}
                catch(error){failure=error;}assert(failure instanceof TypeError,'Opus option range '+member);
            }
        }
    "#,
    );
}

#[test]
fn configuration_clone_preserves_codec_string_and_default_members() {
    check(
        r#"
        for(const codec of ['opus',' opus ','\u00a0','OPUS','opus.123']) {
            const support=await AudioDecoder.isConfigSupported({codec,sampleRate:48000,numberOfChannels:1,unknown:7});
            assert(support.config.codec===codec&&!('unknown' in support.config),'original recognized DOMString');
            assert(support.supported===(codec==='opus'),'no codec trimming or aliases');
        }
        for(const codec of ['', ' ', '\n\r\f\t']) {
            let failure;try{await AudioDecoder.isConfigSupported({codec,sampleRate:48000,numberOfChannels:1});}
            catch(error){failure=error;}assert(failure instanceof TypeError,'ASCII-empty codec is invalid');
        }
        const result=await AudioEncoder.isConfigSupported({codec:'opus',sampleRate:48000,numberOfChannels:1,opus:{}});
        assert(result.config.bitrateMode==='variable'&&result.config.opus.format==='opus','default dictionaries');
        assert(result.config.opus.complexity===9&&result.config.opus.frameDuration===20000,'desktop defaults');
        assert(!result.config.opus.useinbandfec&&!result.config.opus.usedtx,'default switches');
    "#,
    );
}

#[test]
fn configuration_getters_are_ordered_and_snapshotted_before_support_check() {
    check(
        r#"
        const values={bitrate:96000,bitrateMode:'constant',codec:'opus',numberOfChannels:1,
            opus:{complexity:3},sampleRate:48000},visits=[],config={};
        for(const member of Object.keys(values).reverse())Object.defineProperty(config,member,
            {get(){visits.push(member);return values[member];}});
        Object.defineProperty(config,'unknown',{get(){throw Error('unrecognized getter');}});
        const result=await AudioEncoder.isConfigSupported(config);
        assert(visits.join(',')==='bitrate,bitrateMode,codec,numberOfChannels,opus,sampleRate','IDL dictionary ordering');
        values.opus.complexity=9;assert(result.config.opus.complexity===3,'nested dictionary snapshot');
        const storage=new Uint8Array([79,112,117,115,72,101,97,100,1,1,0,0,128,187,0,0,0,0,0]);
        const support=await AudioDecoder.isConfigSupported({codec:'opus',sampleRate:48000,numberOfChannels:1,
            description:new DataView(storage.buffer)});
        storage.fill(0);assert(new Uint8Array(support.config.description)[0]===79,'extradata snapshot');
    "#,
    );
}

#[test]
fn unsupported_capabilities_resolve_false_without_allocating_or_closing_any_codec() {
    check(
        r#"
        for(const config of [
            {codec:'aac',sampleRate:48000,numberOfChannels:1},
            {codec:'opus',sampleRate:44100,numberOfChannels:1},
            {codec:'opus',sampleRate:48000,numberOfChannels:3},
            {codec:'pcm-s16',sampleRate:48000,numberOfChannels:33},
            {codec:'pcm-s16',sampleRate:384001,numberOfChannels:1},
            {codec:'opus',sampleRate:48000,numberOfChannels:1,description:new Uint8Array([1,2,3])}
        ]) assert(!(await AudioDecoder.isConfigSupported(config)).supported,'honest admission '+config.codec);
        for(const bitrate of [0,1,5999,510001,6e9]) {
            const result=await AudioEncoder.isConfigSupported({codec:'opus',sampleRate:48000,numberOfChannels:1,bitrate});
            assert(!result.supported&&result.config.bitrate===bitrate,'valid unsupported bitrate');
        }
        for(const duration of [2500,7500,17500,60000,120000]) {
            const result=await AudioEncoder.isConfigSupported({codec:'opus',sampleRate:48000,numberOfChannels:1,
                opus:{frameDuration:duration}});assert(result.supported,'actual packet duration '+duration);
        }
    "#,
    );
}
