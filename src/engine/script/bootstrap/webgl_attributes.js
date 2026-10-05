    // Dictionary members are obtained once, in lexicographic order, including
    // hints that this backend cannot honor. Conversion precedes native creation.
    // https://webidl.spec.whatwg.org/#es-dictionary
    const webGlContextAttributes = (requested, api = 'webgl1') => {
        requested = requested ?? {};
        if (typeof requested !== 'object' && typeof requested !== 'function')
            throw new TypeError('WebGL context attributes must be a dictionary');
        const defaults = {alpha:true, antialias:true, depth:true, desynchronized:false,
            failIfMajorPerformanceCaveat:false, powerPreference:'default',
            premultipliedAlpha:true, preserveDrawingBuffer:false, stencil:false};
        const converted = {};
        for (const name of Object.keys(defaults)) {
            const value = requested[name];
            if (name === 'powerPreference') {
                const preference = value === undefined ? defaults[name] : `${value}`;
                if (!['default', 'low-power', 'high-performance'].includes(preference))
                    throw new TypeError('Invalid WebGL powerPreference');
                converted[name] = preference;
            } else converted[name] = value === undefined ? defaults[name] : Boolean(value);
        }
        // WebGL2 grants this only through a real multisampled drawing buffer;
        // native allocation failure rejects creation instead of claiming MSAA.
        // WebGL1 retains its admitted single-sample surface for now.
        if (api !== 'webgl2') converted.antialias = false;
        converted.desynchronized = false;
        return converted;
    };
    // Restoration reuses converted, privately owned attributes. Never revisit
    // the author's dictionary or silently relax hardware-only admission.
    const webGlNativeOptions = (attributes, api) => JSON.stringify({
        api, alpha:attributes.alpha, depth:attributes.depth,
        stencil:attributes.stencil, antialias:attributes.antialias,
        preserve:attributes.preserveDrawingBuffer,
        fail_if_major_performance_caveat:attributes.failIfMajorPerformanceCaveat,
        power_preference:attributes.powerPreference
    });
