    // A selected FileList is realm-owned; the native control mirrors only
    // bounded basenames for value, selectors, layout and required validity.
    const inputFileSelections = new WeakMap();
    const inputFileSelectionObservers = new WeakMap();
    const inputFilePickerRequests = new WeakMap();
    const filePickerInputs = new Map();
    const parseFilePickerUpdate = JSON.parse.bind(JSON);
    const stringifyFileNames = JSON.stringify.bind(JSON);
    const dispatchFileEvent = EventTarget.prototype.dispatchEvent;
    const FileInputEvent = Event;
    const FileInputBytes = Uint8Array;
    let decodeFilePickerBytes;
    let makePickedFile;
    Object.defineProperty(globalThis, '__installFilePickerFileFactory', {
        configurable: true, value: factory => {
            makePickedFile = factory;
            decodeFilePickerBytes = globalThis.atob;
        }
    });
    const invalidateInputFileSelection = input => {
        inputFileSelectionObservers.get(input)?.();
        inputFileSelectionObservers.delete(input);
        const id = inputFilePickerRequests.get(input);
        if (id) {
            filePickerInputs.delete(id);
            host('filePickerCancel', id, nodeId(input));
        }
        inputFileSelections.delete(input);
        inputFilePickerRequests.delete(input);
    };
    const utf8FileNameLength = name => {
        let bytes = 0;
        for (let index = 0; index < name.length; index++) {
            const code = name.charCodeAt(index);
            if (code < 0x80) bytes++;
            else if (code < 0x800) bytes += 2;
            else if (code >= 0xd800 && code <= 0xdbff &&
                index + 1 < name.length && name.charCodeAt(index + 1) >= 0xdc00 &&
                name.charCodeAt(index + 1) <= 0xdfff) { bytes += 4; index++; }
            else bytes += 3;
        }
        return bytes;
    };
    const validInputFileName = name => utf8FileNameLength(name) <= 255 &&
        !/[/\\\0\r\n]/.test(name);
    const mirrorInputFileSelection = (input, list) => {
        // DataTransfer.files remains live after assignment. Keep native value
        // and required validity current, while bounding its basename mirror.
        const names = fileListContents(list).slice(0, 128).map(file =>
            validInputFileName(file.name) ? file.name : 'file');
        host('inputSetFiles', nodeId(input), stringifyFileNames(names));
    };
    const selectInputFileList = (input, list) => {
        invalidateInputFileSelection(input);
        inputFileSelections.set(input, list);
        mirrorInputFileSelection(input, list);
        inputFileSelectionObservers.set(input, observeFileList(list, () => {
            if (input.type === 'file' && inputFileSelections.get(input) === list)
                mirrorInputFileSelection(input, list);
        }));
    };
    const clearInputFileSelection = input => {
        invalidateInputFileSelection(input);
        host('inputSetFiles', nodeId(input), '[]');
    };
    const requestFilePicker = target => {
        if (!target.isConnected || inputFilePickerRequests.has(target)) return;
        const id = Number(host('filePickerRequest', nodeId(target)));
        if (id) {
            inputFilePickerRequests.set(target, id);
            filePickerInputs.set(id, target);
        }
    };
    function activateFileInput(target, event) {
        if (!(target instanceof HTMLInputElement) || target.type !== 'file' ||
            event.type !== 'click' || event.defaultPrevented ||
            event.button !== 0 || target.matches(':disabled')) return;
        // The native host admits this only while a real pointer or keyboard
        // activation is dispatching, including input.click() in its handler.
        requestFilePicker(target);
    }
    Object.defineProperty(globalThis, '__receiveFilePickerUpdate', {
        configurable: true,
        value(payload) {
            const update = parseFilePickerUpdate(String(payload));
            const id = Number(update.id);
            const input = filePickerInputs.get(id);
            if (!input || inputFilePickerRequests.get(input) !== id) return;
            filePickerInputs.delete(id);
            inputFilePickerRequests.delete(input);
            if (input.type !== 'file' || !input.isConnected) return;
            if (update.kind === 'canceled') {
                dispatchFileEvent.call(input, markTrusted(new FileInputEvent('cancel', { bubbles: true })));
                return;
            }
            if (update.kind !== 'selected' || !Array.isArray(update.files)) return;
            const files = update.files.map(entry => {
                const binary = decodeFilePickerBytes(entry.data);
                const bytes = new FileInputBytes(binary.length);
                for (let index = 0; index < binary.length; index++)
                    bytes[index] = binary.charCodeAt(index);
                return makePickedFile(bytes, entry.name, entry.type, entry.lastModified);
            });
            selectInputFileList(input, new FileList(fileListToken, files));
            dispatchFileEvent.call(input, markTrusted(new FileInputEvent('input', { bubbles: true, composed: true })));
            dispatchFileEvent.call(input, markTrusted(new FileInputEvent('change', { bubbles: true })));
        }
    });
    Object.defineProperties(HTMLInputElement.prototype, {
        showPicker: {
            configurable: true, writable: true,
            value() {
                if (this.type !== 'file') return;
                if (this.matches(':disabled'))
                    throw new DOMException('The file input is disabled', 'InvalidStateError');
                if (!host('filePickerHasActivation'))
                    throw new DOMException('A user activation is required', 'NotAllowedError');
                requestFilePicker(this);
            }
        },
        files: {
            enumerable: true, configurable: true,
            get() {
                if (this.type !== 'file') return null;
                let selected = inputFileSelections.get(this);
                if (!selected) {
                    selected = new FileList(fileListToken, []);
                    inputFileSelections.set(this, selected);
                }
                return selected;
            },
            set(value) {
                if (this.type !== 'file') return;
                // HTML's nullable setter leaves the current selection untouched.
                if (value === null) return;
                if (!(value instanceof FileList)) throw new TypeError('files requires a FileList or null');
                const files = fileListContents(value);
                if (files.length > 128) throw new TypeError('Too many selected files');
                if (files.some(file => !(file instanceof File) ||
                    !validInputFileName(file.name)))
                    throw new TypeError('Invalid file selection');
                selectInputFileList(this, fileListForInputAssignment(value));
            }
        },
        accept: {
            enumerable: true, configurable: true,
            get() { return this.getAttribute('accept') || ''; },
            set(value) { this.setAttribute('accept', String(value)); }
        }
    });
