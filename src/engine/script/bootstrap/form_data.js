(() => {
    'use strict';
    const trusted = globalThis.__markTrustedEvent;
    const typeOf = control => String(control.type || 'text').toLowerCase();
    const formsBeingConstructed = new WeakSet();
    const eventData = new WeakMap();
    const imageSubmitCoordinates = globalThis.__imageSubmitCoordinates;
    delete globalThis.__imageSubmitCoordinates;
    class FormDataEvent extends Event {
        constructor(type, init) {
            super(type, init);
            if (!(init?.formData instanceof FormData)) throw new TypeError('formData is required');
            eventData.set(this, init.formData);
        }
        get formData() { return eventData.get(this); }
    }
    class FormData {
        constructor(form = undefined, submitter = null) {
            this.__entries = [];
            if (form === undefined) return;
            if (!(form instanceof HTMLFormElement)) throw new TypeError('FormData requires an HTMLFormElement');
            if (submitter !== null) {
                if (!((submitter instanceof HTMLButtonElement && submitter.type === 'submit') ||
                    (submitter instanceof HTMLInputElement && /^(submit|image)$/i.test(submitter.type))))
                    throw new TypeError('The submitter is not a submit button');
                if (submitter.form !== form) throw new DOMException('Submitter belongs to another form', 'NotFoundError');
            }
            if (formsBeingConstructed.has(form)) throw new DOMException('Entry list is being constructed', 'InvalidStateError');
            formsBeingConstructed.add(form);
            try {
                // Include external controls, hidden controls, and image buttons in tree order;
                // form.elements excludes image buttons, so it cannot supply this entry list.
                for (const control of form.getRootNode().querySelectorAll('input,button,select,textarea')) {
                    if (control.form !== form || control.matches(':disabled') || control.closest('datalist')) continue;
                    const name = control.name;
                    const type = typeOf(control);
                    if (control instanceof HTMLButtonElement || /^(submit|image|reset|button)$/.test(type)) {
                        if (control !== submitter) continue;
                    }
                    if (type === 'image') {
                        const [x, y] = imageSubmitCoordinates(control);
                        this.append(name ? name + '.x' : 'x', String(x));
                        this.append(name ? name + '.y' : 'y', String(y));
                        continue;
                    }
                    if (!name || (/^(checkbox|radio)$/.test(type) && !control.checked)) continue;
                    if (control instanceof HTMLSelectElement) {
                        for (const option of control.options) {
                            if (!option.selected || option.disabled || option.parentElement?.matches('optgroup[disabled]')) continue;
                            this.append(name, option.value);
                        }
                    } else if (type === 'file') {
                        const files = Array.from(control.files || []);
                        if (files.length) for (const file of files) this.append(name, file);
                        else this.append(name, new File([], '', { type: 'application/octet-stream' }));
                    } else if (type === 'hidden' && name.toLowerCase() === '_charset_') this.append(name, 'UTF-8');
                    else this.append(name, /^(checkbox|radio)$/.test(type) && !control.hasAttribute('value') ? 'on' : control.value);
                    if (control.dirName) this.append(control.dirName, control.closest('[dir]')?.getAttribute('dir') === 'rtl' ? 'rtl' : 'ltr');
                }
                form.dispatchEvent(trusted(new FormDataEvent('formdata', { bubbles: true, formData: this })));
            } finally {
                formsBeingConstructed.delete(form);
            }
        }
        append(name, value, filename = undefined) {
            name = String(name).toWellFormed();
            if (value instanceof Blob && !(value instanceof File))
                value = new File([value], filename === undefined ? 'blob' : filename, { type: value.type });
            else if (value instanceof File && filename !== undefined)
                value = new File([value], filename, { type: value.type, lastModified: value.lastModified });
            else if (!(value instanceof Blob)) value = String(value).toWellFormed();
            this.__entries.push([name, value]);
        }
        delete(name) { name = String(name); this.__entries = this.__entries.filter(entry => entry[0] !== name); }
        get(name) { return this.__entries.find(entry => entry[0] === String(name))?.[1] ?? null; }
        getAll(name) { return this.__entries.filter(entry => entry[0] === String(name)).map(entry => entry[1]); }
        has(name) { return this.__entries.some(entry => entry[0] === String(name)); }
        set(name, value, filename = undefined) {
            name = String(name);
            const index = this.__entries.findIndex(entry => entry[0] === name);
            if (index < 0) { this.append(name, value, filename); return; }
            const replacement = new FormData(); replacement.append(name, value, filename);
            this.__entries[index] = replacement.__entries[0];
            this.__entries = this.__entries.filter((entry, position) => position <= index || entry[0] !== name);
        }
        forEach(callback, thisArg = undefined) {
            if (typeof callback !== 'function') throw new TypeError('FormData callback must be callable');
            for (const [name, value] of this.__entries) callback.call(thisArg, value, name, this);
        }
        *entries() { for (let index = 0; index < this.__entries.length; index++) yield [...this.__entries[index]]; }
        *keys() { for (const [name] of this.entries()) yield name; }
        *values() { for (const [, value] of this.entries()) yield value; }
        [Symbol.iterator]() { return this.entries(); }
    }
    Object.assign(globalThis, { FormData, FormDataEvent });
    Object.defineProperty(globalThis, '__constructingFormData', {
        configurable: true, value: form => formsBeingConstructed.has(form)
    });
})();
