    const htmlCollectionConstructionToken = {};
    const htmlCollectionResolvers = new WeakMap();

    // Web IDL indexed getters use the Array intrinsics, not a snapshot iterator.
    // next() reads the current length/index, while forEach captures only the length.
    // https://webidl.spec.whatwg.org/#define-the-iteration-methods
    const installIndexedIterator = (prototype, valueIterable = false) => {
        Object.defineProperty(prototype, Symbol.iterator, {
            value: Array.prototype.values, writable: true, configurable: true
        });
        if (valueIterable) {
            for (const method of ['entries', 'keys', 'values', 'forEach'])
                Object.defineProperty(prototype, method, {
                    value: Array.prototype[method], writable: true, enumerable: true, configurable: true
                });
        }
    };

    const collectionItems = collection => {
        const resolve = htmlCollectionResolvers.get(collection);
        if (!resolve) throw new TypeError('Illegal invocation');
        return resolve();
    };
    const collectionIndex = property => {
        if (typeof property !== 'string' || !/^(0|[1-9][0-9]*)$/.test(property)) return null;
        const index = Number(property);
        return Number.isInteger(index) && index < 4294967295 ? index : null;
    };
    const supportedCollectionNames = items => {
        const names = [];
        const seen = new Set();
        for (const item of items) {
            if (item.id && !seen.has(item.id)) {
                names.push(item.id);
                seen.add(item.id);
            }
            const name = item.namespaceURI === htmlNamespace ? item.getAttribute('name') : null;
            if (name && !seen.has(name)) {
                names.push(name);
                seen.add(name);
            }
        }
        return names;
    };

    class HTMLCollection {
        constructor(token) {
            if (token !== htmlCollectionConstructionToken) throw new TypeError('Illegal constructor');
        }
        get length() { return collectionItems(this).length; }
        item(index) { return collectionItems(this)[Number(index) >>> 0] || null; }
        namedItem(name) {
            name = String(name);
            if (!name) return null;
            const items = collectionItems(this);
            for (const item of items) {
                if (item.id === name || (item.namespaceURI === htmlNamespace &&
                    item.getAttribute('name') === name)) return item;
            }
            return null;
        }
        get [Symbol.toStringTag]() { return 'HTMLCollection'; }
    }
    installIndexedIterator(HTMLCollection.prototype);

    class NodeList {
        constructor(token) {
            if (token !== htmlCollectionConstructionToken) throw new TypeError('Illegal constructor');
        }
        get length() { return collectionItems(this).length; }
        item(index) {
            const items = collectionItems(this);
            if (arguments.length === 0) throw new TypeError('NodeList.item requires an index');
            return items[Number(index) >>> 0] || null;
        }
        get [Symbol.toStringTag]() { return 'NodeList'; }
    }
    installIndexedIterator(NodeList.prototype, true);

    // HTMLCollection is a live legacy platform object: every access resolves the
    // current tree, while indexed and named properties remain ordinary reads.
    // https://dom.spec.whatwg.org/#interface-htmlcollection
    const liveIndexedCollection = (resolve, constructor, named, setIndex = null) => {
        const target = new constructor(htmlCollectionConstructionToken);
        const proxy = new Proxy(target, {
            get(target, property, receiver) {
                const index = collectionIndex(property);
                // Indexed getters only expose supported indices. Unlike item(),
                // an absent property falls through to ordinary prototype lookup.
                // https://webidl.spec.whatwg.org/#legacy-platform-object-getownproperty
                if (index !== null) return target.item(index) ?? Reflect.get(target, property, receiver);
                if (named && typeof property === 'string' && !(property in target)) {
                    return target.namedItem(property) || undefined;
                }
                return Reflect.get(target, property, receiver);
            },
            set(target, property, value, receiver) {
                const index = collectionIndex(property);
                if (!named && index !== null && collectionItems(target)[index] !== undefined)
                    return false;
                if (index !== null && setIndex) {
                    setIndex(index, value);
                    return true;
                }
                return Reflect.set(target, property, value, receiver);
            },
            defineProperty(target, property, descriptor) {
                // NodeList has an indexed getter, but no indexed setter. Web IDL
                // rejects definitions at any array index, even an absent one.
                // https://webidl.spec.whatwg.org/#legacy-platform-object-defineownproperty
                if (!named && collectionIndex(property) !== null) return false;
                return Reflect.defineProperty(target, property, descriptor);
            },
            deleteProperty(target, property) {
                const index = collectionIndex(property);
                if (!named && index !== null) return collectionItems(target)[index] === undefined;
                return Reflect.deleteProperty(target, property);
            },
            preventExtensions(target) {
                return named ? Reflect.preventExtensions(target) : false;
            },
            has(target, property) {
                if (collectionIndex(property) !== null) return target.item(property) !== null || property in target;
                return property in target || (named && typeof property === 'string' &&
                    target.namedItem(property) !== null);
            },
            ownKeys(target) {
                const items = collectionItems(target);
                const keys = Reflect.ownKeys(target);
                const seen = new Set(keys);
                for (let index = 0; index < items.length; index++) {
                    const property = String(index);
                    if (!seen.has(property)) {
                        keys.push(property);
                        seen.add(property);
                    }
                }
                if (named) {
                    for (const property of supportedCollectionNames(items)) {
                        if (!seen.has(property)) {
                            keys.push(property);
                            seen.add(property);
                        }
                    }
                }
                if (!named) {
                    const indices = Array.from({ length: items.length }, (_, index) => String(index));
                    return [...indices, ...keys.filter(key => typeof key === 'string' &&
                        collectionIndex(key) === null), ...keys.filter(key => typeof key === 'symbol')];
                }
                return keys;
            },
            getOwnPropertyDescriptor(target, property) {
                const descriptor = Reflect.getOwnPropertyDescriptor(target, property);
                if (descriptor) return descriptor;
                if (typeof property !== 'string') return undefined;
                const index = collectionIndex(property);
                if (index === null && !named) return undefined;
                const value = index === null
                    ? target.namedItem(property)
                    : target.item(index);
                if (value === null) return undefined;
                return { configurable: true, enumerable: index !== null,
                    writable: index !== null && !!setIndex, value };
            }
        });
        htmlCollectionResolvers.set(target, resolve);
        htmlCollectionResolvers.set(proxy, resolve);
        return proxy;
    };
    const liveHtmlCollection = (resolve, constructor = HTMLCollection, setIndex = null) =>
        liveIndexedCollection(resolve, constructor, true, setIndex);
    const liveNodeList = (resolve, constructor = NodeList) =>
        liveIndexedCollection(resolve, constructor, false);

    const selectorCollection = (root, selector) =>
        liveHtmlCollection(() => list(host('queryAll', nodeId(root), selector)));
