    let nativeDocumentFocused = true;
    let nativeVisibilityState = 'visible';
    document.hasFocus = () => nativeDocumentFocused;
    Object.defineProperties(document, {
        hidden: { configurable: true, get: () => nativeVisibilityState !== 'visible' },
        visibilityState: { configurable: true, get: () => nativeVisibilityState }
    });

    const nativeTarget = id => wrap(Number(id) || 0) || document.body || document;
    const nativeModifiers = input => ({
        altKey: !!input.alt, ctrlKey: !!input.control,
        shiftKey: !!input.shift, metaKey: !!input.meta
    });
    const dispatchNativePointer = input => {
        const target = nativeTarget(input.target);
        const dispatchPair = (phase, init) => {
            const names = phase === 'down'
                ? ['pointerdown', 'mousedown']
                : ['pointerup', 'mouseup'];
            let allowed = target.dispatchEvent(markTrusted(new PointerEvent(names[0], init)));
            return target.dispatchEvent(markTrusted(new MouseEvent(names[1], init))) && allowed;
        };
        const names = {
            move: ['pointermove', 'mousemove'],
            down: ['pointerdown', 'mousedown'],
            up: ['pointerup', 'mouseup']
        }[input.phase] || [];
        const init = {
            bubbles: true, cancelable: true, composed: true,
            clientX: input.x, clientY: input.y,
            button: input.button, buttons: input.buttons,
            pointerId: 1, pointerType: 'mouse', isPrimary: true,
            pressure: input.buttons ? 0.5 : 0,
            ...nativeModifiers(input)
        };
        if (input.phase === 'activate') {
            dispatchPair('down', { ...init, buttons: 1, pressure: 0.5 });
            dispatchPair('up', { ...init, buttons: 0, pressure: 0 });
            const name = input.button === 1 ? 'auxclick' : 'click';
            return target.dispatchEvent(markTrusted(new MouseEvent(name, { ...init, buttons: 0 })));
        }
        let allowed = true;
        if (names[0]) allowed = target.dispatchEvent(markTrusted(new PointerEvent(names[0], init))) && allowed;
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
    const dispatchNativeKeyboard = input => nativeTarget(input.target).dispatchEvent(markTrusted(new KeyboardEvent(
        input.phase === 'down' ? 'keydown' : 'keyup', {
            bubbles: true, cancelable: true, composed: true,
            key: input.key, code: input.code, repeat: !!input.repeat,
            keyCode: Number(input.keyCode) || 0, ...nativeModifiers(input)
        }
    )));
    const dispatchNativeText = input => {
        const target = nativeTarget(input.target);
        target.value = String(input.value);
        if (target instanceof HTMLTextAreaElement) target.textContent = target.value;
        if (typeof target.setSelectionRange === 'function') {
            try { target.setSelectionRange(input.selectionStart, input.selectionEnd); } catch (_error) {}
        }
        return target.dispatchEvent(markTrusted(new InputEvent('input', {
            bubbles: true, composed: true, inputType: 'insertText', data: null
        })));
    };
    const dispatchNativeFocus = input => {
        const next = input.focused ? nativeTarget(input.target) : null;
        const previous = document.activeElement;
        if (previous === next && nativeDocumentFocused === !!input.focused) return true;
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

    Object.defineProperty(document, '__dispatchNativeInput', {
        configurable: false,
        value(input) {
            switch (input.kind) {
                case 'pointer': return dispatchNativePointer(input);
                case 'keyboard': return dispatchNativeKeyboard(input);
                case 'text': return dispatchNativeText(input);
                case 'focus': return dispatchNativeFocus(input);
                case 'simple': return dispatchNativeSimple(input);
                case 'scroll':
                    windowObject.scrollX = windowObject.pageXOffset = Number(input.x) || 0;
                    windowObject.scrollY = windowObject.pageYOffset = Number(input.y) || 0;
                    return document.dispatchEvent(markTrusted(new Event('scroll')));
                case 'viewport':
                    windowObject.innerWidth = Number(input.width) || 1;
                    windowObject.innerHeight = Number(input.height) || 1;
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
                case 'media': return applyMediaResponse(input);
                default: return false;
            }
        }
    });
