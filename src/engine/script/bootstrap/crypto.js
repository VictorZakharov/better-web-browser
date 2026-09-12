(() => {
    'use strict';
    const host = globalThis.__hostCall;
    const NativeTypeError = TypeError, NativeDOMException = DOMException, Bytes = Uint8Array;
    // Use internal-slot getters, not author-controlled properties, constructors or tags.
    const typed = Object.getPrototypeOf(Bytes.prototype);
    const getter = (prototype, name) => Function.prototype.call.bind(
        Object.getOwnPropertyDescriptor(prototype, name).get);
    const tag = getter(typed, Symbol.toStringTag);
    const bufferOf = getter(typed, 'buffer'), offsetOf = getter(typed, 'byteOffset');
    const lengthOf = getter(typed, 'byteLength'), dataBufferOf = getter(DataView.prototype, 'buffer');
    const bufferLength = getter(ArrayBuffer.prototype, 'byteLength');
    const resizable = getter(ArrayBuffer.prototype, 'resizable');
    const isView = ArrayBuffer.isView;
    const copy = Function.prototype.call.bind(typed.set);
    const integerTags = new Set(['Int8Array', 'Uint8Array', 'Uint8ClampedArray', 'Int16Array',
        'Uint16Array', 'Int32Array', 'Uint32Array', 'BigInt64Array', 'BigUint64Array']);
    const isIntegerTag = Function.prototype.call.bind(Set.prototype.has, integerTags);
    const randomBytes = length => {
        try { return host('cryptoRandomBytes', length); }
        catch (_) { throw new NativeDOMException('Operating-system randomness is unavailable', 'OperationError'); }
    };
    function Crypto() { throw new NativeTypeError('Illegal constructor'); }
    const crypto = Object.create(Crypto.prototype);
    const requireCrypto = receiver => {
        if (receiver !== crypto) throw new NativeTypeError('Incompatible Crypto receiver');
    };
    Object.defineProperties(Crypto.prototype, {
        [Symbol.toStringTag]: { value: 'Crypto', configurable: true },
        getRandomValues: {
            enumerable: true, configurable: true, writable: true,
            value: function getRandomValues(array) {
                requireCrypto(this);
                if (!isView(array)) throw new NativeTypeError('Expected an ArrayBufferView');
                const kind = tag(array), buffer = kind ? bufferOf(array) : dataBufferOf(array);
                // Web IDL ArrayBufferView has neither [AllowShared] nor [AllowResizable].
                bufferLength(buffer); // Rejects SharedArrayBuffer without trusting instanceof.
                if (resizable(buffer)) throw new NativeTypeError('Resizable buffers are not supported');
                if (!isIntegerTag(kind))
                    throw new NativeDOMException('Expected an integer typed array', 'TypeMismatchError');
                const length = lengthOf(array);
                if (length > 65536)
                    throw new NativeDOMException('Random data exceeds 65536 bytes', 'QuotaExceededError');
                if (length) copy(new Bytes(buffer, offsetOf(array), length), randomBytes(length));
                return array;
            }
        }
    });
    // This realm's trust comes from native document/worker origin state, never location getters.
    if (host('cryptoSecureContext')) Object.defineProperty(Crypto.prototype, 'randomUUID', {
        enumerable: true, configurable: true, writable: true,
        value: function randomUUID() {
            requireCrypto(this);
            const bytes = randomBytes(16);
            bytes[6] = (bytes[6] & 15) | 64;
            bytes[8] = (bytes[8] & 63) | 128;
            const hex = '0123456789abcdef';
            let result = '';
            for (let i = 0; i < 16; i++) {
                if (i === 4 || i === 6 || i === 8 || i === 10) result += '-';
                result += hex[bytes[i] >> 4] + hex[bytes[i] & 15];
            }
            return result;
        }
    });
    Object.defineProperty(Crypto, 'prototype', { writable: false });
    Object.defineProperties(globalThis, {
        Crypto: { value: Crypto, configurable: true, writable: true },
        crypto: { get() { return crypto; }, configurable: true, enumerable: true }
    });
})();
