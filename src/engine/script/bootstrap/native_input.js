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
        const target = nativeTarget(input.target);
        const allowed = target.dispatchEvent(markTrusted(new KeyboardEvent(
        input.phase === 'down' ? 'keydown' : 'keyup', {
            bubbles: true, cancelable: true, composed: true,
            key: input.key, code: input.code, repeat: !!input.repeat,
            keyCode: Number(input.keyCode) || 0, ...nativeModifiers(input)
        }
        )));
        if (allowed && input.phase === 'down' && input.key === 'Enter') implicitSubmission(target);
        return allowed;
    };
    const dispatchNativeText = input => {
        const target = nativeTarget(input.target);
        const value = String(input.value);
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
        if (typeof target.setSelectionRange === 'function') {
            try { target.setSelectionRange(input.selectionStart, input.selectionEnd); } catch (_error) {}
        }
        refreshPatternVerdict(target);
        const allowed = target.dispatchEvent(markTrusted(new InputEvent('input', {
            bubbles: true, composed: true, inputType: 'insertText', data: null
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
        return allowed;
    };
    const dispatchNativeFocus = input => {
        const next = input.focused ? nativeTarget(input.target) : null;
        const previous = document.activeElement;
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
        if (previous) {
            previous.dispatchEvent(markTrusted(new FocusEvent('blur', { relatedTarget: next })));
            previous.dispatchEvent(markTrusted(new FocusEvent('focusout', { bubbles: true, relatedTarget: next })));
        }
        document.activeElement = next;
        nativeDocumentFocused = !!input.focused;
        if (next) {
            next.dispatchEvent(markTrusted(new FocusEvent('focus', { relatedTarget: previous })));
            next.dispatchEvent(markTrusted(new FocusEvent('focusin', { bubbles: true, relatedTarget: previous })));
        }
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
        updateImageElementState(target, true, input.naturalWidth, input.naturalHeight);
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
                        document.dispatchEvent(markTrusted(new Event('visibilitychange')));
                    }
                    if (input.state === 'frozen') document.dispatchEvent(markTrusted(new Event('freeze')));
                    else if (input.previous === 'frozen') document.dispatchEvent(markTrusted(new Event('resume')));
                    return true;
                }
                case 'fullscreen': return applyFullscreenResponse(input);
                case 'pointerLock': return applyPointerLockResponse(input);
                case 'media': return applyMediaResponse(input);
                default: return false;
            }
        }
    });
