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
    const fontURL = source => {
        // Only a single url() source is admitted. local(), image URLs, and unparsed
        // fallback lists cannot be silently treated as downloaded font bytes.
        const match = /^\s*url\(\s*(?:"([^"]*)"|'([^']*)'|([^\s)]*))\s*\)\s*(?:format\(\s*(?:"[^"]*"|'[^']*'|[^)]*)\s*\))?\s*$/i.exec(source);
        return match ? match[1] ?? match[2] ?? match[3] : null;
    };
    const fontBuffer = source => {
        if (source instanceof ArrayBuffer) return new Uint8Array(source.slice(0));
        if (ArrayBuffer.isView(source))
            return new Uint8Array(source.buffer.slice(source.byteOffset, source.byteOffset + source.byteLength));
        return null;
    };
    const fontState = face => {
        const state = fontFaceState.get(face);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const fontInstall = face => {
        const state = fontState(face);
        if (state.status !== 'loaded' || !state.owner || !state.bytes) return;
        const weight = fontWeight(state.descriptors.weight);
        const italic = fontStyle(state.descriptors.style);
        if (weight === null || italic === null ||
            !__hostCall('fontFaceInstall', state.id, state.descriptors.family, weight, italic, state.bytes))
            throw new DOMException('FontFace could not be installed', 'SyntaxError');
    };
    class FontFace {
        constructor(family, source, descriptors = {}) {
            if (arguments.length < 2) throw new TypeError('FontFace requires a family and source');
            descriptors = descriptors == null ? {} : Object(descriptors);
            const values = {family: String(family)};
            for (const name of fontDescriptorNames.slice(1))
                values[name] = descriptors[name] === undefined ? fontDefaults[name] : String(descriptors[name]);
            let resolveLoaded, rejectLoaded;
            const loaded = new Promise((resolve, reject) => {
                resolveLoaded = resolve; rejectLoaded = reject;
            });
            // A rejected face must still expose its rejected loaded promise to consumers.
            loaded.catch(() => {});
            const bytes = fontBuffer(source);
            const url = typeof source === 'string' ? fontURL(source) : null;
            const invalid = !values.family.trim() || fontWeight(values.weight) === null ||
                fontStyle(values.style) === null || (!bytes && url === null);
            fontFaceState.set(this, {
                id: nextFontFaceId++, descriptors: values, bytes, url,
                status: invalid ? 'error' : 'unloaded', owner: null,
                loadPromise: null, loaded, resolveLoaded, rejectLoaded
            });
            if (invalid) {
                rejectLoaded(new DOMException('Invalid FontFace source or descriptors', 'SyntaxError'));
            } else if (bytes) {
                // BufferSource faces begin loading without an explicit load() call.
                queueMicrotask(() => { this.load().catch(() => {}); });
            }
        }
        get status() { return fontState(this).status; }
        get loaded() { return fontState(this).loaded; }
        load() {
            const state = fontState(this);
            if (state.loadPromise) return state.loadPromise;
            if (state.status === 'error') return state.loaded;
            if (state.status === 'loaded') return Promise.resolve(this);
            state.status = 'loading';
            state.owner?._fontLoading(this);
            const bytes = state.bytes
                ? Promise.resolve(state.bytes)
                : fetch(state.url, {mode: 'cors'}).then(response => {
                    if (!response.ok) throw new DOMException('FontFace download failed', 'NetworkError');
                    return response.arrayBuffer();
                }).then(buffer => new Uint8Array(buffer));
            state.loadPromise = bytes.then(data => {
                const weight = fontWeight(state.descriptors.weight);
                const italic = fontStyle(state.descriptors.style);
                if (!__hostCall('fontFaceValidate', state.id, state.descriptors.family,
                    weight, italic, data))
                    throw new DOMException('FontFace source could not be decoded', 'SyntaxError');
                state.bytes = data;
                state.status = 'loaded';
                fontInstall(this);
                state.resolveLoaded(this);
                state.owner?._fontSettled(this, true);
                return this;
            }).catch(error => {
                state.status = 'error';
                state.rejectLoaded(error);
                state.owner?._fontSettled(this, false);
                throw error;
            });
            state.loadPromise.catch(() => {});
            return state.loadPromise;
        }
    }
    for (const name of fontDescriptorNames) {
        Object.defineProperty(FontFace.prototype, name, {
            get() { return fontState(this).descriptors[name]; },
            set(value) {
                const state = fontState(this);
                const next = String(value);
                if (name === 'family' && !next.trim() || name === 'weight' && fontWeight(next) === null ||
                    name === 'style' && fontStyle(next) === null)
                    throw new DOMException('Invalid FontFace descriptor', 'SyntaxError');
                state.descriptors[name] = next;
                fontInstall(this);
            },
            enumerable: true, configurable: true
        });
    }
    Object.defineProperty(FontFace.prototype, Symbol.toStringTag, {value: 'FontFace', configurable: true});
    globalThis.FontFace = FontFace;
