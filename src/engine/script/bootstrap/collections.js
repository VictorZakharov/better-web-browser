    const htmlCollectionConstructionToken = {};
    const htmlCollectionResolvers = new WeakMap();

    const collectionItems = collection => {
        const resolve = htmlCollectionResolvers.get(collection);
        if (!resolve) throw new TypeError('Illegal invocation');
        return resolve();
    };
    const collectionIndex = property => {
        if (typeof property !== 'string' || !/^(0|[1-9][0-9]*)$/.test(property)) return null;
        const index = Number(property);
        return Number.isSafeInteger(index) ? index : null;
    };
    const supportedCollectionNames = items => {
        const names = [];
        const seen = new Set();
        for (const item of items) {
            if (item.id && !seen.has(item.id)) {
                names.push(item.id);
                seen.add(item.id);
            }
        }
        for (const item of items) {
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
            for (const item of items) if (item.id === name) return item;
            for (const item of items) {
                if (item.namespaceURI === htmlNamespace && item.getAttribute('name') === name) return item;
            }
            return null;
        }
        [Symbol.iterator]() { return collectionItems(this)[Symbol.iterator](); }
        get [Symbol.toStringTag]() { return 'HTMLCollection'; }
    }

    // HTMLCollection is a live legacy platform object: every access resolves the
    // current tree, while indexed and named properties remain ordinary reads.
    // https://dom.spec.whatwg.org/#interface-htmlcollection
    const liveHtmlCollection = resolve => {
        const target = new HTMLCollection(htmlCollectionConstructionToken);
        const proxy = new Proxy(target, {
            get(target, property, receiver) {
                const index = collectionIndex(property);
                if (index !== null) return target.item(index);
                if (typeof property === 'string' && !(property in target)) {
                    return target.namedItem(property) || undefined;
                }
                return Reflect.get(target, property, receiver);
            },
            has(target, property) {
                if (collectionIndex(property) !== null) return target.item(property) !== null;
                return property in target ||
                    (typeof property === 'string' && target.namedItem(property) !== null);
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
                for (const property of supportedCollectionNames(items)) {
                    if (!seen.has(property)) {
                        keys.push(property);
                        seen.add(property);
                    }
                }
                return keys;
            },
            getOwnPropertyDescriptor(target, property) {
                const descriptor = Reflect.getOwnPropertyDescriptor(target, property);
                if (descriptor) return descriptor;
                if (typeof property !== 'string') return undefined;
                const index = collectionIndex(property);
                const value = index === null
                    ? target.namedItem(property)
                    : target.item(index);
                if (value === null) return undefined;
                return { configurable: true, enumerable: true, writable: false, value };
            }
        });
        htmlCollectionResolvers.set(target, resolve);
        htmlCollectionResolvers.set(proxy, resolve);
        return proxy;
    };

    const selectorCollection = (root, selector) =>
        liveHtmlCollection(() => list(host('queryAll', root.__id, selector)));
