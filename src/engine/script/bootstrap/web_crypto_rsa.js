    // RSA keys use browser-owned CNG blobs. JWK is the author-visible import/export
    // format; private components and usage policy never travel through page DOM state.
    // https://www.w3.org/TR/webcrypto/#rsa-oaep
    const rsaNames = ['RSA-OAEP', 'RSA-PSS', 'RSASSA-PKCS1-v1_5'];
    const rsaName = algorithm => {
        const value = typeof algorithm === 'string' ? algorithm : algorithm?.name;
        return rsaNames.find(name => name.toUpperCase() === value?.toUpperCase()) || null;
    };
    const rsaUsages = (name, type) => name === 'RSA-OAEP'
        ? (type === 'public' ? ['encrypt', 'wrapKey'] : ['decrypt', 'unwrapKey'])
        : (type === 'public' ? ['verify'] : ['sign']);
    const rsaAlgorithm = algorithm => {
        const name = rsaName(algorithm);
        if (!name) unsupported();
        return { name, hash: {name: hashOf(algorithm.hash)} };
    };
    const rsaJwkAlgorithm = parameters => {
        const hash = parameters.hash.name;
        if (parameters.name === 'RSA-OAEP') return hash === 'SHA-1' ? 'RSA-OAEP' :
            `RSA-OAEP-${hash.slice(4)}`;
        if (parameters.name === 'RSA-PSS') return `PS${hash.slice(4)}`;
        return `RS${hash.slice(4)}`;
    };
    const rsaComponent = (value, size) => {
        const bytes = fromBase64url(value);
        if (!bytes.length || bytes.length > size || (bytes.length > 1 && bytes[0] === 0))
            throw error('DataError', 'Invalid RSA JWK integer');
        const padded = new Bytes(size);
        padded.set(bytes, size - bytes.length);
        return padded;
    };
    const rsaTrim = bytes => {
        let offset = 0;
        while (offset < bytes.length - 1 && bytes[offset] === 0) offset++;
        return base64url(bytes.subarray(offset));
    };
    const rsaBlob = (jwk, privateKey) => {
        const n = fromBase64url(jwk.n), e = fromBase64url(jwk.e);
        if (n.length < 128 || n.length > 512 || !n[0] || !e.length || e.length > 8 ||
            (e.length > 1 && e[0] === 0) || !(e[e.length - 1] & 1))
            throw error('DataError', 'Invalid RSA public components');
        const bits = (n.length - 1) * 8 + (32 - Math.clz32(n[0]));
        const p = privateKey ? fromBase64url(jwk.p) : null;
        const q = privateKey ? fromBase64url(jwk.q) : null;
        if (privateKey && (!p.length || !q.length || p.length > n.length || q.length > n.length))
            throw error('DataError', 'Invalid RSA private components');
        const parts = privateKey ? [e, n, p, q, rsaComponent(jwk.dp, p.length),
            rsaComponent(jwk.dq, q.length), rsaComponent(jwk.qi, p.length),
            rsaComponent(jwk.d, n.length)] : [e, n];
        const blob = new Bytes(24 + parts.reduce((sum, part) => sum + part.length, 0));
        const header = new DataView(blob.buffer);
        header.setUint32(0, privateKey ? 0x33415352 : 0x31415352, true);
        header.setUint32(4, bits, true);
        header.setUint32(8, e.length, true);
        header.setUint32(12, n.length, true);
        header.setUint32(16, p?.length || 0, true);
        header.setUint32(20, q?.length || 0, true);
        let offset = 24;
        for (const part of parts) { blob.set(part, offset); offset += part.length; }
        try { host('cryptoSubtleRsaValidate', privateKey, blob); }
        catch (_) { throw error('DataError', 'Invalid RSA key'); }
        return blob;
    };
    const rsaParts = blob => {
        const header = new DataView(blob.buffer, blob.byteOffset, blob.byteLength);
        const privateKey = header.getUint32(0, true) === 0x33415352;
        const eSize = header.getUint32(8, true), nSize = header.getUint32(12, true);
        const pSize = header.getUint32(16, true), qSize = header.getUint32(20, true);
        const names = privateKey ? ['e', 'n', 'p', 'q', 'dp', 'dq', 'qi', 'd'] : ['e', 'n'];
        const lengths = privateKey ? [eSize, nSize, pSize, qSize, pSize, qSize, pSize, nSize] :
            [eSize, nSize];
        const parts = {};
        let offset = 24;
        names.forEach((name, index) => {
            parts[name] = blob.slice(offset, offset + lengths[index]);
            offset += lengths[index];
        });
        return parts;
    };
    const rsaKey = (parameters, blob, type, extractable, usages) => {
        const key = makeKey(parameters, blob, extractable, usages);
        keys.get(key).type = type;
        return key;
    };
    const rsaImport = (format, data, algorithm, extractable, usages) => {
        const parameters = rsaAlgorithm(algorithm);
        if (format === 'spki' || format === 'pkcs8')
            data = {kty: 'RSA', ...rsaDerImport(format, data)};
        else if (format !== 'jwk' || !data || typeof data !== 'object' || data.kty !== 'RSA')
            unsupported();
        const type = data.d === undefined ? 'public' : 'private';
        if (data.ext === false && extractable) throw error('DataError', 'JWK is not extractable');
        if (data.alg !== undefined && data.alg !== rsaJwkAlgorithm(parameters))
            throw error('DataError', 'RSA JWK algorithm mismatch');
        if (data.use !== undefined && data.use !== (parameters.name === 'RSA-OAEP' ? 'enc' : 'sig'))
            throw error('DataError', 'RSA JWK use mismatch');
        if (data.key_ops !== undefined && (!Array.isArray(data.key_ops) ||
            usages.some(usage => !data.key_ops.includes(usage))))
            throw error('DataError', 'RSA JWK usages do not match');
        const legal = rsaUsages(parameters.name, type);
        if (!Array.isArray(usages) || usages.some(usage => !legal.includes(usage)) ||
            new Set(usages).size !== usages.length || (type === 'private' && !usages.length))
            throw error('SyntaxError', 'Invalid RSA key usages');
        const material = rsaBlob(data, type === 'private');
        const parts = rsaParts(material);
        parameters.modulusLength = new DataView(material.buffer).getUint32(4, true);
        parameters.publicExponent = parts.e.slice();
        return rsaKey(parameters, material, type, extractable, usages);
    };
    const rsaExport = (format, key) => {
        const state = requireKey(key);
        if (!state.extractable) throw error('InvalidAccessError', 'Key is not extractable');
        const parts = rsaParts(state.material);
        if (format === 'spki' || format === 'pkcs8') {
            if (format === 'spki' && state.type !== 'public' ||
                format === 'pkcs8' && state.type !== 'private') unsupported();
            return rsaDerExport(format, parts);
        }
        if (format !== 'jwk') unsupported();
        const jwk = {kty: 'RSA', alg: rsaJwkAlgorithm(state.algorithm),
            key_ops: state.usages.slice(), ext: true};
        for (const [name, part] of Object.entries(parts)) jwk[name] = rsaTrim(part);
        return jwk;
    };
    const rsaGenerate = (algorithm, extractable, usages) => {
        const parameters = rsaAlgorithm(algorithm);
        const bits = Number(algorithm.modulusLength);
        const exponent = copyBytes(algorithm.publicExponent);
        if (!Number.isInteger(bits) || bits < 1024 || bits > 4096 || bits % 8 ||
            exponent.length !== 3 || exponent[0] !== 1 || exponent[1] !== 0 || exponent[2] !== 1)
            throw error('OperationError', 'RSA generation requires 1024–4096 bits and exponent 65537');
        const legal = [...rsaUsages(parameters.name, 'public'),
            ...rsaUsages(parameters.name, 'private')];
        if (!Array.isArray(usages) || !usages.length ||
            usages.some(usage => !legal.includes(usage)) || new Set(usages).size !== usages.length)
            throw error('SyntaxError', 'Invalid RSA pair usages');
        const [publicBlob, privateBlob] = host('cryptoSubtleRsaGenerate', bits);
        parameters.modulusLength = bits;
        parameters.publicExponent = exponent.slice();
        return {publicKey: rsaKey(parameters, publicBlob, 'public', true,
                usages.filter(usage => rsaUsages(parameters.name, 'public').includes(usage))),
            privateKey: rsaKey(parameters, privateBlob, 'private', extractable,
                usages.filter(usage => rsaUsages(parameters.name, 'private').includes(usage)))};
    };
    const rsaOaep = (decrypt, algorithm, key, data, usage) => {
        const state = requireKey(key, usage, 'RSA-OAEP');
        if (state.type !== (decrypt ? 'private' : 'public'))
            throw error('InvalidAccessError', 'RSA-OAEP key type mismatch');
        const label = algorithm?.label === undefined ? new Bytes(0) : copyBytes(algorithm.label);
        try { return resultBuffer(host('cryptoSubtleRsaOaep', decrypt,
            state.algorithm.hash.name, state.material, label, copyBytes(data))); }
        catch (_) { throw error('OperationError', 'RSA-OAEP operation failed'); }
    };
    const rsaSignature = (verify, algorithm, key, data, signature) => {
        const name = rsaName(algorithm), state = requireKey(key,
            verify ? 'verify' : 'sign', name);
        if (state.type !== (verify ? 'public' : 'private'))
            throw error('InvalidAccessError', 'RSA signature key type mismatch');
        const pss = name === 'RSA-PSS';
        const salt = pss ? Number(algorithm.saltLength) : 0;
        if (pss && (!Number.isInteger(salt) || salt < 0 || salt > 0xffffffff))
            throw error('OperationError', 'Invalid RSA-PSS salt length');
        try {
            const result = host('cryptoSubtleRsaSignature', verify, pss,
                state.algorithm.hash.name, salt, state.material, copyBytes(data),
                verify ? copyBytes(signature) : new Bytes(0));
            return verify ? result : resultBuffer(result);
        } catch (_) { throw error('OperationError', 'RSA signature operation failed'); }
    };
    const previousRsaMethods = Object.fromEntries(['importKey', 'exportKey', 'generateKey',
        'encrypt', 'decrypt', 'sign', 'verify', 'wrapKey', 'unwrapKey']
        .map(name => [name, SubtleCrypto.prototype[name]]));
    Object.assign(SubtleCrypto.prototype, {
        importKey(format, data, algorithm, extractable, usages) {
            requireSubtle(this);
            return rsaName(algorithm) ? operation(() => rsaImport(format, data, algorithm,
                extractable, usages)) : previousRsaMethods.importKey.call(this,
                format, data, algorithm, extractable, usages);
        },
        exportKey(format, key) {
            requireSubtle(this);
            return keys.get(key)?.algorithm.name?.startsWith('RSA-') ||
                keys.get(key)?.algorithm.name === 'RSASSA-PKCS1-v1_5'
                ? operation(() => rsaExport(format, key)) :
                previousRsaMethods.exportKey.call(this, format, key);
        },
        generateKey(algorithm, extractable, usages) {
            requireSubtle(this);
            return rsaName(algorithm) ? operation(() => rsaGenerate(algorithm, extractable, usages)) :
                previousRsaMethods.generateKey.call(this, algorithm, extractable, usages);
        },
        encrypt(algorithm, key, data) {
            requireSubtle(this);
            return rsaName(algorithm) === 'RSA-OAEP' ? operation(() =>
                rsaOaep(false, algorithm, key, data, 'encrypt')) :
                previousRsaMethods.encrypt.call(this, algorithm, key, data);
        },
        decrypt(algorithm, key, data) {
            requireSubtle(this);
            return rsaName(algorithm) === 'RSA-OAEP' ? operation(() =>
                rsaOaep(true, algorithm, key, data, 'decrypt')) :
                previousRsaMethods.decrypt.call(this, algorithm, key, data);
        },
        sign(algorithm, key, data) {
            requireSubtle(this);
            return rsaName(algorithm) === 'RSA-PSS' || rsaName(algorithm) === 'RSASSA-PKCS1-v1_5'
                ? operation(() => rsaSignature(false, algorithm, key, data)) :
                previousRsaMethods.sign.call(this, algorithm, key, data);
        },
        verify(algorithm, key, signature, data) {
            requireSubtle(this);
            return rsaName(algorithm) === 'RSA-PSS' || rsaName(algorithm) === 'RSASSA-PKCS1-v1_5'
                ? operation(() => rsaSignature(true, algorithm, key, data, signature)) :
                previousRsaMethods.verify.call(this, algorithm, key, signature, data);
        },
        wrapKey(format, key, wrappingKey, algorithm) {
            requireSubtle(this);
            return rsaName(algorithm) === 'RSA-OAEP' ? operation(() => {
                if (format !== 'raw') unsupported();
                return rsaOaep(false, algorithm, wrappingKey, new Bytes(exportMaterial(format, key)),
                    'wrapKey');
            }) : previousRsaMethods.wrapKey.call(this, format, key, wrappingKey, algorithm);
        },
        unwrapKey(format, wrapped, unwrappingKey, algorithm, unwrappedAlgorithm, extractable, usages) {
            requireSubtle(this);
            return rsaName(algorithm) === 'RSA-OAEP' ? operation(() => {
                if (format !== 'raw') unsupported();
                const material = rsaOaep(true, algorithm, unwrappingKey, wrapped, 'unwrapKey');
                return createImportedKey('raw', material, unwrappedAlgorithm, extractable, usages);
            }) : previousRsaMethods.unwrapKey.call(this, format, wrapped, unwrappingKey,
                algorithm, unwrappedAlgorithm, extractable, usages);
        }
    });
    // A JWK is serialized as UTF-8 JSON before encryption by wrapKey; unwrapKey
    // performs the reverse conversion after authentication/decryption. This is
    // shared by symmetric, EC and RSA keys rather than per-site special cases.
    // https://www.w3.org/TR/webcrypto/#SubtleCrypto-method-wrapKey
    const exportedJwk = key => {
        const state = requireKey(key);
        if (state.algorithm.namedCurve) return ecExport('jwk', key);
        if (rsaName(state.algorithm)) return rsaExport('jwk', key);
        return exportMaterial('jwk', key);
    };
    const importedJwk = (jwk, algorithm, extractable, usages) => {
        if (ecName(algorithm)) return ecImport('jwk', jwk, algorithm, extractable, usages);
        if (rsaName(algorithm)) return rsaImport('jwk', jwk, algorithm, extractable, usages);
        return createImportedKey('jwk', jwk, algorithm, extractable, usages);
    };
    const wrappedJwk = (decrypt, algorithm, key, input) => {
        if (rsaName(algorithm) === 'RSA-OAEP')
            return rsaOaep(decrypt, algorithm, key, input,
                decrypt ? 'unwrapKey' : 'wrapKey');
        const name = nameOf(algorithm);
        if (!name.startsWith('AES-')) unsupported();
        return wrapBytes(decrypt, algorithm, key, input,
            decrypt ? 'unwrapKey' : 'wrapKey');
    };
    const rawWrap = SubtleCrypto.prototype.wrapKey;
    const rawUnwrap = SubtleCrypto.prototype.unwrapKey;
    Object.assign(SubtleCrypto.prototype, {
        wrapKey(format, key, wrappingKey, algorithm) {
            requireSubtle(this);
            if (format !== 'jwk') return rawWrap.call(this, format, key, wrappingKey, algorithm);
            return operation(() => {
                const serialized = new TextEncoder().encode(JSON.stringify(exportedJwk(key)));
                return wrappedJwk(false, algorithm, wrappingKey, serialized);
            });
        },
        unwrapKey(format, wrapped, unwrappingKey, algorithm, unwrappedAlgorithm,
            extractable, usages) {
            requireSubtle(this);
            if (format !== 'jwk') return rawUnwrap.call(this, format, wrapped, unwrappingKey,
                algorithm, unwrappedAlgorithm, extractable, usages);
            return operation(() => {
                const plaintext = wrappedJwk(true, algorithm, unwrappingKey, wrapped);
                let jwk;
                try { jwk = JSON.parse(new TextDecoder('utf-8', {fatal:true}).decode(plaintext)); }
                catch (_) { throw error('DataError', 'Wrapped key is not a valid JWK'); }
                return importedJwk(jwk, unwrappedAlgorithm, extractable, usages);
            });
        }
    });
})();
