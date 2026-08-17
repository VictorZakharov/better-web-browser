(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const markTrusted = globalThis.__markTrustedEvent;
    const target = new EventTarget();
    self.addEventListener = target.addEventListener.bind(target);
    self.removeEventListener = target.removeEventListener.bind(target);
    self.dispatchEvent = target.dispatchEvent.bind(target);
    let messageHandler = null, messageErrorHandler = null, errorHandler = null;
    Object.defineProperties(self, {
        onmessage: { configurable: true, enumerable: true, get: () => messageHandler, set: value => { messageHandler = typeof value === 'function' ? value : null; } },
        onmessageerror: { configurable: true, enumerable: true, get: () => messageErrorHandler, set: value => { messageErrorHandler = typeof value === 'function' ? value : null; } },
        onerror: { configurable: true, enumerable: true, get: () => errorHandler, set: value => { errorHandler = typeof value === 'function' ? value : null; } }
    });
    self.postMessage = (message, transfer = undefined) => {
        const transfers = __cloneTransferList(transfer);
        host('workerPost', __serializeClone(message, transfers));
    };
    self.close = () => host('workerClose');
    self.importScripts = (...urls) => {
        const sources = JSON.parse(String(host('workerImportScripts', JSON.stringify(urls.map(String)))));
        for (const source of sources) (0, eval)(source.code + '\n//# sourceURL=' + source.url);
    };
    self.__dispatchWorkerMessage = serialized => {
        try {
            const event = markTrusted(new MessageEvent('message', { data: __deserializeClone(String(serialized)) }));
            target.dispatchEvent(event); messageHandler?.call(self, event);
        } catch (_) {
            const event = markTrusted(new MessageEvent('messageerror'));
            target.dispatchEvent(event); messageErrorHandler?.call(self, event);
        }
    };
    delete globalThis.__markTrustedEvent;
})();
