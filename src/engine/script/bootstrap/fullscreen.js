    let fullscreenElement = null;
    let nextFullscreenRequest = 1;
    const pendingFullscreenRequests = new Map();

    const queueFullscreenRequest = (element, enter) => new Promise((resolve, reject) => {
        const requestId = nextFullscreenRequest++;
        pendingFullscreenRequests.set(requestId, { element, enter, resolve, reject });
        host('fullscreenRequest', requestId, enter);
    });

    Element.prototype.requestFullscreen = function(_options) {
        if (!this.isConnected)
            return Promise.reject(new DOMException('Element is not connected', 'TypeError'));
        if (this.ownerDocument !== document)
            return Promise.reject(new DOMException('Element belongs to another document', 'TypeError'));
        if (!document.fullscreenEnabled)
            return Promise.reject(new TypeError('Fullscreen is not available in this document'));
        return queueFullscreenRequest(this, true);
    };
    Object.defineProperties(Document.prototype, {
        fullscreenEnabled: { configurable: true, enumerable: true, get: () => host('fullscreenSupported') },
        fullscreenElement: { configurable: true, enumerable: true, get: () => fullscreenElement }
    });
    Document.prototype.exitFullscreen = function() {
        if (!fullscreenElement) return Promise.resolve();
        return queueFullscreenRequest(fullscreenElement, false);
    };
    defineEventHandler(Document.prototype, null, 'fullscreenchange');
    defineEventHandler(Document.prototype, null, 'fullscreenerror');
    defineEventHandler(Element.prototype, null, 'fullscreenchange');
    defineEventHandler(Element.prototype, null, 'fullscreenerror');

    const applyFullscreenResponse = input => {
        const requestId = Number(input.requestId) || 0;
        const pending = requestId ? pendingFullscreenRequests.get(requestId) : null;
        if (requestId) pendingFullscreenRequests.delete(requestId);
        const target = pending?.element || fullscreenElement || document;
        if (input.disposition === 'entered' && pending?.enter) {
            if (fullscreenElement && fullscreenElement !== pending.element)
                host('fullscreenSet', nodeId(fullscreenElement), false);
            fullscreenElement = pending.element;
            host('fullscreenSet', nodeId(fullscreenElement), true);
            pending.resolve();
            target.dispatchEvent(markTrusted(new Event('fullscreenchange', { bubbles: true })));
            return true;
        }
        if (input.disposition === 'exited') {
            if (fullscreenElement) host('fullscreenSet', nodeId(fullscreenElement), false);
            fullscreenElement = null;
            pending?.resolve();
            target.dispatchEvent(markTrusted(new Event('fullscreenchange', { bubbles: true })));
            return true;
        }
        const error = new DOMException('Fullscreen request was denied', 'NotAllowedError');
        pending?.reject(error);
        target.dispatchEvent(markTrusted(new Event('fullscreenerror', { bubbles: true })));
        return false;
    };
