(() => {
    'use strict';
    // FormData has an internal entry list. Fetch captures the snapshot hook
    // during bootstrap and removes it before author code runs.
    // https://xhr.spec.whatwg.org/#interface-formdata
    const entryLists = new WeakMap();
    const formsBeingConstructed = new WeakSet();
    const eventData = new WeakMap();
    const isWindow = typeof HTMLFormElement === 'function';
    const trusted = globalThis.__markTrustedEvent;
    const dispatch = EventTarget.prototype.dispatchEvent;
    const imageSubmitCoordinates = isWindow ? globalThis.__imageSubmitCoordinates : null;
    if (isWindow) delete globalThis.__imageSubmitCoordinates;
    const fileListContents = isWindow ? globalThis.__fileListContents : null;
    if (isWindow) delete globalThis.__fileListContents;
    const typeOf = control => String(control.type || 'text').toLowerCase();
    const scalar = value => {
        if (typeof value === 'symbol') throw new TypeError('A scalar string is required');
        return String(value).toWellFormed();
    };
    const entriesOf = formData => {
        const entries = entryLists.get(formData);
        if (!entries) throw new TypeError('Invalid FormData receiver');
        return entries;
    };
    const copyEntries = entries => entries.map(([name, value]) => [name, value]);
    let BlobConstructor, FileConstructor, makeFile;
    const createEntry = (name, value, filename, filenameGiven = false) => {
        name = scalar(name);
        if (filenameGiven && !(value instanceof BlobConstructor))
            throw new TypeError('A filename requires a Blob value');
        if (value instanceof BlobConstructor) {
            if (!(value instanceof FileConstructor) || filenameGiven)
                value = makeFile(value, filenameGiven ? scalar(filename) : 'blob');
        } else value = scalar(value);
        return [name, value];
    };
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
            entryLists.set(this, []);
            if (form === undefined) return;
            if (!isWindow || !(form instanceof HTMLFormElement))
                throw new TypeError('FormData requires an HTMLFormElement');
            if (submitter !== null) {
                if (!((submitter instanceof HTMLButtonElement && submitter.type === 'submit') ||
                    (submitter instanceof HTMLInputElement && /^(submit|image)$/i.test(submitter.type))))
                    throw new TypeError('The submitter is not a submit button');
                if (submitter.form !== form) throw new DOMException('Submitter belongs to another form', 'NotFoundError');
            }
            if (formsBeingConstructed.has(form)) throw new DOMException('Entry list is being constructed', 'InvalidStateError');
            formsBeingConstructed.add(form);
            try {
                // The event FormData owns the provisional list. HTML returns a
                // clone of its post-event entries to the constructor caller.
                // https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#constructing-the-entry-list
                const eventFormData = new FormData();
                const entries = entriesOf(eventFormData);
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
                        entries.push(createEntry(name ? name + '.x' : 'x', String(x)));
                        entries.push(createEntry(name ? name + '.y' : 'y', String(y)));
                        continue;
                    }
                    if (!name || (/^(checkbox|radio)$/.test(type) && !control.checked)) continue;
                    if (control instanceof HTMLSelectElement) {
                        for (const option of control.options) {
                            if (!option.selected || option.disabled || option.parentElement?.matches('optgroup[disabled]')) continue;
                            entries.push(createEntry(name, option.value));
                        }
                    } else if (type === 'file') {
                        const files = fileListContents(control.files);
                        if (files.length) for (const file of files) entries.push(createEntry(name, file));
                        else entries.push(createEntry(name,
                            makeFile(new BlobConstructor([], { type: 'application/octet-stream' }), '')));
                    } else if (type === 'hidden' && name.toLowerCase() === '_charset_')
                        entries.push(createEntry(name, 'UTF-8'));
                    else entries.push(createEntry(name, /^(checkbox|radio)$/.test(type) &&
                        !control.hasAttribute('value') ? 'on' : control.value));
                    if (control.dirName) entries.push(createEntry(control.dirName,
                        control.closest('[dir]')?.getAttribute('dir') === 'rtl' ? 'rtl' : 'ltr'));
                }
                dispatch.call(form, trusted(new FormDataEvent('formdata', {
                    bubbles: true, formData: eventFormData
                })));
                entryLists.set(this, copyEntries(entriesOf(eventFormData)));
            } finally {
                formsBeingConstructed.delete(form);
            }
        }
        append(name, value, filename = undefined) {
            if (arguments.length < 2) throw new TypeError('FormData.append requires two arguments');
            entriesOf(this).push(createEntry(name, value, filename, filename !== undefined));
        }
        delete(name) {
            if (arguments.length < 1) throw new TypeError('FormData.delete requires a name');
            name = scalar(name);
            entryLists.set(this, entriesOf(this).filter(entry => entry[0] !== name));
        }
        get(name) {
            if (arguments.length < 1) throw new TypeError('FormData.get requires a name');
            name = scalar(name);
            return entriesOf(this).find(entry => entry[0] === name)?.[1] ?? null;
        }
        getAll(name) {
            if (arguments.length < 1) throw new TypeError('FormData.getAll requires a name');
            name = scalar(name);
            return entriesOf(this).filter(entry => entry[0] === name).map(entry => entry[1]);
        }
        has(name) {
            if (arguments.length < 1) throw new TypeError('FormData.has requires a name');
            name = scalar(name);
            return entriesOf(this).some(entry => entry[0] === name);
        }
        set(name, value, filename = undefined) {
            if (arguments.length < 2) throw new TypeError('FormData.set requires two arguments');
            const entry = createEntry(name, value, filename, filename !== undefined);
            const entries = entriesOf(this);
            const index = entries.findIndex(candidate => candidate[0] === entry[0]);
            if (index < 0) { entries.push(entry); return; }
            entries[index] = entry;
            entryLists.set(this, entries.filter((candidate, position) =>
                position <= index || candidate[0] !== entry[0]));
        }
        forEach(callback, thisArg = undefined) {
            if (typeof callback !== 'function') throw new TypeError('FormData callback must be callable');
            for (let index = 0; index < entriesOf(this).length; index++) {
                const [name, value] = entriesOf(this)[index];
                callback.call(thisArg, value, name, this);
            }
        }
        *entries() {
            for (let index = 0; index < entriesOf(this).length; index++)
                yield [...entriesOf(this)[index]];
        }
        *keys() {
            for (let index = 0; index < entriesOf(this).length; index++)
                yield entriesOf(this)[index][0];
        }
        *values() {
            for (let index = 0; index < entriesOf(this).length; index++)
                yield entriesOf(this)[index][1];
        }
    }
    Object.defineProperty(FormData.prototype, Symbol.iterator, {
        configurable: true, writable: true, value: FormData.prototype.entries
    });
    globalThis.FormData = FormData;
    if (isWindow) {
        globalThis.FormDataEvent = FormDataEvent;
        Object.defineProperty(globalThis, '__constructingFormData', {
            configurable: true, value: form => formsBeingConstructed.has(form)
        });
    }
    Object.defineProperty(globalThis, '__installFormDataFileFactory', {
        configurable: true, value: factory => {
            BlobConstructor = globalThis.Blob;
            FileConstructor = globalThis.File;
            makeFile = factory;
        }
    });
    Object.defineProperty(globalThis, '__formDataEntrySnapshot', {
        configurable: true, value: formData => copyEntries(entriesOf(formData))
    });
})();
