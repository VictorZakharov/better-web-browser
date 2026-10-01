    const imageTrackState = value => {
        const state = imageTrackStates.get(value);
        if (!state) throw new TypeError('Illegal ImageTrack receiver');
        return state;
    };
    const imageTrackListState = value => {
        const state = imageTrackListStates.get(value);
        if (!state) throw new TypeError('Illegal ImageTrackList receiver');
        return state;
    };
    class ImageTrack {
        constructor(token, state) {
            if (token !== imageTrackToken) throw new TypeError('Illegal constructor');
            imageTrackStates.set(this, state);
        }
        get animated() { return imageTrackState(this).animated; }
        get frameCount() { return imageTrackState(this).frameCount; }
        get repetitionCount() { return imageTrackState(this).repetitions; }
        get selected() { return imageTrackState(this).selected; }
        set selected(value) {
            const track = imageTrackState(this), selected = Boolean(value);
            if (selected === track.selected) return;
            const decoder = track.decoder;
            if (decoder.closed) return;
            cancelImageDecodes(decoder, frameError('Image track selection changed', 'AbortError'));
            for (const item of decoder.trackItems) imageTrackState(item).selected = false;
            track.selected = selected;
            decoder.selectedIndex = selected ? decoder.trackItems.indexOf(this) : -1;
        }
    }
    class ImageTrackList {
        constructor(token, decoder) {
            if (token !== imageTrackListToken) throw new TypeError('Illegal constructor');
            imageTrackListStates.set(this, decoder);
            return new Proxy(this, {
                get(target, key, receiver) {
                    if (typeof key === 'string' && /^(0|[1-9][0-9]*)$/.test(key))
                        return decoder.trackItems[Number(key)];
                    return Reflect.get(target, key, receiver);
                },
                has(target, key) {
                    if (typeof key === 'string' && /^(0|[1-9][0-9]*)$/.test(key))
                        return Number(key) < decoder.trackItems.length;
                    return Reflect.has(target, key);
                },
                set(target, key, value, receiver) {
                    if (typeof key === 'string' && /^(0|[1-9][0-9]*)$/.test(key)) return false;
                    return Reflect.set(target, key, value, receiver);
                },
                ownKeys(target) {
                    return [...decoder.trackItems.map((_, index) => String(index)), ...Reflect.ownKeys(target)];
                },
                getOwnPropertyDescriptor(target, key) {
                    if (typeof key === 'string' && /^(0|[1-9][0-9]*)$/.test(key) && Number(key) < decoder.trackItems.length)
                        return {value: decoder.trackItems[Number(key)], writable:false, enumerable:true, configurable:true};
                    return Reflect.getOwnPropertyDescriptor(target, key);
                },
                defineProperty(target, key, descriptor) {
                    if (typeof key === 'string' && /^(0|[1-9][0-9]*)$/.test(key)) return false;
                    return Reflect.defineProperty(target, key, descriptor);
                },
                deleteProperty(target, key) {
                    if (typeof key === 'string' && /^(0|[1-9][0-9]*)$/.test(key) && Number(key) < decoder.trackItems.length) return false;
                    return Reflect.deleteProperty(target, key);
                }
            });
        }
        get length() { return imageTrackListState(this).trackItems.length; }
        get selectedIndex() { return imageTrackListState(this).selectedIndex; }
        get selectedTrack() {
            const state = imageTrackListState(this);
            return state.trackItems[state.selectedIndex] ?? null;
        }
        get ready() { return imageTrackListState(this).ready.promise; }
    }
    const createImageTrackList = state => {
        const tracks = new ImageTrackList(imageTrackListToken, state);
        // Getters execute with the Proxy receiver, so both objects are branded.
        imageTrackListStates.set(tracks, state);
        return tracks;
    };
    const setImageTracks = (state, result) => {
        const animated = new ImageTrack(imageTrackToken, {decoder: state,
            animated: result.animated, frameCount: result.frameCount,
            repetitions: result.repetitions === null ? Infinity : result.repetitions,
            selected: false});
        state.trackItems = result.poster ? [new ImageTrack(imageTrackToken, {decoder:state,
            animated:false, frameCount:1, repetitions:0, selected:false}), animated] : [animated];
        state.selectedIndex = result.poster && state.options.preferAnimation !== false ? 1 : 0;
        imageTrackState(state.trackItems[state.selectedIndex]).selected = true;
        state.ready.resolve();
    };
    for (const [constructor, tag] of [[ImageTrack, 'ImageTrack'], [ImageTrackList, 'ImageTrackList']])
        Object.defineProperty(constructor.prototype, Symbol.toStringTag, {value: tag, configurable: true});
