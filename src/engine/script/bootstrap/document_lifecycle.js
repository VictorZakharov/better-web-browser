    // HTML's readiness transitions are not writable properties, nor a resource-idle heuristic.
    // https://html.spec.whatwg.org/multipage/dom.html#current-document-readiness
    Object.defineProperty(Document.prototype, 'readyState', {
        configurable: true, enumerable: true,
        get() {
            if (!documentReadiness.has(this)) throw new TypeError('Illegal invocation');
            return documentReadiness.get(this);
        }
    });
    const setDocumentReadiness = state => {
        if (document.readyState === state) return;
        documentReadiness.set(document, state);
        document.dispatchEvent(markTrusted(new Event('readystatechange')));
    };
    windowObject.__setDocumentInteractive = () => setDocumentReadiness('interactive');
    windowObject.__dispatchDOMContentLoaded = () =>
        document.dispatchEvent(markTrusted(new Event('DOMContentLoaded', { bubbles: true })));
    windowObject.__setDocumentComplete = () => setDocumentReadiness('complete');
    windowObject.__dispatchWindowLoad = () => {
        const event = markTrusted(new Event('load'));
        // Legacy target override changes target, not the Window-only propagation path.
        // https://html.spec.whatwg.org/multipage/parsing.html#the-end
        legacyEventTargets.set(event, document);
        windowObject.dispatchEvent(event);
    };
