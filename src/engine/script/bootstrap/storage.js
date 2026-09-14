    // HTML Storage + Web IDL legacy named properties. Both syntaxes use the
    // same native map; methods inherited from the prototype remain visible.
    (() => {
        const areas = new WeakMap();
        const areaFor = receiver => {
            if (!areas.has(receiver)) throw new TypeError('Incompatible Storage receiver');
            return areas.get(receiver);
        };
        const domString = value => {
            if (typeof value === 'symbol') throw new TypeError('Storage strings cannot be symbols');
            return String(value);
        };
        const required = (args, count) => {
            if (args.length < count) throw new TypeError('Missing Storage argument');
        };
        const set = (area, key, value) => {
            if (!host('storageSet', area, key, domString(value))) {
                throw new QuotaExceededError('Web Storage quota exceeded');
            }
        };
        class Storage {
            constructor() { throw new TypeError('Illegal constructor'); }
            get length() { return host('storageLength', areaFor(this)); }
            key(index) {
                const area = areaFor(this); required(arguments, 1);
                return host('storageKey', area, index >>> 0);
            }
            getItem(key) {
                const area = areaFor(this); required(arguments, 1);
                return host('storageGet', area, domString(key));
            }
            setItem(key, value) {
                const area = areaFor(this); required(arguments, 2);
                set(area, domString(key), value);
            }
            removeItem(key) {
                const area = areaFor(this); required(arguments, 1);
                host('storageRemove', area, domString(key));
            }
            clear() { host('storageClear', areaFor(this)); }
        }
        Object.defineProperty(Storage.prototype, Symbol.toStringTag, {
            value: 'Storage', configurable: true,
        });
        for (const key of ['length', 'key', 'getItem', 'setItem', 'removeItem', 'clear']) {
            Object.defineProperty(Storage.prototype, key, { enumerable: true });
        }
        const create = area => {
            const target = Object.create(Storage.prototype);
            const visible = key => typeof key === 'string' && !Reflect.has(target, key);
            const get = key => host('storageGet', area, key);
            const proxy = new Proxy(target, {
                get(target, key, receiver) {
                    if (visible(key)) return get(key) ?? undefined;
                    return Reflect.get(target, key, receiver);
                },
                set(target, key, value) {
                    if (typeof key === 'symbol') return Reflect.set(target, key, value);
                    set(area, key, value); return true;
                },
                has(target, key) { return Reflect.has(target, key) || (visible(key) && get(key) !== null); },
                deleteProperty(target, key) {
                    if (visible(key)) { host('storageRemove', area, key); return true; }
                    return Reflect.deleteProperty(target, key);
                },
                ownKeys(target) {
                    const keys = [];
                    for (let index = 0; index < host('storageLength', area); index++) {
                        const key = host('storageKey', area, index);
                        if (visible(key)) keys.push(key);
                    }
                    return keys.concat(Reflect.ownKeys(target));
                },
                getOwnPropertyDescriptor(target, key) {
                    if (visible(key)) {
                        const value = get(key);
                        if (value !== null) return { value, writable: true, enumerable: true, configurable: true };
                    }
                    return Reflect.getOwnPropertyDescriptor(target, key);
                },
                defineProperty(target, key, descriptor) {
                    if (typeof key === 'symbol') return Reflect.defineProperty(target, key, descriptor);
                    if ('get' in descriptor || 'set' in descriptor || descriptor.configurable === false) return false;
                    set(area, key, descriptor.value); return true;
                },
                preventExtensions() { return false; },
            });
            areas.set(proxy, area);
            return proxy;
        };
        windowObject.Storage = Storage;
        for (const [name, area] of [['localStorage', 'local'], ['sessionStorage', 'session']]) {
            const value = create(area);
            Object.defineProperty(windowObject, name, { get: () => value, configurable: true, enumerable: true });
        }
    })();
