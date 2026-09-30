    class HTMLTextAreaElement extends HTMLElement {
        get placeholder() { return this.getAttribute('placeholder') || ''; }
        set placeholder(value) { this.setAttribute('placeholder', value); }
        get form() { return associatedForm(this); }
        get value() { return host('textareaValue', nodeId(this)); }
        set value(value) {
            const oldValue = host('textareaValue', nodeId(this));
            host('textareaSetValue', nodeId(this), String(value));
            textSelectionValueChanged(this, oldValue, host('textareaValue', nodeId(this)));
        }
        get defaultValue() { return this.textContent; }
        set defaultValue(value) {
            this.textContent = String(value);
            reconcileTextSelectionValue(this);
        }
        get minLength() { return reflectedInteger(this, 'minlength', -1); }
        set minLength(value) { this.setAttribute('minlength', String(Math.trunc(Number(value)))); }
        get maxLength() { return reflectedInteger(this, 'maxlength', -1); }
        set maxLength(value) { this.setAttribute('maxlength', String(Math.trunc(Number(value)))); }
        get wrap() { return (this.getAttribute('wrap') || 'soft').toLowerCase() === 'hard' ? 'hard' : 'soft'; }
        set wrap(value) { this.setAttribute('wrap', value); }
        get required() { return this.hasAttribute('required'); }
        set required(value) { this.toggleAttribute('required', !!value); }
        get readOnly() { return this.hasAttribute('readonly'); }
        set readOnly(value) { this.toggleAttribute('readonly', !!value); }
        get labels() { return labelsFor(this); }
        get willValidate() { return host('controlWillValidate', nodeId(this)); }
        get validity() { return validityFor(this); }
        get validationMessage() { return validationMessageFor(this); }
        setCustomValidity(message) { host('controlSetCustomValidity', nodeId(this), String(message)); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return reportControlValidity(this); }
        get textLength() { return host('textareaValue', nodeId(this)).length; }
        get selectionStart() { return textSelectionStart(this); }
        set selectionStart(value) { setTextSelectionStart(this, value); }
        get selectionEnd() { return textSelectionEnd(this); }
        set selectionEnd(value) { setTextSelectionEnd(this, value); }
        get selectionDirection() { return textSelectionDirectionValue(this); }
        set selectionDirection(value) { setTextSelectionDirection(this, value); }
        setSelectionRange(start, end, direction = 'none') {
            if (arguments.length < 2) throw new TypeError('setSelectionRange requires start and end');
            setTextSelectionRange(this, start, end, direction);
        }
        setRangeText(replacement, start, end, selectionMode = 'preserve') {
            setTextRangeText(this, replacement, start, end, selectionMode, arguments.length);
        }
        select() { selectTextControl(this); }
    }
