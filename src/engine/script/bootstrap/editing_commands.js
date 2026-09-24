    // The legacy Editing API is still used by editors. Keep the supported set
    // intentionally small: an advertised command must perform an edit, while
    // unsupported commands report false instead of pretending to succeed.
    // https://w3c.github.io/editing/docs/execCommand/
    const editingCommands = new Set([
        'selectall', 'inserttext', 'inserthtml', 'delete'
    ]);
    const textControl = element => element instanceof HTMLTextAreaElement ||
        (element instanceof HTMLInputElement &&
            /^(text|search|tel|url|email|password)$/.test(element.type));
    const editorContext = () => {
        const active = document.activeElement;
        if (textControl(active)) return { control: active, host: active, range: null };
        const selection = documentSelection;
        const range = selection.rangeCount ? selection.getRangeAt(0) : null;
        const focus = range?.startContainer ?? active;
        const element = focus instanceof Element ? focus : focus?.parentElement;
        const host = element && editingHost(element);
        return host && (!range || host.contains(range.endContainer))
            ? { control: null, host, range } : null;
    };
    const commandName = command => String(command).toLowerCase();
    const editTextControl = (control, command, value) => {
        if (command === 'selectall') { control.select(); return true; }
        if (command !== 'inserttext' && command !== 'delete') return false;
        const old = control.value;
        let start = control.selectionStart, end = control.selectionEnd;
        if (command === 'delete' && start === end && start > 0) {
            start--;
            if (start > 0 && /[\uDC00-\uDFFF]/.test(old[start]) &&
                /[\uD800-\uDBFF]/.test(old[start - 1])) start--;
        }
        if (command === 'delete' && start === end) return false;
        const inserted = command === 'inserttext' ? String(value) : '';
        const inputType = command === 'delete' ? 'deleteContentBackward' : 'insertText';
        if (!control.dispatchEvent(new InputEvent('beforeinput', {
            bubbles: true, cancelable: true, composed: true, inputType,
            data: command === 'delete' ? null : inserted
        }))) return false;
        control.value = old.slice(0, start) + inserted + old.slice(end);
        control.setSelectionRange(start + inserted.length, start + inserted.length);
        control.dispatchEvent(new InputEvent('input', {
            bubbles: true, composed: true,
            inputType,
            data: command === 'delete' ? null : inserted
        }));
        return true;
    };
    const editSelection = (context, command, value) => {
        if (command === 'selectall') {
            documentSelection.selectAllChildren(context?.host ?? document.body);
            return true;
        }
        if (!context?.range) return false;
        const range = context.range;
        const inputType = command === 'delete' ? 'deleteContentBackward' :
            command === 'inserttext' ? 'insertText' : 'insertFromPaste';
        if (command === 'delete' && range.collapsed) {
            if (!range.startOffset) return false;
            if (!(range.startContainer instanceof Text)) {
                const previous = range.startContainer.childNodes[range.startOffset - 1];
                if (!previous) return false;
                if (previous instanceof Text)
                    range.setStart(previous, previous.length);
                else {
                    range.setStartBefore(previous);
                    return editSelection(context, command, value);
                }
            }
            let start = range.startOffset - 1;
            const data = range.startContainer.data;
            if (start > 0 && /[\uDC00-\uDFFF]/.test(data[start]) &&
                /[\uD800-\uDBFF]/.test(data[start - 1])) start--;
            range.setStart(range.startContainer, start);
        }
        if (!context.host.dispatchEvent(new InputEvent('beforeinput', {
            bubbles: true, cancelable: true, composed: true, inputType,
            data: command === 'inserttext' ? String(value) : null
        }))) return false;
        let caret = null;
        if (command === 'delete') {
            range.deleteContents();
            caret = { node: range.startContainer, offset: range.startOffset };
        } else if (command === 'inserttext' || command === 'inserthtml') {
            range.deleteContents();
            const fragment = command === 'inserthtml'
                ? range.createContextualFragment(String(value)) : document.createDocumentFragment();
            if (command === 'inserttext') fragment.appendChild(document.createTextNode(String(value)));
            const last = fragment.lastChild;
            if (last) {
                range.insertNode(fragment);
                caret = { node: last.parentNode, offset: nodeIndex(last) + 1 };
            } else caret = { node: range.startContainer, offset: range.startOffset };
        }
        if (caret) documentSelection.collapse(caret.node, caret.offset);
        context.host.dispatchEvent(new InputEvent('input', {
            bubbles: true, composed: true, inputType,
            data: command === 'inserttext' ? String(value) : null
        }));
        return true;
    };
    Document.prototype.execCommand = function(command, _showUI = false, value = '') {
        if (this !== document) throw new TypeError('Invalid Document receiver');
        command = commandName(command);
        if (!editingCommands.has(command)) return false;
        const context = editorContext();
        if (command !== 'selectall' && !context) return false;
        return context?.control ? editTextControl(context.control, command, value) :
            editSelection(context, command, value);
    };
    Document.prototype.queryCommandSupported = function(command) {
        return editingCommands.has(commandName(command));
    };
    Document.prototype.queryCommandEnabled = function(command) {
        command = commandName(command);
        if (!editingCommands.has(command)) return false;
        if (command === 'selectall') return true;
        const context = editorContext();
        if (!context) return false;
        return context.control ? ['inserttext', 'delete'].includes(command) :
            !!context.range;
    };
    Document.prototype.queryCommandState = function(_command) { return false; };
    Document.prototype.queryCommandIndeterm = function(_command) { return false; };
    Document.prototype.queryCommandValue = function(_command) { return ''; };
