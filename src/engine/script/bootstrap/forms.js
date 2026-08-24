    class HTMLInputElement extends HTMLElement {
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
        get indeterminate() { return !!this.__indeterminate; }
        set indeterminate(value) { this.__indeterminate = !!value; }
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
        get willValidate() { return !this.disabled && !['hidden', 'button', 'reset'].includes(this.type); }
        get validity() { return validityFor(this); }
        get validationMessage() { return this.validity.valid ? '' : (this.__customValidity || 'Please enter a valid value.'); }
        setCustomValidity(message) { this.__customValidity = String(message); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return this.checkValidity(); }
    }
    class HTMLTextAreaElement extends HTMLElement {
        get placeholder() { return this.getAttribute('placeholder') || ''; }
        set placeholder(value) { this.setAttribute('placeholder', value); }
        get form() { return associatedForm(this); }
        get value() { return this.__value ?? this.textContent; }
        set value(value) { this.__value = String(value); }
        get defaultValue() { return this.textContent; }
        set defaultValue(value) { this.textContent = String(value); }
        get minLength() { return reflectedInteger(this, 'minlength', -1); }
        set minLength(value) { this.setAttribute('minlength', String(Math.trunc(Number(value)))); }
        get maxLength() { return reflectedInteger(this, 'maxlength', -1); }
        set maxLength(value) { this.setAttribute('maxlength', String(Math.trunc(Number(value)))); }
        get wrap() { return (this.getAttribute('wrap') || 'soft').toLowerCase() === 'hard' ? 'hard' : 'soft'; }
        set wrap(value) { this.setAttribute('wrap', value); }
        get required() { return this.hasAttribute('required'); }
        set required(value) { this.toggleAttribute('required', !!value); }
        get labels() { return labelsFor(this); }
        get willValidate() { return !this.disabled; }
        get validity() { return validityFor(this); }
        get validationMessage() { return this.validity.valid ? '' : (this.__customValidity || 'Please enter a valid value.'); }
        setCustomValidity(message) { this.__customValidity = String(message); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return this.checkValidity(); }
    }
    class HTMLOrderedListElement extends HTMLElement {
        get reversed() { return this.hasAttribute('reversed'); }
        set reversed(value) { this.toggleAttribute('reversed', !!value); }
    }
    class HTMLSelectElement extends HTMLElement {
        get options() { return this.querySelectorAll('option'); }
        get selectedIndex() {
            const options = this.options;
            const selected = options.findIndex(option => option.hasAttribute('selected'));
            return selected >= 0 ? selected : (options.length ? 0 : -1);
        }
        set selectedIndex(value) {
            const selected = Math.trunc(Number(value));
            this.options.forEach((option, index) =>
                option.toggleAttribute('selected', index === selected));
        }
        get value() {
            const option = this.options[this.selectedIndex];
            return option ? (option.getAttribute('value') ?? option.textContent) : '';
        }
        set value(value) {
            value = String(value);
            const options = this.options;
            const selected = options.findIndex(option =>
                (option.getAttribute('value') ?? option.textContent) === value);
            this.selectedIndex = selected;
        }
        get form() { return associatedForm(this); }
        get required() { return this.hasAttribute('required'); }
        set required(value) { this.toggleAttribute('required', !!value); }
        get labels() { return labelsFor(this); }
        get willValidate() { return !this.disabled; }
        get validity() { return validityFor(this); }
        get validationMessage() { return this.validity.valid ? '' : (this.__customValidity || 'Please select an item.'); }
        setCustomValidity(message) { this.__customValidity = String(message); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return this.checkValidity(); }
    }
    class HTMLButtonElement extends HTMLElement {
        get form() { return associatedForm(this); }
        get labels() { return labelsFor(this); }
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
    }
    class HTMLLabelElement extends HTMLElement {
        get htmlFor() { return this.getAttribute('for') || ''; }
        set htmlFor(value) { this.setAttribute('for', value); }
        get control() {
            if (this.htmlFor) return document.getElementById(this.htmlFor);
            return this.querySelector('button, input, meter, output, progress, select, textarea');
        }
        click() {
            super.click();
            this.control?.focus();
        }
    }
    class HTMLFieldSetElement extends HTMLElement {
        get elements() {
            return this.querySelectorAll('button, fieldset, input, object, output, select, textarea');
        }
        get form() { return associatedForm(this); }
        get disabled() { return this.hasAttribute('disabled'); }
        set disabled(value) { this.toggleAttribute('disabled', !!value); }
        get type() { return 'fieldset'; }
    }
    class HTMLDataListElement extends HTMLElement {
        get options() { return this.querySelectorAll('option'); }
    }
    class HTMLOutputElement extends HTMLElement {
        get htmlFor() { return this.__htmlFor ||= new DOMTokenList(this, 'for'); }
        get form() { return associatedForm(this); }
        get name() { return this.getAttribute('name') || ''; }
        set name(value) { this.setAttribute('name', value); }
        get type() { return 'output'; }
        get value() { return this.textContent; }
        set value(value) { this.textContent = String(value); }
        get defaultValue() { return this.__defaultValue ?? this.textContent; }
        set defaultValue(value) {
            value = String(value);
            if (this.__defaultValue === undefined) this.textContent = value;
            else this.__defaultValue = value;
        }
        get labels() { return labelsFor(this); }
        get willValidate() { return false; }
        get validity() { return validValidityState(); }
        get validationMessage() { return ''; }
        setCustomValidity(_message) {}
        checkValidity() { return true; }
        reportValidity() { return true; }
    }
    class HTMLProgressElement extends HTMLElement {
        get value() { return clampedNumberAttribute(this, 'value', 0, 0, this.max); }
        set value(value) { this.setAttribute('value', value); }
        get max() { return positiveNumberAttribute(this, 'max', 1); }
        set max(value) { this.setAttribute('max', value); }
        get position() { return this.hasAttribute('value') ? this.value / this.max : -1; }
        get labels() { return labelsFor(this); }
    }
    class HTMLMeterElement extends HTMLElement {
        get min() { return numberAttribute(this, 'min', 0); }
        set min(value) { this.setAttribute('min', value); }
        get max() { return Math.max(this.min, numberAttribute(this, 'max', 1)); }
        set max(value) { this.setAttribute('max', value); }
        get value() { return clampedNumberAttribute(this, 'value', 0, this.min, this.max); }
        set value(value) { this.setAttribute('value', value); }
        get low() { return clampedNumberAttribute(this, 'low', this.min, this.min, this.max); }
        set low(value) { this.setAttribute('low', value); }
        get high() { return clampedNumberAttribute(this, 'high', this.max, this.low, this.max); }
        set high(value) { this.setAttribute('high', value); }
        get optimum() { return clampedNumberAttribute(this, 'optimum', (this.min + this.max) / 2, this.min, this.max); }
        set optimum(value) { this.setAttribute('optimum', value); }
        get labels() { return labelsFor(this); }
    }
    class HTMLTemplateElement extends HTMLElement {
        get content() { return wrap(host('templateContent', this.__id)); }
    }
    class HTMLFormElement extends HTMLElement {
        get elements() {
            return document.querySelectorAll('button, fieldset, input, object, output, select, textarea')
                .filter(element => associatedForm(element) === this);
        }
        get length() { return this.elements.length; }
        get noValidate() { return this.hasAttribute('novalidate'); }
        set noValidate(value) { this.toggleAttribute('novalidate', !!value); }
        checkValidity() {
            let valid = true;
            for (const control of this.elements) if (typeof control.checkValidity === 'function' && !control.checkValidity()) valid = false;
            return valid;
        }
        reportValidity() { return this.checkValidity(); }
    }
    function reflectedInteger(element, attribute, fallback) {
        const value = Number(element.getAttribute(attribute));
        return Number.isFinite(value) ? Math.trunc(value) : fallback;
    }
    function numberAttribute(element, attribute, fallback) {
        const value = Number(element.getAttribute(attribute));
        return Number.isFinite(value) ? value : fallback;
    }
    function positiveNumberAttribute(element, attribute, fallback) {
        const value = numberAttribute(element, attribute, fallback);
        return value > 0 ? value : fallback;
    }
    function clampedNumberAttribute(element, attribute, fallback, minimum, maximum) {
        return Math.min(maximum, Math.max(minimum, numberAttribute(element, attribute, fallback)));
    }
    function labelsFor(element) {
        return document.querySelectorAll('label').filter(label => label.control === element);
    }
    function validValidityState() {
        return {
            valueMissing: false, typeMismatch: false, patternMismatch: false,
            tooLong: false, tooShort: false, rangeUnderflow: false,
            rangeOverflow: false, stepMismatch: false, badInput: false,
            customError: false, valid: true
        };
    }
    function validityFor(element) {
        const value = String(element.value ?? '');
        const type = String(element.type || '').toLowerCase();
        const required = !!element.required;
        const valueMissing = required && value === '';
        let typeMismatch = false;
        if (value && type === 'email') typeMismatch = !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value);
        if (value && type === 'url') typeMismatch = !/^[a-z][a-z0-9+.-]*:\/\/[^\s]+$/i.test(value);
        let patternMismatch = false;
        const pattern = element.pattern;
        if (value && pattern) {
            try { patternMismatch = !(new RegExp('^(?:' + pattern + ')$')).test(value); } catch (_error) {}
        }
        const numeric = Number(value);
        const hasNumber = value !== '' && Number.isFinite(numeric);
        const minimum = Number(element.min);
        const maximum = Number(element.max);
        const rangeUnderflow = hasNumber && element.min !== '' && Number.isFinite(minimum) && numeric < minimum;
        const rangeOverflow = hasNumber && element.max !== '' && Number.isFinite(maximum) && numeric > maximum;
        const badInput = (type === 'number' || type === 'range') && value !== '' && !hasNumber;
        const customError = !!element.__customValidity;
        const valid = !(valueMissing || typeMismatch || patternMismatch || rangeUnderflow || rangeOverflow || badInput || customError);
        return {
            valueMissing, typeMismatch, patternMismatch,
            tooLong: false, tooShort: false, rangeUnderflow,
            rangeOverflow, stepMismatch: false, badInput,
            customError, valid
        };
    }
    function checkControlValidity(element) {
        if (!element.willValidate || element.validity.valid) return true;
        element.dispatchEvent(new Event('invalid', { cancelable: true }));
        return false;
    }
    function associatedForm(element) {
        const explicit = element.getAttribute('form');
        if (explicit) {
            const form = document.getElementById(explicit);
            return form?.localName === 'form' ? form : null;
        }
        for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) {
            if (ancestor.localName === 'form') return ancestor;
        }
        return null;
    }
