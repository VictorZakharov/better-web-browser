    const webGlExtensionFactories = new Map();
    // No public constructor or thread-count controls: the WebGL extension
    // exposes only the driver's non-blocking completion status enumerant.
    class KHR_parallel_shader_compile {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    Object.defineProperties(KHR_parallel_shader_compile.prototype, {
        [Symbol.toStringTag]:{value:'KHR_parallel_shader_compile'},
        COMPLETION_STATUS_KHR:{value:0x91b1, enumerable:true}
    });
    webGlExtensionFactories.set('khr_parallel_shader_compile', {
        name:'KHR_parallel_shader_compile', create:() => new KHR_parallel_shader_compile(webGlToken)
    });
    const webGlInstancingExtensions = webGlPrivateBrands();
    const webGlExtensionReceiver = (receiver, map, arity, args, name) => {
        const record = map.get(receiver);
        if (!record) throw new TypeError('Illegal ' + name + ' receiver');
        if (args.length < arity) throw new TypeError(name + ' requires ' + arity + ' arguments');
        const state = webGlState(record.context);
        return state.lost || record.epoch !== state.epoch ? null : record.context;
    };
    class ANGLE_instanced_arrays {
        constructor(token, context) {
            if (token !== webGlToken) throw new TypeError('Illegal constructor');
            webGlInstancingExtensions.set(this, {context, epoch:webGlState(context).epoch});
        }
        drawArraysInstancedANGLE(mode, first, count, primcount) {
            const context = webGlExtensionReceiver(this, webGlInstancingExtensions, 4, arguments, 'drawArraysInstancedANGLE');
            mode = webGlScalar('u', mode);
            first = webGlScalar('i', first);
            count = webGlScalar('i', count);
            primcount = webGlScalar('i', primcount);
            if (!context) return;
            webGlCall(context, 'drawArraysInstancedANGLE', [webGlUnsigned(mode), webGlInteger(first), webGlInteger(count), webGlInteger(primcount)]);
            webGlState(context).dirty = true;
            dirtyWebGlCanvas(context);
        }
        drawElementsInstancedANGLE(mode, count, type, offset, primcount) {
            const context = webGlExtensionReceiver(this, webGlInstancingExtensions, 5, arguments, 'drawElementsInstancedANGLE');
            mode = webGlScalar('u', mode);
            count = webGlScalar('i', count);
            type = webGlScalar('u', type);
            offset = webGlScalar('l', offset);
            primcount = webGlScalar('i', primcount);
            if (!context) return;
            webGlCall(context, 'drawElementsInstancedANGLE', [webGlUnsigned(mode), webGlInteger(count), webGlUnsigned(type), Number(offset), webGlInteger(primcount)]);
            webGlState(context).dirty = true;
            dirtyWebGlCanvas(context);
        }
        vertexAttribDivisorANGLE(index, divisor) {
            const context = webGlExtensionReceiver(this, webGlInstancingExtensions, 2, arguments, 'vertexAttribDivisorANGLE');
            index = webGlScalar('u', index);
            divisor = webGlScalar('u', divisor);
            if (context) webGlCall(context, 'vertexAttribDivisorANGLE', [webGlUnsigned(index), webGlUnsigned(divisor)]);
        }
    }
    Object.defineProperties(ANGLE_instanced_arrays.prototype, {
        [Symbol.toStringTag]:{value:'ANGLE_instanced_arrays'},
        VERTEX_ATTRIB_ARRAY_DIVISOR_ANGLE:{value:0x88fe, enumerable:true}
    });
    webGlExtensionFactories.set('angle_instanced_arrays', {
        name:'ANGLE_instanced_arrays', create:context => new ANGLE_instanced_arrays(webGlToken, context)
    });
    class OES_element_index_uint {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    class OES_standard_derivatives {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    class EXT_frag_depth {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    class EXT_shader_texture_lod {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    class EXT_texture_filter_anisotropic {
        constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
    }
    Object.defineProperties(EXT_texture_filter_anisotropic.prototype, {
        [Symbol.toStringTag]:{value:'EXT_texture_filter_anisotropic'},
        TEXTURE_MAX_ANISOTROPY_EXT:{value:0x84fe, enumerable:true},
        MAX_TEXTURE_MAX_ANISOTROPY_EXT:{value:0x84ff, enumerable:true}
    });
    webGlExtensionFactories.set('ext_texture_filter_anisotropic', {
        name:'EXT_texture_filter_anisotropic', create:() => new EXT_texture_filter_anisotropic(webGlToken)
    });
    for (const constructor of [EXT_frag_depth, EXT_shader_texture_lod])
        Object.defineProperty(constructor.prototype, Symbol.toStringTag, {value:constructor.name});
    Object.defineProperty(OES_element_index_uint.prototype, Symbol.toStringTag, {value:'OES_element_index_uint'});
    Object.defineProperties(OES_standard_derivatives.prototype, {
        [Symbol.toStringTag]:{value:'OES_standard_derivatives'},
        FRAGMENT_SHADER_DERIVATIVE_HINT_OES:{value:0x8b8b, enumerable:true}
    });
    for (const [name, constructor] of [['OES_element_index_uint', OES_element_index_uint],
        ['OES_standard_derivatives', OES_standard_derivatives], ['EXT_frag_depth', EXT_frag_depth],
        ['EXT_shader_texture_lod', EXT_shader_texture_lod]])
        webGlExtensionFactories.set(name.toLowerCase(), {name, create:() => new constructor(webGlToken)});
