    class HTMLInputElement extends HTMLElement {
        get alt() { return this.getAttribute('alt') || ''; }
        set alt(value) { this.setAttribute('alt', String(value)); }
        get width() { return this.type === 'image' ? imageDimension(this, 'width') : 0; }
        set width(value) { setImageDimension(this, 'width', value); }
        get height() { return this.type === 'image' ? imageDimension(this, 'height') : 0; }
        set height(value) { setImageDimension(this, 'height', value); }
        // The type IDL attribute is limited to known values; missing and invalid
        // content values select Text, including for libraries deciding how to edit it.
        // https://html.spec.whatwg.org/multipage/input.html#attr-input-type
        get type() {
            const value = (this.getAttribute('type') || '').toLowerCase();
            return ['hidden', 'text', 'search', 'tel', 'url', 'email', 'password', 'date',
                'month', 'week', 'time', 'datetime-local', 'number', 'range', 'color',
                'checkbox', 'radio', 'file', 'submit', 'image', 'reset', 'button'].includes(value) ? value : 'text';
        }
        set type(value) { this.setAttribute('type', value); }
        // Live values live in native control state; defaults stay attributes.
        // https://html.spec.whatwg.org/multipage/input.html#dom-input-value
        get value() {
            switch (this.type) {
                case 'checkbox': case 'radio':
                    return this.getAttribute('value') ?? 'on';
                default:
                    return host('inputValue', nodeId(this));
            }
        }
        set value(value) {
            value = String(value);
            switch (this.type) {
                case 'file':
                    if (value !== '') throw new DOMException(
                        'File inputs may only be set to the empty string', 'InvalidStateError');
                    clearInputFileSelection(this);
                    break;
                case 'checkbox': case 'radio':
                    this.setAttribute('value', value);
                    break;
                default:
                    host('inputSetValue', nodeId(this), value);
                    this.setSelectionRange(this.value.length, this.value.length);
                    refreshPatternVerdict(this);
                    break;
            }
        }
        get defaultValue() { return this.getAttribute('value') ?? ''; }
        set defaultValue(value) { this.setAttribute('value', String(value)); }
        get placeholder() { return this.getAttribute('placeholder') || ''; }
        set placeholder(value) { this.setAttribute('placeholder', value); }
        get form() { return associatedForm(this); }
        get selectionStart() { return this.__selectionStart ?? 0; }
        set selectionStart(value) { this.__selectionStart = Math.max(0, Number(value) || 0); }
        get selectionEnd() { return this.__selectionEnd ?? this.value.length; }
        set selectionEnd(value) { this.__selectionEnd = Math.max(0, Number(value) || 0); }
        get selectionDirection() { return this.__selectionDirection || 'none'; }
        set selectionDirection(value) {
            value = String(value);
            this.__selectionDirection = value === 'forward' || value === 'backward' ? value : 'none';
        }
        setSelectionRange(start, end, direction = 'none') {
            this.selectionStart = start;
            this.selectionEnd = Math.max(this.selectionStart, Number(end) || 0);
            this.selectionDirection = direction;
        }
        select() { this.setSelectionRange(0, this.value.length); }
        get list() {
            const id = this.getAttribute('list');
            const candidate = id ? document.getElementById(id) : null;
            return candidate instanceof HTMLDataListElement ? candidate : null;
        }
        get min() { return this.getAttribute('min') || ''; }
        set min(value) { this.setAttribute('min', value); }
        get max() { return this.getAttribute('max') || ''; }
        set max(value) { this.setAttribute('max', value); }
        get step() { return this.getAttribute('step') || ''; }
        set step(value) { this.setAttribute('step', value); }
        get pattern() { return this.getAttribute('pattern') || ''; }
        set pattern(value) { this.setAttribute('pattern', value); }
        get required() { return this.hasAttribute('required'); }
        set required(value) { this.toggleAttribute('required', !!value); }
        get readOnly() { return this.hasAttribute('readonly'); }
        set readOnly(value) { this.toggleAttribute('readonly', !!value); }
        get autofocus() { return this.hasAttribute('autofocus'); }
        set autofocus(value) { this.toggleAttribute('autofocus', !!value); }
        get autocomplete() { return this.getAttribute('autocomplete') || ''; }
        set autocomplete(value) { this.setAttribute('autocomplete', value); }
        get multiple() { return this.hasAttribute('multiple'); }
        set multiple(value) { this.toggleAttribute('multiple', !!value); }
        get dirName() { return this.getAttribute('dirname') || ''; }
        set dirName(value) { this.setAttribute('dirname', value); }
        get formAction() {
            const value = this.getAttribute('formaction');
            return value == null ? '' : host('resolveUrl', value);
        }
        set formAction(value) { this.setAttribute('formaction', value); }
        get formEnctype() { return this.getAttribute('formenctype') || ''; }
        set formEnctype(value) { this.setAttribute('formenctype', value); }
        get formMethod() { return this.getAttribute('formmethod') || ''; }
        set formMethod(value) { this.setAttribute('formmethod', value); }
        get formNoValidate() { return this.hasAttribute('formnovalidate'); }
        set formNoValidate(value) { this.toggleAttribute('formnovalidate', !!value); }
        get formTarget() { return this.getAttribute('formtarget') || ''; }
        set formTarget(value) { this.setAttribute('formtarget', value); }
        get labels() { return labelsFor(this); }
        get willValidate() { return host('controlWillValidate', nodeId(this)); }
        get validity() { return validityFor(this); }
        get validationMessage() { return validationMessageFor(this); }
        setCustomValidity(message) { host('controlSetCustomValidity', nodeId(this), String(message)); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return reportControlValidity(this); }
        get valueAsNumber() {
            return inputNumericDateTypes.has(this.type)
                ? inputNumberFromValue(this.type, this.value)
                : host('controlValueAsNumber', nodeId(this));
        }
        set valueAsNumber(value) {
            if (inputNumericDateTypes.has(this.type)) {
                const number = Number(value);
                if (!Number.isFinite(number) && !Number.isNaN(number))
                    throw new TypeError('Value is not a finite number.');
                this.value = inputValueFromNumber(this.type, number);
                return;
            }
            const result = JSON.parse(host('controlSetValueAsNumber', nodeId(this), Number(value)));
            if (result.status === 'type') throw new TypeError('Value is not a finite number.');
            if (result.status === 'state') throw new DOMException('The input does not support numeric values.', 'InvalidStateError');
        }
        get valueAsDate() { return inputDateFromValue(this.type, this.value); }
        set valueAsDate(value) {
            if (!inputDateTypes.has(this.type))
                throw new DOMException('The input does not support dates.', 'InvalidStateError');
            if (value !== null && !(value instanceof Date))
                throw new TypeError('valueAsDate requires a Date or null');
            this.value = value === null || !Number.isFinite(value.getTime())
                ? '' : inputValueFromDate(this.type, value);
        }
        stepUp(n = 1) { stepControlValue(this, n, true); }
        stepDown(n = 1) { stepControlValue(this, n, false); }
    }
