(() => {
    'use strict';
    const binaryHost = globalThis.__hostCall;
    const cloneBuffer = ArrayBuffer;
    const cloneIsView = ArrayBuffer.isView;
    const viewConstructors = new Map(['DataView','Uint8Array','Uint8ClampedArray','Int8Array',
        'Uint16Array','Int16Array','Uint32Array','Int32Array','Float16Array','Float32Array',
        'Float64Array','BigInt64Array','BigUint64Array'].map(name => [name, globalThis[name]]));
    const fail = () => { throw new DOMException('The value could not be cloned', 'DataCloneError'); };
    const blobSnapshot = globalThis.__blobStructuredCloneSnapshot;
    delete globalThis.__blobStructuredCloneSnapshot;
    const frames = globalThis.__videoFrameCloneBindings;
    delete globalThis.__videoFrameCloneBindings;
    const audio = globalThis.__audioCloneBindings;
    delete globalThis.__audioCloneBindings;
    const canvas = globalThis.__cloneCanvasBindings;
    delete globalThis.__cloneCanvasBindings;
    const imageData = globalThis.__cloneImageDataBindings;
    delete globalThis.__cloneImageDataBindings;
    const cloneStringify = JSON.stringify, cloneParse = JSON.parse;
    const cloneSetPrototype = Object.setPrototypeOf, cloneKeys = Object.keys;
    const cloneDefineProperty = Object.defineProperty;
    // Wire records are implementation data, not author objects. JSON must not
    // invoke inherited Object/Array.prototype.toJSON during message delivery.
    const wireStringify = value => {
        const protect = item => {
            if (item === null || typeof item !== 'object') return;
            for (const key of cloneKeys(item)) protect(item[key]);
            cloneSetPrototype(item, null);
        };
        protect(value);
        return cloneStringify(value);
    };
    const transferList = options => {
        const source = Array.isArray(options) ? options : options?.transfer;
        if (source === undefined) return [];
        if (source === null || typeof source[Symbol.iterator] !== 'function')
            throw new TypeError('transfer must be an iterable');
        const result = [...source], seen = new Set();
        for (const value of result) {
            if ((!(value instanceof ArrayBuffer) && !globalThis.__clonePortBindings?.isPort(value) &&
                !canvas?.isBitmap(value) && !canvas?.isOffscreen(value) && !frames?.has(value) && !audio?.has(value)) ||
                value.detached || ((canvas?.isBitmap(value) || canvas?.isOffscreen(value)) &&
                    canvas.isDetached(value)) || (frames?.has(value) && frames.closed(value)) ||
                (audio?.has(value) && audio.closed(value)) || seen.has(value)) fail();
            if (globalThis.__clonePortBindings?.isPort(value)) globalThis.__clonePortBindings.describe(value);
            seen.add(value);
        }
        return result;
    };
    const serializeGraph = (input, transfers = [], forStorage = false, packet = false) => {
        const bytesToBase64 = bytes => binaryHost(packet ? 'clonePacketBinaryEncode' : 'cloneBinaryEncode', bytes);
        const encodeBuffer = value => binaryHost(packet ? 'clonePacketBufferEncode' : 'cloneBufferEncode', value);
        transfers = transferList(transfers);
        const seen = new Map(); let nextId = 1;
        const transferredRecords = new Map(transfers.map(value => [value, {}]));
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
            // HTML serializes the graph before running transfer steps. Keep a
            // placeholder so getters can still change the resource before its
            // transfer snapshot, without losing repeated-reference identity.
            if (transferredRecords.has(value)) {
                const record = transferredRecords.get(value);
                record.id = id;
                return record;
            }
            if (audio?.has(value) || audio?.hasChunk(value)) {
                const chunk = audio.hasChunk(value);
                if (!chunk && (forStorage || audio.closed(value))) return fail();
                const record = chunk ? audio.chunkSnapshot(value) : audio.snapshot(value);
                const bytes = bytesToBase64(record.bytes);
                delete record.bytes;
                return {t: chunk ? 'audio-chunk' : 'audio-data', id, v: record, p: bytes};
            }
            if (frames?.has(value)) {
                // WebCodecs frame resources are cloneable/transferable between
                // realms, but explicitly not serializable into persistent stores.
                if (forStorage || frames.closed(value)) return fail();
                const record = frames.snapshot(value);
                const pixels = bytesToBase64(record.pixels);
                delete record.pixels;
                return {t:'video-frame', id, v:record, p:pixels};
            }
            if (globalThis.__clonePortBindings?.isPort(value)) {
                if (!portDescriptors.has(value)) return fail();
                return { t: 'port', id, v: portDescriptors.get(value) };
            }
            if (canvas?.isBitmap(value) || canvas?.isOffscreen(value)) {
                if (canvas.isOffscreen(value) && !transfers.includes(value)) return fail();
                if (canvas.isDetached(value)) return fail();
                const record = canvas.snapshot(value);
                return { t: record.kind, id, w: record.width, h: record.height,
                    cw: record.canvasWidth, ch: record.canvasHeight, o: record.alpha,
                    m: record.mode, a: record.premultiplied, p: bytesToBase64(record.pixels),
                    p16:record.precise?bytesToBase64(record.precise):undefined };
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
            if (imageData?.has(value)) {
                const record = imageData.snapshot(value);
                return {t:'image-data', id, w:record.width, h:record.height,
                    cs:record.colorSpace, pf:record.pixelFormat, v:encode(record.data)};
            }
            if (Array.isArray(value)) return {
                t: 'array', id, l: value.length,
                v: Object.keys(value).map(key => [key, encode(value[key])])
            };
            if (value instanceof Date) return { t: 'date', id, v: value.getTime() };
            if (value instanceof RegExp) return { t: 'regexp', id, s: value.source, f: value.flags };
            if (value instanceof Map) return { t: 'map', id, v: [...value].map(([key, item]) => [encode(key), encode(item)]) };
            if (value instanceof Set) return { t: 'set', id, v: [...value].map(encode) };
            if (value instanceof cloneBuffer) {
                let bytes;
                try { bytes = encodeBuffer(value); } catch { return fail(); }
                return { t: 'buffer', id, v: bytes };
            }
            if (cloneIsView(value)) {
                let metadata;
                try { metadata = binaryHost('cloneBinaryView', value); } catch { return fail(); }
                const [brand, backing, offset, length] = metadata;
                return { t: 'view', id, c: brand, b: encode(backing), o: offset, l: length };
            }
            const blob = blobSnapshot(value);
            if (blob) return {
                t: blob.file ? 'file' : 'blob', id,
                v: bytesToBase64(blob.bytes), y: blob.type,
                n: blob.file?.name, m: blob.file?.lastModified
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
        for (const value of transfers) {
            const target = transferredRecords.get(value);
            if (value instanceof cloneBuffer) {
                target.t = 'buffer';
                try { target.v = encodeBuffer(value); } catch { return fail(); }
                binaryHost('arrayBufferDetach', value);
            } else if (frames?.has(value)) {
                const record = frames.snapshot(value);
                target.t = 'video-frame'; target.p = bytesToBase64(record.pixels);
                delete record.pixels; target.v = record;
                frames.detach(value);
            } else if (audio?.has(value)) {
                const record = audio.snapshot(value);
                target.t = 'audio-data'; target.p = bytesToBase64(record.bytes);
                delete record.bytes; target.v = record;
                audio.detach(value);
            } else if (canvas?.isBitmap(value) || canvas?.isOffscreen(value)) {
                const record = canvas.snapshot(value);
                Object.assign(target, {t:record.kind, w:record.width, h:record.height,
                    cw:record.canvasWidth, ch:record.canvasHeight, o:record.alpha,
                    m:record.mode, a:record.premultiplied, p:bytesToBase64(record.pixels),
                    p16:record.precise ? bytesToBase64(record.precise) : undefined});
                canvas.detach(value);
            } else {
                target.t = 'port'; target.v = globalThis.__clonePortBindings.describe(value);
                globalThis.__clonePortBindings.detach(value);
            }
        }
        // Transfer steps run in list order. A later invalid resource can throw
        // after an earlier one was detached; do not promise rollback here.
        return wireStringify(ports.length ? { __breezeClonePorts: true, payload, ports } : payload);
    };
    const deserializeGraph = (serialized, context, packet = false) => {
        const base64ToBytes = value => binaryHost(packet ? 'clonePacketBinaryDecode' : 'cloneBinaryDecode', value);
        const envelope = cloneParse(String(serialized));
        const transfers = envelope?.__breezeClonePorts === true ? envelope.ports : [];
        const payload = envelope?.__breezeClonePorts === true ? envelope.payload : envelope;
        const references = new Map();
        const ports = new Map();
        const receive = descriptor => {
            const key = cloneStringify(descriptor);
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
            else if (node.t === 'audio-data') value = audio?.receive(node.v, base64ToBytes(node.p)) ?? fail();
            else if (node.t === 'audio-chunk') value = audio?.receiveChunk(node.v, base64ToBytes(node.p)) ?? fail();
            else if (node.t === 'video-frame') value = frames?.receive(node.v, base64ToBytes(node.p)) ?? fail();
            else if (node.t === 'imagebitmap' || node.t === 'offscreencanvas')
                value = canvas?.receive(node, base64ToBytes(node.p),
                    node.p16===undefined?null:base64ToBytes(node.p16)) ?? fail();
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
            else if (node.t === 'image-data') {
                // Older persistent records stored standalone RGBA8 bytes. Keep
                // reading them while new records sub-serialize the actual view
                // to retain graph identity and shared backing-buffer ownership.
                const pixels = node.v === undefined ?
                    new (viewConstructors.get('Uint8ClampedArray'))(base64ToBytes(node.p)) : decode(node.v);
                value = imageData?.receive(node, pixels) ?? fail();
            }
            else if (node.t === 'array') value = new Array(node.l);
            else if (node.t === 'date') value = new Date(node.v);
            else if (node.t === 'regexp') value = new RegExp(node.s, node.f);
            else if (node.t === 'map') value = new Map();
            else if (node.t === 'set') value = new Set();
            else if (node.t === 'buffer') value = base64ToBytes(node.v).buffer;
            else if (node.t === 'view') {
                // Accept the old persisted view envelope as well as the new
                // graph edge, which preserves one backing buffer across views.
                const buffer = typeof node.b === 'string' ? base64ToBytes(node.b).buffer : decode(node.b);
                const constructor = viewConstructors.get(node.c);
                if (typeof constructor === 'function') value = new constructor(buffer, node.o, node.l);
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
            if (node.t === 'array') for (const [key, item] of node.v)
                cloneDefineProperty(value, key, {value:decode(item), enumerable:true, writable:true, configurable:true});
            else if (node.t === 'map') for (const [key, item] of node.v) value.set(decode(key), decode(item));
            else if (node.t === 'set') for (const item of node.v) value.add(decode(item));
            else if (node.t === 'object') for (const [key, item] of node.v)
                cloneDefineProperty(value, key, {value:decode(item), enumerable:true, writable:true, configurable:true});
            return value;
        };
        const data = decode(payload);
        return { data, ports: transfers.map(receive) };
    };
    globalThis.__serializeClone = (input, transfers = [], forStorage = false) => serializeGraph(input, transfers, forStorage);
    globalThis.__deserializeCloneWithPorts = (serialized, context) => deserializeGraph(serialized, context);
    globalThis.__deserializeClone = serialized => deserializeGraph(serialized).data;
    // Worker callbacks capture the decoder; native code captures the serializer and
    // removes both temporary exports before any author code runs. Persistent storage
    // and reentrant structuredClone calls must never produce ephemeral packet tokens.
    globalThis.__serializeWorkerPacket = (input, transfers) => serializeGraph(input, transfers, false, true);
    globalThis.__deserializeWorkerPacketWithPorts = (serialized, context) => deserializeGraph(serialized, context, true);
    globalThis.__cloneTransferList = transferList;
    globalThis.structuredClone = (value, options = {}) =>
        __deserializeClone(__serializeClone(value, transferList(options)));
    // History keeps private references so page scripts cannot replace its storage clone.
    globalThis.__setHistoryStateCloneBindings?.(globalThis.__serializeClone, globalThis.__deserializeClone);
    delete globalThis.__setHistoryStateCloneBindings;
})();
