    const aesParameters = algorithm => {
        if (nameOf(algorithm) !== 'AES-GCM') unsupported();
        const iv = copyBytes(algorithm.iv);
        const aad = algorithm.additionalData === undefined ? new Bytes(0) : copyBytes(algorithm.additionalData);
        const tagLength = algorithm.tagLength === undefined ? 128 : Number(algorithm.tagLength);
        if (![32, 64, 96, 104, 112, 120, 128].includes(tagLength) || !iv.length)
            throw error('OperationError', 'Invalid AES-GCM parameters');
        return { iv, aad, tagLength };
    };
    const crypt = (decrypt, algorithm, key, data, usage = decrypt ? 'decrypt' : 'encrypt') => {
        const name = nameOf(algorithm), state = requireKey(key, usage, name);
        if (!name.startsWith('AES-')) unsupported();
        try {
            if (name === 'AES-GCM') {
                const params = aesParameters(algorithm);
                return resultBuffer(host('cryptoSubtleAesGcm', decrypt, state.material, params.iv,
                    params.aad, copyBytes(data), params.tagLength / 8));
            }
            if (name === 'AES-CBC') {
                const iv = copyBytes(algorithm.iv);
                if (iv.length !== 16) throw error('OperationError', 'AES-CBC requires a 16-byte IV');
                return resultBuffer(host('cryptoSubtleAesCbc', decrypt, state.material, iv, copyBytes(data)));
            }
            if (name === 'AES-CTR') {
                const counter = copyBytes(algorithm.counter), length = Number(algorithm.length);
                if (counter.length !== 16 || !Number.isInteger(length) || length < 1 || length > 128)
                    throw error('OperationError', 'Invalid AES-CTR counter');
                return resultBuffer(host('cryptoSubtleAesCtr', state.material, counter, length, copyBytes(data)));
            }
            unsupported();
        } catch (cause) {
            if (cause?.name === 'NotSupportedError' || cause?.name === 'TypeError') throw cause;
            throw error('OperationError', `${name} operation failed`);
        }
    };
    const wrapBytes = (unwrap, algorithm, key, data, usage) => {
        const name = nameOf(algorithm);
        if (name === 'AES-KW') {
            const state = requireKey(key, usage, 'AES-KW');
            try { return resultBuffer(host('cryptoSubtleAesKw', unwrap, state.material, copyBytes(data))); }
            catch (_) { throw error('OperationError', `AES-KW ${unwrap ? 'unwrap' : 'wrap'} failed`); }
        }
        return crypt(unwrap, algorithm, key, data, usage);
    };
    const derive = (algorithm, baseKey, length, usage) => {
        const name = nameOf(algorithm), state = requireKey(baseKey, usage, name);
        if (!['PBKDF2', 'HKDF'].includes(name)) unsupported();
        const hash = hashOf(algorithm.hash), salt = copyBytes(algorithm.salt);
        if (!Number.isInteger(length) || length < 1 || length % 8 || length > 128 * 1024 * 1024)
            throw error('OperationError', 'Invalid derived bit length');
        try {
            if (name === 'HKDF') return host('cryptoSubtleHkdf', hash, state.material,
                salt, copyBytes(algorithm.info), length / 8);
            const iterations = Number(algorithm.iterations);
            if (!Number.isInteger(iterations) || iterations < 1 || iterations > 0xffffffff)
                throw error('OperationError', 'Invalid PBKDF2 iterations');
            return host('cryptoSubtlePbkdf2', hash, state.material, salt, iterations, length / 8);
        } catch (_) { throw error('OperationError', `${name} derivation failed`); }
    };
    Object.defineProperties(SubtleCrypto.prototype, {
        digest: { configurable: true, writable: true, value: function digest(algorithm, data) {
            requireSubtle(this);
            return operation(() => {
                const name = hashOf(algorithm);
                try { return resultBuffer(host('cryptoSubtleDigest', name, copyBytes(data))); }
                catch (_) { throw error('OperationError', 'Digest failed'); }
            });
        } },
        sign: { configurable: true, writable: true, value: function sign(algorithm, key, data) {
            requireSubtle(this);
            return operation(() => {
                if (nameOf(algorithm) !== 'HMAC') unsupported();
                const state = requireKey(key, 'sign', 'HMAC');
                try { return resultBuffer(host('cryptoSubtleHmac', state.algorithm.hash.name, state.material, copyBytes(data))); }
                catch (_) { throw error('OperationError', 'HMAC signing failed'); }
            });
        } },
        verify: { configurable: true, writable: true, value: function verify(algorithm, key, signature, data) {
            requireSubtle(this);
            return operation(() => {
                if (nameOf(algorithm) !== 'HMAC') unsupported();
                const state = requireKey(key, 'verify', 'HMAC'), actual = copyBytes(signature);
                let expected;
                try { expected = host('cryptoSubtleHmac', state.algorithm.hash.name, state.material, copyBytes(data)); }
                catch (_) { throw error('OperationError', 'HMAC verification failed'); }
                let difference = actual.length ^ expected.length;
                for (let i = 0; i < expected.length; i++) difference |= expected[i] ^ (actual[i] || 0);
                return difference === 0;
            });
        } },
        encrypt: { configurable: true, writable: true, value: function encrypt(algorithm, key, data) {
            requireSubtle(this); return operation(() => crypt(false, algorithm, key, data));
        } },
        decrypt: { configurable: true, writable: true, value: function decrypt(algorithm, key, data) {
            requireSubtle(this); return operation(() => crypt(true, algorithm, key, data));
        } },
        deriveBits: { configurable: true, writable: true, value: function deriveBits(algorithm, baseKey, length) {
            requireSubtle(this); return operation(() => resultBuffer(derive(algorithm, baseKey, Number(length), 'deriveBits')));
        } },
        deriveKey: { configurable: true, writable: true, value: function deriveKey(algorithm, baseKey, derivedKeyType, extractable, keyUsages) {
            requireSubtle(this);
            return operation(() => {
                const parameters = keyParameters(derivedKeyType);
                if (parameters.name === 'PBKDF2' || parameters.name === 'HKDF') unsupported();
                const length = Number(derivedKeyType.length);
                if (![128, 192, 256].includes(length) || !parameters.name.startsWith('AES-')) unsupported();
                const material = derive(algorithm, baseKey, length, 'deriveKey');
                return createImportedKey('raw', material, derivedKeyType, extractable, keyUsages);
            });
        } },
        wrapKey: { configurable: true, writable: true, value: function wrapKey(format, key, wrappingKey, wrapAlgorithm) {
            requireSubtle(this);
            return operation(() => {
                const exported = exportMaterial(format, key);
                if (format !== 'raw') unsupported();
                return wrapBytes(false, wrapAlgorithm, wrappingKey, new Bytes(exported), 'wrapKey');
            });
        } },
        unwrapKey: { configurable: true, writable: true, value: function unwrapKey(format, wrappedKey, unwrappingKey, unwrapAlgorithm, unwrappedKeyAlgorithm, extractable, keyUsages) {
            requireSubtle(this);
            return operation(() => {
                if (format !== 'raw') unsupported();
                let material;
                try { material = wrapBytes(true, unwrapAlgorithm, unwrappingKey, wrappedKey, 'unwrapKey'); }
                catch (_) { throw error('OperationError', 'Key unwrap failed'); }
                return createImportedKey('raw', material, unwrappedKeyAlgorithm, extractable, keyUsages);
            });
        } }
    });
