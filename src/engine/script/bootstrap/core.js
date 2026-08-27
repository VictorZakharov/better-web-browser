(() => {
    'use strict';
    const host = function () { return __hostCall.apply(null, arguments); };
    const isolatedIframeWindow = globalThis.__iframeWindow;
    delete globalThis.__iframeWindow;
    if (typeof String.prototype.substr !== 'function') {
        Object.defineProperty(String.prototype, 'substr', {
            configurable: true,
            writable: true,
            value(start, length) {
                const string = String(this);
                const size = string.length;
                let from = Number(start) || 0;
                from = from < 0 ? Math.max(size + Math.ceil(from), 0) : Math.min(Math.floor(from), size);
                if (length === undefined) return string.slice(from);
                let count = Number(length);
                if (Number.isNaN(count) || count <= 0) return '';
                if (count !== Infinity) count = Math.floor(count);
                return string.slice(from, Math.min(from + count, size));
            }
        });
    }
    const cache = new Map();
    let refreshWindowNamedProperties = () => {};
    let maybeUpgradeCustomElement = element => element;
    let upgradeCustomElementTree = () => {};
    let connectCustomElementTree = () => {};
    let disconnectCustomElementTree = () => {};
    let adoptCustomElementTree = () => {};
    let customElementAttributeChanged = () => {};
    let scheduleSlotChangeCheck = () => {};
    let shadowRootForTraversal = () => null;
    let constructCustomElement = () => { throw new TypeError('Illegal constructor'); };
    const list = value => {
        if (!value) return [];
        const result = value.split(',').filter(Boolean).map(id => wrap(Number(id)));
        result.item = index => result[index] || null;
        return result;
    };
