(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const data = globalThis.__networkData;
    const trusted = globalThis.__markTrustedEvent;
    const constructing = globalThis.__constructingFormData;
    delete globalThis.__constructingFormData;
    const FormDataConstructor = FormData;
    const validateForSubmission = globalThis.__staticFormValidation;
    const queueNavigation = globalThis.setTimeout;
    const cancelNavigation = globalThis.clearTimeout;
    const plannedTasks = new WeakMap();
    const submitting = new WeakSet();
    const submitters = new WeakMap();
    class SubmitEvent extends Event {
        constructor(type, init = {}) {
            if (arguments.length === 0) throw new TypeError('Event type is required');
            super(type, init);
            const submitter = init?.submitter ?? null;
            if (submitter !== null && !(submitter instanceof HTMLElement)) throw new TypeError('Invalid submitter');
            submitters.set(this, submitter);
        }
        get submitter() { return submitters.get(this) ?? null; }
    }
    globalThis.SubmitEvent = SubmitEvent;
    const enumerated = (value, allowed, fallback) => {
        value = String(value || '').toLowerCase();
        return allowed.includes(value) ? value : fallback;
    };
    const methods = ['get', 'post', 'dialog'];
    const encodings = ['application/x-www-form-urlencoded', 'multipart/form-data', 'text/plain'];
    const reflect = (attribute, normalize = value => value || '') => ({
        enumerable: true, configurable: true,
        get() { return normalize(this.getAttribute(attribute)); },
        set(value) { this.setAttribute(attribute, String(value)); }
    });
    Object.defineProperties(HTMLFormElement.prototype, {
        action: { enumerable: true, configurable: true,
            get() { return new URL(this.getAttribute('action') || document.URL, this.baseURI).href; },
            set(value) { this.setAttribute('action', value); } },
        method: reflect('method', value => enumerated(value, methods, 'get')),
        enctype: reflect('enctype', value => enumerated(value, encodings, encodings[0])),
        encoding: reflect('enctype', value => enumerated(value, encodings, encodings[0])),
        target: reflect('target'), acceptCharset: reflect('accept-charset'),
        submit: { enumerable: true, configurable: true, writable: true,
            value() { submit(this, null, true); } },
        requestSubmit: { enumerable: true, configurable: true, writable: true,
            value(submitter = null) {
                if (!(this instanceof HTMLFormElement)) throw new TypeError('Illegal invocation');
                if (submitter !== null) {
                    if (!(submitter instanceof HTMLElement) ||
                        !((submitter instanceof HTMLButtonElement && submitter.type === 'submit') ||
                          (submitter instanceof HTMLInputElement && /^(submit|image)$/i.test(submitter.type))))
                        throw new TypeError('The submitter is not a submit button');
                    if (submitter.form !== this) throw new DOMException('Submitter belongs to another form', 'NotFoundError');
                }
                submit(this, submitter, false);
            } }
    });
    const attribute = (form, button, name) => button?.hasAttribute('form' + name)
        ? button.getAttribute('form' + name) : form.getAttribute(name);
    const newline = value => String(value).replace(/\r\n|\r|\n/g, '\r\n');
    // HTML submission constructs the entry list after submit listeners, then reads the
    // attributes after formdata listeners. Direct submit skips validation and submit.
    // https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#form-submission-algorithm
    function submit(form, button, direct) {
        if (!(form instanceof HTMLFormElement)) throw new TypeError('Illegal invocation');
        if (!form.isConnected || form.ownerDocument !== document || constructing(form)) return;
        if (!direct) {
            if (submitting.has(form)) return;
            submitting.add(form);
            try {
                // Internal submission validates without author-overridable
                // methods: static validation only, no focus or reporting.
                if (!form.noValidate && !button?.formNoValidate && validateForSubmission(form).length) return;
                if (!form.dispatchEvent(trusted(new SubmitEvent('submit', {
                    bubbles: true, cancelable: true, submitter: button
                })))) return;
            } finally { submitting.delete(form); }
        }
        if (!form.isConnected) return;
        const entries = new FormDataConstructor(form, button);
        if (!form.isConnected) return;
        const method = enumerated(attribute(form, button, 'method'), methods, 'get');
        if (method === 'dialog') {
            const dialog = form.closest('dialog');
            if (dialog) dialog.close(button ? button.value : undefined);
            return;
        }
        let url;
        try { url = new URL(attribute(form, button, 'action') || document.URL, form.baseURI); }
        catch (_) { return; }
        // Network navigation supports HTTP(S); never turn another scheme into a GET.
        if (!['http:', 'https:'].includes(url.protocol)) return;
        const target = attribute(form, button, 'target') ?? document.querySelector('base[target]')?.getAttribute('target') ?? '';
        const pairs = Array.from(entries, ([name, value]) =>
            [newline(name), newline(value instanceof File ? value.name : value)]);
        let post = null;
        const encoding = enumerated(attribute(form, button, 'enctype'), encodings, encodings[0]);
        if (method === 'get') url.search = '?' + new URLSearchParams(pairs).toString();
        else {
            let body;
            if (encoding === 'multipart/form-data') body = data.extractBody(entries);
            else {
                const text = encoding === 'text/plain'
                    ? pairs.map(([name, value]) => name + '=' + value + '\r\n').join('')
                    : new URLSearchParams(pairs).toString();
                body = { bytes: data.encoder.encode(text), type: encoding };
            }
            post = { content_type: body.type, body: Array.from(body.bytes) };
        }
        const noreferrer = /(?:^|\s)noreferrer(?:\s|$)/i.test(form.getAttribute('rel') || '');
        const sameContext = !target || /^(?:_self|_top|_parent)$/i.test(target);
        const replace_history = sameContext && document.readyState !== 'complete';
        const token = host('planFormNavigation', url.href, JSON.stringify({ target, post, noreferrer, replace_history }), form.__id);
        if (plannedTasks.has(form)) cancelNavigation(plannedTasks.get(form));
        plannedTasks.set(form, queueNavigation(() => {
            plannedTasks.delete(form);
            host('commitFormNavigation', form.__id, token);
        }, 0));
    }
})();
