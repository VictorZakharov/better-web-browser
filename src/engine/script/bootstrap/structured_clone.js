(() => {
    'use strict';
    const bytesToBase64 = bytes => {
        let binary = '';
        for (let start = 0; start < bytes.length; start += 0x4000)
            binary += String.fromCharCode(...bytes.subarray(start, start + 0x4000));
        return btoa(binary);
    };
    const base64ToBytes = value => {
        const binary = atob(value), bytes = new Uint8Array(binary.length);
        for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
        return bytes;
    };
    const fail = () => { throw new DOMException('The value could not be cloned', 'DataCloneError'); };
    const transferList = options => {
        const source = Array.isArray(options) ? options : options?.transfer;
        if (source === undefined) return [];
        if (source === null || typeof source[Symbol.iterator] !== 'function')
            throw new TypeError('transfer must be an iterable');
        const result = [...source], seen = new Set();
        for (const value of result) {
            if (!(value instanceof ArrayBuffer) || value.detached || seen.has(value)) fail();
            seen.add(value);
        }
        return result;
    };
    globalThis.__serializeClone = (input, transfers = []) => {
        transfers = transferList(transfers);
        const seen = new Map(); let nextId = 1;
        const encode = value => {
            if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
            if (typeof value === 'undefined') return { t: 'undefined' };
            if (typeof value === 'bigint') return { t: 'bigint', v: String(value) };
            if (typeof value === 'number') {
                if (Number.isNaN(value)) return { t: 'number', v: 'nan' };
                if (value === Infinity) return { t: 'number', v: 'infinity' };
                if (value === -Infinity) return { t: 'number', v: '-infinity' };
                if (Object.is(value, -0)) return { t: 'number', v: '-0' };
                return value;
            }
            if (typeof value !== 'object') return fail();
            if (seen.has(value)) return { t: 'reference', v: seen.get(value) };
            const id = nextId++; seen.set(value, id);
            if (Array.isArray(value)) return {
                t: 'array', id, l: value.length,
                v: Object.keys(value).map(key => [key, encode(value[key])])
            };
            if (value instanceof Date) return { t: 'date', id, v: value.getTime() };
            if (value instanceof RegExp) return { t: 'regexp', id, s: value.source, f: value.flags };
            if (value instanceof Map) return { t: 'map', id, v: [...value].map(([key, item]) => [encode(key), encode(item)]) };
            if (value instanceof Set) return { t: 'set', id, v: [...value].map(encode) };
            if (value instanceof ArrayBuffer)
                return { t: 'buffer', id, v: bytesToBase64(new Uint8Array(value)) };
            if (ArrayBuffer.isView?.(value)) return {
                t: 'view', id, c: value.constructor.name,
                b: bytesToBase64(new Uint8Array(value.buffer)), o: value.byteOffset,
                l: value instanceof DataView ? value.byteLength : value.length
            };
            if (typeof Blob === 'function' && value instanceof Blob) return {
                t: value instanceof File ? 'file' : 'blob', id,
                v: bytesToBase64(value.__bytes), y: value.type,
                n: value.name, m: value.lastModified
            };
            if (value instanceof Error) return { t: 'error', id, n: value.name, m: value.message, s: value.stack };
            const prototype = Object.getPrototypeOf(value);
            if (prototype !== Object.prototype && prototype !== null) return fail();
            const entries = [];
            for (const key of Object.keys(value)) entries.push([key, encode(value[key])]);
            return { t: 'object', id, n: prototype === null, v: entries };
        };
        const serialized = JSON.stringify(encode(input));
        for (const buffer of transfers) __hostCall('arrayBufferDetach', buffer);
        return serialized;
    };
    globalThis.__deserializeClone = serialized => {
        const references = new Map();
        const decode = node => {
            if (node === null || typeof node !== 'object') return node;
            if (node.t === 'reference') {
                if (!references.has(node.v)) return fail();
                return references.get(node.v);
            }
            if (node.t === 'undefined') return undefined;
            if (node.t === 'bigint') return BigInt(node.v);
            if (node.t === 'number') return ({ nan: NaN, infinity: Infinity, '-infinity': -Infinity, '-0': -0 })[node.v];
            let value;
            if (node.t === 'array') value = new Array(node.l);
            else if (node.t === 'date') value = new Date(node.v);
            else if (node.t === 'regexp') value = new RegExp(node.s, node.f);
            else if (node.t === 'map') value = new Map();
            else if (node.t === 'set') value = new Set();
            else if (node.t === 'buffer') value = base64ToBytes(node.v).buffer;
            else if (node.t === 'view') {
                const bytes = base64ToBytes(node.b), constructor = globalThis[node.c];
                if (node.c === 'DataView') value = new DataView(bytes.buffer, node.o, node.l);
                else if (typeof constructor === 'function') value = new constructor(bytes.buffer, node.o, node.l);
                else return fail();
            } else if (node.t === 'blob') value = new Blob([base64ToBytes(node.v)], { type: node.y });
            else if (node.t === 'file') value = new File([base64ToBytes(node.v)], node.n, { type: node.y, lastModified: node.m });
            else if (node.t === 'error') { value = new Error(node.m); value.name = node.n; value.stack = node.s; }
            else if (node.t === 'object') value = node.n ? Object.create(null) : {};
            else return fail();
            if (node.id) references.set(node.id, value);
            if (node.t === 'array') for (const [key, item] of node.v) value[key] = decode(item);
            else if (node.t === 'map') for (const [key, item] of node.v) value.set(decode(key), decode(item));
            else if (node.t === 'set') for (const item of node.v) value.add(decode(item));
            else if (node.t === 'object') for (const [key, item] of node.v) value[key] = decode(item);
            return value;
        };
        return decode(JSON.parse(String(serialized)));
    };
    globalThis.__cloneTransferList = transferList;
    globalThis.structuredClone = (value, options = {}) =>
        __deserializeClone(__serializeClone(value, transferList(options)));
})();
