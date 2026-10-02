    class VideoDecoder extends EventTarget {
        constructor(init) {
            super();
            if(!arguments.length)throw new TypeError('VideoDecoder requires callbacks');
            const options=frameDictionary(init),error=frameRequired(options.error,'error'),output=frameRequired(options.output,'output');
            if(typeof error!=='function'||typeof output!=='function')throw new TypeError('VideoDecoder callbacks must be functions');
            const lifetime={id:null};videoCodecFinalizer.register(this,lifetime,this);
            videoCodecStates.set(this,{owner:this,error,output,lifetime,id:null,status:'unconfigured',
                queue:[],active:null,timer:null,config:null,size:0,bytes:0,generation:0,keyRequired:true,
                flushes:new Set(),dequeueScheduled:false,ondequeue:null,dequeueListener:null});
        }
        get state(){return videoCodecState(this).status;}
        get decodeQueueSize(){return videoCodecState(this).size;}
        get ondequeue(){return videoCodecState(this).ondequeue;}
        set ondequeue(value) {
            const state=videoCodecState(this),handler=typeof value==='function'?value:null;
            if(!handler&&state.dequeueListener) {
                EventTarget.prototype.removeEventListener.call(this,'dequeue',state.dequeueListener);state.dequeueListener=null;
            }else if(handler&&!state.dequeueListener) {
                state.dequeueListener=event=>state.ondequeue?.call(this,event);
                EventTarget.prototype.addEventListener.call(this,'dequeue',state.dequeueListener);
            }
            state.ondequeue=handler;
        }
        configure(value) {
            const state=videoCodecState(this);
            if(!arguments.length)throw new TypeError('Video configuration is required');
            const config=videoCodecConfig(value);
            if(state.status==='closed')throw frameError('VideoDecoder is closed');
            enqueueVideoCodec(state,{kind:'configure',config});state.status='configured';state.keyRequired=true;
        }
        decode(chunk) {
            videoCodecState(this);
            if(!arguments.length)throw new TypeError('Encoded video chunk is required');
            const input=encodedVideoState(chunk),state=configuredVideoCodec(this);
            if(state.keyRequired&&input.type!=='key')throw frameError('A key chunk is required','DataError');
            if(input.bytes.length>4*1024*1024)throw frameError('Video packet exceeds 4 MiB','QuotaExceededError');
            enqueueVideoCodec(state,{kind:'input',bytes:new Uint8Array(input.bytes),
                timestamp:input.timestamp,duration:input.duration,key:input.type==='key'});
            state.keyRequired=false;
        }
        flush() {
            try {
                const state=configuredVideoCodec(this),request={kind:'flush'};
                const promise=new Promise((resolve,reject)=>{request.resolve=resolve;request.reject=reject;});
                enqueueVideoCodec(state,request);state.flushes.add(request);state.keyRequired=true;
                return promise;
            }catch(error){return Promise.reject(error);}
        }
        reset() {
            const state=videoCodecState(this);
            if(state.status==='closed')throw frameError('VideoDecoder is closed');
            videoCodecAbort(state);state.status='unconfigured';
        }
        close() {
            const state=videoCodecState(this);
            if(state.status==='closed')throw frameError('VideoDecoder is closed');
            videoCodecAbort(state);state.status='closed';videoCodecFinalizer.unregister(this);
        }
        static isConfigSupported(config) {
            if(!arguments.length)return Promise.reject(new TypeError('Video configuration is required'));
            return videoCodecSupport(config);
        }
    }
    Object.defineProperty(VideoDecoder.prototype,Symbol.toStringTag,{value:'VideoDecoder',configurable:true});
    frameIDL([VideoDecoder]);
    if(host('videoCodecSecureContext'))Object.assign(globalThis,{VideoDecoder});
