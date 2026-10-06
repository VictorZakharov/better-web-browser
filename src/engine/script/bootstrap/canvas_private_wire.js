    // Canvas bridge payloads are implementation-owned records. Serialize an
    // owned, null-prototype snapshot so an inherited toJSON cannot observe or
    // retain the renderer's live path arrays, transform, or paint metadata.
    const canvasWireStringify = JSON.stringify;
    const canvasWireKeys = Object.keys, canvasWireArray = Array.isArray;
    const canvasWireCreate = Object.create, canvasWireSetPrototype = Object.setPrototypeOf;
    const canvasWireDefine = Object.defineProperty;
    const canvasPrivateWireStringify = value => {
        let nodes = 0;
        const snapshot = (source, depth) => {
            if (++nodes > 65536 || depth > 12)
                throw new DOMException('Canvas request exceeds the serialization budget', 'NotSupportedError');
            if (source === null || typeof source !== 'object') return source;
            const copy = canvasWireArray(source) ? [] : canvasWireCreate(null);
            if (canvasWireArray(source)) canvasWireSetPrototype(copy, null);
            for (const key of canvasWireKeys(source)) canvasWireDefine(copy, key, {
                value:snapshot(source[key], depth+1), enumerable:true, writable:true, configurable:true
            });
            return copy;
        };
        return canvasWireStringify(snapshot(value, 0));
    };
