// Window and Worker interoperability and key-policy checks for real NIST-curve CNG keys.
function runEllipticCurveFixture() {
    const subtle = crypto.subtle;
    const assert = (condition, message) => { if (!condition) throw Error(message); };
    const bytes = text => new TextEncoder().encode(text);
    const hex = value => [...new Uint8Array(value)].map(byte => byte.toString(16).padStart(2, '0')).join('');
    const rejects = async (promise, expected) => {
        let actual = 'resolved';
        try { await promise; } catch (error) { actual = error.name; }
        assert(actual === expected, `${actual} rather than ${expected}`);
    };
    return (async () => {
        for (const curve of ['P-256', 'P-384', 'P-521']) {
            const bits = curve === 'P-256' ? 256 : curve === 'P-384' ? 384 : 528;
            const alice = await subtle.generateKey({name: 'ECDH', namedCurve: curve}, true,
                ['deriveBits', 'deriveKey']);
            const bob = await subtle.generateKey({name: 'ECDH', namedCurve: curve}, true,
                ['deriveBits']);
            assert(alice.publicKey.type === 'public' && alice.privateKey.type === 'private',
                `${curve} key types`);
            assert(alice.publicKey.algorithm.namedCurve === curve &&
                alice.privateKey.usages.join(',') === 'deriveBits,deriveKey', `${curve} metadata`);
            const first = await subtle.deriveBits({name: 'ECDH', public: bob.publicKey},
                alice.privateKey, bits);
            const second = await subtle.deriveBits({name: 'ECDH', public: alice.publicKey},
                bob.privateKey, bits);
            assert(hex(first) === hex(second) && new Uint8Array(first).some(Boolean),
                `${curve} agreement`);
            const publicRaw = await subtle.exportKey('raw', alice.publicKey);
            assert(new Uint8Array(publicRaw).length === 1 + bits / 4 &&
                new Uint8Array(publicRaw)[0] === 4, `${curve} raw point`);
            const importedPublic = await subtle.importKey('raw', publicRaw,
                {name: 'ECDH', namedCurve: curve}, true, []);
            const privateJwk = await subtle.exportKey('jwk', alice.privateKey);
            const importedPrivate = await subtle.importKey('jwk', privateJwk,
                {name: 'ECDH', namedCurve: curve}, true, ['deriveBits']);
            assert(hex(await subtle.deriveBits({name: 'ECDH', public: importedPublic},
                bob.privateKey, bits)) === hex(first), `${curve} imported public`);
            assert(hex(await subtle.deriveBits({name: 'ECDH', public: bob.publicKey},
                importedPrivate, bits)) === hex(first), `${curve} imported private`);
            await rejects(subtle.deriveBits({name: 'ECDH', public: alice.privateKey},
                bob.privateKey, bits), 'InvalidAccessError');
            await rejects(subtle.importKey('raw', new Uint8Array(1 + bits / 4),
                {name: 'ECDH', namedCurve: curve}, true, []), 'DataError');
            await rejects(subtle.importKey('jwk', {...privateJwk, crv: 'P-999'},
                {name: 'ECDH', namedCurve: curve}, true, ['deriveBits']), 'DataError');
            const nonExtractable = await subtle.generateKey({name: 'ECDH', namedCurve: curve},
                false, ['deriveBits']);
            await rejects(subtle.exportKey('jwk', nonExtractable.privateKey), 'InvalidAccessError');
            assert((await subtle.exportKey('jwk', nonExtractable.publicKey)).crv === curve,
                `${curve} public extractability`);
        }
        const alice = await subtle.generateKey({name: 'ECDH', namedCurve: 'P-256'}, true,
            ['deriveKey']);
        const bob = await subtle.generateKey({name: 'ECDH', namedCurve: 'P-256'}, true,
            ['deriveKey']);
        const aKey = await subtle.deriveKey({name: 'ECDH', public: bob.publicKey},
            alice.privateKey, {name: 'AES-GCM', length: 128}, false, ['encrypt']);
        const bKey = await subtle.deriveKey({name: 'ECDH', public: alice.publicKey},
            bob.privateKey, {name: 'AES-GCM', length: 128}, false, ['decrypt']);
        const iv = new Uint8Array(12), message = bytes('shared secret');
        const ciphertext = await subtle.encrypt({name: 'AES-GCM', iv}, aKey, message);
        assert(hex(await subtle.decrypt({name: 'AES-GCM', iv}, bKey, ciphertext)) ===
            hex(message), 'derived AES key');

        for (const [curve, hash] of [['P-256', 'SHA-256'], ['P-384', 'SHA-384'],
            ['P-521', 'SHA-512']]) {
            const pair = await subtle.generateKey({name: 'ECDSA', namedCurve: curve}, true,
                ['sign', 'verify']);
            const algorithm = {name: 'ECDSA', hash};
            const signature = await subtle.sign(algorithm, pair.privateKey, message);
            assert(await subtle.verify(algorithm, pair.publicKey, signature, message),
                `${curve} signature`);
            assert(!await subtle.verify(algorithm, pair.publicKey, signature,
                bytes('modified')), `${curve} altered message`);
            const invalid = new Uint8Array(signature); invalid[0] ^= 1;
            assert(!await subtle.verify(algorithm, pair.publicKey, invalid, message),
                `${curve} altered signature`);
            const publicJwk = await subtle.exportKey('jwk', pair.publicKey);
            const privateJwk = await subtle.exportKey('jwk', pair.privateKey);
            assert(publicJwk.kty === 'EC' && !('d' in publicJwk) &&
                privateJwk.d, `${curve} JWK shape`);
            const importedPublic = await subtle.importKey('jwk', publicJwk,
                {name: 'ECDSA', namedCurve: curve}, true, ['verify']);
            const importedPrivate = await subtle.importKey('jwk', privateJwk,
                {name: 'ECDSA', namedCurve: curve}, true, ['sign']);
            assert(await subtle.verify(algorithm, importedPublic,
                await subtle.sign(algorithm, importedPrivate, message), message),
                `${curve} JWK roundtrip`);
            await rejects(subtle.sign(algorithm, pair.publicKey, message), 'InvalidAccessError');
            await rejects(subtle.verify(algorithm, pair.privateKey, signature, message),
                'InvalidAccessError');
        }
        console.log('elliptic curves passed');
    })().catch(error => { throw error; });
}
