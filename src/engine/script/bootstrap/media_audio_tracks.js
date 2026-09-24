    // The decoder currently accepts one synchronized AAC stream per loaded
    // resource. Keep the list empty before metadata and tie enabled to the
    // actual mixer volume, rather than advertising a decorative track list.
    // https://html.spec.whatwg.org/multipage/media.html#audiotracklist
    const audioTrackLists = new WeakMap();
    const audioTrackConstructionToken = {};

    class AudioTrack {
        constructor(token, media) {
            if (token !== audioTrackConstructionToken) throw new TypeError('Illegal constructor');
            this.__media = media;
            this.__enabled = true;
        }
        get id() { return ''; }
        get kind() { return 'main'; }
        get label() { return ''; }
        get language() { return ''; }
        get enabled() { return this.__enabled; }
        set enabled(value) {
            const enabled = Boolean(value);
            if (this.__enabled === enabled) return;
            this.__enabled = enabled;
            const media = this.__media;
            const state = mediaStateFor(media);
            if (audioTrackListFor(media).__track !== this) return;
            state.audioTrackEnabled = enabled;
            mediaCommand(media, 0, 'configure', effectiveVolumeMillis(state));
            queueMediaTask(() => audioTrackListFor(media).dispatchEvent(markTrusted(new Event('change'))));
        }
    }

    class AudioTrackList extends EventTarget {
        constructor(token) {
            super();
            if (token !== audioTrackConstructionToken) throw new TypeError('Illegal constructor');
            this.__track = null;
        }
        get length() { return this.__track ? 1 : 0; }
        item(index) { return Number(index) === 0 ? this.__track : null; }
        getTrackById(id) {
            return this.__track && String(id) === this.__track.id ? this.__track : null;
        }
        *[Symbol.iterator]() { if (this.__track) yield this.__track; }
    }
    for (const type of ['addtrack', 'removetrack', 'change'])
        cueHandler(AudioTrackList.prototype, type);

    const audioTrackListFor = media => {
        let list = audioTrackLists.get(media);
        if (!list) {
            list = mediaTrackListProxy(new AudioTrackList(audioTrackConstructionToken));
            audioTrackLists.set(media, list);
        }
        return list;
    };
    Object.defineProperty(HTMLMediaElement.prototype, 'audioTracks', {
        enumerable: true, configurable: true, get() { return audioTrackListFor(this); }
    });

    class TrackEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            const track = init == null ? null : init.track ?? null;
            if (track != null && !(track instanceof AudioTrack) &&
                !(track instanceof VideoTrack) && !(track instanceof TextTrack))
                throw new TypeError('Expected a media track');
            Object.defineProperty(this, 'track', { value: track, enumerable: true });
        }
    }
    globalThis.TrackEvent = TrackEvent;
    globalThis.AudioTrack = AudioTrack;
    globalThis.AudioTrackList = AudioTrackList;

    const exposeDecodedAudioTrack = media => {
        const list = audioTrackListFor(media);
        if (list.__track) return;
        const track = new AudioTrack(audioTrackConstructionToken, media);
        list.__track = track;
        mediaStateFor(media).audioTrackEnabled = true;
        queueMediaTask(() => {
            if (list.__track === track)
                list.dispatchEvent(markTrusted(new TrackEvent('addtrack', { track })));
        });
    };
    const clearDecodedAudioTrack = media => {
        const list = audioTrackLists.get(media);
        if (!list?.__track) return;
        const track = list.__track;
        list.__track = null;
        queueMediaTask(() => list.dispatchEvent(markTrusted(new TrackEvent('removetrack', { track }))));
    };
