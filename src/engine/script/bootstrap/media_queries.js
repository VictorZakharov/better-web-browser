    // CSSOM View gives each document a live, creation-ordered list of MediaQueryList objects.
    // The retained realm owns this bounded list, so navigation drops every listener at once.
    // https://drafts.csswg.org/cssom-view/#mediaquerylist
    const MAX_MEDIA_QUERY_LISTS = 4096;
    const mediaQueryListToken = {};
    const mediaQueryListState = new WeakMap();
    const trackedMediaQueryLists = [];

    class MediaQueryListEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            Object.defineProperties(this, {
                matches: { enumerable: true, value: !!init.matches },
                media: {
                    enumerable: true,
                    value: init.media === undefined ? '' : String(init.media)
                }
            });
        }
    }

    class MediaQueryList extends EventTarget {
        constructor(token, query) {
            super();
            if (token !== mediaQueryListToken) throw new TypeError('Illegal constructor');
            const media = host('mediaSerialize', query);
            mediaQueryListState.set(this, {
                media,
                matches: !!host('mediaMatches', media)
            });
        }
        get media() { return mediaQueryListState.get(this).media; }
        get matches() { return mediaQueryListState.get(this).matches; }
        addListener(callback) { this.addEventListener('change', callback); }
        removeListener(callback) { this.removeEventListener('change', callback); }
    }
    defineEventHandler(MediaQueryList.prototype, null, 'change');
    Object.defineProperty(MediaQueryList.prototype, Symbol.toStringTag,
        { configurable: true, value: 'MediaQueryList' });
    Object.defineProperty(MediaQueryListEvent.prototype, Symbol.toStringTag,
        { configurable: true, value: 'MediaQueryListEvent' });

    windowObject.MediaQueryList = MediaQueryList;
    windowObject.MediaQueryListEvent = MediaQueryListEvent;
    windowObject.matchMedia = function(query) {
        if (trackedMediaQueryLists.length >= MAX_MEDIA_QUERY_LISTS) {
            throw new DOMException(
                'This document created too many live media-query lists',
                'QuotaExceededError'
            );
        }
        const list = new MediaQueryList(mediaQueryListToken, String(query));
        trackedMediaQueryLists.push(list);
        return list;
    };

    // Update every matches state before invoking any callback. This is observable when two
    // equivalent lists inspect one another from their change listeners.
    const prepareMediaQueryChanges = () => {
        const changed = [];
        for (const list of trackedMediaQueryLists) {
            const state = mediaQueryListState.get(list);
            const matches = !!host('mediaMatches', state.media);
            if (matches === state.matches) continue;
            state.matches = matches;
            changed.push(list);
        }
        return changed;
    };
    const dispatchMediaQueryChanges = changed => {
        for (const list of changed) {
            const state = mediaQueryListState.get(list);
            list.dispatchEvent(markTrusted(new MediaQueryListEvent('change', state)));
        }
    };
