    // One decoded H.264 video track is available after metadata. Selection
    // changes the renderer's image source while the media clock and audio run.
    // https://html.spec.whatwg.org/multipage/media.html#videotracklist
    const videoTrackLists = new WeakMap();
    const videoTrackConstructionToken = {};

    class VideoTrack {
        constructor(token, media) {
            if (token !== videoTrackConstructionToken) throw new TypeError('Illegal constructor');
            this.__media = media;
            this.__selected = true;
        }
        get id() { return ''; }
        get kind() { return 'main'; }
        get label() { return ''; }
        get language() { return ''; }
        get selected() { return this.__selected; }
        set selected(value) {
            const selected = Boolean(value);
            if (this.__selected === selected) return;
            this.__selected = selected;
            const media = this.__media, list = videoTrackListFor(media);
            if (list.__track !== this) return;
            mediaCommand(media, 0, 'select-video', selected);
            queueMediaTask(() => list.dispatchEvent(markTrusted(new Event('change'))));
        }
    }

    class VideoTrackList extends EventTarget {
        constructor(token) {
            super();
            if (token !== videoTrackConstructionToken) throw new TypeError('Illegal constructor');
            this.__track = null;
        }
        get length() { return this.__track ? 1 : 0; }
        get selectedIndex() { return this.__track?.selected ? 0 : -1; }
        item(index) { return Number(index) === 0 ? this.__track : null; }
        getTrackById(id) {
            return this.__track && String(id) === this.__track.id ? this.__track : null;
        }
        *[Symbol.iterator]() { if (this.__track) yield this.__track; }
    }
    for (const type of ['addtrack', 'removetrack', 'change'])
        cueHandler(VideoTrackList.prototype, type);

    const videoTrackListFor = media => {
        let list = videoTrackLists.get(media);
        if (!list) {
            list = mediaTrackListProxy(new VideoTrackList(videoTrackConstructionToken));
            videoTrackLists.set(media, list);
        }
        return list;
    };
    Object.defineProperty(HTMLMediaElement.prototype, 'videoTracks', {
        enumerable: true, configurable: true, get() { return videoTrackListFor(this); }
    });
    globalThis.VideoTrack = VideoTrack;
    globalThis.VideoTrackList = VideoTrackList;

    const exposeDecodedVideoTrack = media => {
        const list = videoTrackListFor(media);
        if (list.__track) return;
        const track = new VideoTrack(videoTrackConstructionToken, media);
        list.__track = track;
        queueMediaTask(() => {
            if (list.__track === track)
                list.dispatchEvent(markTrusted(new TrackEvent('addtrack', { track })));
        });
    };
    const clearDecodedVideoTrack = media => {
        const list = videoTrackLists.get(media);
        if (!list?.__track) return;
        const track = list.__track;
        list.__track = null;
        queueMediaTask(() => list.dispatchEvent(markTrusted(new TrackEvent('removetrack', { track }))));
    };
