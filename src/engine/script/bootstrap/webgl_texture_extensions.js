    // No public constructors: these WebGL extension interfaces use
    // LegacyNoInterfaceObject in both Window and Worker realms.
    // https://registry.khronos.org/webgl/extensions/OES_texture_half_float/
    const webGlTextureExtensionConstants = [
        ['EXT_color_buffer_float', {}],
        ['OES_texture_float', {}],
        ['OES_texture_half_float', {HALF_FLOAT_OES:0x8d61}],
        ['OES_texture_float_linear', {}],
        ['OES_texture_half_float_linear', {}],
        ['WEBGL_depth_texture', {UNSIGNED_INT_24_8_WEBGL:0x84fa}],
        ['EXT_sRGB', {SRGB_EXT:0x8c40, SRGB_ALPHA_EXT:0x8c42,
            SRGB8_ALPHA8_EXT:0x8c43, FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING_EXT:0x8210}],
        ['WEBGL_color_buffer_float', {
            RGBA32F_EXT:0x8814,
            FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT:0x8211,
            UNSIGNED_NORMALIZED_EXT:0x8c17
        }],
        ['EXT_color_buffer_half_float', {
            RGBA16F_EXT:0x881a, RGB16F_EXT:0x881b,
            FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE_EXT:0x8211,
            UNSIGNED_NORMALIZED_EXT:0x8c17
        }]
    ];
    for (const [name, constants] of webGlTextureExtensionConstants) {
        const prototype = Object.create(Object.prototype);
        Object.defineProperty(prototype, Symbol.toStringTag, {value:name});
        for (const [key,value] of Object.entries(constants))
            Object.defineProperty(prototype,key,{value,enumerable:true});
        webGlExtensionFactories.set(name.toLowerCase(), {
            name, create:() => Object.create(prototype)
        });
    }
