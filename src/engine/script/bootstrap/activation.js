    // Activation is a DOM event default, including synthetic MouseEvent clicks. Trusted
    // links remain renderer-owned (fragment scrolling, modifiers and tab disposition).
    const imageSubmitCoordinates = new WeakMap();
    Object.defineProperty(globalThis, '__imageSubmitCoordinates', {
        configurable: true, value: submitter => imageSubmitCoordinates.get(submitter) || [0, 0]
    });
    function activateNavigation(target, event) {
        if (!(event instanceof MouseEvent) || event.type !== 'click' || event.defaultPrevented || event.button !== 0) return;
        const element = target instanceof Element ? target : target.parentElement;
        const activation = element?.closest('a[href],button,input');
        if (!activation || activation.matches(':disabled')) return;
        if (activation.localName === 'a') {
            if (!event.isTrusted) {
                const url = hyperlinkUrl(activation);
                if (!url) return;
                const target = activation.getAttribute('target') ?? document.querySelector('base[target]')?.getAttribute('target') ?? '';
                if (!target || /^(?:_self|_top|_parent)$/i.test(target)) navigateLocation(url, false);
                else host('navigateRequest', url, JSON.stringify({ target }));
            }
        } else if (activation.form && /^(submit|image)$/i.test(activation.type)) {
            if (activation.type === 'image') {
                const rect = activation.getBoundingClientRect();
                const x = Math.max(0, Math.floor(event.clientX - rect.left));
                const y = Math.max(0, Math.floor(event.clientY - rect.top));
                imageSubmitCoordinates.set(activation, [x, y]);
            }
            try { HTMLFormElement.prototype.requestSubmit.call(activation.form, activation); }
            finally { imageSubmitCoordinates.delete(activation); }
        } else if (activation.form && activation.type === 'reset') {
            HTMLFormElement.prototype.reset.call(activation.form);
        }
    }
    function implicitSubmission(target) {
        if (!(target instanceof HTMLInputElement) || !target.form) return;
        // Enter commits a user edit before the submission validates it.
        const entry = focusCommitted.get(target);
        if (entry?.edited && isCommitTarget(target) && target.isConnected) {
            focusCommitted.delete(target);
            target.dispatchEvent(markTrusted(new Event('change', { bubbles: true })));
        }
        const form = target.form;
        const controls = Array.from(form.getRootNode().querySelectorAll('input,button')).filter(control => control.form === form);
        const button = controls.find(control => /^(submit|image)$/i.test(control.type));
        if (button) {
            if (!button.matches(':disabled')) button.dispatchEvent(markTrusted(new MouseEvent('click', {
                bubbles: true, cancelable: true, composed: true
            })));
            return;
        }
        const blocks = control => control instanceof HTMLInputElement &&
            /^(?:text|search|tel|url|email|password|date|month|week|time|datetime-local|number)?$/i.test(control.type);
        if (blocks(target) && controls.filter(blocks).length <= 1) HTMLFormElement.prototype.requestSubmit.call(form);
    }
