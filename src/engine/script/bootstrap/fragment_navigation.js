    class HashChangeEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            Object.defineProperties(this, {
                oldURL: { value: init?.oldURL === undefined ? '' : String(init.oldURL), enumerable: true },
                newURL: { value: init?.newURL === undefined ? '' : String(init.newURL), enumerable: true }
            });
        }
    }
    windowObject.HashChangeEvent = HashChangeEvent;
    function navigateLocation(value, replace) {
        const oldURL = currentUrl;
        const result = host('fragmentNavigation', String(value), !!replace);
        if (result === null) {
            currentUrl = host('navigate', String(value), !!replace);
            return;
        }
        currentUrl = result[0];
        if (result[1] !== null && setViewportScrollOffsets(viewportScrollX, result[1]))
            queueViewportScrollEvent();
        if (currentUrl !== oldURL) {
            const newURL = currentUrl;
            historyState = null;
            if (!replace) advanceHistoryIndex();
            firePopState();
            queueTimer(() => windowObject.dispatchEvent(markTrusted(
                new HashChangeEvent('hashchange', { oldURL, newURL })
            )), 0, false, [], 'fragment hashchange');
        }
    }
