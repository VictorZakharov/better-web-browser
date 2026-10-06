    let nativeDocumentFocused = true;
    let nativeVisibilityState = 'visible';
    document.hasFocus = () => nativeDocumentFocused;
    Object.defineProperties(document, {
        hidden: { configurable: true, get: () => nativeVisibilityState !== 'visible' },
        visibilityState: { configurable: true, get: () => nativeVisibilityState }
    });

    const nativeTarget = id => wrap(Number(id) || 0) || document.body || document;
    // Text commit tracking: change fires when a user-edited control commits
    // (blur or Enter), not for programmatic writes while focused.
    const focusCommitted = new WeakMap();
    const isCommitTarget = target =>
        (target instanceof HTMLInputElement && /^(text|search|tel|url|email|password|number)$/i.test(target.type)) ||
        target instanceof HTMLTextAreaElement;
    // User editing changes the control's value internally, not by invoking an
    // author-installed value setter (e.g. a framework's programmatic-write tracker).
    const nativeValueSetters = [
        [HTMLInputElement, Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set],
        [HTMLTextAreaElement, Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set],
        [HTMLSelectElement, Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set],
        [Element, Object.getOwnPropertyDescriptor(Element.prototype, 'value').set]
    ];
    const nativeModifiers = input => ({
        altKey: !!input.alt, ctrlKey: !!input.control,
        shiftKey: !!input.shift, metaKey: !!input.meta
    });
    let previousPointerPosition = null;
    let pointerLockPosition = null;
    const dispatchNativePointer = input => {
        const locked = pointerLockElement !== null &&
            nativeTarget(input.target) === pointerLockElement;
        const relativeMove = locked && input.phase === 'lockedmove';
        const nativePosition = { x: Number(input.x) - viewportScrollX,
            y: Number(input.y) - viewportScrollY };
        const position = locked ? (pointerLockPosition || nativePosition) : nativePosition;
        const movementX = relativeMove ? Number(input.x) : locked ? 0 :
            previousPointerPosition ? position.x - previousPointerPosition.x : 0;
        const movementY = relativeMove ? Number(input.y) : locked ? 0 :
            previousPointerPosition ? position.y - previousPointerPosition.y : 0;
        if (!locked) previousPointerPosition = input.phase === 'leave' ? null : position;
        const target = nativeTarget(input.target);
        const dispatchPair = (phase, init) => {
            const names = phase === 'down'
                ? ['pointerdown', 'mousedown']
                : ['pointerup', 'mouseup'];
            let allowed = target.dispatchEvent(markTrusted(new PointerEvent(names[0], init)));
            return target.dispatchEvent(markTrusted(new MouseEvent(names[1], init))) && allowed;
        };
        // A mouse chord changes PointerEvent.button via pointermove, not another
        // pointerdown/up. Legacy mouse events still describe each changed button.
        // https://www.w3.org/TR/pointerevents3/#chorded-button-interactions
        const changedMask = [1, 4, 2][input.button] || 0;
        const names = {
            move: ['pointermove', 'mousemove'],
            lockedmove: ['pointermove', 'mousemove'],
            down: [input.buttons === changedMask ? 'pointerdown' : 'pointermove', 'mousedown'],
            up: [input.buttons ? 'pointermove' : 'pointerup', 'mouseup']
        }[input.phase] || [];
        const init = {
            bubbles: true, cancelable: true, composed: true,
            // The renderer hit-tests document coordinates; DOM client coordinates use viewport space.
            clientX: position.x, clientY: position.y, movementX, movementY,
            view: windowObject,
            button: input.button, buttons: input.buttons,
            pointerId: 1, pointerType: 'mouse', isPrimary: true,
            pressure: input.buttons ? 0.5 : 0,
            ...nativeModifiers(input)
        };
        if (!locked && processDragPointer(input, target, init)) return true;
        if (!locked) dispatchPointerBoundary(input.boundary, init);
        if (input.phase === 'leave') return true;
        if (input.phase === 'activate') {
            dispatchPair('down', { ...init, buttons: 1, pressure: 0.5 });
            dispatchPair('up', { ...init, buttons: 0, pressure: 0 });
            const name = input.button === 1 ? 'auxclick' : 'click';
            return target.dispatchEvent(markTrusted(new MouseEvent(name, { ...init, buttons: 0 })));
        }
        let allowed = true;
        if (names[0]) allowed = target.dispatchEvent(markTrusted(new PointerEvent(names[0], {
            ...init, button: input.phase === 'move' || relativeMove ? -1 : init.button
        }))) && allowed;
        if (names[1]) allowed = target.dispatchEvent(markTrusted(new MouseEvent(names[1], init))) && allowed;
        if (input.phase === 'up' && input.button === 2) {
            allowed = target.dispatchEvent(markTrusted(new MouseEvent('contextmenu', init))) && allowed;
        }
        if (input.activate) {
            const name = input.button === 1 ? 'auxclick' : 'click';
            return target.dispatchEvent(markTrusted(new MouseEvent(name, init)));
        }
        return allowed;
    };
    const dispatchNativeKeyboard = input => {
        // The DND processing model suppresses device input until the drag ends;
        // Escape is the one keyboard action that aborts it.
        if (pointerDrag?.active) {
            if (input.phase === 'down' && input.key === 'Escape') cancelPointerDrag();
            return true;
        }
        const target = nativeTarget(input.target);
        const allowed = target.dispatchEvent(markTrusted(new KeyboardEvent(
        input.phase === 'down' ? 'keydown' : 'keyup', {
            bubbles: true, cancelable: true, composed: true,
            key: input.key, code: input.code, repeat: !!input.repeat,
            keyCode: Number(input.keyCode) || 0, ...nativeModifiers(input)
        }
        )));
        if (allowed && input.phase === 'down' && !input.repeat &&
            (input.key === 'Enter' || input.key === ' ') &&
            target instanceof HTMLInputElement && target.type === 'file') {
            target.dispatchEvent(markTrusted(new MouseEvent('click', {
                bubbles: true, cancelable: true, composed: true, button: 0
            })));
        }
        if (allowed && input.phase === 'down' && input.key === 'Enter') implicitSubmission(target);
        return allowed;
    };
    const authoritativeText = target => [String(target.value),
        Number(target.selectionStart) || 0, Number(target.selectionEnd) || 0];
    // Read rollback state after the native event's microtask checkpoint. A
    // beforeinput listener may cancel the edit and then change the value in a
    // Promise job; the renderer's final DOM state must win over Win32's edit.
    Object.defineProperty(document, '__nativeTextSnapshot', {
        configurable: false,
        value(id) {
            const target = wrap(Number(id) || 0);
            if (!(target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)) return null;
            const snapshot = authoritativeText(target);
            // Bound the JS serialization before Rust applies the exact UTF-8
            // MAX_RENDERER_TEXT_INPUT_BYTES (64 KiB) codec limit.
            return snapshot[0].length <= 65536 ? snapshot : null;
        }
    });
    const nativeEditData = (previous, next, start, end, inputType) => {
        const prefix = previous.slice(0, start);
        const suffix = previous.slice(end);
        if (inputType === 'insertText') {
            if (!next.startsWith(prefix) || !next.endsWith(suffix) ||
                next.length < prefix.length + suffix.length) return undefined;
            return next.slice(start, next.length - suffix.length);
        }
        if (start !== end) return next === prefix + suffix ? null : undefined;
        if (inputType === 'deleteContentBackward') {
            const remaining = next.slice(0, next.length - suffix.length);
            return next.length < previous.length && next.endsWith(suffix) &&
                prefix.startsWith(remaining) ? null : undefined;
        }
        if (inputType === 'deleteContentForward') {
            return next.length < previous.length && next.startsWith(prefix) &&
                suffix.endsWith(next.slice(prefix.length)) ? null : undefined;
        }
        return undefined;
    };
    const dispatchNativeText = input => {
        if (pointerDrag?.active) return input.kind === 'nativeText' ? [false] : true;
        const target = nativeTarget(input.target);
        const value = String(input.value);
        const ordinaryEdit = input.kind === 'nativeText' &&
            target instanceof HTMLInputElement && /^(text|search|password)$/i.test(target.type);
        // Input Events Level 2: typed text and ordinary backward/forward deletion
        // get a cancelable beforeinput before the DOM edit, then input after it.
        // Paste, IME composition, accessibility replacements, and other
        // unclassified Win32 changes are not falsely labeled insertText: they
        // retain the legacy commit path with inputType "" until separately modeled.
        // https://www.w3.org/TR/input-events-2/#interface-InputEvent-Attributes
        const inputType = ordinaryEdit ? String(input.inputType || '') : 'insertText';
        let editData = null;
        if (ordinaryEdit && inputType) {
            const before = String(target.value);
            const start = input.beforeSelectionStart;
            const end = input.beforeSelectionEnd;
            if (!Number.isInteger(start) || !Number.isInteger(end) ||
                start < 0 || start > end || end > before.length) {
                return [false];
            }
            editData = nativeEditData(before, value, start, end, inputType);
            if (editData === undefined) return [false];
            // Native ingress must not echo a pre-edit value snapshot back to the
            // HWND; only author selection changes enqueue mirror actions.
            try { __applyNativeTextSelection(target, start, end, 'none'); }
            catch (_error) { return [false]; }
            const allowed = target.dispatchEvent(markTrusted(new InputEvent('beforeinput', {
                bubbles: true, cancelable: true, composed: true, inputType, data: editData
            })));
            // The native EDIT has already changed. The host reads the renderer's
            // authoritative state after Promise jobs for generation-scoped rollback.
            if (!allowed || String(target.value) !== before ||
                target.selectionStart !== start || target.selectionEnd !== end) {
                return [false];
            }
        }
        // A user edit is an internal operation, not the programmatic value
        // setter: it sets the dirty flag, marks user-edited length state, and
        // retains unconvertible number text for badInput.
        let changed = true;
        if (target instanceof HTMLInputElement) {
            changed = host('inputUserEdit', nodeId(target), value);
        } else if (target instanceof HTMLTextAreaElement) {
            changed = host('textareaUserEdit', nodeId(target), value);
        } else if (target instanceof HTMLSelectElement) {
            changed = host('selectUserPick', nodeId(target), value);
        } else {
            const setter = nativeValueSetters.find(([kind]) => target instanceof kind)?.[1];
            if (setter) Reflect.apply(setter, target, [value]);
        }
        if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)
            __applyNativeTextSelection(target, input.selectionStart, input.selectionEnd, input.direction);
        refreshPatternVerdict(target);
        const allowed = target.dispatchEvent(markTrusted(new InputEvent('input', {
            bubbles: true, composed: true, inputType, data: editData
        })));
        if (target instanceof HTMLSelectElement && changed) {
            target.dispatchEvent(markTrusted(new Event('change', { bubbles: true })));
        }
        if (isCommitTarget(target)) {
            const entry = focusCommitted.get(target) || { edited: false };
            entry.edited = true;
            focusCommitted.set(target, entry);
        } else if (target instanceof HTMLInputElement && target.type === 'range' && changed) {
            target.dispatchEvent(markTrusted(new Event('change', { bubbles: true })));
        }
        return input.kind === 'nativeText' ? [true] : allowed;
    };
    const dispatchNativeFocus = input => {
        const next = input.focused ? focusTargetForElement(nativeTarget(input.target)) : null;
        const previous = focusedAreaForDocument(document);
        if (previous === next && nativeDocumentFocused === !!input.focused) return true;
        // A user-edited control commits its change on blur.
        if (previous && previous !== next) {
            const entry = focusCommitted.get(previous);
            focusCommitted.delete(previous);
            if (entry?.edited && isCommitTarget(previous) && previous.isConnected) {
                previous.dispatchEvent(markTrusted(new Event('change', { bubbles: true })));
            }
        }
        if (next && isCommitTarget(next)) focusCommitted.set(next, { edited: false });
        const wasDocumentFocused = nativeDocumentFocused;
        host('setFocus', next ? nodeId(next) : 0);
        setFocusedAreaForDocument(document, next);
        nativeDocumentFocused = !!input.focused;
        dispatchFocusTransition(previous, next, true);
        if (wasDocumentFocused !== nativeDocumentFocused) {
            windowObject.dispatchEvent(markTrusted(new FocusEvent(input.focused ? 'focus' : 'blur')));
        }
        return true;
    };
    const dispatchNativeSimple = input => nativeTarget(input.target).dispatchEvent(markTrusted(new Event(
        String(input.type), { bubbles: !!input.bubbles, cancelable: !!input.cancelable }
    )));
    const dispatchNativeImageResource = input => {
        const target = nativeTarget(input.target);
        updateImageElementState(target, true, input.naturalWidth, input.naturalHeight, input.type === 'error');
        return target.dispatchEvent(markTrusted(new Event(String(input.type))));
    };

    Object.defineProperty(document, '__dispatchNativeInput', {
        configurable: false,
        value(input) {
            switch (input.kind) {
                case 'fragmentNavigation': navigateLocation(input.url, false); return true;
                case 'wheel': return nativeTarget(input.target).dispatchEvent(markTrusted(new WheelEvent('wheel', {
                    bubbles: true, cancelable: true, composed: true, view: windowObject,
                    clientX: input.x - viewportScrollX, clientY: input.y - viewportScrollY,
                    deltaX: input.deltaX, deltaY: input.deltaY, deltaMode: 0, ...nativeModifiers(input)
                })));
                case 'elementScroll': queueElementScrollEvent(nativeTarget(input.target), true); return true;
                case 'pointer': return dispatchNativePointer(input);
                case 'keyboard': return dispatchNativeKeyboard(input);
                case 'text': return dispatchNativeText(input);
                case 'nativeText': return dispatchNativeText(input);
                case 'selection': {
                    const target = nativeTarget(input.target);
                    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)
                        __applyNativeTextSelection(target, input.selectionStart, input.selectionEnd, input.direction);
                    return true;
                }
                case 'focus': return dispatchNativeFocus(input);
                case 'simple': return dispatchNativeSimple(input);
                case 'imageResource': return dispatchNativeImageResource(input);
                case 'scroll': {
                    if (!setViewportScrollOffsets(Number(input.x) || 0, Number(input.y) || 0)) return true;
                    viewportScrollDelay = userScrollEndDelay;
                    if (viewportScrollEventPending) return true;
                    // CSSOM View viewport scroll events target Document and bubble to Window.
                    const allowed = document.dispatchEvent(markTrusted(new Event('scroll', { bubbles: true })));
                    if (!viewportScrollEventPending) {
                        queueScrollEnd(document, userScrollEndDelay, true);
                        viewportScrollDelay = 0;
                    }
                    return allowed;
                }
                case 'viewport':
                    windowObject.innerWidth = Math.round(Number(input.width) || 1);
                    windowObject.innerHeight = Math.round(Number(input.height) || 1);
                    layoutViewportWidth = Number(input.layoutWidth) || 1;
                    layoutViewportHeight = Number(input.layoutHeight) || 1;
                    windowObject.devicePixelRatio = Number(input.scale) || 1;
                    const mediaChanges = prepareMediaQueryChanges();
                    const resizeAllowed =
                        windowObject.dispatchEvent(markTrusted(new UIEvent('resize')));
                    dispatchMediaQueryChanges(mediaChanges);
                    return resizeAllowed;
                case 'lifecycle': {
                    const next = input.state === 'active' ? 'visible' : 'hidden';
                    if (next !== nativeVisibilityState) {
                        nativeVisibilityState = next;
                        if (next === 'hidden' && typeof releaseAllWakeLocks === 'function') releaseAllWakeLocks();
                        document.dispatchEvent(markTrusted(new Event('visibilitychange')));
                    }
                    if (input.state === 'frozen') document.dispatchEvent(markTrusted(new Event('freeze')));
                    else if (input.previous === 'frozen') document.dispatchEvent(markTrusted(new Event('resume')));
                    return true;
                }
                case 'fullscreen': return applyFullscreenResponse(input);
                case 'pointerLock': return applyPointerLockResponse(input);
                case 'wakeLock': return typeof applyWakeLockUpdate === 'function' && applyWakeLockUpdate(input);
                case 'media': return applyMediaResponse(input);
                case 'mediaSource': return applyOrdinaryMediaSourceEvent(input);
                default: return false;
            }
        }
    });
