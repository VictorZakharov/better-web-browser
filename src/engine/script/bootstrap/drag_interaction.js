    // Pointer-driven same-document drag-and-drop. The drag data store remains
    // writable only during dragstart, protected while traversing targets, and
    // readable only during drop, matching HTML's event-phase security model.
    let pointerDrag = null;
    const draggableAncestor = start => {
        let node = start;
        while (node && node instanceof Element) {
            if (node instanceof HTMLElement && node.draggable) return node;
            node = node.parentElement;
        }
        return null;
    };
    const defaultDragEffect = drag => {
        const allowed = drag.transfer.effectAllowed;
        if (allowed === 'none') return 'none';
        if (allowed === 'link' || allowed === 'linkMove') return 'link';
        if (allowed === 'move') return 'move';
        if (allowed === 'uninitialized' && drag.source instanceof HTMLAnchorElement &&
            drag.source.hasAttribute('href')) return 'link';
        return 'copy';
    };
    const fireDragEvent = (type, target, drag, init, mode, cancelable = true) => {
        setDragDataMode(drag.transfer, mode);
        // HTML assigns a fresh dropEffect for each DND event; it is not carried
        // over from the previous target's handler.
        drag.transfer.dropEffect = type === 'dragenter' || type === 'dragover'
            ? defaultDragEffect(drag) : type === 'drop' || type === 'dragend'
                ? drag.operation : 'none';
        const event = markTrusted(new DragEvent(type, {
            ...init, bubbles: true, cancelable, composed: true,
            dataTransfer: drag.transfer
        }));
        const allowed = target.dispatchEvent(event);
        setDragDataMode(drag.transfer, 'protected');
        return allowed;
    };
    const allowedDragEffect = transfer => {
        const store = dragStore(transfer);
        const choices = {
            copy: ['copy', 'copyLink', 'copyMove', 'all', 'uninitialized'],
            link: ['link', 'copyLink', 'linkMove', 'all', 'uninitialized'],
            move: ['move', 'copyMove', 'linkMove', 'all', 'uninitialized']
        };
        return choices[store.dropEffect]?.includes(store.effectAllowed) ? store.dropEffect : 'none';
    };
    const populateDefaultDragData = drag => {
        const source = drag.source;
        const attribute = source instanceof HTMLAnchorElement ? 'href' :
            source instanceof HTMLImageElement ? 'src' : null;
        if (!attribute || !source.hasAttribute(attribute)) return;
        try {
            drag.transfer.setData('text/uri-list',
                host('strictResolveUrl', source.getAttribute(attribute), source.baseURI));
        } catch (_) { /* Invalid source URL contributes no URI item. */ }
    };
    const finishDrag = (drag, init, leave) => {
        if (leave && drag.target)
            fireDragEvent('dragleave', drag.target, drag, init, 'protected', false);
        if (leave) drag.operation = 'none';
        fireDragEvent('dragend', drag.source, drag, init, 'protected', false);
        setDragDataMode(drag.transfer, 'disabled');
        pointerDrag = null;
    };
    const processDragPointer = (input, target, init) => {
        if (input.phase === 'down') {
            if (pointerDrag?.active) return true;
            pointerDrag = null;
            if (input.button !== 0 || !(input.buttons & 1)) return false;
            const source = draggableAncestor(target);
            pointerDrag = source ? {
                source, x: Number(input.x), y: Number(input.y),
                transfer: new DataTransfer(), active: false, selection: null,
                target: null, operation: 'none'
            } : null;
            if (pointerDrag) dragStore(pointerDrag.transfer).effectAllowed = 'uninitialized';
            return false;
        }
        const drag = pointerDrag;
        if (!drag) return false;
        drag.lastInit = init;
        if (input.phase === 'leave') {
            if (drag.active) finishDrag(drag, init, true);
            else pointerDrag = null;
            return drag.active;
        }
        if (input.phase === 'move') {
            if (!(input.buttons & 1)) {
                if (drag.active) finishDrag(drag, init, true);
                else pointerDrag = null;
                return drag.active;
            }
            if (!drag.active) {
                if (Math.hypot(Number(input.x) - drag.x, Number(input.y) - drag.y) < 5) return false;
                populateDefaultDragData(drag);
                if (!fireDragEvent('dragstart', drag.source, drag, init, 'readwrite')) {
                    setDragDataMode(drag.transfer, 'disabled');
                    pointerDrag = null;
                    return false;
                }
                drag.active = true;
                drag.source.dispatchEvent(markTrusted(new PointerEvent('pointercancel', {
                    ...init, button: -1, buttons: 0, pressure: 0
                })));
            }
            if (!fireDragEvent('drag', drag.source, drag, init, 'protected')) {
                finishDrag(drag, init, true);
                return true;
            }
            // The immediate selection and accepted current target are distinct.
            // A rejected element falls back to the body; entering the body itself
            // without canceling dragenter leaves the old current target unchanged.
            const selection = target instanceof Element ? target : null;
            if (selection !== drag.selection) {
                drag.selection = selection;
                if (selection !== drag.target) {
                    let next = selection;
                    if (selection && fireDragEvent('dragenter', selection, drag, init, 'protected')) {
                        if (selection === document.body) next = drag.target;
                        else {
                            fireDragEvent('dragenter', document.body || document, drag, init, 'protected');
                            next = document.body;
                        }
                    }
                    if (next !== drag.target) {
                        const previous = drag.target;
                        drag.target = next;
                        drag.operation = 'none';
                        if (previous) fireDragEvent('dragleave', previous, drag,
                            { ...init, relatedTarget: next }, 'protected', false);
                    }
                }
            }
            if (!drag.target) { drag.operation = 'none'; return true; }
            const allowed = fireDragEvent('dragover', drag.target, drag, init, 'protected');
            drag.operation = allowed ? 'none' : allowedDragEffect(drag.transfer);
            return true;
        }
        if (input.phase !== 'up') return drag.active;
        if (drag.active && (input.buttons & 1)) return true;
        pointerDrag = null;
        if (!drag.active) return false;
        if (drag.operation !== 'none' && drag.target) {
            if (fireDragEvent('drop', drag.target, drag, init, 'readonly'))
                drag.operation = 'none';
            else drag.operation = drag.transfer.dropEffect;
        } else if (drag.target) {
            fireDragEvent('dragleave', drag.target, drag, init, 'protected', false);
        }
        fireDragEvent('dragend', drag.source, drag, init, 'protected', false);
        setDragDataMode(drag.transfer, 'disabled');
        return true;
    };
    const cancelPointerDrag = () => {
        if (!pointerDrag?.active) return false;
        finishDrag(pointerDrag, pointerDrag.lastInit, true);
        return true;
    };
