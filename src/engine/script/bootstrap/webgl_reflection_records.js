    // Native reflection results are branded platform records, not ordinary
    // objects with own, non-configurable copies of IDL attributes.
    // https://registry.khronos.org/webgl/specs/latest/1.0/#5.9
    // https://webidl.spec.whatwg.org/#es-attributes
    const webGlReflectionRecords = new WeakMap();
    const webGlReflectionGet = Function.prototype.call.bind(WeakMap.prototype.get);
    const webGlReflectionSet = Function.prototype.call.bind(WeakMap.prototype.set);
    const webGlReflectionFields = new Map([
        ['WebGLActiveInfo', ['size','type','name']],
        ['WebGLShaderPrecisionFormat', ['rangeMin','rangeMax','precision']]
    ]);
    for (const [name, fields] of webGlReflectionFields) {
        for (const field of fields) {
            const getter = function() {
                const record = webGlReflectionGet(webGlReflectionRecords, this);
                if (!record || record.type !== name) throw new TypeError('Illegal '+name+' receiver');
                return record.values[field];
            };
            Object.defineProperty(getter, 'name', {value:'get '+field});
            Object.defineProperty(webGlObjectClasses[name].prototype, field,
                {enumerable:true, configurable:true, get:getter});
        }
    }
    const webGlReflectionRecord = (name, native) => {
        if (!native) return null;
        const fields = webGlReflectionFields.get(name);
        if (!fields) throw new TypeError('Unknown WebGL reflection record');
        const values = Object.create(null);
        for (const field of fields) values[field] = native[field];
        const result = new webGlObjectClasses[name](webGlToken);
        webGlReflectionSet(webGlReflectionRecords, result, {type:name, values});
        return result;
    };
