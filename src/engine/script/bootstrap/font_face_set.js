    // CSS Font Loading setlike collection and per-document loading lifecycle.
    // CSS-connected @font-face rules remain owned by CSS; this set tracks script faces.
    // https://www.w3.org/TR/css-font-loading-3/#font-face-set-interface
    const fontSetState = new WeakMap();
    const fontSet = receiver => {
        const state = fontWeakGet(fontSetState, receiver);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    class FontFaceSet extends EventTarget {
        constructor(secret) {
            if (secret !== fontSetToken) throw new TypeError('Illegal constructor');
            super();
            fontWeakSet(fontSetState, this, {
                faces: new Set(), pending: new Set(), completed: [], failed: [],
                cssFaces: new Map(),
                ready: Promise.resolve(this), resolveReady: null
            });
        }
        get size() { syncFontFaces(this); return fontSet(this).faces.size; }
        get status() { syncFontFaces(this); return fontSet(this).pending.size ? 'loading' : 'loaded'; }
        get ready() { syncFontFaces(this); return fontSet(this).ready; }
        _syncCSSFaces() {
            const state = fontSet(this);
            const rules = fontJSONParse(fontHost('fontFaceCSSFaces'));
            const live = new Set();
            for (const rule of rules) {
                const key = fontJSONStringify([rule.family, rule.weight, rule.italic, rule.source, rule.unicodeRange, rule.featureSettings]);
                live.add(key);
                let face = state.cssFaces.get(key);
                if (!face) {
                    face = new FontFace(fontHost('fontFaceSerializeFamily',rule.family),
                        rule.source,
                        {weight: String(rule.weight), style: rule.italic ? 'italic' : 'normal', unicodeRange: rule.unicodeRange, featureSettings: rule.featureSettings});
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
                const owned = fontState(face);
                // Removing the rule permanently disconnects this object. Readding
                // that rule creates a new object; retained references become ordinary faces.
                owned.cssConnected = false;
                owned.owner = null;
                state.completed = state.completed.filter(value => value !== face);
                state.failed = state.failed.filter(value => value !== face);
                if (state.pending.delete(face) && !state.pending.size) finishFontLoading(this);
                if (owned.bytes) fontHost('fontFaceRemove', owned.id);
            }
        }
        add(face) {
            syncFontFaces(this);
            const state = fontSet(this);
            if (!fontWeakHas(fontFaceState,face)) throw new TypeError('Expected a FontFace');
            const owned = fontState(face);
            if (state.faces.has(face)) return this;
            if (owned.cssConnected)
                throw new DOMException('CSS-connected FontFace cannot be added', 'InvalidModificationError');
            if (owned.owner && owned.owner !== this)
                throw new DOMException('FontFace belongs to another FontFaceSet', 'InvalidModificationError');
            // Admission can fail at the resource budget. Do not publish membership
            // until the native registry has accepted the loaded bytes.
            const previousOwner = owned.owner;
            owned.owner = this;
            try { if (owned.status === 'loaded') fontInstall(face); }
            catch (error) { owned.owner = previousOwner; throw error; }
            state.faces.add(face);
            if (owned.status === 'loading') markFontLoading(this,face);
            return this;
        }
        delete(face) {
            syncFontFaces(this);
            const state = fontSet(this);
            if (fontState(face).cssConnected) return false;
            if (!state.faces.delete(face)) return false;
            const owned = fontState(face);
            owned.owner = null;
            state.completed = state.completed.filter(value => value !== face);
            state.failed = state.failed.filter(value => value !== face);
            if (state.pending.delete(face) && !state.pending.size) finishFontLoading(this);
            fontHost('fontFaceRemove', owned.id);
            return true;
        }
        clear() { syncFontFaces(this); for (const face of [...fontSet(this).faces]) deleteFontFromSet(this,face); }
        has(face) { fontSet(this);fontState(face);syncFontFaces(this); return fontSet(this).faces.has(face); }
        forEach(callback, thisArg) {
            syncFontFaces(this);
            if (typeof callback !== 'function') throw new TypeError('Expected callback');
            for (const face of fontSet(this).faces) callback.call(thisArg, face, face, this);
        }
        values() { syncFontFaces(this); return fontSet(this).faces.values(); }
        keys() { syncFontFaces(this);return fontSet(this).faces.values(); }
        entries() { syncFontFaces(this); return fontSet(this).faces.entries(); }
        [Symbol.iterator]() { syncFontFaces(this);return fontSet(this).faces.values(); }
        check(shorthand, text = ' ') {
            fontSet(this);
            if (!arguments.length) throw new TypeError('check requires a font shorthand');
            shorthand=fontString(shorthand);text=fontString(text);
            syncFontFaces(this);
            return matchFontFaces(this,shorthand,text)
                .every(face => fontState(face).status === 'loaded');
        }
        load(shorthand, text = ' ') {
            let faces;
            try {
                fontSet(this);
                if (!arguments.length) throw new TypeError('load requires a font shorthand');
                shorthand=fontString(shorthand);text=fontString(text);
                syncFontFaces(this);
                faces = matchFontFaces(this,shorthand,text);
            }
            catch (error) { return Promise.reject(error); }
            return Promise.all(faces.map(face => loadFontFace(face)));
        }
        _fontLoading(face) {
            const state = fontSet(this);
            if (state.pending.has(face)) return;
            if (!state.pending.size) {
                if (!state.resolveReady)
                    state.ready = new Promise(resolve => { state.resolveReady = resolve; });
                queueFontTask(() => fontEventDispatch(this,fontTrustEvent(new FontFaceSetLoadEvent('loading'))));
            }
            state.pending.add(face);
        }
        _fontSettled(face, success) {
            const state = fontSet(this);
            if (!state.pending.delete(face)) return;
            (success ? state.completed : state.failed).push(face);
            if (!state.pending.size) finishFontLoading(this);
        }
        _finishLoading() {
            const state = fontSet(this);
            state.resolveReady?.(this);
            state.resolveReady = null;
            queueFontTask(() => {
                const completed = state.completed.splice(0).filter(face => state.faces.has(face));
                const failed = state.failed.splice(0).filter(face => state.faces.has(face));
                fontEventDispatch(this,fontTrustEvent(new FontFaceSetLoadEvent('loadingdone', {fontfaces: completed})));
                if (failed.length)
                    fontEventDispatch(this,fontTrustEvent(new FontFaceSetLoadEvent('loadingerror', {fontfaces: failed})));
            });
        }
    }
    // Platform lifecycle helpers are lexical, not author-overridable methods.
    const syncFontFaces=Function.call.bind(FontFaceSet.prototype._syncCSSFaces);
    const markFontLoading=Function.call.bind(FontFaceSet.prototype._fontLoading);
    const markFontSettled=Function.call.bind(FontFaceSet.prototype._fontSettled);
    const finishFontLoading=Function.call.bind(FontFaceSet.prototype._finishLoading);
    const deleteFontFromSet=Function.call.bind(FontFaceSet.prototype.delete);
    for (const name of ['_syncCSSFaces','_fontLoading','_fontSettled','_finishLoading'])
        delete FontFaceSet.prototype[name];
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
    if (typeof Document === 'function') {
        Object.defineProperty(Document.prototype, 'fonts', {
            get() { syncFontFaces(documentFonts); return documentFonts; }, enumerable: true, configurable: true
        });
    } else {
        // CSS Font Loading's worker FontFaceSource starts empty and remains
        // independent of the creating document's FontFaceSet.
        Object.defineProperty(globalThis, 'fonts', {
            get() { return documentFonts; }, enumerable: true, configurable: true
        });
    }
    globalThis.FontFaceSet = FontFaceSet;
    globalThis.FontFaceSetLoadEvent = FontFaceSetLoadEvent;
