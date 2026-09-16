    // The same tokenizer serves network insertion points and script-created input streams.
    // Author JS runs with no native host borrow; nested scripts do not drain microtasks.
    function pumpDocumentParser(target, stream) {
        for (;;) {
            const next = host(stream ? 'parserStreamStep' : 'parserWriteStep', target.__id);
            if (!next) break;
            if (next.changed) parserDomChanged(next.ids);
            if (next.node) {
                const prepared = host('parserWritePrepare', target.__id, next.node);
                if (prepared) {
                    const previous = document._currentScript;
                    document._currentScript = wrap(next.node);
                    host('parserScriptEnter', target.__id);
                    try {
                        host('runParserScript', prepared.code, prepared.url);
                        host('parserWriteExecuted');
                    } catch (error) {
                        reportGlobalException(error, 'written script', windowObject, prepared.url);
                    } finally {
                        host('parserScriptLeave', target.__id);
                        document._currentScript = previous;
                    }
                }
            }
            if (next.ended) finishDocumentStream(target);
            if (next.done) break;
        }
    }
    function writeIntoActiveParser(target, text) {
        if (!host('parserWriteBegin', target.__id, text)) return false;
        try { pumpDocumentParser(target, false); }
        finally { host('parserWriteEnd', target.__id); }
        if (host('documentStreamNeedsClose', target.__id)) pumpDocumentParser(target, true);
        return true;
    }
