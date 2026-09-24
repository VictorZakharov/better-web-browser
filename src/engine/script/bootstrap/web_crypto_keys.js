    const base64url = bytes => {
        let binary = '';
        for (const byte of bytes) binary += String.fromCharCode(byte);
        return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
    };
    const fromBase64url = input => {
        if (typeof input !== 'string' || !/^[A-Za-z0-9_-]*$/.test(input) || input.length % 4 === 1)
            throw error('DataError', 'Invalid JWK key encoding');
        const binary = atob(input.replace(/-/g, '+').replace(/_/g, '/'));
        const bytes = Bytes.from(binary, character => character.charCodeAt(0));
        if (base64url(bytes) !== input) throw error('DataError', 'Non-canonical JWK key encoding');
        return bytes;
    };
    const jwkAlgorithm = (name, hash, length) => name === 'HMAC'
        ? ({ 'SHA-1': 'HS1', 'SHA-256': 'HS256', 'SHA-384': 'HS384', 'SHA-512': 'HS512' })[hash]
        : `A${length}${name.slice(4)}`;
    const keyParameters = algorithm => {
        const name = nameOf(algorithm);
        if (name === 'HMAC') {
            const hash = hashOf(algorithm.hash);
            return { name, hash: { name: hash } };
        }
        if (name.startsWith('AES-') || name === 'PBKDF2' || name === 'HKDF') return { name };
        unsupported();
    };
    const createImportedKey = (format, keyData, algorithm, extractable, keyUsages) => {
        const parameters = keyParameters(algorithm);
        const { name } = parameters;
        const usages = validateUsages(name, keyUsages);
        if ((name === 'PBKDF2' || name === 'HKDF') && extractable)
            throw error('SyntaxError', 'Base keys cannot be extractable');
        let material;
        if (format === 'raw') material = copyBytes(keyData);
        else if (format === 'jwk' && name !== 'PBKDF2' && name !== 'HKDF') {
            if (!keyData || typeof keyData !== 'object' || keyData.kty !== 'oct')
                throw error('DataError', 'Expected a symmetric JWK');
            material = fromBase64url(keyData.k);
            if (keyData.ext === false && extractable) throw error('DataError', 'JWK is not extractable');
            if (keyData.key_ops && (!Array.isArray(keyData.key_ops) || usages.some(u => !keyData.key_ops.includes(u))))
                throw error('DataError', 'JWK usages do not match');
            const expected = jwkAlgorithm(name, parameters.hash?.name, material.length * 8);
            if (keyData.alg !== undefined && keyData.alg !== expected) throw error('DataError', 'JWK algorithm mismatch');
        } else unsupported();
        if (name.startsWith('AES-') && ![16, 24, 32].includes(material.length))
            throw error('DataError', 'AES keys must be 128, 192, or 256 bits');
        if (name === 'HMAC' && !material.length) throw error('DataError', 'HMAC key is empty');
        if (name === 'HMAC') parameters.length = material.length * 8;
        if (name.startsWith('AES-')) parameters.length = material.length * 8;
        return makeKey(parameters, material, extractable, usages);
    };
    const exportMaterial = (format, key) => {
        const state = requireKey(key);
        if (!state.extractable) throw error('InvalidAccessError', 'Key is not extractable');
        if (format === 'raw') return resultBuffer(state.material.slice());
        if (format === 'jwk' && !['PBKDF2', 'HKDF'].includes(state.algorithm.name)) return {
            kty: 'oct', k: base64url(state.material),
            alg: jwkAlgorithm(state.algorithm.name, state.algorithm.hash?.name, state.algorithm.length),
            key_ops: state.usages.slice(), ext: state.extractable
        };
        unsupported();
    };
    Object.defineProperties(SubtleCrypto.prototype, {
        importKey: { configurable: true, writable: true, value: function importKey(format, keyData, algorithm, extractable, keyUsages) {
            requireSubtle(this);
            return operation(() => createImportedKey(format, keyData, algorithm, extractable, keyUsages));
        } },
        exportKey: { configurable: true, writable: true, value: function exportKey(format, key) {
            requireSubtle(this);
            return operation(() => exportMaterial(format, key));
        } },
        generateKey: { configurable: true, writable: true, value: function generateKey(algorithm, extractable, keyUsages) {
            requireSubtle(this);
            return operation(() => {
                const parameters = keyParameters(algorithm), { name } = parameters;
                if (name === 'PBKDF2' || name === 'HKDF') unsupported();
                const usages = validateUsages(name, keyUsages);
                if (!usages.length) throw error('SyntaxError', 'Secret keys require usages');
                let length;
                if (name.startsWith('AES-')) {
                    length = Number(algorithm.length);
                    if (![128, 192, 256].includes(length)) throw error('OperationError', 'Invalid AES key length');
                } else {
                    const hash = parameters.hash.name;
                    length = algorithm.length === undefined ? (hash === 'SHA-384' || hash === 'SHA-512' ? 1024 : 512) : Number(algorithm.length);
                    if (!Number.isInteger(length) || length < 1 || length > 65536)
                        throw error('OperationError', 'Invalid HMAC key length');
                }
                const material = host('cryptoRandomBytes', Math.ceil(length / 8));
                if (length % 8) material[material.length - 1] &= 0xff << (8 - length % 8);
                parameters.length = length;
                return makeKey(parameters, material, extractable, usages);
            });
        } }
    });
