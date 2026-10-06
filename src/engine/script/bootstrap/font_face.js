    // CSS Font Loading: a face owns its source and loading promise. Membership in a
    // FontFaceSet, not construction/loading alone, admits its bytes to layout.
    // https://www.w3.org/TR/css-font-loading-3/#font-face-interface
    const fontFaceState = new WeakMap();
    let nextFontFaceId = 1;
    const fontDescriptorNames = [
        'family', 'style', 'weight', 'stretch', 'unicodeRange', 'variant',
        'featureSettings', 'variationSettings', 'display', 'ascentOverride',
        'descentOverride', 'lineGapOverride'
    ];
    const fontDefaults = Object.freeze({
        style: 'normal', weight: 'normal', stretch: 'normal',
        unicodeRange: 'U+0-10FFFF', variant: 'normal', featureSettings: 'normal',
        variationSettings: 'normal', display: 'auto', ascentOverride: 'normal',
        descentOverride: 'normal', lineGapOverride: 'normal'
    });
    const fontWeight = value => {
        if (value === 'normal') return 400;
        if (value === 'bold') return 700;
        const number = Number(value);
        return Number.isInteger(number) && number >= 1 && number <= 1000 ? number : null;
    };
    const fontStyle = value => value === 'normal' ? false : value === 'italic' ? true : null;
    const fontState = face => {
        const state = fontWeakGet(fontFaceState, face);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const fontInstall = face => {
        const state = fontState(face);
        if (state.status !== 'loaded' || !state.owner || !state.bytes) return;
        const weight = fontWeight(state.descriptors.weight);
        const italic = fontStyle(state.descriptors.style);
        if (weight === null || italic === null ||
            !fontHost('fontFaceInstall', state.id, state.descriptors.family, weight, italic, state.bytes,
                state.descriptors.unicodeRange, state.descriptors.featureSettings))
            throw new DOMException('FontFace could not be installed', 'SyntaxError');
    };
    class FontFace {
        constructor(family, source, descriptors = {}) {
            if (arguments.length < 2) throw new TypeError('FontFace requires a family and source');
            const convertedFamily = fontString(family);
            const input = fontBufferSource(source);
            const values = fontDescriptorDictionary(descriptors);
            values.family = convertedFamily;
            const range = fontHost('fontFaceRangeSerialize', values.unicodeRange);
            if (range !== null) values.unicodeRange = range;
            const features = fontHost('fontFaceFeaturesSerialize', values.featureSettings);
            if (features !== null) values.featureSettings = features;
            let resolveLoaded, rejectLoaded;
            const loaded = new Promise((resolve, reject) => {
                resolveLoaded = resolve; rejectLoaded = reject;
            });
            // A rejected face must still expose its rejected loaded promise to consumers.
            loaded.catch(() => {});
            const bytes = 'string' in input ? null : fontCopySource(input);
            const urls = 'string' in input ? fontHost('fontFaceSourceURLs', input.string) : null;
            const invalid = !fontHost('fontFaceFamilyValid',values.family) || fontWeight(values.weight) === null ||
                fontStyle(values.style) === null || range === null || features === null || (!bytes && urls === null);
            fontWeakSet(fontFaceState, this, {
                id: nextFontFaceId++, descriptors: values, bytes, urls,
                status: invalid ? 'error' : 'unloaded', owner: null,
                loadPromise: null, loaded, resolveLoaded, rejectLoaded
            });
            if (invalid) {
                rejectLoaded(new DOMException('Invalid FontFace source or descriptors', 'SyntaxError'));
            } else if (bytes) {
                // BufferSource faces begin loading without an explicit load() call.
                queueFontTask(() => beginFontFaceLoad(this));
            }
        }
        get status() { return fontState(this).status; }
        get loaded() { return fontState(this).loaded; }
        load() {
            // Web IDL converts exceptions from Promise-returning operations to
            // rejected promises, including an invalid receiver.
            let state;
            try { state = fontState(this); }
            catch (error) { return Promise.reject(error); }
            // Buffer-backed faces are already scheduled by construction. load()
            // never advances that task, and every call returns the loaded promise.
            if (!state.bytes && state.status === 'unloaded') beginFontFaceLoad(this);
            return state.loaded;
        }
    }
    const loadFontFace=Function.call.bind(FontFace.prototype.load);
    for (const name of fontDescriptorNames) {
        Object.defineProperty(FontFace.prototype, name, {
            get() { return fontState(this).descriptors[name]; },
            set(value) {
                const state = fontState(this);
                let next = fontString(value);
                if (name === 'unicodeRange') {
                    next = fontHost('fontFaceRangeSerialize', next);
                    if (next === null) throw new DOMException('Invalid FontFace unicode-range', 'SyntaxError');
                }
                if (name === 'featureSettings') {
                    next = fontHost('fontFaceFeaturesSerialize', next);
                    if (next === null) throw new DOMException('Invalid FontFace feature settings', 'SyntaxError');
                }
                if (name === 'family' && !fontHost('fontFaceFamilyValid',next) || name === 'weight' && fontWeight(next) === null ||
                    name === 'style' && fontStyle(next) === null)
                    throw new DOMException('Invalid FontFace descriptor', 'SyntaxError');
                const previous = state.descriptors[name];
                state.descriptors[name] = next;
                try { fontInstall(this); }
                catch (error) { state.descriptors[name] = previous; throw error; }
            },
            enumerable: true, configurable: true
        });
    }
    // CSS Fonts renamed stretch to width; both IDL attributes share the value.
    Object.defineProperty(FontFace.prototype, 'width',
        Object.getOwnPropertyDescriptor(FontFace.prototype, 'stretch'));
    Object.defineProperty(FontFace.prototype, Symbol.toStringTag, {value: 'FontFace', configurable: true});
    globalThis.FontFace = FontFace;
