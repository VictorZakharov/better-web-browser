use super::*;

#[test]
fn iir_preserves_stereo_feedback_tail_until_old_history_can_no_longer_affect_output() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(2,512,8000);
        const buffer=context.createBuffer(2,128,8000);
        buffer.getChannelData(0).fill(.25); buffer.getChannelData(1).fill(.5);
        const stereo=new AudioBufferSourceNode(context,{buffer});
        const mono=new ConstantSourceNode(context,{offset:.75});
        const filter=new IIRFilterNode(context,{feedforward:[1],feedback:[1,-.5],
            channelInterpretation:'discrete'});
        stereo.connect(filter).connect(context.destination); stereo.start();
        context.suspend(128/8000).then(()=> {
            stereo.disconnect(filter); mono.connect(filter); mono.start(context.currentTime);
            return context.resume();
        });
        context.startRendering().then(result=> {
            const left=result.getChannelData(0),right=result.getChannelData(1);
            let l=0,r=0;
            for(let i=0;i<512;i++) {
                l=(i<128?.25:.75)+l*.5; r=(i<128?.5:0)+r*.5;
                const expectedRight=i<384?r:l;
                if(Math.abs(left[i]-l)>1e-6 || Math.abs(right[i]-expectedRight)>1e-6)
                    throw Error('IIR tail lost or revived at '+i+': '+right[i]);
            }
            console.log('IIR channel tail passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: IIR channel tail passed"]);
}

#[test]
fn fir_delayed_input_history_is_retained_even_before_any_output_has_arrived() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(2,256,8000);
        context.destination.channelInterpretation='discrete';
        const buffer=context.createBuffer(2,128,8000); buffer.getChannelData(1)[127]=.5;
        const source=new AudioBufferSourceNode(context,{buffer});
        const coefficients=new Array(20).fill(0); coefficients[19]=1;
        const filter=new IIRFilterNode(context,{feedforward:coefficients,feedback:[1],
            channelInterpretation:'discrete'});
        source.connect(filter).connect(context.destination); source.start();
        context.suspend(128/8000).then(()=> {source.disconnect(filter); return context.resume();});
        context.startRendering().then(result=> {
            for(let i=0;i<256;i++)
                if(result.getChannelData(0)[i]!==0 || result.getChannelData(1)[i] !== (i===146?.5:0))
                    throw Error('FIR input tail was dropped: '+i);
            console.log('FIR pending input tail passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: FIR pending input tail passed"]);
}

#[test]
fn biquad_wider_history_is_not_deleted_at_a_discrete_input_layout_decrease() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(2,256,8000);
        context.destination.channelInterpretation='discrete';
        const buffer=context.createBuffer(2,128,8000); buffer.getChannelData(1)[127]=1;
        const source=new AudioBufferSourceNode(context,{buffer});
        const filter=new BiquadFilterNode(context,{type:'lowpass',frequency:1000,Q:1,
            channelInterpretation:'discrete'});
        source.connect(filter).connect(context.destination); source.start();
        context.suspend(128/8000).then(()=> {source.disconnect(filter); return context.resume();});
        context.startRendering().then(result=> {
            if(!result.getChannelData(0).every(v=>v===0) ||
                Math.abs(result.getChannelData(1)[128])<.001 ||
                Math.abs(result.getChannelData(1)[129])<.001)
                throw Error('biquad dropped right-channel response at the boundary');
            console.log('biquad channel tail passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: biquad channel tail passed"]);
}

#[test]
fn convolution_keeps_old_wider_output_until_its_finite_impulse_tail_expires() {
    let (_, outcome) = execute_html(
        r#"<script>
        const context=new OfflineAudioContext(2,640,8000);
        const buffer=context.createBuffer(2,128,8000); buffer.getChannelData(1)[127]=.5;
        const impulse=context.createBuffer(1,201,8000); impulse.getChannelData(0)[200]=1;
        const source=new AudioBufferSourceNode(context,{buffer});
        const convolver=new ConvolverNode(context,{buffer:impulse,disableNormalization:true,
            channelInterpretation:'discrete'});
        source.connect(convolver).connect(context.destination); source.start();
        context.suspend(128/8000).then(()=> {source.disconnect(convolver); return context.resume();});
        context.startRendering().then(result=> {
            for(let i=0;i<640;i++)
                if(Math.abs(result.getChannelData(0)[i])>1e-6 ||
                    Math.abs(result.getChannelData(1)[i]-(i===327?.5:0))>1e-6)
                    throw Error('convolver channel tail at '+i);
            console.log('convolution channel tail passed');
        });
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: convolution channel tail passed"]);
}
