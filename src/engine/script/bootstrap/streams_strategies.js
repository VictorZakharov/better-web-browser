(() => {
    'use strict';

    const highWaterMark = init => {
        if (init === undefined || init === null || !('highWaterMark' in Object(init)))
            throw new TypeError('highWaterMark is required');
        // QueuingStrategyInit converts to unrestricted double. The stream constructor,
        // not the queuing-strategy constructor, rejects negative or NaN marks.
        return Number(init.highWaterMark);
    };

    class ByteLengthQueuingStrategy {
        constructor(init) { this.highWaterMark = highWaterMark(init); }
        size(chunk) { return chunk.byteLength; }
    }

    class CountQueuingStrategy {
        constructor(init) { this.highWaterMark = highWaterMark(init); }
        size() { return 1; }
    }

    Object.assign(globalThis, { ByteLengthQueuingStrategy, CountQueuingStrategy });
})();
