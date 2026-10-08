    // Author code may replace public built-ins. Private bitmap storage must not
    // become the receiver/argument of those callbacks, even when origin-tainted.
    // Capture once during bootstrap, without freezing the page's public objects.
    const canvasPrivateWeakGet = Function.call.bind(WeakMap.prototype.get);
    const canvasPrivateWeakSet = Function.call.bind(WeakMap.prototype.set);
    const canvasPrivateWeakHas = Function.call.bind(WeakMap.prototype.has);
    const canvasPrivateSetHas = Function.call.bind(Set.prototype.has);
    const canvasPrivateSetAdd = Function.call.bind(Set.prototype.add);
    const canvasPrivateSetDelete = Function.call.bind(Set.prototype.delete);
    const canvasPrivateSetEach = Function.call.bind(Set.prototype.forEach);
    const canvasPrivateSetSize = Function.call.bind(Object.getOwnPropertyDescriptor(Set.prototype, 'size').get);
    const canvasPrivateWeakDelete = Function.call.bind(WeakMap.prototype.delete);
    const canvasPrivateWeakSetHas = Function.call.bind(WeakSet.prototype.has);
    const canvasPrivateWeakSetAdd = Function.call.bind(WeakSet.prototype.add);
    const canvasPrivateWeakSetDelete = Function.call.bind(WeakSet.prototype.delete);
    const canvasPrivateApply = Reflect.apply;
    const canvasPrivateWeakRef = WeakRef;
    const canvasPrivateDeref = Function.call.bind(WeakRef.prototype.deref);
    const canvasPrivateTripletMap = (values, callback) =>
        [callback(values[0]), callback(values[1]), callback(values[2])];
    const canvasPrivateSort = Function.call.bind(Array.prototype.sort);
    const canvasPrivatePop = Function.call.bind(Array.prototype.pop);
    const canvasPrivateDefine = Object.defineProperty;
    const canvasPrivateCreate = Object.create;
    const canvasPrivatePush = (values, value) => {
        const descriptor = canvasPrivateCreate(null);
        descriptor.value = value; descriptor.writable = true;
        descriptor.enumerable = true; descriptor.configurable = true;
        canvasPrivateDefine(values, values.length, descriptor);
    };
    const canvasPrivateMath = (() => {
        const result = Object.create(null);
        for (const name of Object.getOwnPropertyNames(Math)) result[name] = Math[name];
        return result;
    })();
    const canvasPrivatePixelArray = Uint8ClampedArray;
    const canvasPrivateByteArray = Uint8Array;
    const canvasPrivateFloatArray = Float32Array;
    const canvasPrivateWords = Uint16Array;
    const canvasPrivateTyped = Object.getPrototypeOf(Uint8Array.prototype);
    const canvasPrivateCount = Function.call.bind(Object.getOwnPropertyDescriptor(canvasPrivateTyped, 'length').get);
    const canvasPrivateByteLength = Function.call.bind(Object.getOwnPropertyDescriptor(canvasPrivateTyped, 'byteLength').get);
    const canvasPrivateBuffer = Function.call.bind(Object.getOwnPropertyDescriptor(canvasPrivateTyped, 'buffer').get);
    const canvasPrivateOffset = Function.call.bind(Object.getOwnPropertyDescriptor(canvasPrivateTyped, 'byteOffset').get);
    const canvasPrivateTypedSet = Function.call.bind(canvasPrivateTyped.set);
    const canvasPrivateColorBytes = channels => {
        const bytes = new canvasPrivateByteArray(4);
        for (let index = 0; index < 4; index++) bytes[index] = channels[index];
        return bytes;
    };
    const canvasPrivateView = (source, start, end) => {
        // No @@species lookup or replaceable subarray method. The callers own
        // the bounds, and their bitmap samples are either byte or 16-bit words.
        const count = canvasPrivateCount(source);
        const elementBytes = count ? canvasPrivateByteLength(source) / count : 1;
        const Constructor = elementBytes === 2 ? canvasPrivateWords : canvasPrivatePixelArray;
        return new Constructor(canvasPrivateBuffer(source), canvasPrivateOffset(source) + start * elementBytes, end - start);
    };
