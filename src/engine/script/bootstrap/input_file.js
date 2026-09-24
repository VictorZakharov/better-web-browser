    // A selected FileList is realm-owned; the native control mirrors only
    // bounded basenames for value, selectors, layout and required validity.
    const inputFileSelections = new WeakMap();
    const clearInputFileSelection = input => {
        inputFileSelections.delete(input);
        host('inputSetFiles', nodeId(input), '[]');
    };
    Object.defineProperties(HTMLInputElement.prototype, {
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
                if (value === null) { clearInputFileSelection(this); return; }
                if (!(value instanceof FileList)) throw new TypeError('files requires a FileList or null');
                if (value.length > 128) throw new TypeError('Too many selected files');
                const files = Array.from(value);
                if (files.some(file => !(file instanceof File) ||
                    file.name.length > 255 || /[/\\\0]/.test(file.name)))
                    throw new TypeError('Invalid file selection');
                host('inputSetFiles', nodeId(this), JSON.stringify(files.map(file => file.name)));
                inputFileSelections.set(this, value);
            }
        },
        accept: {
            enumerable: true, configurable: true,
            get() { return this.getAttribute('accept') || ''; },
            set(value) { this.setAttribute('accept', String(value)); }
        }
    });
