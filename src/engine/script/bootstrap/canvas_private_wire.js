    // Canvas bridge payloads are implementation-owned records. Serialize an
    // owned, null-prototype snapshot so an inherited toJSON cannot observe or
    // retain the renderer's live path arrays, transform, or paint metadata.
    const canvasWireStringify = JSON.stringify;
    const canvasWireKeys = Object.keys, canvasWireArray = Array.isArray;
    const canvasWireCreate = Object.create, canvasWireSetPrototype = Object.setPrototypeOf;
    const canvasWireDescriptor = Object.getOwnPropertyDescriptor, canvasWireOwn = Object.hasOwn;
    const canvasPrivateWireStringify = value => {
        let nodes = 0;
        const snapshot = (source, depth) => {
            if (++nodes > 65536 || depth > 12)
                throw new DOMException('Canvas request exceeds the serialization budget', 'NotSupportedError');
            if (source === null || typeof source !== 'object') return source;
            const copy = canvasWireArray(source) ? [] : canvasWireCreate(null);
            if (canvasWireArray(source)) canvasWireSetPrototype(copy, null);
            const keys = canvasWireKeys(source);
            // The copied keys are dense own strings: indexed access never runs
            // Array.prototype's mutable iterator. Do not evaluate accessors on
            // an unexpected internal record or inherit descriptor get/set hooks.
            for (let index = 0; index < keys.length; index++) {
                const key = keys[index], descriptor = canvasWireDescriptor(source, key);
                if (!descriptor || !canvasWireOwn(descriptor, 'value'))
                    throw new DOMException('Canvas request requires owned data records', 'NotSupportedError');
                // Both records and arrays have null prototypes, so this creates
                // an own data property without invoking an inherited setter.
                copy[key] = snapshot(descriptor.value, depth+1);
            }
            return copy;
        };
        return canvasWireStringify(snapshot(value, 0));
    };
