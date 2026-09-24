    // CSS Font Loading setlike collection and per-document loading lifecycle.
    // CSS-connected @font-face rules remain owned by CSS; this set tracks script faces.
    // https://www.w3.org/TR/css-font-loading-3/#font-face-set-interface
    const fontSetState = new WeakMap();
    const fontSet = receiver => {
        const state = fontSetState.get(receiver);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const fontSetMatch = (face, family) => fontState(face).descriptors.family
        .replace(/^['"]|['"]$/g, '').toLowerCase() === family.toLowerCase();
    const fontShorthandFamily = shorthand => {
        // CSS font shorthand requires a size before the family list. This parser only
        // selects a face after recognizing that boundary; unknown shorthand fails closed.
        const match = /(?:^|\s)(?:\d+(?:\.\d+)?)(?:px|pt|em|rem|%)\s*(?:\/\s*\S+\s*)?(.+)$/i.exec(String(shorthand));
        if (!match) throw new DOMException('Invalid font shorthand', 'SyntaxError');
        const family = match[1].split(',')[0].trim().replace(/^['"]|['"]$/g, '');
        if (!family) throw new DOMException('Invalid font shorthand', 'SyntaxError');
        return family;
    };
    class FontFaceSetLoadEvent extends Event {
        constructor(type, init = {}) {
            super(type);
            this.fontfaces = Object.freeze(Array.from(init.fontfaces ?? []));
        }
    }
    class FontFaceSet extends EventTarget {
        constructor(secret) {
            if (secret !== fontSetToken) throw new TypeError('Illegal constructor');
            super();
            fontSetState.set(this, {
                faces: new Set(), pending: new Set(), completed: [], failed: [],
                cssFaces: new Map(),
                ready: Promise.resolve(this), resolveReady: null
            });
        }
        get size() { this._syncCSSFaces(); return fontSet(this).faces.size; }
        get status() { this._syncCSSFaces(); return fontSet(this).pending.size ? 'loading' : 'loaded'; }
        get ready() { this._syncCSSFaces(); return fontSet(this).ready; }
        _syncCSSFaces() {
            const state = fontSet(this);
            const rules = JSON.parse(__hostCall('fontFaceCSSFaces'));
            const live = new Set();
            for (const rule of rules) {
                const key = JSON.stringify([rule.family, rule.weight, rule.italic, rule.url]);
                live.add(key);
                let face = state.cssFaces.get(key);
                if (!face) {
                    face = new FontFace(rule.family,
                        `url("${rule.url.replace(/"/g, '%22')}")`,
                        {weight: String(rule.weight), style: rule.italic ? 'italic' : 'normal'});
                    fontState(face).cssConnected = true;
                    fontState(face).owner = this;
                    state.cssFaces.set(key, face);
                    state.faces.add(face);
                }
                const entry = fontState(face);
                if (rule.loaded && entry.status === 'unloaded') {
                    entry.status = 'loaded';
                    entry.resolveLoaded(face);
                }
            }
            for (const [key, face] of state.cssFaces) {
                if (live.has(key)) continue;
                state.cssFaces.delete(key);
                state.faces.delete(face);
                fontState(face).owner = null;
                if (fontState(face).bytes) __hostCall('fontFaceRemove', fontState(face).id);
            }
        }
        add(face) {
            const state = fontSet(this);
            if (!(face instanceof FontFace)) throw new TypeError('Expected a FontFace');
            const owned = fontState(face);
            if (owned.cssConnected)
                throw new DOMException('CSS-connected FontFace cannot be added', 'InvalidModificationError');
            if (owned.owner && owned.owner !== this)
                throw new DOMException('FontFace belongs to another FontFaceSet', 'InvalidModificationError');
            if (state.faces.has(face)) return this;
            state.faces.add(face);
            owned.owner = this;
            if (owned.status === 'loaded') fontInstall(face);
            else if (owned.status === 'loading') this._fontLoading(face);
            return this;
        }
        delete(face) {
            const state = fontSet(this);
            if (face instanceof FontFace && fontState(face).cssConnected) return false;
            if (!state.faces.delete(face)) return false;
            const owned = fontState(face);
            owned.owner = null;
            if (state.pending.delete(face) && !state.pending.size) this._finishLoading();
            __hostCall('fontFaceRemove', owned.id);
            return true;
        }
        clear() { for (const face of [...fontSet(this).faces]) this.delete(face); }
        has(face) { this._syncCSSFaces(); return fontSet(this).faces.has(face); }
        forEach(callback, thisArg) {
            this._syncCSSFaces();
            if (typeof callback !== 'function') throw new TypeError('Expected callback');
            for (const face of fontSet(this).faces) callback.call(thisArg, face, face, this);
        }
        values() { this._syncCSSFaces(); return fontSet(this).faces.values(); }
        keys() { return this.values(); }
        entries() { this._syncCSSFaces(); return fontSet(this).faces.entries(); }
        [Symbol.iterator]() { return this.values(); }
        check(shorthand, text = ' ') {
            this._syncCSSFaces();
            const family = fontShorthandFamily(shorthand);
            void text;
            return [...fontSet(this).faces].filter(face => fontSetMatch(face, family))
                .every(face => face.status === 'loaded');
        }
        load(shorthand, text = ' ') {
            this._syncCSSFaces();
            let family;
            try { family = fontShorthandFamily(shorthand); }
            catch (error) { return Promise.reject(error); }
            void text;
            const faces = [...fontSet(this).faces].filter(face => fontSetMatch(face, family));
            return Promise.all(faces.map(face => face.load()));
        }
        _fontLoading(face) {
            const state = fontSet(this);
            if (state.pending.has(face)) return;
            if (!state.pending.size) {
                state.completed.length = 0;
                state.failed.length = 0;
                // A second load can begin before the previous completion microtask.
                // Keep the unresolved ready promise across that boundary.
                if (!state.resolveReady)
                    state.ready = new Promise(resolve => { state.resolveReady = resolve; });
                queueMicrotask(() => this.dispatchEvent(new Event('loading')));
            }
            state.pending.add(face);
        }
        _fontSettled(face, success) {
            const state = fontSet(this);
            if (!state.pending.delete(face)) return;
            (success ? state.completed : state.failed).push(face);
            if (!state.pending.size) this._finishLoading();
        }
        _finishLoading() {
            const state = fontSet(this);
            const completed = state.completed.splice(0);
            const failed = state.failed.splice(0);
            const resolveReady = state.resolveReady;
            queueMicrotask(() => {
                this.dispatchEvent(new FontFaceSetLoadEvent('loadingdone', {fontfaces: completed}));
                if (failed.length)
                    this.dispatchEvent(new FontFaceSetLoadEvent('loadingerror', {fontfaces: failed}));
                if (!state.pending.size && state.resolveReady === resolveReady) {
                    resolveReady?.(this);
                    state.resolveReady = null;
                }
            });
        }
    }
    const fontSetToken = {};
    for (const type of ['loading', 'loadingdone', 'loadingerror']) {
        Object.defineProperty(FontFaceSet.prototype, `on${type}`, {
            get() { return fontSet(this)[`on${type}`] ?? null; },
            set(callback) {
                const state = fontSet(this);
                if (state[`on${type}`]) this.removeEventListener(type, state[`on${type}`]);
                state[`on${type}`] = typeof callback === 'function' ? callback : null;
                if (state[`on${type}`]) this.addEventListener(type, state[`on${type}`]);
            }, configurable: true, enumerable: true
        });
    }
    Object.defineProperty(FontFaceSet.prototype, Symbol.toStringTag,
        {value: 'FontFaceSet', configurable: true});
    Object.defineProperty(FontFaceSetLoadEvent.prototype, Symbol.toStringTag,
        {value: 'FontFaceSetLoadEvent', configurable: true});
    const documentFonts = new FontFaceSet(fontSetToken);
    Object.defineProperty(Document.prototype, 'fonts', {
        get() { documentFonts._syncCSSFaces(); return documentFonts; }, enumerable: true, configurable: true
    });
    globalThis.FontFaceSet = FontFaceSet;
    globalThis.FontFaceSetLoadEvent = FontFaceSetLoadEvent;
