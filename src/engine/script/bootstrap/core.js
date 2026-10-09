(() => {
    'use strict';
    // __hostCall is the outer bootstrap closure's private native binding. The
    // page-global property is removed when initialization ends.
    const host = __hostCall;
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
    const nodeHandles = new WeakMap();
    // Internal algorithms must not read author-replaceable DOM accessors or the
    // legacy __metadata fields. The native identity and immutable node metadata
    // are recorded when a validated wrapper is constructed.
    const nativeNodeMetadata = new WeakMap();
    const nativeNodeType = node => nativeNodeMetadata.get(node)?.type ?? host('nodeType', nodeId(node));
    const nativeNodeLocalName = node => nativeNodeMetadata.get(node)?.localName ?? host('localName', nodeId(node));
    const nativeNodeNamespace = node => nativeNodeMetadata.get(node)?.namespaceURI ?? host('namespaceUri', nodeId(node));
    const nodeId = node => {
        if (node == null) return 0;
        let id = nodeHandles.get(node);
        if (id === undefined) {
            id = host('nodeHandle', node);
            if (!id) return 0;
            nodeHandles.set(node, id);
        }
        return id;
    };
    const isNode = value => nodeHandles.has(value) || !!nodeId(value);
    let refreshWindowNamedProperties = () => {};
    let refreshWindowNamedPropertyValues = () => {};
    let maybeUpgradeCustomElement = element => element;
    let upgradeCustomElementTree = () => {};
    let connectCustomElementTree = () => {};
    let disconnectCustomElementTree = () => {};
    let adoptCustomElementTree = () => {};
    let customElementAttributeChanged = () => {};
    let validateCanvasAttributeSet = () => {};
    let canvasAttributeChanged = () => {};
    let scheduleSlotChangeCheck = () => {};
    let shadowRootForTraversal = () => null;
    let focusedAreaForDocument = () => null;
    let setFocusedAreaForDocument = () => {};
    let activeElementForRoot = () => null;
    let focusTargetForElement = () => null;
    let focusElement = () => {};
    let blurElement = () => {};
    let repairDetachedFocus = () => {};
    let queuePolicyViolation = () => {};
    let resetAttributeNameMode = () => {};
    let transitionBeforeAttributeChange = () => null;
    let syncCssAnimations = () => {};
    let transitionAfterAttributeChange = () => {};
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
    let parserCollectionEpoch = 0;
    const childCollectionVersions = new WeakMap();
    const markChildCollectionsChanged = (...nodes) => {
        for (const node of nodes.flat()) {
            if (!node || (typeof node !== 'object' && typeof node !== 'function')) continue;
            childCollectionVersions.set(node, (childCollectionVersions.get(node) || 0) + 1);
            invalidateMutationAncestors(node);
        }
    };
    const childCollection = (node, elements) => {
        let records = childCollectionCache.get(node);
        if (!records) childCollectionCache.set(node, records = {});
        const key = elements ? 'elements' : 'nodes';
        let record = records[key];
        if (elements) {
            if (!record) {
                const value = liveHtmlCollection(() =>
                    list(host('elementChildren', nodeId(node))));
                records[key] = record = { value };
            }
            return record.value;
        }
        if (!record) {
            // DOM childNodes is a SameObject live NodeList. Resolve lazily on
            // collection access, including references retained across mutations;
            // merely obtaining the collection must not serialize the whole tree.
            // https://dom.spec.whatwg.org/#dom-node-childnodes
            record = { version: -1, parserEpoch: -1, items: [] };
            record.value = liveNodeList(() => {
                const version = childCollectionVersions.get(node) || 0;
                if (record.version !== version || record.parserEpoch !== parserCollectionEpoch) {
                    record.items = list(host('children', nodeId(node)));
                    record.version = version;
                    record.parserEpoch = parserCollectionEpoch;
                }
                return record.items;
            });
            records[key] = record;
        }
        return record.value;
    };
    const elementSibling = (node, next) => {
        let sibling = next ? node.nextSibling : node.previousSibling;
        while (sibling && sibling.nodeType !== Node.ELEMENT_NODE)
            sibling = next ? sibling.nextSibling : sibling.previousSibling;
        return sibling;
    };
