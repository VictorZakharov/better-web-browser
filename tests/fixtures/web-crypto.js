// Shared Window/Worker Web Crypto contract; vectors from FIPS 180, RFC 4231, RFC 6070 and NIST GCM.
function runWebCryptoFixture() {
    const assert = (condition, message) => { if (!condition) throw Error(message); };
    const hex = value => [...new Uint8Array(value)].map(byte => byte.toString(16).padStart(2, '0')).join('');
    const bytes = value => new TextEncoder().encode(value);
    const rejects = async (promise, expected) => {
        let actual = 'resolved';
        try { await promise; } catch (error) { actual = error.name; }
        assert(actual === expected, `${actual} rather than ${expected}`);
    };
    return (async () => {
        assert(crypto.subtle instanceof SubtleCrypto, 'interface');
        assert(Object.prototype.toString.call(crypto.subtle) === '[object SubtleCrypto]', 'tag');
        assert(hex(await crypto.subtle.digest('SHA-256', bytes('abc'))) ===
            'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad', 'digest');
        const hmac = await crypto.subtle.importKey('raw', bytes('key'),
            { name: 'HMAC', hash: 'SHA-256' }, false, ['sign', 'verify']);
        const signature = await crypto.subtle.sign('HMAC', hmac, bytes('The quick brown fox jumps over the lazy dog'));
        assert(hex(signature) === 'f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8', 'HMAC');
        assert(await crypto.subtle.verify('HMAC', hmac, signature, bytes('The quick brown fox jumps over the lazy dog')), 'verify');
        assert(!await crypto.subtle.verify('HMAC', hmac, signature, bytes('changed')), 'tampered signature');
        await rejects(crypto.subtle.exportKey('raw', hmac), 'InvalidAccessError');
        await rejects(crypto.subtle.encrypt({ name: 'AES-GCM', iv: new Uint8Array(12) }, hmac, bytes('x')), 'InvalidAccessError');
        const password = await crypto.subtle.importKey('raw', bytes('password'), 'PBKDF2', false, ['deriveBits', 'deriveKey']);
        const derived = await crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-1', salt: bytes('salt'), iterations: 1 }, password, 160);
        assert(hex(derived) === '0c60c80f961f0e71f3a9b524af6012062fe037a6', 'PBKDF2');
        await rejects(crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256',
            salt: bytes('salt'), iterations: 1000001 }, password, 128), 'OperationError');
        const ikm = await crypto.subtle.importKey('raw', new Uint8Array(22).fill(0x0b), 'HKDF', false, ['deriveBits']);
        const hkdf = await crypto.subtle.deriveBits({ name: 'HKDF', hash: 'SHA-256',
            salt: Uint8Array.from({ length: 13 }, (_, i) => i),
            info: Uint8Array.from({ length: 10 }, (_, i) => 0xf0 + i) }, ikm, 336);
        assert(hex(hkdf) === '3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865', 'HKDF');
        const key = await crypto.subtle.importKey('raw', new Uint8Array(16), 'AES-GCM', true,
            ['encrypt', 'decrypt', 'wrapKey', 'unwrapKey']);
        const params = { name: 'AES-GCM', iv: new Uint8Array(12) };
        const ciphertext = await crypto.subtle.encrypt(params, key, new Uint8Array(16));
        assert(hex(ciphertext) === '0388dace60b6a392f328c2b971b2fe78ab6e47d42cec13bdf53a67b21257bddf', 'AES-GCM');
        assert(hex(await crypto.subtle.decrypt(params, key, ciphertext)) === '00000000000000000000000000000000', 'decrypt');
        const cbcKey = await crypto.subtle.importKey('raw', Uint8Array.from([0x2b,0x7e,0x15,0x16,0x28,0xae,0xd2,0xa6,0xab,0xf7,0x15,0x88,0x09,0xcf,0x4f,0x3c]), 'AES-CBC', true, ['encrypt','decrypt']);
        const cbcIv = Uint8Array.from({ length: 16 }, (_, i) => i);
        const block = Uint8Array.from([0x6b,0xc1,0xbe,0xe2,0x2e,0x40,0x9f,0x96,0xe9,0x3d,0x7e,0x11,0x73,0x93,0x17,0x2a]);
        const cbc = await crypto.subtle.encrypt({ name: 'AES-CBC', iv: cbcIv }, cbcKey, block);
        assert(hex(cbc).startsWith('7649abac8119b246cee98e9b12e9197d'), 'AES-CBC');
        assert(hex(await crypto.subtle.decrypt({ name: 'AES-CBC', iv: cbcIv }, cbcKey, cbc)) === hex(block), 'CBC decrypt');
        const ctrKey = await crypto.subtle.importKey('raw', await crypto.subtle.exportKey('raw', cbcKey), 'AES-CTR', true, ['encrypt', 'decrypt']);
        const counter = Uint8Array.from([0xf0,0xf1,0xf2,0xf3,0xf4,0xf5,0xf6,0xf7,0xf8,0xf9,0xfa,0xfb,0xfc,0xfd,0xfe,0xff]);
        const ctr = await crypto.subtle.encrypt({ name: 'AES-CTR', counter, length: 128 }, ctrKey, block);
        assert(hex(ctr) === '874d6191b620e3261bef6864990db6ce', 'AES-CTR');
        assert(hex(await crypto.subtle.decrypt({ name: 'AES-CTR', counter, length: 128 }, ctrKey, ctr)) === hex(block), 'CTR decrypt');
        const corrupt = new Uint8Array(ciphertext); corrupt[31] ^= 1;
        await rejects(crypto.subtle.decrypt(params, key, corrupt), 'OperationError');
        const jwk = await crypto.subtle.exportKey('jwk', key);
        assert(jwk.kty === 'oct' && jwk.alg === 'A128GCM' && jwk.ext, 'JWK export');
        const restored = await crypto.subtle.importKey('jwk', jwk, 'AES-GCM', true, ['encrypt']);
        assert(hex(await crypto.subtle.exportKey('raw', restored)) === '00000000000000000000000000000000', 'JWK import');
        const generated = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, true, ['encrypt']);
        assert(generated.algorithm.length === 256 && generated.type === 'secret', 'generateKey');
        const kek = await crypto.subtle.importKey('raw', Uint8Array.from({length: 16}, (_, index) => index),
            'AES-KW', true, ['wrapKey', 'unwrapKey']);
        const keyToWrap = await crypto.subtle.importKey('raw', Uint8Array.from({length: 16}, (_, index) =>
            index < 8 ? index * 0x11 : (index - 8) * 0x11 + 0x88), 'AES-GCM', true, ['encrypt']);
        const wrapped = await crypto.subtle.wrapKey('raw', keyToWrap, kek, 'AES-KW');
        assert(hex(wrapped) === '1fa68b0a8112b447aef34bd8fb5a7b829d3e862371d2cfe5', 'AES-KW vector');
        const unwrapped = await crypto.subtle.unwrapKey('raw', wrapped, kek, 'AES-KW',
            'AES-GCM', true, ['encrypt']);
        assert(hex(await crypto.subtle.exportKey('raw', unwrapped)) ===
            '00112233445566778899aabbccddeeff', 'AES-KW unwrap');
        const damaged = new Uint8Array(wrapped); damaged[0] ^= 1;
        await rejects(crypto.subtle.unwrapKey('raw', damaged, kek, 'AES-KW',
            'AES-GCM', true, ['encrypt']), 'OperationError');
        await rejects(crypto.subtle.encrypt('AES-KW', kek, new Uint8Array(16)), 'InvalidAccessError');
        await rejects(crypto.subtle.importKey('raw', new Uint8Array(3), 'AES-GCM', true, ['encrypt']), 'DataError');
        console.log('web crypto passed');
    })().catch(error => { throw error; });
}
