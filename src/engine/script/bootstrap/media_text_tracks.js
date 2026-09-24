    // HTML timed text tracks are live views over <track> children plus script-created tracks.
    // Cues are selected against the media clock, not wall-clock timer approximations.
    const textTrackStates = new WeakMap(), cueStates = new WeakMap(), elementTracks = new WeakMap();
    const addedTextTracks = new WeakMap(), textTrackLists = new WeakMap();
    const lastMediaCaptions = new WeakMap();
    const cueOwners = new WeakMap(), cueLists = new WeakMap();
    const trackKinds = new Set(['subtitles', 'captions', 'descriptions', 'chapters', 'metadata']);
    // Web IDL legacy indexed properties are observable beyond bracket reads:
    // property presence, enumeration, and descriptors follow the live list.
    // https://webidl.spec.whatwg.org/#legacy-platform-object
    const mediaTrackListProxy = list => new Proxy(list, {
        get(target, name, receiver) {
            const index = collectionIndex(name);
            if (index !== null) return target.item(index) ?? undefined;
            return Reflect.get(target, name, receiver);
        },
        has(target, name) {
            const index = collectionIndex(name);
            return index !== null ? index < target.length : Reflect.has(target, name);
        },
        ownKeys(target) {
            return [...Array(target.length).keys()].map(String).concat(Reflect.ownKeys(target));
        },
        getOwnPropertyDescriptor(target, name) {
            const index = collectionIndex(name);
            return index !== null ? index < target.length ? {
                value: target.item(index), writable: false, enumerable: true, configurable: true
            } : undefined : Reflect.getOwnPropertyDescriptor(target, name);
        },
        set(target, name, value, receiver) {
            return collectionIndex(name) === null && Reflect.set(target, name, value, receiver);
        },
        defineProperty(target, name, descriptor) {
            return collectionIndex(name) === null && Reflect.defineProperty(target, name, descriptor);
        },
        deleteProperty(target, name) {
            const index = collectionIndex(name);
            return index === null || index >= target.length && Reflect.deleteProperty(target, name);
        }
    });
    class TextTrackCue extends EventTarget {
        constructor(startTime, endTime) {
            super();
            if (new.target === TextTrackCue) throw new TypeError('Illegal constructor');
            cueStates.set(this, { startTime: Number(startTime), endTime: Number(endTime),
                id: '', pauseOnExit: false });
        }
        get track() { return cueOwners.get(this) ?? null; }
        get id() { return cueStates.get(this).id; }
        set id(value) { cueStates.get(this).id = String(value); }
        get pauseOnExit() { return cueStates.get(this).pauseOnExit; }
        set pauseOnExit(value) { cueStates.get(this).pauseOnExit = Boolean(value); }
        get startTime() { return cueStates.get(this).startTime; }
        set startTime(value) { cueStates.get(this).startTime = Number(value); this.track?._resort(); }
        get endTime() { return cueStates.get(this).endTime; }
        set endTime(value) { cueStates.get(this).endTime = Number(value); this.track?._resort(); }
    }
    class VTTCue extends TextTrackCue {
        constructor(startTime, endTime, text) {
            super(startTime, endTime);
            if (!Number.isFinite(this.startTime) || !Number.isFinite(this.endTime))
                throw new TypeError('Cue times must be finite');
            this.text = String(text);
            this.vertical = '';
            this.snapToLines = true;
            this.line = 'auto';
            this.lineAlign = 'start';
            this.position = 'auto';
            this.positionAlign = 'auto';
            this.size = 100;
            this.align = 'center';
        }
        getCueAsHTML() { return parseVttCueText(this.text); }
    }
    const cueHandler = (prototype, name) => {
        const slot = new WeakMap();
        Object.defineProperty(prototype, 'on' + name, {
            enumerable: true, configurable: true,
            get() { return slot.get(this) ?? null; },
            set(value) {
                const previous = slot.get(this);
                if (previous) this.removeEventListener(name, previous);
                if (typeof value === 'function') { slot.set(this, value); this.addEventListener(name, value); }
                else slot.delete(this);
            }
        });
    };
    cueHandler(TextTrackCue.prototype, 'enter');
    cueHandler(TextTrackCue.prototype, 'exit');

    class TextTrackCueList {
        constructor(getItems) { this._getItems = getItems; }
        get length() { return this._getItems().length; }
        item(index) { return this._getItems()[Number(index)] ?? null; }
        getCueById(id) { return this._getItems().find(cue => cue.id === String(id)) ?? null; }
        [Symbol.iterator]() { return this._getItems()[Symbol.iterator](); }
    }
    const cueList = getItems => mediaTrackListProxy(new TextTrackCueList(getItems));
    class TextTrack extends EventTarget {
        constructor(token, media, element, kind, label, language) {
            super();
            if (token !== trackConstructionToken) throw new TypeError('Illegal constructor');
            textTrackStates.set(this, { media, element, kind, label, language,
                mode: 'disabled', cues: [], active: [], loading: false, generation: 0 });
        }
        get kind() { const s = textTrackStates.get(this); return s.element?.kind ?? s.kind; }
        get label() { const s = textTrackStates.get(this); return s.element?.label ?? s.label; }
        get language() { const s = textTrackStates.get(this); return s.element?.srclang ?? s.language; }
        get id() { return textTrackStates.get(this).element?.id ?? ''; }
        get inBandMetadataTrackDispatchType() { return ''; }
        get mode() { return textTrackStates.get(this).mode; }
        set mode(value) {
            value = String(value);
            if (!['disabled', 'hidden', 'showing'].includes(value)) throw new TypeError('Invalid text track mode');
            const state = textTrackStates.get(this);
            if (state.mode === value) return;
            state.mode = value;
            if (value !== 'disabled') loadTextTrack(this);
            updateTextTracks(state.media);
            textTrackListFor(state.media).dispatchEvent(new Event('change'));
        }
        get cues() {
            const state = textTrackStates.get(this);
            return state.element && state.element.readyState !== HTMLTrackElement.LOADED
                ? null : (cueLists.get(this) ?? cueLists.set(this, cueList(() => state.cues)).get(this));
        }
        get activeCues() {
            const state = textTrackStates.get(this);
            if (state.mode === 'disabled') return null;
            return state.activeList ??= cueList(() => state.active);
        }
        addCue(cue) {
            if (!(cue instanceof TextTrackCue)) throw new TypeError('Expected a TextTrackCue');
            if (!Number.isFinite(cue.startTime) || !Number.isFinite(cue.endTime) ||
                cue.startTime > cue.endTime) throw new DOMException('Invalid cue interval', 'InvalidStateError');
            const old = cueOwners.get(cue);
            if (old && old !== this) old.removeCue(cue);
            const state = textTrackStates.get(this);
            if (!state.cues.includes(cue)) state.cues.push(cue);
            cueOwners.set(cue, this);
            this._resort();
        }
        removeCue(cue) {
            const state = textTrackStates.get(this), at = state.cues.indexOf(cue);
            if (at < 0) throw new DOMException('Cue is not in this track', 'NotFoundError');
            state.cues.splice(at, 1);
            cueOwners.delete(cue);
            updateTextTracks(state.media);
        }
        _resort() {
            const state = textTrackStates.get(this);
            state.cues.sort((a, b) => a.startTime - b.startTime || b.endTime - a.endTime);
            updateTextTracks(state.media);
        }
    }
    const trackConstructionToken = {};
    cueHandler(TextTrack.prototype, 'cuechange');
    const textTrackListFor = media => {
        let list = textTrackLists.get(media);
        if (!list) {
            list = mediaTrackListProxy(new TextTrackList(media));
            textTrackLists.set(media, list);
        }
        return list;
    };
    const textTracksFor = media => {
        const fromElements = [...media.children].filter(node => node instanceof HTMLTrackElement)
            .map(node => {
                const track = node.track;
                textTrackStates.get(track).media = media;
                return track;
            });
        return [...fromElements, ...(addedTextTracks.get(media) ?? [])];
    };
    class TextTrackList extends EventTarget {
        constructor(media) { super(); this._media = media; }
        get length() { return textTracksFor(this._media).length; }
        item(index) { return textTracksFor(this._media)[Number(index)] ?? null; }
        getTrackById(id) { return textTracksFor(this._media).find(track => track.id === String(id)) ?? null; }
        [Symbol.iterator]() { return textTracksFor(this._media)[Symbol.iterator](); }
    }
    cueHandler(TextTrackList.prototype, 'addtrack');
    cueHandler(TextTrackList.prototype, 'removetrack');
    cueHandler(TextTrackList.prototype, 'change');
    Object.defineProperty(HTMLMediaElement.prototype, 'textTracks', {
        enumerable: true, configurable: true, get() { return textTrackListFor(this); }
    });
    HTMLMediaElement.prototype.addTextTrack = function(kind, label = '', language = '') {
        kind = String(kind);
        if (!trackKinds.has(kind)) throw new TypeError('Invalid text track kind');
        const track = new TextTrack(trackConstructionToken, this, null, kind, String(label), String(language));
        const list = addedTextTracks.get(this) ?? [];
        list.push(track); addedTextTracks.set(this, list);
        const trackList = textTrackListFor(this);
        queueMediaTask(() => trackList.dispatchEvent(markTrusted(new TrackEvent('addtrack', { track }))));
        return track;
    };
    class HTMLTrackElement extends HTMLElement {
        constructor(id, ...metadata) {
            if (id === undefined) throw new TypeError('Illegal constructor');
            super(id, ...metadata);
        }
        get kind() { const value = this.getAttribute('kind'); return trackKinds.has(value) ? value : 'subtitles'; }
        set kind(value) { this.setAttribute('kind', String(value)); }
        get src() {
            const value = this.getAttribute('src');
            return value === null ? '' : host('resolveUrl', value);
        }
        set src(value) {
            this.setAttribute('src', String(value));
            const state = elementTracks.get(this);
            if (state && state.track.mode !== 'disabled') {
                textTrackStates.get(state.track).generation++;
                textTrackStates.get(state.track).loading = false;
                state.readyState = HTMLTrackElement.NONE;
                loadTextTrack(state.track);
            }
        }
        get srclang() { return this.getAttribute('srclang') ?? ''; }
        set srclang(value) { this.setAttribute('srclang', String(value)); }
        get label() { return this.getAttribute('label') ?? ''; }
        set label(value) { this.setAttribute('label', String(value)); }
        get default() { return this.hasAttribute('default'); }
        set default(value) { this.toggleAttribute('default', Boolean(value)); }
        get readyState() { return elementTracks.get(this)?.readyState ?? HTMLTrackElement.NONE; }
        get track() {
            let state = elementTracks.get(this);
            if (!state) {
                const media = this.parentElement instanceof HTMLMediaElement ? this.parentElement : null;
                const track = new TextTrack(trackConstructionToken, media, this, this.kind, this.label, this.srclang);
                state = { track, readyState: HTMLTrackElement.NONE, source: '' };
                elementTracks.set(this, state);
                if (this.default && media && !textTracksFor(media).some(other =>
                    other !== track && other.mode === 'showing' && other.kind === track.kind))
                    track.mode = 'showing';
            }
            return state.track;
        }
    }
    for (const [name, value] of Object.entries({ NONE: 0, LOADING: 1, LOADED: 2, ERROR: 3 })) {
        Object.defineProperty(HTMLTrackElement, name, { value, enumerable: true });
        Object.defineProperty(HTMLTrackElement.prototype, name, { value, enumerable: true });
    }
    const loadTextTrack = track => {
        const state = textTrackStates.get(track), element = state.element;
        if (!element || !element.src || state.loading) return;
        const record = elementTracks.get(element);
        if (record.readyState === HTMLTrackElement.LOADED && record.source === element.src) return;
        const generation = ++state.generation, source = element.src;
        state.loading = true;
        record.readyState = HTMLTrackElement.LOADING;
        fetch(source, { mode: 'cors' }).then(response => {
            if (!response.ok) throw new Error('Text track request failed');
            return response.text();
        }).then(text => {
            if (state.generation !== generation) return;
            const cues = parseWebVtt(text);
            for (const cue of state.cues) cueOwners.delete(cue);
            state.cues = cues;
            for (const cue of cues) cueOwners.set(cue, track);
            record.readyState = HTMLTrackElement.LOADED;
            record.source = source;
            state.loading = false;
            element.dispatchEvent(new Event('load'));
            updateTextTracks(state.media);
        }).catch(() => {
            if (state.generation !== generation) return;
            record.readyState = HTMLTrackElement.ERROR;
            state.loading = false;
            element.dispatchEvent(new Event('error'));
        });
    };
    const updateTextTracks = media => {
        if (!media) return;
        const time = Number(mediaStateFor(media).currentTime) || 0;
        for (const track of textTracksFor(media)) {
            const state = textTrackStates.get(track);
            const next = state.mode === 'disabled' ? [] : state.cues.filter(cue =>
                cue.startTime <= time && time < cue.endTime);
            if (next.length === state.active.length && next.every((cue, index) => cue === state.active[index])) continue;
            const previous = state.active;
            state.active = next;
            for (const cue of previous) if (!next.includes(cue)) cue.dispatchEvent(new Event('exit'));
            for (const cue of next) if (!previous.includes(cue)) cue.dispatchEvent(new Event('enter'));
            track.dispatchEvent(new Event('cuechange'));
            state.element?.dispatchEvent(new Event('cuechange'));
        }
        const cues = textTracksFor(media).filter(track => {
            const state = textTrackStates.get(track);
            return state.mode === 'showing' && (track.kind === 'subtitles' || track.kind === 'captions');
        }).flatMap(track => textTrackStates.get(track).active.map(cue =>
            cue instanceof VTTCue ? {
                text: cue.getCueAsHTML().textContent.slice(0, 512),
                line: cue.snapToLines && Number.isInteger(cue.line) ?
                    Math.max(-32768, Math.min(32767, cue.line)) : null,
                linePercent: !cue.snapToLines && Number.isFinite(Number(cue.line)) ?
                    Math.max(0, Math.min(100, Math.round(Number(cue.line)))) : null,
                positionPercent: Number.isFinite(Number(cue.position)) ?
                    Math.max(0, Math.min(100, Math.round(Number(cue.position)))) : 50,
                sizePercent: Number.isFinite(Number(cue.size)) ?
                    Math.max(0, Math.min(100, Math.round(Number(cue.size)))) : 100,
                align: cue.align, positionAlign: cue.positionAlign
            } : null)).filter(cue => cue?.text).slice(0, 4);
        const serialized = JSON.stringify(cues);
        if (lastMediaCaptions.get(media) !== serialized) {
            lastMediaCaptions.set(media, serialized);
            mediaCommand(media, 0, 'caption', serialized);
        }
    };
    Object.assign(globalThis, { TextTrack, TextTrackList, TextTrackCue, TextTrackCueList,
        VTTCue, HTMLTrackElement });
