    // LegacyNoInterfaceObject extension types share Window/Worker registration.
    // The native backend admits formats only after each exact extension is enabled.
    const webGlCompressionDefinitions = [
        ['WEBGL_compressed_texture_s3tc', {
            COMPRESSED_RGB_S3TC_DXT1_EXT:0x83f0,
            COMPRESSED_RGBA_S3TC_DXT1_EXT:0x83f1,
            COMPRESSED_RGBA_S3TC_DXT3_EXT:0x83f2,
            COMPRESSED_RGBA_S3TC_DXT5_EXT:0x83f3
        }],
        ['WEBGL_compressed_texture_s3tc_srgb', {
            COMPRESSED_SRGB_S3TC_DXT1_EXT:0x8c4c,
            COMPRESSED_SRGB_ALPHA_S3TC_DXT1_EXT:0x8c4d,
            COMPRESSED_SRGB_ALPHA_S3TC_DXT3_EXT:0x8c4e,
            COMPRESSED_SRGB_ALPHA_S3TC_DXT5_EXT:0x8c4f
        }],
        ['EXT_texture_compression_rgtc', {
            COMPRESSED_RED_RGTC1_EXT:0x8dbb,
            COMPRESSED_SIGNED_RED_RGTC1_EXT:0x8dbc,
            COMPRESSED_RED_GREEN_RGTC2_EXT:0x8dbd,
            COMPRESSED_SIGNED_RED_GREEN_RGTC2_EXT:0x8dbe
        }]
    ];
    for (const [name, constants] of webGlCompressionDefinitions) {
        const constructor = class {
            constructor(token) { if (token !== webGlToken) throw new TypeError('Illegal constructor'); }
        };
        Object.defineProperty(constructor, 'name', {value:name});
        Object.defineProperty(constructor.prototype, Symbol.toStringTag, {value:name});
        for (const [constant, value] of Object.entries(constants)) {
            Object.defineProperty(constructor.prototype, constant, {value, enumerable:true});
            Object.defineProperty(constructor, constant, {value, enumerable:true});
        }
        webGlExtensionFactories.set(name.toLowerCase(), {name, create:() => new constructor(webGlToken)});
    }
