    // Web Crypto elliptic-curve keys are serialized as JWK/raw points at the API boundary.
    // Provider-specific ECC blobs never become an author-visible key format.
    // https://www.w3.org/TR/webcrypto/#ecdh
    const ecCurves = { 'P-256': {bytes: 32, ecdh: [827016005, 843793221],
            ecdsa: [827540293, 844317509]},
        'P-384': {bytes: 48, ecdh: [860570437, 877347653],
            ecdsa: [861094725, 877871941]},
        'P-521': {bytes: 66, ecdh: [894124869, 910902085],
            ecdsa: [894649157, 911426373]} };
    const ecName = algorithm => {
        const name = typeof algorithm === 'string' ? algorithm : algorithm?.name;
        return typeof name === 'string' && /^(ECDH|ECDSA)$/i.test(name) ? name.toUpperCase() : null;
    };
    const ecParameters = algorithm => {
        const name = ecName(algorithm);
        const namedCurve = algorithm?.namedCurve;
        if (!name || !Object.hasOwn(ecCurves, namedCurve)) unsupported();
        return {name, namedCurve};
    };
    const ecBlob = (parameters, x, y, d) => {
        const curve = ecCurves[parameters.namedCurve], size = curve.bytes;
        if (x.length !== size || y.length !== size || (d && d.length !== size))
            throw error('DataError', 'Wrong EC coordinate length');
        const material = new Bytes(8 + (d ? 3 : 2) * size);
        const magic = curve[parameters.name.toLowerCase()][d ? 1 : 0];
        const header = new DataView(material.buffer);
        header.setUint32(0, magic, true); header.setUint32(4, size, true);
        material.set(x, 8); material.set(y, 8 + size);
        if (d) material.set(d, 8 + 2 * size);
        try { host('cryptoSubtleEcValidate', parameters.name, parameters.namedCurve, material); }
        catch (_) { throw error('DataError', 'Invalid EC point or private key'); }
        return material;
    };
    const ecKey = (parameters, material, type, extractable, usages) => {
        const key = makeKey(parameters, material, extractable, usages);
        keys.get(key).type = type;
        return key;
    };
    const ecAllowedUsages = (name, type) => name === 'ECDH'
        ? (type === 'private' ? ['deriveBits', 'deriveKey'] : [])
        : (type === 'private' ? ['sign'] : ['verify']);
    const ecUsages = (name, type, usages) => {
        const legal = ecAllowedUsages(name, type);
        if (!Array.isArray(usages) || usages.some(usage => !legal.includes(usage)) ||
            new Set(usages).size !== usages.length)
            throw error('SyntaxError', 'Invalid EC key usages');
        if (type === 'private' && !usages.length)
            throw error('SyntaxError', 'Private EC keys require a usage');
        return usages.slice();
    };
    const ecImport = (format, keyData, algorithm, extractable, usages) => {
        const parameters = ecParameters(algorithm), size = ecCurves[parameters.namedCurve].bytes;
        let x, y, d, type;
        if (format === 'raw') {
            const raw = copyBytes(keyData);
            if (raw.length !== 1 + 2 * size || raw[0] !== 4)
                throw error('DataError', 'Expected an uncompressed EC point');
            x = raw.slice(1, 1 + size); y = raw.slice(1 + size); type = 'public';
        } else if (format === 'jwk') {
            if (!keyData || typeof keyData !== 'object' || keyData.kty !== 'EC' ||
                keyData.crv !== parameters.namedCurve)
                throw error('DataError', 'EC JWK curve mismatch');
            x = fromBase64url(keyData.x); y = fromBase64url(keyData.y);
            d = keyData.d === undefined ? null : fromBase64url(keyData.d);
            type = d ? 'private' : 'public';
            if (keyData.ext === false && extractable)
                throw error('DataError', 'JWK is not extractable');
            if (keyData.use !== undefined && keyData.use !==
                (parameters.name === 'ECDSA' ? 'sig' : 'enc'))
                throw error('DataError', 'JWK use mismatch');
            if (keyData.key_ops !== undefined && (!Array.isArray(keyData.key_ops) ||
                usages.some(usage => !keyData.key_ops.includes(usage))))
                throw error('DataError', 'JWK usages do not match');
        } else unsupported();
        return ecKey(parameters, ecBlob(parameters, x, y, d), type, extractable,
            ecUsages(parameters.name, type, usages));
    };
    const ecExport = (format, key) => {
        const state = requireKey(key);
        if (!state.extractable) throw error('InvalidAccessError', 'Key is not extractable');
        const size = ecCurves[state.algorithm.namedCurve].bytes;
        const x = state.material.slice(8, 8 + size), y = state.material.slice(8 + size, 8 + 2 * size);
        if (format === 'raw' && state.type === 'public') {
            const raw = new Bytes(1 + 2 * size);
            raw[0] = 4; raw.set(x, 1); raw.set(y, 1 + size);
            return resultBuffer(raw);
        }
        if (format !== 'jwk') unsupported();
        const jwk = {kty: 'EC', crv: state.algorithm.namedCurve,
            x: base64url(x), y: base64url(y), key_ops: state.usages.slice(), ext: true};
        if (state.type === 'private') jwk.d = base64url(state.material.slice(8 + 2 * size));
        return jwk;
    };
    const ecGenerate = (algorithm, extractable, keyUsages) => {
        const parameters = ecParameters(algorithm);
        const usages = validateUsages(parameters.name, keyUsages);
        if (!usages.length) throw error('SyntaxError', 'EC key pair requires usages');
        const [publicBlob, privateBlob] = host('cryptoSubtleEcGenerate',
            parameters.name, parameters.namedCurve);
        return {
            publicKey: ecKey(parameters, publicBlob, 'public', true,
                usages.filter(usage => ecAllowedUsages(parameters.name, 'public').includes(usage))),
            privateKey: ecKey(parameters, privateBlob, 'private', extractable,
                usages.filter(usage => ecAllowedUsages(parameters.name, 'private').includes(usage)))
        };
    };
    const ecDerive = (algorithm, baseKey, length, usage) => {
        const state = requireKey(baseKey, usage, 'ECDH');
        const peer = requireKey(algorithm.public, null, 'ECDH');
        if (state.type !== 'private' || peer.type !== 'public' ||
            state.algorithm.namedCurve !== peer.algorithm.namedCurve)
            throw error('InvalidAccessError', 'ECDH keys must be a matching private/public pair');
        const fullBits = ecCurves[state.algorithm.namedCurve].bytes * 8;
        if (length === null || length === undefined) length = fullBits;
        length = Number(length);
        if (!Number.isInteger(length) || length < 1 || length > fullBits)
            throw error('OperationError', 'Invalid ECDH bit length');
        const bytes = host('cryptoSubtleEcDerive', 'ECDH', state.algorithm.namedCurve,
            state.material, peer.material).slice(0, Math.ceil(length / 8));
        if (length % 8) bytes[bytes.length - 1] &= 0xff << (8 - length % 8);
        return bytes;
    };
    const baseImport = SubtleCrypto.prototype.importKey;
    const baseExport = SubtleCrypto.prototype.exportKey;
    const baseGenerate = SubtleCrypto.prototype.generateKey;
    const baseDeriveBits = SubtleCrypto.prototype.deriveBits;
    const baseDeriveKey = SubtleCrypto.prototype.deriveKey;
    const baseSign = SubtleCrypto.prototype.sign;
    const baseVerify = SubtleCrypto.prototype.verify;
    Object.assign(SubtleCrypto.prototype, {
        importKey(format, keyData, algorithm, extractable, usages) {
            requireSubtle(this);
            return ecName(algorithm) ? operation(() => ecImport(format, keyData, algorithm,
                extractable, usages)) : baseImport.call(this, format, keyData, algorithm, extractable, usages);
        },
        exportKey(format, key) {
            requireSubtle(this);
            return keys.get(key)?.algorithm.namedCurve ? operation(() => ecExport(format, key)) :
                baseExport.call(this, format, key);
        },
        generateKey(algorithm, extractable, usages) {
            requireSubtle(this);
            return ecName(algorithm) ? operation(() => ecGenerate(algorithm, extractable, usages)) :
                baseGenerate.call(this, algorithm, extractable, usages);
        },
        deriveBits(algorithm, baseKey, length) {
            requireSubtle(this);
            return ecName(algorithm) === 'ECDH' ? operation(() =>
                resultBuffer(ecDerive(algorithm, baseKey, length, 'deriveBits'))) :
                baseDeriveBits.call(this, algorithm, baseKey, length);
        },
        deriveKey(algorithm, baseKey, derivedKeyType, extractable, usages) {
            requireSubtle(this);
            return ecName(algorithm) === 'ECDH' ? operation(() => {
                const parameters = keyParameters(derivedKeyType), length = Number(derivedKeyType.length);
                if (!parameters.name.startsWith('AES-') || ![128, 192, 256].includes(length))
                    unsupported();
                return createImportedKey('raw', ecDerive(algorithm, baseKey, length, 'deriveKey'),
                    derivedKeyType, extractable, usages);
            }) : baseDeriveKey.call(this, algorithm, baseKey, derivedKeyType, extractable, usages);
        },
        sign(algorithm, key, data) {
            requireSubtle(this);
            return ecName(algorithm) === 'ECDSA' ? operation(() => {
                const state = requireKey(key, 'sign', 'ECDSA');
                if (state.type !== 'private') throw error('InvalidAccessError', 'ECDSA signing needs private key');
                return resultBuffer(host('cryptoSubtleEcSign', 'ECDSA', state.algorithm.namedCurve,
                    hashOf(algorithm.hash), state.material, copyBytes(data)));
            }) : baseSign.call(this, algorithm, key, data);
        },
        verify(algorithm, key, signature, data) {
            requireSubtle(this);
            return ecName(algorithm) === 'ECDSA' ? operation(() => {
                const state = requireKey(key, 'verify', 'ECDSA');
                if (state.type !== 'public') throw error('InvalidAccessError', 'ECDSA verify needs public key');
                return host('cryptoSubtleEcVerify', 'ECDSA', state.algorithm.namedCurve,
                    hashOf(algorithm.hash), state.material, copyBytes(signature), copyBytes(data));
            }) : baseVerify.call(this, algorithm, key, signature, data);
        }
    });
