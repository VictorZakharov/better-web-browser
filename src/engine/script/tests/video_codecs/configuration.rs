//! Configuration snapshots and honest bounded capability reporting.
use super::*;

#[test]
fn av1_config_ignores_description_contents_but_snapshots_the_authors_view() {
    check(&format!(
        r#"
        {}
        const backing=new Uint8Array([9,8,7,6,5]);
        const config={{codec:'av01.0.04M.08',description:new DataView(backing.buffer,1,3),ignored:8}};
        const pending=VideoDecoder.isConfigSupported(config);backing.fill(255);
        structuredClone(backing.buffer,{{transfer:[backing.buffer]}});
        const support=await pending;
        assert(support.supported&&!('ignored' in support.config),'recognized dictionary');
        assert([...support.config.description].join(',')==='8,7,6','only original metadata view');
        let output;
        const decoder=new VideoDecoder({{output:frame=>output=frame,error:error=>{{throw error;}}}});
        decoder.configure(support.config);support.config.description.fill(0);
        decoder.decode(new EncodedVideoChunk({{type:'key',timestamp:0,data:new Uint8Array(videoPackets[0])}}));
        await decoder.flush();assert(output.codedWidth===16,'AV1 description unused');output.close();decoder.close();
    "#,
        source()
    ));
}

#[test]
fn unsupported_profiles_hardware_and_color_overrides_are_not_advertised() {
    check(
        r#"
        for(const codec of ['av01.1.04M.08','av01.0.04M.10','av01.0.04H.08','vp8','avc1.42001E'])
            assert(!(await VideoDecoder.isConfigSupported({codec})).supported,'unsupported decoder '+codec);
        for(const extension of [{hardwareAcceleration:'prefer-hardware'},{codedWidth:4000,codedHeight:4000},
            {colorSpace:{primaries:'bt2020'}},{colorSpace:{}}])
            assert(!(await VideoDecoder.isConfigSupported({codec:'av01.0.04M.08',...extension})).supported,
                'unimplemented or over-budget configuration');
        assert(typeof VideoEncoder==='undefined','no placeholder encoder');
    "#,
    );
}

#[test]
fn invalid_video_dictionaries_reject_before_native_configuration() {
    check(
        r#"
        for(const value of [null,{}, {codec:''},{codec:'   '},{codec:'av01.0.04M.08',codedWidth:16},
            {codec:'av01.0.04M.08',codedWidth:0,codedHeight:16},{codec:'av01.0.04M.08',rotation:Infinity},
            {codec:'av01.0.04M.08',hardwareAcceleration:'magic'}]) {
            let failure;try{await VideoDecoder.isConfigSupported(value);}catch(error){failure=error;}
            assert(failure?.name==='TypeError','invalid video dictionary');
        }
        let settled=false;
        const config={codec:'av01.0.04M.08'};
        const pending=VideoDecoder.isConfigSupported(config).then(result=>{settled=true;return result;});
        config.codec='vp8';await Promise.resolve();assert(!settled,'later task capability query');
        assert((await pending).supported,'synchronous snapshot');
    "#,
    );
}

#[test]
fn dictionary_getters_follow_webidl_order_and_ignore_unrecognized_members() {
    check(
        r#"
        const order=[];
        const values={codec:'av01.0.04M.08',codedHeight:16,codedWidth:16,
            description:new Uint8Array([1]),displayAspectHeight:1,displayAspectWidth:2,
            flip:true,hardwareAcceleration:'prefer-software',optimizeForLatency:true,rotation:80};
        const input=new Proxy(values,{get(target,name){order.push(name);return target[name];}});
        const support=await VideoDecoder.isConfigSupported(input);
        assert(support.supported,'recognized codec');
        assert(order.join(',')==='codec,codedHeight,codedWidth,colorSpace,description,displayAspectHeight,'+
            'displayAspectWidth,flip,hardwareAcceleration,optimizeForLatency,rotation','dictionary order');
        assert(!('colorSpace' in support.config),'absent optional dictionary not synthesized');
        assert(support.config.flip&&support.config.optimizeForLatency&&support.config.rotation===80,'converted members');
    "#,
    );
}

#[test]
fn dimension_range_failures_and_detached_description_reject_without_native_work() {
    check(
        r#"
        for(const width of [-1,Infinity,NaN,4294967296]) {
            let failure;try{await VideoDecoder.isConfigSupported({codec:'av01.0.04M.08',codedWidth:width,codedHeight:16});}
            catch(error){failure=error;}
            assert(failure?.name==='TypeError','EnforceRange dimensions '+width);
        }
        const bytes=new Uint8Array([1]);structuredClone(bytes.buffer,{transfer:[bytes.buffer]});
        let failure;try{await VideoDecoder.isConfigSupported({codec:'av01.0.04M.08',description:bytes});}
        catch(error){failure=error;}
        assert(failure?.name==='TypeError','detached description');
        const support=await VideoDecoder.isConfigSupported({codec:'av01.0.04M.08',codedWidth:16.9,codedHeight:16.1});
        assert(support.supported&&support.config.codedWidth===16&&support.config.codedHeight===16,'fractional EnforceRange truncation');
    "#,
    );
}
