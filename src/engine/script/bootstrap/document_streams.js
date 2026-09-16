    function checkDynamicMarkupTarget(target) {
        if (!(target instanceof Document)) throw new TypeError('Illegal invocation');
        if (!host('isHtmlDocument', target.__id) || (target === document && throwOnDynamicMarkupInsertion))
            throw new DOMException('Dynamic markup insertion is not allowed for this document', 'InvalidStateError');
    }
    function openDocumentStream(target) {
        const removed = [...target.childNodes];
        const erased = host('documentOpen', target.__id);
        if (erased === null) return target;
        // Erase, rather than replacing, listener entries: an in-progress dispatch snapshot
        // must see each old listener's removed flag too. Detached unrelated nodes survive.
        const erase = node => {
            const storage = storageFor(node);
            for (const listener of [...(listenerStore.get(storage) || [])]) removeListener(storage, listener);
            eventHandlerStore.delete(storage);
        };
        for (const id of erased.split(',')) {
            const node = cache.get(Number(id));
            if (node) erase(node);
        }
        if (target.defaultView) erase(target.defaultView);
        documentReadiness.set(target, 'loading');
        target.activeElement = null;
        markChildCollectionsChanged(target);
        if (removed.length) queueMutationRecord(target, 'childList', { removedNodes: removed });
        refreshWindowNamedProperties();
        for (const child of removed) disconnectCustomElementTree(child);
        return target;
    }
    function finishDocumentStream(target) {
        if (target.readyState !== 'loading') return;
        host('documentStreamFinished', target.__id);
        documentReadiness.set(target, 'interactive');
        target.dispatchEvent(markTrusted(new Event('readystatechange')));
        if (!target.defaultView) documentReadiness.set(target, 'complete');
    }
    globalThis.__resumeDocumentStream = id => pumpDocumentParser(wrap(id), true);
