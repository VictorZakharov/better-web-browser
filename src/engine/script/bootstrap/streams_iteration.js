(() => {
    'use strict';
    // Web IDL's async-iterator machinery serializes next/return calls. Keep
    // the queue explicit so return() can release an idle reader synchronously.
    const asyncIteratorPrototype = Object.getPrototypeOf(
        Object.getPrototypeOf(async function* () {}).prototype);
    const slots = new WeakMap();
    const prototype = Object.create(asyncIteratorPrototype);
    const done = value => ({ value, done: true });

    const advance = state => {
        if (state.busy || !state.queue.length) return;
        state.busy = true;
        const { operation, resolve, reject } = state.queue.shift();
        let result;
        try { result = operation(); }
        catch (error) { result = Promise.reject(error); }
        Promise.resolve(result).then(value => {
            resolve(value);
            state.busy = false;
            advance(state);
        }, error => {
            reject(error);
            state.busy = false;
            advance(state);
        });
    };
    const schedule = (state, operation) => new Promise((resolve, reject) => {
        state.queue.push({ operation, resolve, reject });
        advance(state);
    });

    Object.assign(prototype, {
        next() {
            const state = slots.get(this);
            if (!state) return Promise.reject(new TypeError('Invalid stream iterator'));
            return schedule(state, () => {
                if (state.finished) return done(undefined);
                return state.reader.read().then(result => {
                    if (result.done) {
                        state.finished = true;
                        state.reader.releaseLock();
                    }
                    return result;
                }, error => {
                    state.finished = true;
                    state.reader.releaseLock();
                    throw error;
                });
            });
        },
        return(value) {
            const state = slots.get(this);
            if (!state) return Promise.reject(new TypeError('Invalid stream iterator'));
            return schedule(state, () => {
                if (state.finished) return done(value);
                state.finished = true;
                const cancellation = state.preventCancel
                    ? Promise.resolve() : state.reader.cancel(value);
                state.reader.releaseLock();
                return Promise.resolve(cancellation).then(() => done(value));
            });
        }
    });

    const values = function (options = {}) {
        if (!(this instanceof ReadableStream))
            throw new TypeError('ReadableStream.values requires a stream');
        const preventCancel = !!Object(options).preventCancel;
        const iterator = Object.create(prototype);
        slots.set(iterator, {
            reader: this.getReader(), preventCancel, finished: false,
            busy: false, queue: []
        });
        return iterator;
    };
    Object.defineProperties(ReadableStream.prototype, {
        values: { value: values, writable: true, enumerable: true, configurable: true },
        [Symbol.asyncIterator]: {
            value: values, writable: true, enumerable: true, configurable: true
        }
    });
})();
