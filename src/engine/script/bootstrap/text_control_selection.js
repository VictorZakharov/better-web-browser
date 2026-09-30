    // HTML text-control selections use UTF-16 code-unit offsets into the API
    // value, even while a control is detached or has no native edit window.
    // https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#textFieldSelection
    const textSelectionStates = new WeakMap();
    const textSelectionChangeQueued = new WeakSet();
    const textSelectionGetAttribute = Element.prototype.getAttribute;
    const textSelectionDispatch = EventTarget.prototype.dispatchEvent;
    const TextSelectionEvent = Event;
    const nonselectableInputTypes = new Set(['hidden', 'email', 'date', 'month', 'week', 'time',
        'datetime-local', 'number', 'range', 'color', 'checkbox', 'radio', 'file', 'submit',
        'image', 'reset', 'button']);
    const inputTypeHasTextSelection = type =>
        !nonselectableInputTypes.has(String(type || '').toLowerCase());
    const textSelectionApplies = control => {
        if (control instanceof HTMLTextAreaElement) return true;
        if (control instanceof HTMLInputElement)
            return inputTypeHasTextSelection(textSelectionGetAttribute.call(control, 'type'));
        throw new TypeError('Illegal text control receiver');
    };
    const requireTextSelection = control => {
        if (!textSelectionApplies(control))
            throw new DOMException('The input type has no text selection', 'InvalidStateError');
    };
    const textSelectionValue = control => String(host(control instanceof HTMLTextAreaElement
        ? 'textareaValue' : 'inputValue', nodeId(control)));
    const rawTextSelectionState = control => {
        let state = textSelectionStates.get(control);
        if (!state) {
            state = { start: 0, end: 0, direction: 'none' };
            textSelectionStates.set(control, state);
        }
        return state;
    };
    const textSelectionState = control => {
        const state = rawTextSelectionState(control);
        const length = textSelectionValue(control).length;
        state.start = Math.min(state.start, length);
        state.end = Math.min(state.end, length);
        return state;
    };
    // WebIDL integer conversion uses ToNumber, which rejects BigInt even when
    // it came from an object's valueOf; Number() would accept it instead.
    const textSelectionUnsigned = value => +value >>> 0;
    const textSelectionString = value => {
        if (typeof value === 'symbol') throw new TypeError('A string is required');
        return String(value);
    };
    const textSelectionDirection = value => {
        value = textSelectionString(value);
        return value === 'forward' || value === 'backward' ? value : 'none';
    };
    const queueTextSelectionEvent = (control, type) => queueTimer(() =>
        textSelectionDispatch.call(control, markTrusted(new TextSelectionEvent(type, { bubbles: true }))),
        0, false, [], 'text selection event');
    const notifyTextSelection = (control, select) => {
        // Selection API coalesces selectionchange per target, while HTML queues
        // one select task for each modification made by set selection range.
        if (!textSelectionChangeQueued.has(control)) {
            textSelectionChangeQueued.add(control);
            queueTimer(() => {
                textSelectionChangeQueued.delete(control);
                textSelectionDispatch.call(control, markTrusted(new TextSelectionEvent(
                    'selectionchange', { bubbles: true })));
            }, 0, false, [], 'text selectionchange event');
        }
        if (select) queueTextSelectionEvent(control, 'select');
    };
    const updateTextSelection = (control, start, end, direction, source, select) => {
        const state = rawTextSelectionState(control);
        const length = textSelectionValue(control).length;
        start = Math.min(textSelectionUnsigned(start), length);
        end = Math.min(textSelectionUnsigned(end), length);
        if (end <= start) start = end;
        direction = textSelectionDirection(direction);
        const changed = state.start !== start || state.end !== end || state.direction !== direction;
        state.start = start; state.end = end; state.direction = direction;
        if (source === 'script') host('textSelectionSet', nodeId(control), start, end, direction);
        if (changed) notifyTextSelection(control, select);
    };
    const textSelectionStart = control => {
        if (!textSelectionApplies(control)) return null;
        return textSelectionState(control).start;
    };
    const textSelectionEnd = control => {
        if (!textSelectionApplies(control)) return null;
        return textSelectionState(control).end;
    };
    const textSelectionDirectionValue = control => {
        if (!textSelectionApplies(control)) return null;
        return textSelectionState(control).direction;
    };
    const setTextSelectionStart = (control, value) => {
        requireTextSelection(control);
        const state = textSelectionState(control), start = textSelectionUnsigned(value);
        updateTextSelection(control, start, Math.max(start, state.end), state.direction, 'script', true);
    };
    const setTextSelectionEnd = (control, value) => {
        requireTextSelection(control);
        const state = textSelectionState(control);
        updateTextSelection(control, state.start, value, state.direction, 'script', true);
    };
    const setTextSelectionDirection = (control, value) => {
        requireTextSelection(control);
        const state = textSelectionState(control);
        updateTextSelection(control, state.start, state.end, value, 'script', true);
    };
    const setTextSelectionRange = (control, start, end, direction = 'none') => {
        requireTextSelection(control);
        updateTextSelection(control, start, end, direction, 'script', true);
    };
    const selectTextControl = control => {
        if (!textSelectionApplies(control)) return;
        updateTextSelection(control, 0, textSelectionValue(control).length, 'none', 'script', true);
    };
    const textSelectionValueChanged = (control, oldValue, newValue, moveToEnd = true) => {
        if (!textSelectionApplies(control) || oldValue === newValue) return;
        const state = rawTextSelectionState(control);
        const end = newValue.length;
        if (moveToEnd) updateTextSelection(control, end, end, 'none', 'script', false);
        else updateTextSelection(control, Math.min(state.start, end), Math.min(state.end, end),
            state.direction, 'script', false);
    };
    const reconcileTextSelectionValue = control => {
        if (!textSelectionApplies(control)) return;
        const state = rawTextSelectionState(control);
        const length = textSelectionValue(control).length;
        // Reset/default-value changes also mirror the value when no range changes.
        updateTextSelection(control, Math.min(state.start, length), Math.min(state.end, length),
            state.direction, 'script', false);
    };
    const textInputTypeChanged = (control, oldType, newType) => {
        if (inputTypeHasTextSelection(oldType) || !inputTypeHasTextSelection(newType)) return;
        textSelectionStates.set(control, { start: 0, end: 0, direction: 'none' });
        host('textSelectionSet', nodeId(control), 0, 0, 'none');
    };
    const __applyNativeTextSelection = (control, start, end, direction = 'none') => {
        if (!textSelectionApplies(control)) return;
        updateTextSelection(control, start, end, direction, 'native', true);
    };
    const setTextRangeText = (control, replacement, start, end, mode, argumentCount) => {
        requireTextSelection(control);
        if (argumentCount < 1 || argumentCount === 2)
            throw new TypeError('setRangeText requires one or at least three arguments');
        replacement = textSelectionString(replacement);
        mode = argumentCount >= 4 && mode !== undefined ? textSelectionString(mode) : 'preserve';
        if (!['select', 'start', 'end', 'preserve'].includes(mode))
            throw new TypeError('Invalid selection mode');
        const state = textSelectionState(control);
        const oldValue = textSelectionValue(control);
        const oldStart = state.start, oldEnd = state.end;
        const operation = control instanceof HTMLTextAreaElement ? 'textareaSetValue' : 'inputSetValue';
        // setRangeText sets the dirty value flag even when its range is invalid.
        host(operation, nodeId(control), oldValue);
        if (argumentCount === 1) { start = oldStart; end = oldEnd; }
        else { start = textSelectionUnsigned(start); end = textSelectionUnsigned(end); }
        if (start > end) throw new DOMException('Start exceeds end', 'IndexSizeError');
        start = Math.min(start, oldValue.length);
        end = Math.min(end, oldValue.length);
        const newValue = oldValue.slice(0, start) + replacement + oldValue.slice(end);
        host(operation, nodeId(control), newValue);
        if (control instanceof HTMLInputElement) refreshPatternVerdict(control);
        const newEnd = start + replacement.length;
        if (mode === 'select') { updateTextSelection(control, start, newEnd, 'none', 'script', true); return; }
        if (mode === 'start') { updateTextSelection(control, start, start, 'none', 'script', true); return; }
        if (mode === 'end') { updateTextSelection(control, newEnd, newEnd, 'none', 'script', true); return; }
        // The preserve adjustments apply only to the one-argument overload;
        // explicit ranges retain the old selection, subject to final clamping.
        const delta = replacement.length - (end - start);
        const preservedStart = argumentCount === 1 && oldStart > end ? oldStart + delta
            : argumentCount === 1 && oldStart > start ? start : oldStart;
        const preservedEnd = argumentCount === 1 && oldEnd > end ? oldEnd + delta
            : argumentCount === 1 && oldEnd > start ? newEnd : oldEnd;
        updateTextSelection(control, preservedStart, preservedEnd, 'none', 'script', true);
    };
