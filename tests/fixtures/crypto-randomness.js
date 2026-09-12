// Original cross-engine Web Crypto randomness contract; also runs in a dedicated worker.
function checkCryptoRandomness() {
    const failures = [];
    let checks = 0;
    const check = (name, run) => {
        checks++;
        try { run(); } catch (error) { failures.push(name + ': ' + error.message); }
    };
    const assert = (condition, message = 'assertion failed') => {
        if (!condition) throw new Error(message);
    };
    const throws = (name, run) => {
        let actual = 'no exception';
        try { run(); } catch (error) { actual = error.name; }
        assert(actual === name, actual + ' instead of ' + name);
    };
    check('interface', () => {
        assert(crypto === globalThis.crypto && crypto instanceof Crypto);
        assert(Object.prototype.toString.call(crypto) === '[object Crypto]');
        throws('TypeError', () => new Crypto());
        throws('TypeError', () => crypto.getRandomValues.call({}, new Uint8Array(4)));
    });
    for (const Type of [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array,
        Int32Array, Uint32Array, BigInt64Array, BigUint64Array]) {
        check(Type.name, () => {
            const values = new Type(32);
            assert(crypto.getRandomValues(values) === values);
            // Smoke check, not a statistical/security proof: catches the old 8-bit-per-element fill.
            const bytes = new Uint8Array(values.buffer);
            for (let lane = 0; lane < Type.BYTES_PER_ELEMENT; lane++) {
                let nonzero = false;
                for (let i = lane; i < bytes.length; i += Type.BYTES_PER_ELEMENT)
                    nonzero ||= bytes[i] !== 0;
                assert(nonzero, 'zero byte lane ' + lane);
            }
        });
    }
    check('view boundaries', () => {
        const bytes = new Uint8Array(80).fill(0xa5);
        const view = new Uint32Array(bytes.buffer, 8, 16);
        crypto.getRandomValues(view);
        assert(bytes.slice(0, 8).every(x => x === 0xa5));
        assert(bytes.slice(72).every(x => x === 0xa5));
    });
    check('quota in bytes', () => {
        crypto.getRandomValues(new Uint32Array(16384));
        const large = new Uint32Array(16385).fill(0x12345678);
        throws('QuotaExceededError', () => crypto.getRandomValues(large));
        assert(large.every(x => x === 0x12345678));
        const empty = new Uint8Array(0);
        assert(crypto.getRandomValues(empty) === empty);
    });
    check('argument validation', () => {
        for (const value of [undefined, null, {}, [], new ArrayBuffer(8),
            new Proxy(new Uint8Array(8), {})])
            throws('TypeError', () => crypto.getRandomValues(value));
        for (const value of [new Float32Array(8), new Float64Array(8), new DataView(new ArrayBuffer(8))])
            throws('TypeMismatchError', () => crypto.getRandomValues(value));
        if (typeof SharedArrayBuffer === 'function')
            throws('TypeError', () => crypto.getRandomValues(new Uint8Array(new SharedArrayBuffer(8))));
        throws('TypeError', () => crypto.getRandomValues(new Uint8Array(new ArrayBuffer(8, { maxByteLength: 16 }))));
    });
    check('detached empty view', () => {
        const values = new Uint8Array(8);
        values.buffer.transfer();
        assert(crypto.getRandomValues(values) === values);
    });
    check('unforgeable view slots', () => {
        const values = new Uint32Array(32);
        for (const name of ['buffer', 'byteOffset', 'byteLength', 'length', 'constructor'])
            Object.defineProperty(values, name, { get() { throw new Error('author getter ' + name); } });
        Object.defineProperty(values, Symbol.toStringTag, { value: 'Float32Array' });
        assert(crypto.getRandomValues(values) === values);
        assert(values[0] !== 0 || values[1] !== 0);
    });
    check('independent of Math.random and public method', () => {
        const random = Math.random, fill = crypto.getRandomValues;
        try {
            Math.random = () => { throw new Error('insecure RNG used'); };
            fill.call(crypto, new Uint8Array(32));
            if (typeof crypto.randomUUID === 'function') {
                crypto.getRandomValues = () => { throw new Error('public method used'); };
                const ids = new Set();
                for (let i = 0; i < 32; i++) {
                    const id = crypto.randomUUID();
                    assert(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(id));
                    ids.add(id);
                }
                assert(ids.size === 32);
                throws('TypeError', () => crypto.randomUUID.call({}));
            }
        } finally { Math.random = random; crypto.getRandomValues = fill; }
    });
    return { checks, failures };
}
