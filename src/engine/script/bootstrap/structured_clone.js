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
            const canvas = globalThis.__cloneCanvasBindings;
            if ((!(value instanceof ArrayBuffer) && !globalThis.__clonePortBindings?.isPort(value) &&
                !canvas?.isBitmap(value) && !canvas?.isOffscreen(value)) ||
                value.detached || ((canvas?.isBitmap(value) || canvas?.isOffscreen(value)) &&
                    canvas.isDetached(value)) || seen.has(value)) fail();
            if (globalThis.__clonePortBindings?.isPort(value)) globalThis.__clonePortBindings.describe(value);
            seen.add(value);
        }
        return result;
    };
    globalThis.__serializeClone = (input, transfers = []) => {
        transfers = transferList(transfers);
        const seen = new Map(); let nextId = 1;
        const portDescriptors = new Map();
        for (const port of transfers.filter(value => globalThis.__clonePortBindings?.isPort(value)))
            portDescriptors.set(port, globalThis.__clonePortBindings.describe(port));
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
            if (globalThis.__clonePortBindings?.isPort(value)) {
                if (!portDescriptors.has(value)) return fail();
                return { t: 'port', id, v: portDescriptors.get(value) };
            }
            const canvas = globalThis.__cloneCanvasBindings;
            if (canvas?.isBitmap(value) || canvas?.isOffscreen(value)) {
                if (canvas.isOffscreen(value) && !transfers.includes(value)) return fail();
                if (canvas.isDetached(value)) return fail();
                const record = canvas.snapshot(value);
                return { t: record.kind, id, w: record.width, h: record.height,
                    m: record.mode, p: bytesToBase64(record.pixels) };
            }
            if (typeof DOMMatrixReadOnly === 'function' && value instanceof DOMMatrixReadOnly)
                return { t: 'dom-matrix', id, v: Array.from(value.toFloat64Array(), encode),
                    d: value.is2D, r: value instanceof DOMMatrix };
            if (typeof DOMPointReadOnly === 'function' && value instanceof DOMPointReadOnly)
                return { t: 'dom-point', id, v: [value.x, value.y, value.z, value.w].map(encode),
                    r: value instanceof DOMPoint };
            if (typeof DOMQuad === 'function' && value instanceof DOMQuad)
                return { t: 'dom-quad', id, v: [value.p1, value.p2, value.p3, value.p4]
                    .map(point => [point.x, point.y, point.z, point.w].map(encode)) };
            if (typeof DOMRectReadOnly === 'function' && value instanceof DOMRectReadOnly)
                return { t: 'dom-rect', id, v: [value.x, value.y, value.width, value.height].map(encode),
                    r: value instanceof DOMRect };
            if (typeof ImageData === 'function' && value instanceof ImageData)
                return { t: 'image-data', id, w: value.width, h: value.height,
                    p: bytesToBase64(new Uint8Array(value.data.buffer,
                        value.data.byteOffset, value.data.byteLength)) };
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
            if (value instanceof QuotaExceededError) return { t: 'quota-error', id, m: value.message, q: value.quota, r: value.requested };
            if (value instanceof Error) return { t: 'error', id, n: value.name, m: value.message, s: value.stack };
            const prototype = Object.getPrototypeOf(value);
            if (prototype !== Object.prototype && prototype !== null) return fail();
            const entries = [];
            for (const key of Object.keys(value)) entries.push([key, encode(value[key])]);
            return { t: 'object', id, n: prototype === null, v: entries };
        };
        const payload = encode(input);
        const ports = [...portDescriptors.values()];
        const serialized = JSON.stringify(ports.length ? { __breezeClonePorts: true, payload, ports } : payload);
        for (const value of transfers) {
            if (value instanceof ArrayBuffer) __hostCall('arrayBufferDetach', value);
            else if (globalThis.__cloneCanvasBindings?.isBitmap(value) ||
                globalThis.__cloneCanvasBindings?.isOffscreen(value))
                globalThis.__cloneCanvasBindings.detach(value);
            else globalThis.__clonePortBindings.detach(value);
        }
        return serialized;
    };
    globalThis.__deserializeCloneWithPorts = (serialized, context) => {
        const envelope = JSON.parse(String(serialized));
        const transfers = envelope?.__breezeClonePorts === true ? envelope.ports : [];
        const payload = envelope?.__breezeClonePorts === true ? envelope.payload : envelope;
        const references = new Map();
        const ports = new Map();
        const receive = descriptor => {
            const key = JSON.stringify(descriptor);
            if (!ports.has(key)) ports.set(key, globalThis.__clonePortBindings?.receive(descriptor, context) ?? fail());
            return ports.get(key);
        };
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
            if (node.t === 'port') value = receive(node.v);
            else if (node.t === 'imagebitmap' || node.t === 'offscreencanvas')
                value = globalThis.__cloneCanvasBindings?.receive(node, base64ToBytes(node.p)) ?? fail();
            else if (node.t === 'dom-matrix') {
                const numbers = node.v.map(decode);
                value = new (node.r ? DOMMatrix : DOMMatrixReadOnly)(node.d ?
                    [numbers[0], numbers[1], numbers[4], numbers[5], numbers[12], numbers[13]] : numbers);
            }
            else if (node.t === 'dom-point') value = new (node.r ? DOMPoint : DOMPointReadOnly)(
                ...node.v.map(decode));
            else if (node.t === 'dom-quad') value = new DOMQuad(...node.v.map(
                point => ({x: decode(point[0]), y: decode(point[1]),
                    z: decode(point[2]), w: decode(point[3])})));
            else if (node.t === 'dom-rect') value = new (node.r ? DOMRect : DOMRectReadOnly)(
                ...node.v.map(decode));
            else if (node.t === 'image-data') value = new ImageData(
                new Uint8ClampedArray(base64ToBytes(node.p).buffer), node.w, node.h);
            else if (node.t === 'array') value = new Array(node.l);
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
            else if (node.t === 'quota-error') value = new QuotaExceededError(node.m, {
                ...(node.q === null ? {} : { quota: node.q }), ...(node.r === null ? {} : { requested: node.r }),
            });
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
        const data = decode(payload);
        return { data, ports: transfers.map(receive) };
    };
    globalThis.__deserializeClone = serialized => __deserializeCloneWithPorts(serialized).data;
    globalThis.__cloneTransferList = transferList;
    globalThis.structuredClone = (value, options = {}) =>
        __deserializeClone(__serializeClone(value, transferList(options)));
})();
