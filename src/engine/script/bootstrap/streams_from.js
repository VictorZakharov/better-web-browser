(() => {
    'use strict';
    const isObject = value => value !== null &&
        (typeof value === 'object' || typeof value === 'function');
    const getMethod = (object, key) => {
        const method = object[key];
        if (method == null) return undefined;
        if (typeof method !== 'function') throw new TypeError('Iterator method is not callable');
        return method;
    };

    // Web IDL async_sequence<any> accepts only objects. A synchronous iterator
    // is wrapped like CreateAsyncFromSyncIterator, including awaiting yielded
    // promises, while an asynchronous iterator's yielded value is not awaited.
    ReadableStream.from = function from(iterable) {
        if (!isObject(iterable)) throw new TypeError('Expected an async iterable object');
        let method = getMethod(iterable, Symbol.asyncIterator);
        const sync = method === undefined;
        if (sync) method = getMethod(iterable, Symbol.iterator);
        if (!method) throw new TypeError('Expected an async iterable object');
        const iterator = method.call(iterable);
        if (!isObject(iterator)) throw new TypeError('Iterator method returned a non-object');
        const next = iterator.next;
        if (typeof next !== 'function') throw new TypeError('Iterator next is not callable');

        const readNext = () => {
            let step;
            try { step = next.call(iterator); }
            catch (error) { return Promise.reject(error); }
            return Promise.resolve(step).then(result => {
                if (!isObject(result)) throw new TypeError('Iterator result is not an object');
                if (result.done) return { done: true };
                return sync ? Promise.resolve(result.value).then(value => ({ value }))
                    : { value: result.value };
            });
        };
        const closeIterator = reason => {
            let method;
            try { method = getMethod(iterator, 'return'); }
            catch (error) { return Promise.reject(error); }
            if (!method) return Promise.resolve();
            let result;
            try { result = method.call(iterator, reason); }
            catch (error) { return Promise.reject(error); }
            return Promise.resolve(result).then(value => {
                if (!isObject(value)) throw new TypeError('Iterator return result is not an object');
            });
        };
        return new ReadableStream({
            pull(controller) {
                return readNext().then(result => {
                    if (result.done) controller.close();
                    else controller.enqueue(result.value);
                }, error => controller.error(error));
            },
            cancel: closeIterator
        }, { highWaterMark: 0 });
    };
})();
