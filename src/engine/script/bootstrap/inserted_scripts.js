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
    const isHtmlScript = node => {
        const id = nodeId(node);
        return host('nodeType', id) === 1 && host('namespaceUri', id) === htmlNamespace &&
            host('localName', id) === 'script';
    };
    function prepareInsertedScript(node) {
        if (!isHtmlScript(node)) return;
        const prepared = host('prepareInsertedScript', nodeId(node));
        if (!prepared) return;
        const previous = document._currentScript;
        const root = host('rootNode', nodeId(node), false);
        document._currentScript = root && host('shadowHost', root) ? null : node;
        host('parserScriptEnter', nodeId(document));
        try {
            runPreparedInlineScript(node, prepared);
        } catch (error) {
            reportGlobalException(error, 'inline script', windowObject, prepared.url);
        } finally {
            host('parserScriptLeave', nodeId(document));
            document._currentScript = previous;
        }
    }
    function runPreparedInlineScript(node, prepared) {
        const result = host('runParserScript', prepared.code, prepared.url, nodeId(node));
        if (result === true) {
            host('parserWriteExecuted');
        } else if (typeof result === 'string') {
            const report = JSON.parse(result);
            for (const violation of report.violations) {
                queuePolicyViolation(node, {
                    documentURI: report.documentUrl,
                    blockedURI: 'inline',
                    effectiveDirective: 'script-src-elem',
                    violatedDirective: 'script-src-elem',
                    originalPolicy: violation.originalPolicy,
                    sourceFile: report.documentUrl,
                    sample: violation.sample,
                    disposition: 'enforce',
                });
            }
        }
    }
    function scriptChildrenChanged(parent) {
        if (!isHtmlScript(parent)) return;
        if (scriptMutationBatch) scriptMutationBatch.push(() => prepareInsertedScript(parent));
        else prepareInsertedScript(parent);
    }
    function insertedScriptSteps(parent, roots) {
        const scripts = [];
        for (const root of roots) {
            // Snapshot before any script executes: removed later siblings are then checked
            // for connectivity at preparation, and newly inserted nodes run their own steps.
            for (const node of inclusiveElementDescendants(root))
                if (isHtmlScript(node)) scripts.push(node);
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
