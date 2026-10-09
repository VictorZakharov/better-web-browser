    // WebGL2 is a separate Web IDL interface, not a subclass of WebGL1.
    // Canvas context mode locks the chosen version for the lifetime of the canvas.
    // https://registry.khronos.org/webgl/specs/latest/2.0/webgl2.idl
    class WebGL2RenderingContext {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    const webGl2Prototype = WebGL2RenderingContext.prototype;
    const webGl2PrivateApply = Reflect.apply;
    const webGl2PrivateFind = Function.call.bind(Array.prototype.find);
    const webGl2PrivatePush = Function.call.bind(Array.prototype.push);
    for (const key of Reflect.ownKeys(WebGLRenderingContext.prototype)) {
        if (key === 'constructor' || key === Symbol.toStringTag) continue;
        Object.defineProperty(webGl2Prototype, key,
            Object.getOwnPropertyDescriptor(WebGLRenderingContext.prototype, key));
    }
    for (const [key, value] of Object.entries(webGlConstants)) {
        Object.defineProperty(WebGL2RenderingContext, key, {enumerable:true, value});
    }
    Object.defineProperty(webGl2Prototype, Symbol.toStringTag, {value:'WebGL2RenderingContext'});
    Object.defineProperty(WebGL2RenderingContext, 'length', {value:0});
    Object.defineProperty(globalThis, 'WebGL2RenderingContext',
        {configurable:true,writable:true,value:WebGL2RenderingContext});

    for (const name of ['WebGLQuery', 'WebGLSampler', 'WebGLSync',
        'WebGLTransformFeedback', 'WebGLVertexArrayObject']) {
        const constructor = class {
            constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
        };
        Object.defineProperty(constructor, 'name', {value:name});
        Object.defineProperty(constructor, 'length', {value:0});
        Object.setPrototypeOf(constructor.prototype,WebGLObject.prototype);
        Object.setPrototypeOf(constructor,WebGLObject);
        Object.defineProperty(constructor.prototype, Symbol.toStringTag, {value:name});
        webGlObjectClasses[name] = constructor;
        Object.defineProperty(globalThis,name,{configurable:true,writable:true,value:constructor});
    }

    const webGl2State = context => {
        const state = webGlState(context);
        if (state.api !== 'webgl2') throw new TypeError('Illegal WebGL2 receiver');
        return state;
    };
    const webGl2Sequence = (value, kind) => {
        const method = value === null || value === undefined ? undefined : value[Symbol.iterator];
        if (typeof method !== 'function') throw new TypeError('Expected an iterable WebGL list');
        const iterator = webGl2PrivateApply(method, value, []), result = [];
        for (const entry of {[Symbol.iterator]:() => iterator}) {
            if (result.length >= 8192) throw new RangeError('WebGL list exceeds the command budget');
            webGl2PrivatePush(result,webGlScalar(kind, entry));
        }
        return result;
    };
    const webGl2UnsignedLongLong = value => {
        const number = webGlNumber(value);
        return webGlIdlFinite(number) ? webGlIdlNumber(webGlIdlUintN(64,webGlIdlBigInt(webGlIdlTrunc(number)))) : 0;
    };
    const webGl2PureNumericTail = (args, signature, index) => {
        for (let slot=index+1;slot<signature.length;slot++) {
            if (signature[slot] !== 'a' && signature[slot] !== 'u') return false;
            if (args[slot] !== undefined && typeof args[slot] !== 'number') return false;
        }
        return true;
    };
    const webGl2Method = (name, arity, signature, interfaces, implementation, lostResult) => {
        // Most IDL declarations have a fixed interface slot plan. Compile it
        // once rather than searching with a new callback for every argument of
        // every uniform/state call. A null prototype excludes author getters.
        const fixedInterfaces = typeof interfaces === 'function' ? null : webGlWireCreate(null);
        if (fixedInterfaces) for (let i = 0; i < interfaces.length; i++)
            fixedInterfaces[interfaces[i][0]] = interfaces[i];
        const method = function(...args) {
            // Missing optional IDL arguments are undefined, not inherited
            // Array.prototype getters/setters on this private rest vector.
            webGlWirePrototype(args,null);
            const state = webGl2State(this);
            if (args.length < arity) throw new TypeError(name + ' requires at least ' + arity + ' arguments');
            const selectedSignature = typeof signature === 'function' ? signature(args.length) : signature;
            const selectedInterfaces = typeof interfaces === 'function' ? interfaces(args.length) : interfaces;
            // Conversion order is observable even when the context is lost.
            for (let index = 0; index < selectedSignature.length; index++) {
                const entry = fixedInterfaces ? fixedInterfaces[index] :
                    webGl2PrivateFind(selectedInterfaces,entry => entry[0] === index);
                if (entry) {
                    if (typeof entry[1] === 'function') args[index] = entry[1](args[index],
                        entry[3] === true && webGl2PureNumericTail(args,selectedSignature,index));
                    else webGlConvertInterface(args, ...entry);
                }
                const kind = selectedSignature[index];
                if (webGlWireIncludes('UIS',kind)) args[index] = webGl2Sequence(args[index], {U:'u', I:'i', S:'s'}[kind]);
                else if (kind === 'a') {
                    args[index] = webGl2UnsignedLongLong(args[index]);
                } else if (kind !== '-') args[index] = webGlScalar(kind, args[index]);
            }
            if (state.lost) return lostResult;
            return webGl2PrivateApply(implementation, this, args);
        };
        Object.defineProperties(method, {name:{value:name}, length:{value:arity}});
        Object.defineProperty(webGl2Prototype, name, {enumerable:true, configurable:true, writable:true, value:method});
    };
    const webGl2Handle = (context, value, type, nullable = false) => {
        const id = webGlHandle(context, value, type, nullable);
        if (id > 0 && webGlObjects.get(value).deleted) {
            webGlError(context, 0x0502);
            return -1;
        }
        return id;
    };
    const webGl2Invoke = (context, op, i = [], f = [], text = '', bytes) => {
        // Mutating Web IDL operations return undefined, not the transport's null.
        webGlCall(context, op, i, f, text, bytes);
    };
