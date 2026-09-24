    // DER-only SPKI and PKCS#8 for the rsaEncryption OID. A strict parser
    // rejects BER indefinite lengths, non-minimal integers, and trailing data.
    // https://www.rfc-editor.org/rfc/rfc5280 and https://www.rfc-editor.org/rfc/rfc5958
    const rsaDerOid = Bytes.from([0x06, 0x09, 0x2a, 0x86, 0x48, 0x86,
        0xf7, 0x0d, 0x01, 0x01, 0x01]);
    const rsaDerNull = Bytes.from([0x05, 0x00]);
    const rsaDerJoin = parts => {
        const bytes = new Bytes(parts.reduce((sum, part) => sum + part.length, 0));
        let offset = 0;
        for (const part of parts) { bytes.set(part, offset); offset += part.length; }
        return bytes;
    };
    const rsaDerLength = length => {
        if (length < 128) return Bytes.of(length);
        if (length < 256) return Bytes.of(0x81, length);
        if (length < 65536) return Bytes.of(0x82, length >>> 8, length & 255);
        throw error('OperationError', 'RSA DER object exceeds limit');
    };
    const rsaDerItem = (tag, payload) => rsaDerJoin([Bytes.of(tag),
        rsaDerLength(payload.length), payload]);
    const rsaDerSequence = (...parts) => rsaDerItem(0x30, rsaDerJoin(parts));
    const rsaDerInteger = value => {
        let offset = 0;
        while (offset < value.length - 1 && value[offset] === 0) offset++;
        const unsigned = value.subarray(offset);
        const positive = unsigned[0] & 0x80 ? rsaDerJoin([Bytes.of(0), unsigned]) : unsigned;
        return rsaDerItem(0x02, positive);
    };
    const rsaDerAlgorithm = () => rsaDerSequence(rsaDerOid, rsaDerNull);
    const rsaDerExport = (format, parts) => {
        if (format === 'spki') {
            const publicKey = rsaDerSequence(rsaDerInteger(parts.n), rsaDerInteger(parts.e));
            return resultBuffer(rsaDerSequence(rsaDerAlgorithm(),
                rsaDerItem(0x03, rsaDerJoin([Bytes.of(0), publicKey]))));
        }
        if (format === 'pkcs8' && parts.d) {
            const version = rsaDerInteger(Bytes.of(0));
            const privateKey = rsaDerSequence(version, ...['n', 'e', 'd', 'p', 'q',
                'dp', 'dq', 'qi'].map(name => rsaDerInteger(parts[name])));
            return resultBuffer(rsaDerSequence(version, rsaDerAlgorithm(),
                rsaDerItem(0x04, privateKey)));
        }
        unsupported();
    };
    class RsaDerReader {
        constructor(bytes) { this.bytes = bytes; this.offset = 0; }
        item(tag) {
            if (this.offset + 2 > this.bytes.length || this.bytes[this.offset++] !== tag)
                throw error('DataError', 'Unexpected RSA DER tag');
            let length = this.bytes[this.offset++];
            if (length & 0x80) {
                const count = length & 0x7f;
                if (!count || count > 2 || this.offset + count > this.bytes.length ||
                    this.bytes[this.offset] === 0)
                    throw error('DataError', 'Invalid RSA DER length');
                length = 0;
                for (let index = 0; index < count; index++)
                    length = length * 256 + this.bytes[this.offset++];
                if (length < 128 || count === 2 && length < 256)
                    throw error('DataError', 'Non-minimal RSA DER length');
            }
            if (this.offset + length > this.bytes.length)
                throw error('DataError', 'Truncated RSA DER item');
            const result = this.bytes.subarray(this.offset, this.offset + length);
            this.offset += length;
            return result;
        }
        sequence() { return new RsaDerReader(this.item(0x30)); }
        integer() {
            const value = this.item(0x02);
            if (!value.length || value[0] & 0x80 ||
                value.length > 1 && value[0] === 0 && !(value[1] & 0x80))
                throw error('DataError', 'Invalid RSA DER integer');
            return value[0] === 0 && value.length > 1 ? value.subarray(1) : value;
        }
        complete() {
            if (this.offset !== this.bytes.length)
                throw error('DataError', 'Trailing RSA DER data');
        }
    }
    const rsaDerCheckAlgorithm = reader => {
        const algorithm = reader.sequence();
        const oid = algorithm.item(0x06);
        if (oid.length !== rsaDerOid.length - 2 ||
            !oid.every((byte, index) => byte === rsaDerOid[index + 2]) ||
            algorithm.item(0x05).length !== 0)
            throw error('DataError', 'Expected rsaEncryption algorithm');
        algorithm.complete();
    };
    const rsaDerVersion = reader => {
        const version = reader.integer();
        if (version.length !== 1 || version[0] !== 0)
            throw error('DataError', 'Unsupported RSA DER version');
    };
    const rsaDerImport = (format, input) => {
        const bytes = copyBytes(input);
        if (bytes.length > 4096) throw error('DataError', 'RSA DER key exceeds limit');
        const root = new RsaDerReader(bytes), outer = root.sequence();
        let parts;
        if (format === 'spki') {
            rsaDerCheckAlgorithm(outer);
            const bitString = outer.item(0x03);
            if (!bitString.length || bitString[0] !== 0)
                throw error('DataError', 'Invalid RSA public key bit string');
            const inner = new RsaDerReader(bitString.subarray(1)), key = inner.sequence();
            parts = {n: key.integer(), e: key.integer()};
            key.complete(); inner.complete();
        } else if (format === 'pkcs8') {
            rsaDerVersion(outer);
            rsaDerCheckAlgorithm(outer);
            const inner = new RsaDerReader(outer.item(0x04)), key = inner.sequence();
            rsaDerVersion(key);
            parts = {};
            for (const name of ['n', 'e', 'd', 'p', 'q', 'dp', 'dq', 'qi'])
                parts[name] = key.integer();
            key.complete(); inner.complete();
        } else unsupported();
        outer.complete(); root.complete();
        return Object.fromEntries(Object.entries(parts).map(([name, value]) =>
            [name, base64url(value)]));
    };
