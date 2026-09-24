(() => {
    'use strict';
    const host = globalThis.__hostCall;
    // The interface is [SecureContext]. Do not advertise it when the native provider is absent.
    if (!host('cryptoSecureContext') || !host('cryptoSubtleAvailable')) return;
    const Bytes = Uint8Array, isView = ArrayBuffer.isView;
    const typed = Object.getPrototypeOf(Bytes.prototype);
    const getter = (prototype, name) => Function.prototype.call.bind(
        Object.getOwnPropertyDescriptor(prototype, name).get);
    const viewBuffer = getter(typed, 'buffer'), viewOffset = getter(typed, 'byteOffset');
    const viewLength = getter(typed, 'byteLength'), dataBuffer = getter(DataView.prototype, 'buffer');
    const dataOffset = getter(DataView.prototype, 'byteOffset');
    const dataLength = getter(DataView.prototype, 'byteLength');
    const bufferLength = getter(ArrayBuffer.prototype, 'byteLength');
    const keys = new WeakMap();
    const error = (name, message) => new DOMException(message, name);
    const unsupported = () => { throw error('NotSupportedError', 'Unsupported Web Crypto algorithm'); };
    const copyBytes = value => {
        if (isView(value)) {
            const data = value instanceof DataView;
            const buffer = data ? dataBuffer(value) : viewBuffer(value);
            bufferLength(buffer); // Reject SharedArrayBuffer, including shared views.
            return new Bytes(buffer, data ? dataOffset(value) : viewOffset(value),
                data ? dataLength(value) : viewLength(value)).slice();
        }
        bufferLength(value); // Native getter rejects non-ArrayBuffer and detached values.
        return new Bytes(value).slice();
    };
    const resultBuffer = bytes => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
    const nameOf = algorithm => {
        const name = typeof algorithm === 'string' ? algorithm : algorithm?.name;
        if (typeof name !== 'string') throw new TypeError('An algorithm name is required');
        const upper = name.toUpperCase();
        if (upper === 'SHA-1' || upper === 'SHA-256' || upper === 'SHA-384' || upper === 'SHA-512') return upper;
        if (['HMAC', 'PBKDF2', 'HKDF', 'AES-GCM', 'AES-CBC', 'AES-CTR', 'AES-KW'].includes(upper)) return upper;
        return unsupported();
    };
    const hashOf = algorithm => {
        const name = nameOf(algorithm);
        if (!name.startsWith('SHA-')) unsupported();
        return name;
    };
    const operation = (fn) => Promise.resolve().then(fn);
    const allowedUsages = {
        'HMAC': ['sign', 'verify'],
        'PBKDF2': ['deriveKey', 'deriveBits'],
        'HKDF': ['deriveKey', 'deriveBits'],
        'AES-GCM': ['encrypt', 'decrypt', 'wrapKey', 'unwrapKey'],
        'AES-CBC': ['encrypt', 'decrypt', 'wrapKey', 'unwrapKey'],
        'AES-CTR': ['encrypt', 'decrypt', 'wrapKey', 'unwrapKey'],
        'AES-KW': ['wrapKey', 'unwrapKey'],
        'ECDH': ['deriveBits', 'deriveKey'], 'ECDSA': ['sign', 'verify']
    };
    const validateUsages = (name, usages) => {
        const legal = allowedUsages[name];
        if (!legal || !Array.isArray(usages) || usages.some(usage => !legal.includes(usage)) ||
            new Set(usages).size !== usages.length)
            throw error('SyntaxError', 'Invalid key usages');
        return usages.slice();
    };
    const requireKey = (key, usage, name) => {
        const state = keys.get(key);
        if (!state) throw new TypeError('Expected a CryptoKey');
        if (name && state.algorithm.name !== name) throw error('InvalidAccessError', 'Key algorithm mismatch');
        if (usage && !state.usages.includes(usage)) throw error('InvalidAccessError', 'Key usage is not permitted');
        return state;
    };
    function CryptoKey() { throw new TypeError('Illegal constructor'); }
    Object.defineProperties(CryptoKey.prototype, {
        [Symbol.toStringTag]: { value: 'CryptoKey', configurable: true },
        type: { enumerable: true, configurable: true, get() { return requireKey(this).type; } },
        extractable: { enumerable: true, configurable: true, get() { return requireKey(this).extractable; } },
        algorithm: { enumerable: true, configurable: true, get() {
            const value = requireKey(this).algorithm;
            if (value.publicExponent) return {name: value.name, modulusLength: value.modulusLength,
                publicExponent: value.publicExponent.slice(), hash: {...value.hash}};
            return value.hash ? { name: value.name, hash: { ...value.hash }, length: value.length } : { ...value };
        } },
        usages: { enumerable: true, configurable: true, get() { return requireKey(this).usages.slice(); } }
    });
    const makeKey = (algorithm, material, extractable, usages) => {
        const key = Object.create(CryptoKey.prototype);
        keys.set(key, { type: 'secret', algorithm, material: material.slice(),
            extractable: Boolean(extractable), usages: usages.slice() });
        return key;
    };
    function SubtleCrypto() { throw new TypeError('Illegal constructor'); }
    const subtle = Object.create(SubtleCrypto.prototype);
    const requireSubtle = receiver => { if (receiver !== subtle) throw new TypeError('Incompatible SubtleCrypto receiver'); };
    Object.defineProperty(SubtleCrypto.prototype, Symbol.toStringTag, { value: 'SubtleCrypto', configurable: true });
    Object.defineProperty(Crypto.prototype, 'subtle', {
        enumerable: true, configurable: true,
        get() { if (this !== crypto) throw new TypeError('Incompatible Crypto receiver'); return subtle; }
    });
    Object.defineProperties(globalThis, {
        CryptoKey: { value: CryptoKey, writable: true, configurable: true },
        SubtleCrypto: { value: SubtleCrypto, writable: true, configurable: true }
    });
