    // WebGL2 is a separate Web IDL interface, not a subclass of WebGL1.
    // This staged layer is private until the complete canvas interface is admitted.
    // https://registry.khronos.org/webgl/specs/latest/2.0/webgl2.idl
    class WebGL2RenderingContext {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    const webGl2Prototype = WebGL2RenderingContext.prototype;
    for (const key of Reflect.ownKeys(WebGLRenderingContext.prototype)) {
        if (key === 'constructor' || key === Symbol.toStringTag) continue;
        Object.defineProperty(webGl2Prototype, key,
            Object.getOwnPropertyDescriptor(WebGLRenderingContext.prototype, key));
    }
    for (const [key, value] of Object.entries(webGlConstants)) {
        Object.defineProperty(WebGL2RenderingContext, key, {enumerable:true, value});
    }
    Object.defineProperty(webGl2Prototype, Symbol.toStringTag, {value:'WebGL2RenderingContext'});

    for (const name of ['WebGLQuery', 'WebGLSampler', 'WebGLSync',
        'WebGLTransformFeedback', 'WebGLVertexArrayObject']) {
        const constructor = class {
            constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
        };
        Object.defineProperty(constructor, 'name', {value:name});
        Object.setPrototypeOf(constructor.prototype,WebGLObject.prototype);
        Object.setPrototypeOf(constructor,WebGLObject);
        Object.defineProperty(constructor.prototype, Symbol.toStringTag, {value:name});
        webGlObjectClasses[name] = constructor;
    }

    const webGl2State = context => {
        const state = webGlState(context);
        if (state.api !== 'webgl2') throw new TypeError('Illegal WebGL2 receiver');
        return state;
    };
    const webGl2Sequence = (value, kind) => {
        const method = value === null || value === undefined ? undefined : value[Symbol.iterator];
        if (typeof method !== 'function') throw new TypeError('Expected an iterable WebGL list');
        const iterator = Reflect.apply(method, value, []), result = [];
        for (const entry of {[Symbol.iterator]:() => iterator}) {
            if (result.length >= 8192) throw new RangeError('WebGL list exceeds the command budget');
            result.push(webGlScalar(kind, entry));
        }
        return result;
    };
    const webGl2UnsignedLongLong = value => {
        const number = webGlNumber(value);
        return Number.isFinite(number) ? Number(BigInt.asUintN(64,BigInt(Math.trunc(number)))) : 0;
    };
    const webGl2Method = (name, arity, signature, interfaces, implementation, lostResult) => {
        const method = function(...args) {
            const state = webGl2State(this);
            if (args.length < arity) throw new TypeError(name + ' requires at least ' + arity + ' arguments');
            const selectedSignature = typeof signature === 'function' ? signature(args.length) : signature;
            const selectedInterfaces = typeof interfaces === 'function' ? interfaces(args.length) : interfaces;
            // Conversion order is observable even when the context is lost.
            for (let index = 0; index < selectedSignature.length; index++) {
                const entry = selectedInterfaces.find(entry => entry[0] === index);
                if (entry) {
                    if (typeof entry[1] === 'function') args[index] = entry[1](args[index]);
                    else webGlConvertInterface(args, ...entry);
                }
                const kind = selectedSignature[index];
                if ('UIS'.includes(kind)) args[index] = webGl2Sequence(args[index], {U:'u', I:'i', S:'s'}[kind]);
                else if (kind === 'a') {
                    args[index] = webGl2UnsignedLongLong(args[index]);
                } else if (kind !== '-') args[index] = webGlScalar(kind, args[index]);
            }
            if (state.lost) return lostResult;
            return Reflect.apply(implementation, this, args);
        };
        Object.defineProperties(method, {name:{value:name}, length:{value:arity}});
        Object.defineProperty(webGl2Prototype, name, {configurable:true, writable:true, value:method});
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
