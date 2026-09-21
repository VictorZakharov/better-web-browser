    // Constraint validation interfaces: one live ValidityState per control,
    // internal static/interactive validation immune to author overrides.
    // https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#the-constraint-validation-api
    const validityStates = new WeakMap();
    const validityElements = new WeakMap();
    class ValidityState {
        constructor() { throw new TypeError('Illegal constructor'); }
        get valueMissing() { return validityFlags(this).valueMissing; }
        get typeMismatch() { return validityFlags(this).typeMismatch; }
        get patternMismatch() { return validityFlags(this).patternMismatch; }
        get tooLong() { return validityFlags(this).tooLong; }
        get tooShort() { return validityFlags(this).tooShort; }
        get rangeUnderflow() { return validityFlags(this).rangeUnderflow; }
        get rangeOverflow() { return validityFlags(this).rangeOverflow; }
        get stepMismatch() { return validityFlags(this).stepMismatch; }
        get badInput() { return validityFlags(this).badInput; }
        get customError() { return validityFlags(this).customError; }
        get valid() { return validityFlags(this).valid; }
    }
    // Web IDL attributes are enumerable on the prototype, unlike class syntax defaults.
    for (const name of ['valueMissing', 'typeMismatch', 'patternMismatch', 'tooLong',
        'tooShort', 'rangeUnderflow', 'rangeOverflow', 'stepMismatch', 'badInput',
        'customError', 'valid']) {
        const descriptor = Object.getOwnPropertyDescriptor(ValidityState.prototype, name);
        Object.defineProperty(ValidityState.prototype, name, { ...descriptor, enumerable: true });
    }
    // LegacyPlatformObjectGetOwnProperty-style class string for
    // assert_class_string checks; non-enumerable like other built-ins.
    Object.defineProperty(ValidityState.prototype, Symbol.toStringTag, { value: 'ValidityState' });
    function validityFor(element) {
        let state = validityStates.get(element);
        if (!state) {
            state = Object.create(ValidityState.prototype);
            validityStates.set(element, state);
            validityElements.set(state, element);
        }
        return state;
    }
    function validityFlags(state) {
        const element = validityElements.get(state);
        if (!element) throw new TypeError('Illegal invocation');
        return controlValidation(element).flags;
    }
    function controlValidation(element) {
        // Native query: pattern regexes evaluate on this realm's isolate.
        return JSON.parse(host('controlValidation', nodeId(element)));
    }
    function validationMessageFor(element) {
        return controlValidation(element).message;
    }
    // Refreshes the stored pattern verdict after scripted value/pattern
    // changes so selector matching (which never re-enters V8) stays current.
    // The store op records a state invalidation when the verdict flips.
    function refreshPatternVerdict(element) {
        if (!(element instanceof HTMLInputElement)) return;
        const pattern = element.getAttribute('pattern');
        if (pattern === null) return;
        const value = element.value;
        if (!value) return;
        const values = element.type === 'email' && element.multiple
            ? value.split(',').map(part => part.trim()) : [value];
        let verdict = true;
        try {
            // Validity is judged on the raw pattern: anchoring first can
            // accidentally balance a stray paren (e.g. `a)(b`), so never
            // test what does not compile raw. Invalid patterns impose no
            // constraint and leave no verdict behind.
            new RegExp(pattern, 'v');
            const expression = new RegExp('^(?:' + pattern + ')$', 'v');
            verdict = values.every(candidate => expression.test(candidate));
        } catch (_error) { return; }
        host('controlPatternVerdict', nodeId(element), verdict, pattern, JSON.stringify(values));
    }
    // Attribute writes that can change pattern inputs refresh the verdict.
    function maybeRefreshPatternVerdict(element, localName) {
        if (element.localName !== 'input' ||
            !['value', 'pattern', 'type', 'multiple'].includes(localName)) return;
        if (!element.hasAttribute('pattern')) return;
        refreshPatternVerdict(element);
    }
    function isCandidateInvalid(control) {
        if (!(control instanceof HTMLInputElement || control instanceof HTMLTextAreaElement ||
            control instanceof HTMLSelectElement || control instanceof HTMLButtonElement)) return false;
        if (!host('controlWillValidate', nodeId(control))) return false;
        return !controlValidation(control).flags.valid;
    }
    function checkControlValidity(element) {
        if (!isCandidateInvalid(element)) return true;
        // Invalid events from validation are untrusted: no user gesture fired them.
        element.dispatchEvent(new Event('invalid', { cancelable: true }));
        return false;
    }
    function reportControlValidity(element) {
        if (!isCandidateInvalid(element)) return true;
        const notCanceled = element.dispatchEvent(new Event('invalid', { cancelable: true }));
        if (notCanceled) focusInvalidControl(element);
        return false;
    }
    function validationControls(form) {
        return form.getRootNode()
            .querySelectorAll('input,button,select,textarea')
            .filter(control => associatedForm(control) === form);
    }
    function staticFormValidation(form) {
        const unhandled = [];
        for (const control of validationControls(form)) {
            if (!isCandidateInvalid(control)) continue;
            const notCanceled = control.dispatchEvent(new Event('invalid', { cancelable: true }));
            if (notCanceled) unhandled.push(control);
        }
        return unhandled;
    }
    function focusInvalidControl(control) {
        // Interactive reporting marks the control (visible feedback follows
        // through layout and native presentation) and focuses it when possible.
        host('controlReportInvalid', nodeId(control));
        if (!control.matches(':disabled')) control.focus();
    }
    // Internal submission shares the reporting step across IIFE boundaries.
    globalThis.__reportInvalidControl = focusInvalidControl;
    function stepControlValue(input, n, up) {
        const result = JSON.parse(host('controlStep', nodeId(input), Number(n), up));
        if (result.status === 'state') throw new DOMException('The input has no allowed value step.', 'InvalidStateError');
    }
    // Internal submission validates without touching author-overridable methods.
    globalThis.__staticFormValidation = staticFormValidation;
