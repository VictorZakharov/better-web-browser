use super::*;

fn block_contract(size: usize) {
    let html = format!(
        r#"<body><script>
        const size={size},context=new OfflineAudioContext(4,size*2+17,8000);
        const buffer=context.createBuffer(4,size*2+17,8000);
        for(let channel=0;channel<4;channel++)
            for(let frame=0;frame<buffer.length;frame++)
                buffer.getChannelData(channel)[frame]=(channel+1)/8+(frame%128)/1024;
        const source=new AudioBufferSourceNode(context,{{buffer}});
        const processor=context.createScriptProcessor(size,4,4);
        processor.channelInterpretation='discrete';
        let calls=0;
        processor.onaudioprocess=event=> {{
            const start=calls*size;
            if(event.inputBuffer.length!==size||event.outputBuffer.length!==size||
                Math.round(event.playbackTime*8000)!==start+size)
                throw Error('processing block metadata size '+size);
            for(let channel=0;channel<4;channel++) {{
                const input=event.inputBuffer.getChannelData(channel);
                const output=event.outputBuffer.getChannelData(channel);
                for(let frame=0;frame<size;frame++) {{
                    const expected=(channel+1)/8+((start+frame)%128)/1024;
                    if(input[frame]!==expected) throw Error('accumulated block lost quantum '+frame);
                    output[frame]=input[frame]/2;
                }}
            }}
            calls++;
        }};
        source.connect(processor).connect(context.destination);source.start();
        context.startRendering().then(result=> {{
            if(calls!==2||result.length!==size*2+17) throw Error('partial final block length');
            for(let channel=0;channel<4;channel++)
                for(let frame=0;frame<result.length;frame++) {{
                    const expected=frame<size?0:((channel+1)/8+((frame-size)%128)/1024)/2;
                    if(result.getChannelData(channel)[frame]!==expected)
                        throw Error('produced block sample '+channel+':'+frame);
                }}
            console.log('processing block '+size+' passed');
        }});
        </script>"#,
    );
    let (_, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        [format!("log: processing block {size} passed")]
    );
}

macro_rules! block_tests {
    ($($name:ident: $size:literal),+ $(,)?) => {$(
        #[test]
        fn $name() { block_contract($size); }
    )+};
}

block_tests! {
    block_256_preserves_four_channel_pcm: 256,
    block_512_preserves_four_channel_pcm: 512,
    block_1024_preserves_four_channel_pcm: 1024,
    block_2048_preserves_four_channel_pcm: 2048,
    block_4096_preserves_four_channel_pcm: 4096,
    block_8192_preserves_four_channel_pcm: 8192,
    block_16384_preserves_four_channel_pcm: 16384,
}

#[test]
fn multiple_processors_with_different_block_sizes_keep_independent_boundaries() {
    let (_, outcome) = execute_html(
        r#"<body><script>
        const context=new OfflineAudioContext(1,1536,8000);
        const fast=context.createScriptProcessor(256,0,1),slow=context.createScriptProcessor(512,0,1);
        let fastCalls=0,slowCalls=0;
        fast.onaudioprocess=event=>{fastCalls++;event.outputBuffer.getChannelData(0).fill(.25);};
        slow.onaudioprocess=event=>{slowCalls++;event.outputBuffer.getChannelData(0).fill(.5);};
        fast.connect(context.destination);slow.connect(context.destination);
        context.startRendering().then(buffer=> {
            if(fastCalls!==5||slowCalls!==2) throw Error('shared callback boundary');
            for(let i=0;i<1536;i++) {
                const expected=i<256?0:i<512?.25:.75;
                if(buffer.getChannelData(0)[i]!==expected) throw Error('processor block interference '+i);
            }
            console.log('independent processing blocks passed');
        });
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        outcome.console,
        ["log: independent processing blocks passed"]
    );
}
