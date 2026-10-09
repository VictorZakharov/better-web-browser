    // The shared operation table is installed by webgl_bootstrap before this file.
    // Converted setters are copied at the public call, never at a later flush.
    const webGlPacketCodes = new Map();
    for (let index=0;index<webGlPacketOperations.length;index++)
        webGlPacketCodes.set(webGlPacketOperations[index],index);
    const webGlPacketCode = Function.call.bind(Map.prototype.get,webGlPacketCodes);
    const webGlPacketFloat64 = Float64Array;
    const webGlPacketThen = Function.call.bind(Promise.prototype.then);
    const webGlPacketPromise = Promise.resolve();
    const webGlPacketConstructor = webGlWireCreate(null);
    webGlPacketConstructor[Symbol.species] = Promise;
    // Captured then alone is insufficient: SpeciesConstructor would otherwise
    // read a later author replacement of Promise.prototype.constructor/species.
    Object.defineProperty(webGlPacketPromise,'constructor',{value:webGlPacketConstructor});
    const webGlPacketCapacity = 128*(4+64);
    let webGlPacket = null, webGlPacketUsed=0, webGlPacketCount=0;
    let webGlPacketContexts=webGlWireCreate(null), webGlPacketScheduled=false;
    const flushWebGlCommands = () => {
        if (!webGlPacketCount) return;
        const contexts=webGlPacketContexts,count=webGlPacketCount,used=webGlPacketUsed;
        webGlPacketContexts=webGlWireCreate(null);
        webGlPacketCount=webGlPacketUsed=0;
        // The native boundary copies/admit-checks the entire used prefix before
        // queueing any record. The private buffer may then be reused immediately.
        const lost=host('webglCommandPacket',webGlPacket,used);
        if (!lost.length) return;
        for(let index=0;index<count;index++) {
            const context=contexts[index],state=webGlState(context);
            for(let entry=0;entry<lost.length;entry++) if(state.id===lost[entry]) {
                loseWebGlContext(context,false);break;
            }
        }
    };
    const queueWebGlNumericPacket = (context,state,op,integers,floats,text,bytes) => {
        const code=webGlPacketCode(op);
        if (code===undefined || text!=='' || bytes!==undefined) return false;
        const typed=webGlWireFloat32(floats);
        const floatCount=typed?webGlWireTypedLength(floats):floats.length;
        const integerCount=integers.length,needed=4+integerCount+floatCount;
        if (integerCount+floatCount>64) return false;
        for(let index=0;index<integerCount;index++) if (!webGlWireInteger(integers[index])) return false;
        if (!typed) for(let index=0;index<floatCount;index++) if (typeof floats[index]!=='number') return false;
        if (webGlPacketCount===128 || webGlPacketUsed+needed>webGlPacketCapacity) flushWebGlCommands();
        // The caller already checked the private receiver brand. Loss may mutate
        // that same state during a capacity flush, so check it again afterwards.
        if (state.lost) return true;
        if (!webGlPacket) {
            webGlPacket=new webGlPacketFloat64(webGlPacketCapacity);
            webGlWirePrototype(webGlPacket,null);
        }
        let cursor=webGlPacketUsed;
        webGlPacket[cursor++]=code;webGlPacket[cursor++]=state.id;
        webGlPacket[cursor++]=integerCount;webGlPacket[cursor++]=floatCount;
        for(let index=0;index<integerCount;index++) webGlPacket[cursor++]=integers[index];
        for(let index=0;index<floatCount;index++) webGlPacket[cursor++]=floats[index];
        webGlPacketContexts[webGlPacketCount++]=context;webGlPacketUsed=cursor;
        if (!webGlPacketScheduled) {
            webGlPacketScheduled=true;
            // Submission is not GPU result publication. Native task completion
            // still occurs after the microtask checkpoint, including nested jobs.
            webGlPacketThen(webGlPacketPromise,()=>{
                webGlPacketScheduled=false;flushWebGlCommands();
            });
        }
        if (webGlPacketCount===128) flushWebGlCommands();
        return true;
    };
