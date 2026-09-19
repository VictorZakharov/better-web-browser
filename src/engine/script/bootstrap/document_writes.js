    // The same tokenizer serves network insertion points and script-created input streams.
    // Author JS runs with no native host borrow; nested scripts do not drain microtasks.
    function pumpDocumentParser(target, stream) {
        for (;;) {
            const next = host(stream ? 'parserStreamStep' : 'parserWriteStep', nodeId(target));
            if (!next) break;
            if (next.changed) parserDomChanged(next.ids, next.mutations);
            if (next.customElement) constructParserElement(target, wrap(next.customElement));
            if (next.node) {
                const prepared = host('parserWritePrepare', nodeId(target), next.node);
                if (prepared) {
                    const previous = document._currentScript;
                    document._currentScript = wrap(next.node);
                    host('parserScriptEnter', nodeId(target));
                    try {
                        host('runParserScript', prepared.code, prepared.url);
                        host('parserWriteExecuted');
                    } catch (error) {
                        reportGlobalException(error, 'written script', windowObject, prepared.url);
                    } finally {
                        host('parserScriptLeave', nodeId(target));
                        document._currentScript = previous;
                    }
                }
            }
            if (next.ended) finishDocumentStream(target);
            if (next.done) break;
        }
    }
    function writeIntoActiveParser(target, text) {
        if (!host('parserWriteBegin', nodeId(target), text)) return false;
        try { pumpDocumentParser(target, false); }
        finally { host('parserWriteEnd', nodeId(target)); }
        if (host('documentStreamNeedsClose', nodeId(target))) pumpDocumentParser(target, true);
        return true;
    }
