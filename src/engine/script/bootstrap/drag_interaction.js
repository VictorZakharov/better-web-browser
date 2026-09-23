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
    const fireDragEvent = (type, target, drag, init, mode, cancelable = true) => {
        setDragDataMode(drag.transfer, mode);
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
        const allowed = store.effectAllowed;
        const requested = store.dropEffect === 'none' ? 'copy' : store.dropEffect;
        if (allowed === 'all' || allowed === 'uninitialized') return requested;
        if (allowed === requested || allowed.includes(requested[0].toUpperCase() + requested.slice(1)) ||
            allowed.startsWith(requested)) return requested;
        return 'none';
    };
    const processDragPointer = (input, target, init) => {
        if (input.phase === 'down') {
            pointerDrag = null;
            if (input.button !== 0 || !(input.buttons & 1)) return false;
            const source = draggableAncestor(target);
            pointerDrag = source ? {
                source, x: Number(input.x), y: Number(input.y),
                transfer: new DataTransfer(), active: false, target: null, accepted: false
            } : null;
            if (pointerDrag) dragStore(pointerDrag.transfer).effectAllowed = 'uninitialized';
            return false;
        }
        const drag = pointerDrag;
        if (!drag) return false;
        if (input.phase === 'move') {
            if (!(input.buttons & 1)) {
                if (drag.active) {
                    fireDragEvent('dragend', drag.source, drag, init, 'protected', false);
                    setDragDataMode(drag.transfer, 'disabled');
                }
                pointerDrag = null;
                return false;
            }
            if (!drag.active) {
                if (Math.hypot(Number(input.x) - drag.x, Number(input.y) - drag.y) < 5) return false;
                if (!fireDragEvent('dragstart', drag.source, drag, init, 'readwrite')) {
                    setDragDataMode(drag.transfer, 'disabled');
                    pointerDrag = null;
                    return false;
                }
                drag.active = true;
            }
            fireDragEvent('drag', drag.source, drag, init, 'protected', false);
            if (target !== drag.target) {
                if (drag.target) fireDragEvent('dragleave', drag.target, drag, init, 'protected', false);
                drag.target = target;
                drag.accepted = false;
                fireDragEvent('dragenter', target, drag, init, 'protected');
            }
            const allowed = fireDragEvent('dragover', target, drag, init, 'protected');
            drag.accepted = !allowed && allowedDragEffect(drag.transfer) !== 'none';
            drag.transfer.dropEffect = drag.accepted ? allowedDragEffect(drag.transfer) : 'none';
            return true;
        }
        if (input.phase !== 'up') return false;
        pointerDrag = null;
        if (!drag.active) return false;
        if (drag.target !== target) {
            if (drag.target) fireDragEvent('dragleave', drag.target, drag, init, 'protected', false);
            drag.accepted = false;
        }
        if (drag.accepted && target)
            fireDragEvent('drop', target, drag, init, 'readonly');
        else drag.transfer.dropEffect = 'none';
        fireDragEvent('dragend', drag.source, drag, init, 'protected', false);
        setDragDataMode(drag.transfer, 'disabled');
        return true;
    };
