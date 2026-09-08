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
    let refreshWindowNamedPropertyValues = () => {};
    let maybeUpgradeCustomElement = element => element;
    let upgradeCustomElementTree = () => {};
    let connectCustomElementTree = () => {};
    let disconnectCustomElementTree = () => {};
    let adoptCustomElementTree = () => {};
    let customElementAttributeChanged = () => {};
    let scheduleSlotChangeCheck = () => {};
    let shadowRootForTraversal = () => null;
    let resetAttributeNameMode = () => {};
    let invalidateMutationAncestors = () => {};
    let replaceElementInnerHtml = () => {};
    let constructCustomElement = () => { throw new TypeError('Illegal constructor'); };
    const list = value => {
        if (!value) return [];
        const result = value.split(',').filter(Boolean).map(id => wrap(Number(id)));
        result.item = index => result[index] || null;
        return result;
    };
    const childCollectionCache = new WeakMap();
    const childCollectionVersions = new WeakMap();
    const markChildCollectionsChanged = (...nodes) => {
        for (const node of nodes.flat()) {
            if (!node || (typeof node !== 'object' && typeof node !== 'function')) continue;
            childCollectionVersions.set(node, (childCollectionVersions.get(node) || 0) + 1);
            invalidateMutationAncestors(node);
        }
    };
    const childCollection = (node, elements) => {
        const version = childCollectionVersions.get(node) || 0;
        let records = childCollectionCache.get(node);
        if (!records) childCollectionCache.set(node, records = {});
        const key = elements ? 'elements' : 'nodes';
        let record = records[key];
        if (elements) {
            if (!record) {
                const value = liveHtmlCollection(() =>
                    list(host('elementChildren', node.__id)));
                records[key] = record = { version, value };
            }
            return record.value;
        }
        if (!record) {
            const value = list(host('children', node.__id));
            records[key] = record = { version, value };
        } else if (record.version !== version) {
            const next = list(host('children', node.__id));
            record.value.splice(0, record.value.length, ...next);
            record.version = version;
        }
        return record.value;
    };
    const elementSibling = (node, next) => {
        let sibling = next ? node.nextSibling : node.previousSibling;
        while (sibling && sibling.nodeType !== Node.ELEMENT_NODE)
            sibling = next ? sibling.nextSibling : sibling.previousSibling;
        return sibling;
    };
