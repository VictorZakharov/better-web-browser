    // Closed native command records contain only owned primitive lists. Page
    // JSON/toJSON hooks must not see driver IDs or fabricate native replies.
    // This is not a replacement for observable Web IDL argument conversion.
    const webGlWireStringify = JSON.stringify, webGlWireParseJson = JSON.parse;
    const webGlWireCreate = Object.create, webGlWirePrototype = Object.setPrototypeOf;
    const webGlWireKeys = Object.keys, webGlWireOwn = Object.hasOwn;
    const webGlWireFinite = Number.isFinite, webGlWireNaN = Number.isNaN;
    const webGlWireInteger = Number.isSafeInteger, webGlWireSame = Object.is;
    const webGlWireIncludes = Function.call.bind(String.prototype.includes);
    const webGlWireBytes = Function.call.bind(Function.prototype[Symbol.hasInstance], Uint8Array);
    const webGlWirePrimitive = value => {
        const type = typeof value;
        if (value !== null && type !== 'undefined' && type !== 'boolean' && type !== 'number' && type !== 'string')
            throw new TypeError('WebGL private wire requires converted primitive values');
        return value;
    };
    const webGlWireArray = values => {
        const result = [];
        webGlWirePrototype(result, null);
        for (let i = 0; i < values.length; i++) result[i] = webGlWirePrimitive(values[i]);
        return result;
    };
    const webGlWireList = values => webGlWireStringify(webGlWireArray(values));
    const webGlWireOptions = values => {
        const record = webGlWireCreate(null), keys = webGlWireKeys(values);
        for (let i = 0; i < keys.length; i++) record[keys[i]] = webGlWirePrimitive(values[keys[i]]);
        return webGlWireStringify(record);
    };
    const webGlWireCommand = (op, integers = [], floats = [], text = '') => {
        const record = webGlWireCreate(null), encoded = [];
        webGlWirePrototype(encoded, null);
        for (let i = 0; i < integers.length; i++)
            if (!webGlWireInteger(integers[i])) return null;
        for (let i = 0; i < floats.length; i++) {
            const value = floats[i];
            if (typeof value !== 'number') return null;
            encoded[i] = webGlWireSame(value, -0) ? '-0' : webGlWireFinite(value) ? value :
                webGlWireNaN(value) ? 'nan' : value > 0 ? 'inf' : '-inf';
        }
        record.op = webGlWirePrimitive(op); record.i = webGlWireArray(integers); record.f = encoded; record.text = webGlWirePrimitive(text);
        return webGlWireStringify(record);
    };
    const webGlWireParse = raw => {
        // Ordinary replies avoid the recursive walk. Only genuine native float
        // sentinel records need a reviver, never an inherited author property.
        return webGlWireParseJson(raw, webGlWireIncludes(raw, '"webglFloat"') ? (key, entry) => {
            if (entry && typeof entry === 'object' && webGlWireKeys(entry).length === 1 && webGlWireOwn(entry, 'webglFloat'))
                return entry.webglFloat === '-0' ? -0 : entry.webglFloat === 'nan' ? NaN : entry.webglFloat === 'inf' ? Infinity : -Infinity;
            return entry;
        } : undefined);
    };
    const webGlWireLost = value => value !== null && typeof value === 'object' &&
        webGlWireOwn(value, 'lost') && value.lost;
