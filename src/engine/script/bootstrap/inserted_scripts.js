    // DOM post-connection steps run after the whole insertion and its mutation records,
    // but before the inserting call returns. Reentrant insertions get their own steps.
    // https://html.spec.whatwg.org/multipage/scripting.html#script-processing-model
    let scriptMutationBatch = null;
    function withScriptMutationBatch(callback) {
        if (scriptMutationBatch) return callback();
        const pending = [];
        scriptMutationBatch = pending;
        try { return callback(); }
        finally {
            scriptMutationBatch = null;
            for (const step of pending) step();
        }
    }
    function prepareInsertedScript(node) {
        if (!(node instanceof HTMLScriptElement)) return;
        const prepared = host('prepareInsertedScript', nodeId(node));
        if (!prepared) return;
        const previous = document._currentScript;
        document._currentScript = node.getRootNode() instanceof ShadowRoot ? null : node;
        host('parserScriptEnter', nodeId(document));
        try {
            host('runParserScript', prepared.code, prepared.url);
            host('parserWriteExecuted');
        } catch (error) {
            reportGlobalException(error, 'inline script', windowObject, prepared.url);
        } finally {
            host('parserScriptLeave', nodeId(document));
            document._currentScript = previous;
        }
    }
    function scriptChildrenChanged(parent) {
        if (!(parent instanceof HTMLScriptElement)) return;
        if (scriptMutationBatch) scriptMutationBatch.push(() => prepareInsertedScript(parent));
        else prepareInsertedScript(parent);
    }
    function insertedScriptSteps(parent, roots) {
        const scripts = [];
        for (const root of roots) {
            // Snapshot before any script executes: removed later siblings are then checked
            // for connectivity at preparation, and newly inserted nodes run their own steps.
            for (const node of inclusiveElementDescendants(root))
                if (node instanceof HTMLScriptElement) scripts.push(node);
        }
        const run = () => {
            for (const script of scripts) prepareInsertedScript(script);
            // DOM insertion finishes descendant post-connection steps before the
            // parent's children-changed steps (including a script parent).
            prepareInsertedScript(parent);
        };
        if (scriptMutationBatch) scriptMutationBatch.push(run);
        else run();
    }
