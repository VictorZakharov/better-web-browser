    // HTML live checkedness, radio groups, and legacy click activation.
    // https://html.spec.whatwg.org/multipage/input.html#dom-input-checked
    const resettingForms = new WeakSet();
    Object.defineProperties(HTMLInputElement.prototype, {
        checked: { configurable: true, get() { return host('inputChecked', nodeId(this)); },
            set(value) { host('inputSetChecked', nodeId(this), !!value); } },
        defaultChecked: { configurable: true, get() { return this.hasAttribute('checked'); },
            set(value) { this.toggleAttribute('checked', !!value); } },
        indeterminate: { configurable: true, get() { return host('inputIndeterminate', nodeId(this)); },
            set(value) { host('inputSetIndeterminate', nodeId(this), !!value); } }
    });
    function radioGroup(input) {
        return String(host('inputRadioGroup', nodeId(input))).split(',').filter(Boolean).map(id => wrap(Number(id)));
    }
    function checkableType(input) {
        return input instanceof HTMLInputElement && /^(checkbox|radio)$/i.test(input.type);
    }
    function prepareCheckableActivation(target, event) {
        if (!(event instanceof MouseEvent) || event.type !== 'click' || !checkableType(target) || target.matches(':disabled')) return null;
        const old = { target, checked: target.checked, indeterminate: target.indeterminate,
            radio: target.type.toLowerCase() === 'radio', previous: null };
        if (old.radio) old.previous = target.checked ? target : radioGroup(target).find(input => input.checked) || null;
        target.checked = old.radio || !old.checked;
        if (!old.radio) target.indeterminate = false;
        return old;
    }
    function finishCheckableActivation(old, event) {
        if (!old) return;
        const input = old.target;
        if (event.defaultPrevented) {
            if (input.type.toLowerCase() === 'checkbox') {
                input.checked = old.checked; input.indeterminate = old.indeterminate;
            } else if (input.type.toLowerCase() === 'radio') {
                if (old.previous && (old.previous === input || radioGroup(input).includes(old.previous))) old.previous.checked = true;
                else input.checked = false;
            }
        } else if (input.isConnected && (!old.radio || !old.checked)) {
            input.dispatchEvent(markTrusted(new Event('input', { bubbles: true, composed: true })));
            input.dispatchEvent(markTrusted(new Event('change', { bubbles: true })));
        }
    }
    function activateLabel(target, event) {
        if (!(event instanceof MouseEvent) || event.type !== 'click' || event.defaultPrevented) return;
        for (let node = target; node instanceof Element; node = node.parentElement) {
            if (node instanceof HTMLLabelElement) {
                const control = node.control;
                if (control && !control.contains(target) && !control.matches(':disabled')) {
                    control.focus();
                    const click = new MouseEvent('click', { bubbles: true, cancelable: true, composed: true });
                    control.dispatchEvent(event.isTrusted ? markTrusted(click) : click);
                }
                return;
            }
            if (/^(a|button|input|select|textarea|label)$/.test(node.localName)) return;
        }
    }
    HTMLFormElement.prototype.reset = function() {
        if (resettingForms.has(this)) return;
        resettingForms.add(this);
        try {
            if (!this.dispatchEvent(new Event('reset', { bubbles: true, cancelable: true }))) return;
            host('formResetControls', nodeId(this));
        } finally { resettingForms.delete(this); }
    };
