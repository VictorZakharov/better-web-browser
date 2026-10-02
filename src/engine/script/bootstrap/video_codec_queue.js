    const videoCodecDequeue = state => {
        if(state.dequeueScheduled)return;
        state.dequeueScheduled=true;
        setTimeout(()=>{
            try {EventTarget.prototype.dispatchEvent.call(state.owner,audioCodecTrustedEvent(new Event('dequeue')));}
            finally {state.dequeueScheduled=false;}
        },0);
    };
    const videoCodecAbort = (state,error=frameError('Video decoding was aborted','AbortError')) => {
        state.generation++;
        if(state.timer!==null)clearTimeout(state.timer);
        state.timer=null;
        if(state.id!==null)host('videoCodecClose',state.id);
        state.id=null;state.lifetime.id=null;
        state.active=null;state.queue=[];state.bytes=0;state.config=null;state.keyRequired=true;
        for(const request of state.flushes)request.reject(error);
        state.flushes.clear();
        if(state.size){state.size=0;videoCodecDequeue(state);}
    };
    const videoCodecFail = (state,error,name='EncodingError') => {
        const exception=error instanceof DOMException?error:frameError(String(error?.message??error),name);
        videoCodecAbort(state,exception);state.status='closed';
        try {state.error.call(undefined,exception);}catch(callbackError){setTimeout(()=>{throw callbackError;},0);}
    };
    const videoCodecOutputs = (state,outputs,generation) => {
        for(const output of outputs) {
            if(state.generation!==generation||state.status==='closed')return;
            const config=state.config;
            let displayWidth=output.width,displayHeight=output.height;
            if(config.displayAspectWidth!==undefined) {
                const ratio=config.displayAspectWidth/config.displayAspectHeight;
                if(displayWidth/displayHeight<ratio)displayWidth=Math.round(displayHeight*ratio);
                else displayHeight=Math.round(displayWidth/ratio);
            }
            const rotation=frameRotation(config.rotation);
            if(rotation===90||rotation===270)[displayWidth,displayHeight]=[displayHeight,displayWidth];
            const frame=new VideoFrame(output.bytes,{format:'RGBA',codedWidth:output.width,codedHeight:output.height,
                displayWidth,displayHeight,timestamp:output.timestamp,duration:output.duration??undefined,
                rotation:config.rotation,flip:config.flip,
                colorSpace:{primaries:'bt709',transfer:'iec61966-2-1',matrix:'rgb',fullRange:true}});
            try {state.output.call(undefined,frame);}catch(error){setTimeout(()=>{throw error;},0);}
        }
    };
    const videoCodecPoll = state => {
        const generation=state.generation;
        state.timer=setTimeout(()=>{
            state.timer=null;
            if(state.generation!==generation||state.status==='closed')return;
            try {
                const result=host('videoCodecPoll',state.id);
                if(result.status==='pending'){videoCodecPoll(state);return;}
                if(result.status!=='ready'){videoCodecFail(state,result.message);return;}
                const command=state.active;state.active=null;
                videoCodecOutputs(state,result.outputs,generation);
                if(state.generation!==generation)return;
                if(command.kind==='flush'){state.flushes.delete(command);command.resolve();}
                videoCodecRun(state);
            }catch(error){if(state.generation===generation)videoCodecFail(state,error);}
        },1);
    };
    const videoCodecRun = state => {
        if(state.active||state.timer!==null||!state.queue.length||state.status==='closed')return;
        const command=state.queue.shift();state.active=command;
        try {
            if(command.kind==='configure') {
                if(!videoCodecSupported(command.config)) {
                    videoCodecFail(state,'Unsupported video decoder configuration','NotSupportedError');return;
                }
                if(state.id!==null)host('videoCodecClose',state.id);
                state.config=command.config;
                state.id=host('videoCodecStart',videoCodecNativeConfig(command.config));
                state.lifetime.id=state.id;
            }else if(command.kind==='flush')host('videoCodecFlush',state.id);
            else {
                state.size--;state.bytes-=command.bytes.length;videoCodecDequeue(state);
                host('videoCodecInput',state.id,command.bytes,command.timestamp,command.duration,command.key);
                command.bytes=null;
            }
            videoCodecPoll(state);
        }catch(error){videoCodecFail(state,error);}
    };
    const enqueueVideoCodec = (state,command) => {
        const bytes=command.bytes?.length??0;
        if(state.queue.length>=32||state.bytes+bytes>8*1024*1024)
            throw frameError('Video decoder queue exceeds 32 commands or 8 MiB','QuotaExceededError');
        state.queue.push(command);
        if(command.kind==='input'){state.size++;state.bytes+=bytes;}
        if(state.timer===null&&!state.active)
            state.timer=setTimeout(()=>{state.timer=null;videoCodecRun(state);},0);
    };
