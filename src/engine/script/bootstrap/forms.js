    class HTMLObjectElement extends HTMLElement {
        // Object elements expose willValidate as false (headless Chrome
        // 153): present for the constraint-validation API shape, never a
        // candidate.
        get willValidate() { return false; }
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
        set length(value) { this.options.length = value; }
        item(index) { return this.options.item(index); }
        namedItem(name) { return this.options.namedItem(name); }
        add(element, before) { this.options.add(element, before); }
        remove(index) {
            if (arguments.length === 0) return Element.prototype.remove.call(this);
            this.options.remove(index);
        }
        get selectedIndex() { return host('selectSelectedIndex', nodeId(this)); }
        set selectedIndex(value) {
            host('selectSetSelectedIndex', nodeId(this), (+value) | 0);
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
            return cachedControlCollection(fieldsetLists, this,
                () => Array.from(this.querySelectorAll(listedControlSelector)));
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
            return select ? Array.prototype.indexOf.call(select.options, this) : 0;
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
        get value() { return progressNumericValues(this).value; }
        set value(value) { setNumberAttribute(this, 'value', value); }
        get max() { return positiveNumberAttribute(this, 'max', 1); }
        set max(value) { setNumberAttribute(this, 'max', value, true); }
        get position() {
            if (!host('attrHas', nodeId(this), 'value')) return -1;
            const { value, maximum } = progressNumericValues(this);
            return value / maximum;
        }
        get labels() { return labelsFor(this); }
    }
    class HTMLMeterElement extends HTMLElement {
        get min() { return numberAttribute(this, 'min', 0); }
        set min(value) { setNumberAttribute(this, 'min', value); }
        get max() { return meterNumericValues(this).maximum; }
        set max(value) { setNumberAttribute(this, 'max', value); }
        get value() { return meterNumericValues(this).value; }
        set value(value) { setNumberAttribute(this, 'value', value); }
        get low() { return meterNumericValues(this).low; }
        set low(value) { setNumberAttribute(this, 'low', value); }
        get high() { return meterNumericValues(this).high; }
        set high(value) { setNumberAttribute(this, 'high', value); }
        get optimum() { return meterNumericValues(this).optimum; }
        set optimum(value) { setNumberAttribute(this, 'optimum', value); }
        get labels() { return labelsFor(this); }
    }
    class HTMLTemplateElement extends HTMLElement {
        get content() { return wrap(host('templateContent', nodeId(this))); }
        get htmlFor() { return this.getAttribute('for') || ''; }
        set htmlFor(value) { this.setAttribute('for', String(value)); }
        get shadowRootMode() {
            const value = (this.getAttribute('shadowrootmode') || '').toLowerCase();
            return value === 'open' || value === 'closed' ? value : '';
        }
        set shadowRootMode(value) { this.setAttribute('shadowrootmode', String(value)); }
        get shadowRootDelegatesFocus() { return this.hasAttribute('shadowrootdelegatesfocus'); }
        set shadowRootDelegatesFocus(value) {
            this.toggleAttribute('shadowrootdelegatesfocus', !!value);
        }
        get shadowRootSerializable() { return this.hasAttribute('shadowrootserializable'); }
        set shadowRootSerializable(value) {
            this.toggleAttribute('shadowrootserializable', !!value);
        }
        get shadowRootSlotAssignment() {
            const value = (this.getAttribute('shadowrootslotassignment') || '').toLowerCase();
            return value === 'manual' ? 'manual' : 'named';
        }
        set shadowRootSlotAssignment(value) {
            this.setAttribute('shadowrootslotassignment', String(value));
        }
        get shadowRootClonable() { return this.hasAttribute('shadowrootclonable'); }
        set shadowRootClonable(value) { this.toggleAttribute('shadowrootclonable', !!value); }
        get shadowRootCustomElementRegistry() {
            return this.getAttribute('shadowrootcustomelementregistry') || '';
        }
        set shadowRootCustomElementRegistry(value) {
            this.setAttribute('shadowrootcustomelementregistry', String(value));
        }
    }
    class HTMLFormElement extends HTMLElement {
        get elements() {
            return cachedControlCollection(formLists, this, () =>
                Array.from(this.getRootNode().querySelectorAll(listedControlSelector))
                    .filter(element => associatedForm(element) === this &&
                        !(element instanceof HTMLInputElement && element.type === 'image')),
                liveFormControlsCollection);
        }
        get length() { return this.elements.length; }
        get noValidate() { return this.hasAttribute('novalidate'); }
        set noValidate(value) { this.toggleAttribute('novalidate', !!value); }
        checkValidity() {
            return staticFormValidation(this).valid;
        }
        reportValidity() {
            const { valid, unhandled } = staticFormValidation(this);
            if (unhandled.length) focusInvalidControl(unhandled[0]);
            return valid;
        }
    }
    function nonNegativeLongAttribute(element, attribute) {
        const text = host('attrGet', nodeId(element), attribute);
        // ReflectNonNegative parses an initial decimal integer, rejects values
        // outside the signed-long range, and defaults missing/invalid to -1.
        // https://html.spec.whatwg.org/multipage/common-dom-interfaces.html#reflecting-content-attributes-in-idl-attributes
        const match = text === null ? null : /^[\t\n\f\r ]*([+-]?[0-9]+)/.exec(text);
        const value = match ? Number(match[1]) : NaN;
        return value >= 0 && value <= 2147483647 ? (value === 0 ? 0 : value) : -1;
    }
    function setNonNegativeLongAttribute(element, attribute, value) {
        // Web IDL long first coerces to Number, truncates and wraps to 32 bits.
        // NaN/infinity become zero; BigInt/Symbol coercion still throws.
        // https://webidl.spec.whatwg.org/#es-long
        value = (+value) | 0;
        if (value < 0) throw new DOMException('The length must not be negative', 'IndexSizeError');
        setAttributeValueInternal(element, attribute, String(value));
    }
    function numberAttribute(element, attribute, fallback) {
        const text = host('attrGet', nodeId(element), attribute);
        if (text === null) return fallback;
        // HTML parses a decimal prefix, rather than ECMAScript Number's entire
        // string: missing/empty values fail, "0x10" is zero and "2e+" is two.
        // Only ASCII whitespace is skipped. The optional exponent deliberately
        // requires digits so an incomplete exponent preserves the significand.
        // https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#rules-for-parsing-floating-point-number-values
        const match = /^[\t\n\f\r ]*([+-]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?)/.exec(text);
        const value = match ? Number(match[1]) : NaN;
        return Number.isFinite(value) ? (value === 0 ? 0 : value) : fallback;
    }
    function setNumberAttribute(element, attribute, value, positiveOnly = false) {
        // Web IDL double uses ToNumber and rejects non-finite results. Unary +
        // also rejects BigInt (including objects returning it) unlike Number().
        // https://html.spec.whatwg.org/multipage/common-dom-interfaces.html#reflecting-content-attributes-in-idl-attributes
        value = +value;
        if (!Number.isFinite(value)) throw new TypeError('The value must be a finite number');
        // ReflectPositive applies to progress.max: nonpositive IDL assignment
        // is ignored, while invalid content attributes still use the fallback.
        if (!positiveOnly || value > 0)
            setAttributeValueInternal(element, attribute, String(value));
    }
    function positiveNumberAttribute(element, attribute, fallback) {
        const value = numberAttribute(element, attribute, fallback);
        return value > 0 ? value : fallback;
    }
    function clampedNumberAttribute(element, attribute, fallback, minimum, maximum) {
        return Math.min(maximum, Math.max(minimum, numberAttribute(element, attribute, fallback)));
    }
    function progressNumericValues(element) {
        const maximum = positiveNumberAttribute(element, 'max', 1);
        return { maximum, value: clampedNumberAttribute(element, 'value', 0, 0, maximum) };
    }
    function meterNumericValues(element) {
        // Resolve the six gauge points from content attributes in specification
        // order; own properties or overridden public getters cannot change them.
        const minimum = numberAttribute(element, 'min', 0);
        const maximum = Math.max(minimum, numberAttribute(element, 'max', 1));
        const value = clampedNumberAttribute(element, 'value', 0, minimum, maximum);
        const low = clampedNumberAttribute(element, 'low', minimum, minimum, maximum);
        const high = clampedNumberAttribute(element, 'high', maximum, low, maximum);
        // Same-sign bounds can subtract safely; opposite signs cannot.
        const midpoint = minimum < 0 && maximum > 0
            ? minimum / 2 + maximum / 2 : minimum + (maximum - minimum) / 2;
        const optimum = clampedNumberAttribute(element, 'optimum', midpoint, minimum, maximum);
        return { minimum, maximum, value, low, high, optimum };
    }
    function labelsFor(element) {
        return document.querySelectorAll('label').filter(label => label.control === element);
    }
    const listedControlSelector = 'button, fieldset, input, object, output, select, textarea';
    const formLists = new WeakMap();
    const fieldsetLists = new WeakMap();
    const selectOptionLists = new WeakMap();
    const selectedOptionLists = new WeakMap();
    function cachedControlCollection(cache, owner, resolve, make = liveHtmlCollection) {
        let list = cache.get(owner);
        if (!list) {
            list = make(resolve);
            cache.set(owner, list);
        }
        return list;
    }
    function selectOptions(select) {
        return cachedControlCollection(selectOptionLists, select, () => {
            const ids = String(host('selectOptionIds', nodeId(select)));
            return ids ? ids.split(',').map(id => wrap(Number(id))).filter(Boolean) : [];
        }, resolve => liveOptionsCollection(select, resolve));
    }
    function selectSelectedOptions(select) {
        return cachedControlCollection(selectedOptionLists, select,
            () => Array.from(selectOptions(select)).filter(option => option.selected));
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
