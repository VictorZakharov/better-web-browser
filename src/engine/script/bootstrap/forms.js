    class HTMLInputElement extends HTMLElement {
        // The type IDL attribute is limited to known values; missing and invalid
        // content values select Text, including for libraries deciding how to edit it.
        // https://html.spec.whatwg.org/multipage/input.html#attr-input-type
        get type() {
            const value = (this.getAttribute('type') || '').toLowerCase();
            return /^(hidden|text|search|tel|url|email|password|date|month|week|time|datetime-local|number|range|color|checkbox|radio|file|submit|image|reset|button)$/.test(value) ? value : 'text';
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
        get valueAsNumber() { return host('controlValueAsNumber', nodeId(this)); }
        set valueAsNumber(value) {
            const result = JSON.parse(host('controlSetValueAsNumber', nodeId(this), Number(value)));
            if (result.status === 'type') throw new TypeError('Value is not a finite number.');
            if (result.status === 'state') throw new DOMException('The input does not support numeric values.', 'InvalidStateError');
        }
        // Temporal value access stays an explicit boundary: no date parsing
        // in this experiment, so the API reports its inapplicability.
        get valueAsDate() { return null; }
        set valueAsDate(_value) { throw new DOMException('The input does not support dates.', 'InvalidStateError'); }
        stepUp(n = 1) { stepControlValue(this, n, true); }
        stepDown(n = 1) { stepControlValue(this, n, false); }
    }
    class HTMLTextAreaElement extends HTMLElement {
        get placeholder() { return this.getAttribute('placeholder') || ''; }
        set placeholder(value) { this.setAttribute('placeholder', value); }
        get form() { return associatedForm(this); }
        get value() { return host('textareaValue', nodeId(this)); }
        set value(value) { host('textareaSetValue', nodeId(this), String(value)); }
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
        get willValidate() { return host('controlWillValidate', nodeId(this)); }
        get validity() { return validityFor(this); }
        get validationMessage() { return validationMessageFor(this); }
        setCustomValidity(message) { host('controlSetCustomValidity', nodeId(this), String(message)); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return reportControlValidity(this); }
        get textLength() { return host('textareaValue', nodeId(this)).length; }
    }
    class HTMLOrderedListElement extends HTMLElement {
        get reversed() { return this.hasAttribute('reversed'); }
        set reversed(value) { this.toggleAttribute('reversed', !!value); }
    }
    class HTMLSelectElement extends HTMLElement {
        get multiple() { return this.hasAttribute('multiple'); }
        set multiple(value) { this.toggleAttribute('multiple', !!value); }
        get type() { return this.multiple ? 'select-multiple' : 'select-one'; }
        get options() { return selectOptions(this); }
        get selectedOptions() { return selectSelectedOptions(this); }
        get length() { return this.options.length; }
        get selectedIndex() { return host('selectSelectedIndex', nodeId(this)); }
        set selectedIndex(value) {
            host('selectSetSelectedIndex', nodeId(this), Math.trunc(Number(value) || 0));
        }
        get value() { return host('selectValue', nodeId(this)); }
        set value(value) { host('selectSetValue', nodeId(this), String(value)); }
        get form() { return associatedForm(this); }
        get required() { return this.hasAttribute('required'); }
        set required(value) { this.toggleAttribute('required', !!value); }
        get labels() { return labelsFor(this); }
        get willValidate() { return host('controlWillValidate', nodeId(this)); }
        get validity() { return validityFor(this); }
        get validationMessage() { return validationMessageFor(this); }
        setCustomValidity(message) { host('controlSetCustomValidity', nodeId(this), String(message)); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return reportControlValidity(this); }
    }
    class HTMLButtonElement extends HTMLElement {
        get form() { return associatedForm(this); }
        get labels() { return labelsFor(this); }
        get type() {
            const value = (this.getAttribute('type') || '').toLowerCase();
            return ['submit', 'reset', 'button'].includes(value) ? value : 'submit';
        }
        set type(value) { this.setAttribute('type', value); }
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
    }
    class HTMLLabelElement extends HTMLElement {
        get htmlFor() { return this.getAttribute('for') || ''; }
        set htmlFor(value) { this.setAttribute('for', value); }
        get control() {
            if (this.htmlFor) return document.getElementById(this.htmlFor);
            return this.querySelector('button, input, meter, output, progress, select, textarea');
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
        get willValidate() { return false; }
        get validity() { return validityFor(this); }
        get validationMessage() { return ''; }
        setCustomValidity(message) { host('controlSetCustomValidity', nodeId(this), String(message)); }
        checkValidity() { return true; }
        reportValidity() { return true; }
    }
    class HTMLOptionElement extends HTMLElement {
        get selected() { return host('optionSelected', nodeId(this)); }
        set selected(value) { host('optionSetSelected', nodeId(this), !!value); }
        get defaultSelected() { return this.hasAttribute('selected'); }
        set defaultSelected(value) { this.toggleAttribute('selected', !!value); }
        get index() {
            const select = this.closest('select');
            return select ? select.options.indexOf(this) : 0;
        }
        get text() { return optionText(this); }
        set text(value) { this.textContent = String(value); }
        get label() {
            return this.hasAttribute('label') ? this.getAttribute('label') : optionText(this);
        }
        set label(value) { this.setAttribute('label', value); }
        get value() {
            return this.hasAttribute('value') ? this.getAttribute('value') : optionText(this);
        }
        set value(value) { this.setAttribute('value', value); }
    }
    class HTMLDataListElement extends HTMLElement {
        get options() { return this.__options ||= selectorCollection(this, 'option'); }
    }
    class HTMLOutputElement extends HTMLElement {
        get htmlFor() { return this.__htmlFor ||= new DOMTokenList(this, 'for'); }
        get form() { return associatedForm(this); }
        get name() { return this.getAttribute('name') || ''; }
        set name(value) { this.setAttribute('name', value); }
        get type() { return 'output'; }
        get value() { return host('outputValue', nodeId(this)); }
        set value(value) { host('outputSetValue', nodeId(this), String(value)); }
        get defaultValue() { return host('outputDefaultValue', nodeId(this)); }
        set defaultValue(value) { host('outputSetDefault', nodeId(this), String(value)); }
        get labels() { return labelsFor(this); }
        get willValidate() { return false; }
        get validity() { return validityFor(this); }
        get validationMessage() { return ''; }
        setCustomValidity(message) { host('controlSetCustomValidity', nodeId(this), String(message)); }
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
        get content() { return wrap(host('templateContent', nodeId(this))); }
    }
    class HTMLFormElement extends HTMLElement {
        get elements() {
            return document.querySelectorAll('button, fieldset, input, object, output, select, textarea')
                .filter(element => associatedForm(element) === this && !(element instanceof HTMLInputElement && element.type.toLowerCase() === 'image'));
        }
        get length() { return this.elements.length; }
        get noValidate() { return this.hasAttribute('novalidate'); }
        set noValidate(value) { this.toggleAttribute('novalidate', !!value); }
        checkValidity() {
            let valid = true;
            for (const control of validationControls(this)) {
                if (!checkControlValidity(control)) valid = false;
            }
            return valid;
        }
        reportValidity() {
            const unhandled = staticFormValidation(this);
            if (!unhandled.length) return true;
            focusInvalidControl(unhandled[0]);
            return false;
        }
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
    const selectOptionLists = new WeakMap();
    const selectedOptionLists = new WeakMap();
    function selectOptions(select) {
        // Same visible list object with current contents, like HTMLOptionsCollection.
        let list = selectOptionLists.get(select);
        if (!list) {
            list = [];
            selectOptionLists.set(select, list);
        }
        const ids = String(host('selectOptionIds', nodeId(select)));
        const fresh = ids ? ids.split(',').map(id => wrap(Number(id))).filter(Boolean) : [];
        list.length = 0;
        list.push(...fresh);
        return list;
    }
    function selectSelectedOptions(select) {
        let list = selectedOptionLists.get(select);
        if (!list) {
            list = [];
            selectedOptionLists.set(select, list);
        }
        const fresh = selectOptions(select).filter(option => option.selected);
        list.length = 0;
        list.push(...fresh);
        return list;
    }
    function optionText(option) {
        return option.textContent.replace(/[\t\n\f\r ]+/g, ' ').trim();
    }
    function associatedForm(element) {
        const explicit = element.getAttribute('form');
        if (explicit !== null && element.localName !== 'img') {
            const form = element.ownerDocument.getElementById(explicit);
            return form?.localName === 'form' ? form : null;
        }
        for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) {
            if (ancestor.localName === 'form') return ancestor;
        }
        return null;
    }
