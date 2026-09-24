// Shared Window/Worker RSA crypto contract. Windows CNG supplies key generation,
// OAEP encryption and signature padding; the binding owns Web Crypto key policy.
function runRsaCryptoFixture() {
    const subtle = crypto.subtle;
    const bytes = text => new TextEncoder().encode(text);
    const hex = value => [...new Uint8Array(value)]
        .map(byte => byte.toString(16).padStart(2, '0')).join('');
    const assert = (condition, message) => { if (!condition) throw Error(message); };
    const rejects = async (promise, expected) => {
        let found = 'resolved';
        try { await promise; } catch (cause) { found = cause.name; }
        assert(found === expected, `${found} rather than ${expected}`);
    };
    return (async () => {
        const exponent = Uint8Array.from([1, 0, 1]);
        const oaep = await subtle.generateKey({name: 'RSA-OAEP', modulusLength: 1024,
            publicExponent: exponent, hash: 'SHA-256'}, true,
            ['encrypt', 'decrypt', 'wrapKey', 'unwrapKey']);
        assert(oaep.publicKey.type === 'public' && oaep.privateKey.type === 'private', 'RSA pair');
        assert(oaep.publicKey.extractable && oaep.publicKey.usages.join(',') === 'encrypt,wrapKey',
            'RSA public policy');
        const displayedExponent = oaep.publicKey.algorithm.publicExponent;
        displayedExponent[0] = 0;
        assert(oaep.publicKey.algorithm.publicExponent[0] === 1,
            'RSA algorithm getter protects key parameters');
        const params = {name: 'RSA-OAEP', label: bytes('label')};
        const encrypted = await subtle.encrypt(params, oaep.publicKey, bytes('Breeze'));
        assert(hex(await subtle.decrypt(params, oaep.privateKey, encrypted)) ===
            hex(bytes('Breeze')), 'RSA-OAEP roundtrip');
        await rejects(subtle.decrypt({name: 'RSA-OAEP', label: bytes('other')},
            oaep.privateKey, encrypted), 'OperationError');
        await rejects(subtle.encrypt('RSA-OAEP', oaep.privateKey, bytes('x')),
            'InvalidAccessError');
        const publicJwk = await subtle.exportKey('jwk', oaep.publicKey);
        const privateJwk = await subtle.exportKey('jwk', oaep.privateKey);
        assert(publicJwk.alg === 'RSA-OAEP-256' && !publicJwk.d && privateJwk.d &&
            privateJwk.p && privateJwk.q && privateJwk.dp && privateJwk.dq && privateJwk.qi,
            'RSA JWK components');
        const restoredPublic = await subtle.importKey('jwk', publicJwk,
            {name: 'RSA-OAEP', hash: 'SHA-256'}, true, ['encrypt']);
        const restoredPrivate = await subtle.importKey('jwk', privateJwk,
            {name: 'RSA-OAEP', hash: 'SHA-256'}, true, ['decrypt']);
        const spki = await subtle.exportKey('spki', oaep.publicKey);
        const pkcs8 = await subtle.exportKey('pkcs8', oaep.privateKey);
        const spkiPublic = await subtle.importKey('spki', spki,
            {name: 'RSA-OAEP', hash: 'SHA-256'}, true, ['encrypt']);
        const pkcs8Private = await subtle.importKey('pkcs8', pkcs8,
            {name: 'RSA-OAEP', hash: 'SHA-256'}, true, ['decrypt']);
        const derCiphertext = await subtle.encrypt('RSA-OAEP', spkiPublic, bytes('DER'));
        assert(hex(await subtle.decrypt('RSA-OAEP', pkcs8Private, derCiphertext)) ===
            hex(bytes('DER')), 'RSA DER roundtrip');
        const trailing = new Uint8Array(spki.byteLength + 1);
        trailing.set(new Uint8Array(spki));
        await rejects(subtle.importKey('spki', trailing,
            {name:'RSA-OAEP', hash:'SHA-256'}, true, ['encrypt']), 'DataError');
        const indefinite = new Uint8Array(spki); indefinite[1] = 0x80;
        await rejects(subtle.importKey('spki', indefinite,
            {name:'RSA-OAEP', hash:'SHA-256'}, true, ['encrypt']), 'DataError');
        const second = await subtle.encrypt('RSA-OAEP', restoredPublic, bytes('restored'));
        assert(hex(await subtle.decrypt('RSA-OAEP', restoredPrivate, second)) ===
            hex(bytes('restored')), 'RSA JWK roundtrip');
        const bad = {...privateJwk}; delete bad.dp;
        await rejects(subtle.importKey('jwk', bad, {name:'RSA-OAEP', hash:'SHA-256'},
            true, ['decrypt']), 'DataError');
        const keyToWrap = await subtle.importKey('raw', new Uint8Array(16),
            'AES-GCM', true, ['encrypt']);
        const wrapped = await subtle.wrapKey('raw', keyToWrap, oaep.publicKey, params);
        const unwrapped = await subtle.unwrapKey('raw', wrapped, oaep.privateKey,
            params, 'AES-GCM', true, ['encrypt']);
        assert(hex(await subtle.exportKey('raw', unwrapped)) === '00'.repeat(16),
            'RSA-OAEP key wrap');
        const aesWrapper = await subtle.generateKey({name:'AES-GCM', length:128},
            true, ['wrapKey', 'unwrapKey']);
        const aesParameters = {name:'AES-GCM', iv:new Uint8Array(12)};
        const wrappedJwk = await subtle.wrapKey('jwk', keyToWrap, aesWrapper, aesParameters);
        const unwrappedJwk = await subtle.unwrapKey('jwk', wrappedJwk, aesWrapper,
            aesParameters, 'AES-GCM', true, ['encrypt']);
        assert(hex(await subtle.exportKey('raw', unwrappedJwk)) === '00'.repeat(16),
            'JWK wrap and unwrap');
        const damagedJwk = new Uint8Array(wrappedJwk); damagedJwk[0] ^= 1;
        await rejects(subtle.unwrapKey('jwk', damagedJwk, aesWrapper,
            aesParameters, 'AES-GCM', true, ['encrypt']), 'OperationError');
        const pss = await subtle.generateKey({name:'RSA-PSS', modulusLength:1024,
            publicExponent:exponent, hash:'SHA-256'}, true, ['sign', 'verify']);
        const pssParams = {name:'RSA-PSS', saltLength:32};
        const signature = await subtle.sign(pssParams, pss.privateKey, bytes('message'));
        assert(await subtle.verify(pssParams, pss.publicKey, signature, bytes('message')),
            'RSA-PSS verify');
        assert(!await subtle.verify(pssParams, pss.publicKey, signature, bytes('changed')),
            'RSA-PSS rejects tampering');
        await rejects(subtle.sign({name:'RSA-PSS', saltLength:-1}, pss.privateKey,
            bytes('x')), 'OperationError');
        const pkcs = await subtle.generateKey({name:'RSASSA-PKCS1-v1_5',
            modulusLength:1024, publicExponent:exponent, hash:'SHA-256'},
            true, ['sign', 'verify']);
        const deterministic = await subtle.sign('RSASSA-PKCS1-v1_5', pkcs.privateKey,
            bytes('message'));
        assert(await subtle.verify('RSASSA-PKCS1-v1_5', pkcs.publicKey,
            deterministic, bytes('message')), 'RSASSA verify');
        assert(!await subtle.verify('RSASSA-PKCS1-v1_5', pkcs.publicKey,
            deterministic, bytes('changed')), 'RSASSA rejects tampering');
        console.log('RSA crypto passed');
    })().catch(error => { throw error; });
}
