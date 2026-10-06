    // Platform receiver checks precede argument conversion on every 2D member,
    // including an empty/reset operation. Preserve method identity metadata
    // while private drawing state remains independent of author expandos.
    const bindCanvasContextReceivers = prototype => {
        for (const name of Object.getOwnPropertyNames(prototype)) {
            if (name === 'constructor') continue;
            const descriptor = Object.getOwnPropertyDescriptor(prototype, name);
            for (const member of ['value','get','set']) {
                const implementation = descriptor[member];
                if (typeof implementation !== 'function') continue;
                const method = function(...args) {
                    canvasDrawingState(this);
                    return canvasPathApply(implementation, this, args);
                };
                canvasPathDefine(method, 'name', {value:implementation.name, configurable:true});
                canvasPathDefine(method, 'length', {value:implementation.length, configurable:true});
                descriptor[member] = method;
            }
            descriptor.enumerable = true;
            descriptor.configurable = true;
            canvasPathDefine(prototype, name, descriptor);
        }
    };
    bindCanvasContextReceivers(CanvasRenderingContext2D.prototype);
    bindCanvasContextReceivers(OffscreenCanvasRenderingContext2D.prototype);
    canvasPathDefine(CanvasRenderingContext2D, 'length', {value:0, configurable:true});
    canvasPathDefine(OffscreenCanvasRenderingContext2D, 'length', {value:0, configurable:true});
