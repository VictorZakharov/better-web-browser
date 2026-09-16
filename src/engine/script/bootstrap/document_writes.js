    // HTML's write algorithm feeds the current tokenizer, stopping at this call's insertion
    // point. It must not consume the network tail or run a microtask checkpoint on a nested
    // script's return while its caller is still on the JavaScript stack.
    // https://html.spec.whatwg.org/multipage/dynamic-markup-insertion.html#document.write()
    function writeIntoActiveParser(target, text) {
        if (!host('parserWriteBegin', target.__id, text)) return false;
        try {
            for (;;) {
                const next = host('parserWriteStep');
                if (!next) break;
                if (next.changed) parserDomChanged(next.ids);
                if (next.node) {
                    const prepared = host('parserWritePrepare', next.node);
                    if (prepared) {
                        const previous = document._currentScript;
                        document._currentScript = wrap(next.node);
                        try {
                            host('runParserScript', prepared.code, prepared.url);
                            host('parserWriteExecuted');
                        } catch (error) {
                            reportGlobalException(error, 'written script', windowObject, prepared.url);
                        } finally {
                            document._currentScript = previous;
                        }
                    }
                }
                if (next.done) break;
            }
        } finally {
            host('parserWriteEnd');
        }
        return true;
    }
