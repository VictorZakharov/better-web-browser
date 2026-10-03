    // Dictionary members are obtained once, in lexicographic order, including
    // hints that this backend cannot honor. Conversion precedes native creation.
    // https://webidl.spec.whatwg.org/#es-dictionary
    const webGlContextAttributes = requested => {
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
        // WARP has no multisampled drawing buffer or desynchronized presenter.
        // Do not advertise requested hints as capabilities that were granted.
        converted.antialias = false;
        converted.desynchronized = false;
        return converted;
    };
