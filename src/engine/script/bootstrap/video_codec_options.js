    const videoCodecStates = new WeakMap();
    const videoCodecFinalizer = new FinalizationRegistry(lifetime=>{
        if (lifetime.id !== null) host('videoCodecClose',lifetime.id);
    });
    const videoCodecConfig = value => {
        const input=frameDictionary(value),config={};
        config.codec=frameString(frameRequired(input.codec,'codec'));
        for(const name of ['codedHeight','codedWidth']) {
            const value=input[name];if(value!==undefined)config[name]=frameUint(value);
        }
        const color=input.colorSpace;
        if(color!==undefined)config.colorSpace=frameColorOptions(color);
        const description=input.description;
        if(description!==undefined) {
            const bytes=frameBuffer(description);
            if(bytes.byteLength>65536)throw frameError('Video description exceeds 64 KiB','NotSupportedError');
            config.description=new Uint8Array(bytes);
        }
        for(const name of ['displayAspectHeight','displayAspectWidth']) {
            const value=input[name];if(value!==undefined)config[name]=frameUint(value);
        }
        config.flip=Boolean(input.flip);
        config.hardwareAcceleration=frameEnum(input.hardwareAcceleration,
            ['no-preference','prefer-hardware','prefer-software'],'no-preference');
        config.optimizeForLatency=Boolean(input.optimizeForLatency);
        const rotation=input.rotation;
        config.rotation=rotation===undefined?0:bitmapNumber(rotation);
        if(!config.codec.trim())throw new TypeError('Video codec must not be empty');
        for(const [width,height] of [['codedWidth','codedHeight'],['displayAspectWidth','displayAspectHeight']]) {
            if((config[width]===undefined)!==(config[height]===undefined)||config[width]===0||config[height]===0)
                throw new TypeError('Video dimensions require a nonzero pair');
        }
        if(!Number.isFinite(config.rotation))throw new TypeError('Video rotation must be finite');
        return config;
    };
    const videoCodecNativeConfig = config => JSON.stringify({...config,
        ...(config.description===undefined?{}:{description:Array.from(config.description)})});
    const videoCodecSupported = config => Boolean(host('videoCodecSupported',videoCodecNativeConfig(config)));
    const videoCodecState = value => {
        const state=videoCodecStates.get(value);
        if(!state)throw new TypeError('Illegal VideoDecoder receiver');
        return state;
    };
    const configuredVideoCodec = value => {
        const state=videoCodecState(value);
        if(state.status!=='configured')throw frameError('Video decoder is not configured');
        return state;
    };
    const videoCodecSupport = value => {
        try {
            const config=videoCodecConfig(value);
            return new Promise((resolve,reject)=>setTimeout(()=>{
                try {resolve({supported:videoCodecSupported(config),config});}catch(error){reject(error);}
            },0));
        }catch(error){return Promise.reject(error);}
    };
