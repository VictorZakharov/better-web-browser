    // CanvasPath methods convert every IDL argument before consulting the CTM
    // or mutating geometry. Number() is not Web IDL ToNumber: it accepts BigInt.
    const canvasPathApply = Reflect.apply;
    const canvasPathDefine = Object.defineProperty;
    const canvasPathReceiver = (receiver, pathReceiver) => {
        const valid = pathReceiver ? canvasPathData.has(receiver) : canvas2dOwners.has(receiver);
        if (!valid) throw new TypeError('Illegal CanvasPath receiver');
    };
    const canvasPathNumbers = (args, required, name) => {
        if (args.length < required) throw new TypeError(`${name} requires ${required} arguments`);
        const converted = [];
        for (let index = 0; index < required; index++) converted.push(+args[index]);
        return converted;
    };
    const canvasRadiusDictionary = value => {
        if (value === null || value === undefined) return {x:0, y:0};
        if (typeof value !== 'object' && typeof value !== 'function') return +value;
        // DOMPointInit dictionary conversion is lexicographic, including the
        // unused w/z members. Their getters and conversion errors are observable.
        const w = value.w, convertedW = w === undefined ? 1 : +w;
        const x = value.x, convertedX = x === undefined ? 0 : +x;
        const y = value.y, convertedY = y === undefined ? 0 : +y;
        const z = value.z, convertedZ = z === undefined ? 0 : +z;
        void convertedW; void convertedZ;
        return {x:convertedX, y:convertedY};
    };
    const canvasRadiusUnion = value => {
        if (value === undefined) return 0;
        if (value !== null && (typeof value === 'object' || typeof value === 'function')) {
            const iterator = value[Symbol.iterator];
            if (iterator !== undefined && iterator !== null) {
                if (typeof iterator !== 'function') throw new TypeError('Radii iterator is not callable');
                return canvasPathSequence(value, iterator, canvasRadiusDictionary);
            }
        }
        return canvasRadiusDictionary(value);
    };
    const canvasPathSequence = idlSequenceFromIterator;
    const bindCanvasPathNumbers = (prototype, descriptors) => {
        const pathReceiver = prototype === Path2D.prototype;
        for (const [name, count, option] of descriptors) {
            const implementation = prototype[name];
            const method = function(...args) {
                canvasPathReceiver(this, pathReceiver);
                const converted = canvasPathNumbers(args, count, name);
                if (option === 'boolean') converted.push(args[count] === undefined ? false : !!args[count]);
                else if (option === 'radii') converted.push(canvasRadiusUnion(args[count]));
                return canvasPathApply(implementation, this, converted);
            };
            canvasPathDefine(method, 'name', {value:name, configurable:true});
            canvasPathDefine(method, 'length', {value:count, configurable:true});
            canvasPathDefine(prototype, name, {value:method, enumerable:true, writable:true, configurable:true});
        }
    };
