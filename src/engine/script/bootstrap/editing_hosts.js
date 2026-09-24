    // HTML §6.8: the enumerated contenteditable state inherits through HTML
    // ancestors; designMode is per-document, not a global Window toggle.
    const designModeStates = new WeakMap();
    const editableState = element => {
        const raw = element.getAttribute('contenteditable');
        if (raw === null) return 'inherit';
        const value = raw.toLowerCase();
        if (value === '' || value === 'true') return 'true';
        if (value === 'false' || value === 'plaintext-only') return value;
        return 'inherit';
    };
    const editingHost = start => {
        let node = start;
        while (node && node instanceof Element) {
            if (node instanceof HTMLElement) {
                const state = editableState(node);
                if (state === 'false') return null;
                if (state === 'true' || state === 'plaintext-only') return node;
            }
            node = node.parentElement;
        }
        return document.designMode === 'on' ? document.body : null;
    };
    Object.defineProperties(HTMLElement.prototype, {
        spellcheck: {
            configurable: true, enumerable: true,
            get() {
                const value = this.getAttribute('spellcheck');
                if (value !== null) {
                    const state = value.toLowerCase();
                    if (state === '' || state === 'true') return true;
                    if (state === 'false') return false;
                }
                // Breeze's default is false-by-default, inherited by descendants.
                // The IDL value reflects the chosen behavior, not the availability
                // of a user's dictionary or a future platform checker.
                return this.parentElement instanceof HTMLElement && this.parentElement.spellcheck;
            },
            set(value) { this.setAttribute('spellcheck', Boolean(value) ? 'true' : 'false'); }
        },
        contentEditable: {
            configurable: true, enumerable: true,
            get() { return editableState(this); },
            set(value) {
                value = String(value).toLowerCase();
                if (!['true', 'false', 'plaintext-only', 'inherit'].includes(value))
                    throw new DOMException('Invalid contentEditable value', 'SyntaxError');
                if (value === 'inherit') this.removeAttribute('contenteditable');
                else this.setAttribute('contenteditable', value);
            }
        },
        isContentEditable: {
            configurable: true, enumerable: true,
            get() { return editingHost(this) !== null; }
        }
    });
    Object.defineProperty(Document.prototype, 'designMode', {
        configurable: true, enumerable: true,
        get() { return designModeStates.get(this) ?? 'off'; },
        set(value) {
            value = String(value).toLowerCase();
            if (value === 'on' || value === 'off') designModeStates.set(this, value);
        }
    });
