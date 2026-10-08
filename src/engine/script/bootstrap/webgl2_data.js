    // Intrinsic getters prevent author-shadowed byteLength/buffer properties
    // from changing the byte range sent to native code. They also accept genuine
    // cross-realm views without trusting Symbol.toStringTag or instanceof.
    const webGl2TypedPrototype = Object.getPrototypeOf(Uint8Array.prototype);
    const webGl2ByteArray = Uint8Array;
    const webGl2IsView = ArrayBuffer.isView;
    const webGl2TypedValues = webGl2TypedPrototype.values;
    const webGl2ByteSubarray = webGl2TypedPrototype.subarray;
    const webGl2ByteSet = webGl2TypedPrototype.set;
    const webGl2Getter = (prototype, name) => Object.getOwnPropertyDescriptor(prototype,name).get;
    const webGl2TypedGetters = Object.fromEntries(['buffer','byteLength','byteOffset','length']
        .map(name => [name,webGl2Getter(webGl2TypedPrototype,name)]));
    const webGl2TypedName = webGl2Getter(webGl2TypedPrototype,Symbol.toStringTag);
    const webGl2DataGetters = Object.fromEntries(['buffer','byteLength','byteOffset']
        .map(name => [name,webGl2Getter(DataView.prototype,name)]));
    const webGl2BufferLength = webGl2Getter(ArrayBuffer.prototype,'byteLength');
    const webGl2SharedLength = typeof SharedArrayBuffer === 'function' ?
        webGl2Getter(SharedArrayBuffer.prototype,'byteLength') : null;
    const webGl2ElementSizes = {
        Int8Array:1, Uint8Array:1, Uint8ClampedArray:1, Int16Array:2, Uint16Array:2,
        Int32Array:4, Uint32Array:4, Float32Array:4, Float64Array:8,
        BigInt64Array:8, BigUint64Array:8, Float16Array:2
    };
    const webGl2View = value => {
        if (!webGl2IsView(value)) throw new TypeError('Expected an ArrayBufferView');
        const name = Reflect.apply(webGl2TypedName,value,[]);
        // Intrinsic length getters return zero for an out-of-bounds resizable
        // view. ValidateTypedArray must reject it instead of uploading zero bytes.
        if (name !== undefined) Reflect.apply(webGl2TypedValues,value,[]);
        const getters = name === undefined ? webGl2DataGetters : webGl2TypedGetters;
        const buffer = Reflect.apply(getters.buffer,value,[]);
        const byteOffset = Reflect.apply(getters.byteOffset,value,[]);
        const byteLength = Reflect.apply(getters.byteLength,value,[]);
        const elementSize = name === undefined ? 1 : webGl2ElementSizes[name];
        if (!elementSize) throw new TypeError('Unsupported buffer view');
        // Constructing the view detects detached backing stores before IPC.
        const bytes = new webGl2ByteArray(buffer,byteOffset,byteLength);
        return {bytes,buffer,byteOffset,byteLength,elementSize,length:byteLength/elementSize,name};
    };
    const webGl2Source = value => {
        if (value === null) return {bytes:new webGl2ByteArray(),elementSize:1,length:0};
        if (webGl2IsView(value)) return webGl2View(value);
        let byteLength;
        try { byteLength = Reflect.apply(webGl2BufferLength,value,[]); }
        catch (error) {
            if (!webGl2SharedLength) throw error;
            byteLength = Reflect.apply(webGl2SharedLength,value,[]);
        }
        return {bytes:new webGl2ByteArray(value,0,byteLength),elementSize:1,length:byteLength};
    };
    const webGl2Slice = (context, source, offset, length, error=0x0501) => {
        if (!Number.isSafeInteger(offset) || offset > source.length ||
            !Number.isSafeInteger(length) || length > source.length-offset) {
            webGlError(context,error); return null;
        }
        const count = length === 0 ? source.length-offset : length;
        return Reflect.apply(webGl2ByteSubarray,source.bytes,[offset*source.elementSize,(offset+count)*source.elementSize]);
    };
    const webGl2NumericConstructors = {f:Float32Array,i:Int32Array,u:Uint32Array};
    const webGl2NumericNames = {f:'Float32Array',i:'Int32Array',u:'Uint32Array'};
    const webGlNumericResizable = webGl2Getter(ArrayBuffer.prototype,'resizable');
    const webGlNumericGrowable = typeof SharedArrayBuffer === 'function' ?
        webGl2Getter(SharedArrayBuffer.prototype,'growable') : null;
    const webGlNumericDetached = buffer => {
        try { new webGl2ByteArray(buffer,0,0); return false; }
        catch (error) { if (error instanceof TypeError) return true; throw error; }
    };
    const webGlNumericTypedArgument = (kind,value,limit=1048576) => {
        if (!webGl2IsView(value) || Reflect.apply(webGl2TypedName,value,[]) !== webGl2NumericNames[kind])
            return null;
        // Float32List/Int32List/Uint32List select genuine buffer-view brands
        // before consulting an author's iterator, including in another realm.
        // [AllowShared] does not imply [AllowResizable]. Detached buffers are
        // a WebGL INVALID_VALUE, not an IDL exception or a sequence fallback.
        // https://registry.khronos.org/webgl/specs/latest/1.0/#TYPES
        const buffer=Reflect.apply(webGl2TypedGetters.buffer,value,[]);
        let resizable;
        try { resizable=Reflect.apply(webGlNumericResizable,buffer,[]); }
        catch (error) {
            if (!webGlNumericGrowable) throw error;
            resizable=Reflect.apply(webGlNumericGrowable,buffer,[]);
        }
        if (resizable) throw new TypeError('WebGL numeric lists require a fixed-length backing store');
        if (webGlNumericDetached(buffer)) return {values:[],length:0,detached:true};
        const offset=Reflect.apply(webGl2TypedGetters.byteOffset,value,[]);
        const length=Reflect.apply(webGl2TypedGetters.length,value,[]);
        if (length>limit) throw new RangeError('WebGL typed numeric list exceeds the snapshot budget');
        const source=new webGl2NumericConstructors[kind](buffer,offset,length);
        const values=new webGl2NumericConstructors[kind](length);
        // Match the existing multi-draw contract and Chrome: conversion takes
        // an independent snapshot before later offset/length getters can mutate
        // or detach the author buffer. Builtin set safely reads shared memory;
        // no shared byte span crosses into Rust or the separate GL owner thread.
        Reflect.apply(webGl2ByteSet,values,[source]);
        return {values,length,detached:false};
    };
    const webGl2NumericArgument = kind => value => {
        const typed=webGlNumericTypedArgument(kind,value);
        if (typed) return typed;
        const values = webGl2Sequence(value,kind);
        return {values,length:values.length};
    };
    const webGlNumericValidate = (context,source) => {
        if (source.detached) {
            webGlError(context,0x0501);return false;
        }
        return true;
    };
    const webGl2NumericSlice = (context,source,offset,length) => {
        const count = length === 0 ? source.length-offset : length;
        if (!Number.isSafeInteger(offset) || offset > source.length || count < 0 ||
            count > source.length-offset || count > 8192) {
            webGlError(context,count > 8192 ? 0x0505 : 0x0501); return null;
        }
        const values = [];
        for (let i = 0; i < count; i++) values.push(source.values[offset+i]);
        return values;
    };
