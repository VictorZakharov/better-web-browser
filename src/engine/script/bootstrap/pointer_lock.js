    // Pointer Lock is browser-owned: the DOM state changes only after the
    // native window has captured and hidden the cursor. A request may be
    // denied when focus, activation, or document ownership changed in flight.
    let pointerLockElement = null;
    let nextPointerLockRequest = 1;
    const pendingPointerLockRequests = new Map();

    const pointerLockTargetForRoot = root => {
        let target = pointerLockElement;
        while (target && target.getRootNode() !== root)
            target = target.getRootNode().host || null;
        return target;
    };
    Object.defineProperty(Document.prototype, 'pointerLockElement', {
        configurable: true, enumerable: true,
        get() { return this === document ? pointerLockTargetForRoot(this) : null; }
    });
    Object.defineProperty(ShadowRoot.prototype, 'pointerLockElement', {
        configurable: true, enumerable: true,
        get() { return pointerLockTargetForRoot(this); }
    });
    defineEventHandler(Document.prototype, null, 'pointerlockchange');
    defineEventHandler(Document.prototype, null, 'pointerlockerror');

    const pointerLockFailure = (name, message) => {
        const error = new DOMException(message, name);
        queueMicrotask(() => document.dispatchEvent(markTrusted(new Event('pointerlockerror'))));
        return Promise.reject(error);
    };
    Element.prototype.requestPointerLock = function(options = {}) {
        if (!(this instanceof Element)) throw new TypeError('Invalid pointer lock receiver');
        if (!this.isConnected || this.ownerDocument !== document)
            return pointerLockFailure('WrongDocumentError', 'Element is not in the active document');
        if (options?.unadjustedMovement)
            return pointerLockFailure('NotSupportedError', 'Raw mouse movement is unavailable');
        if (!host('pointerLockSupported'))
            return pointerLockFailure('NotSupportedError', 'Pointer Lock is unavailable');
        const requestId = nextPointerLockRequest++;
        return new Promise((resolve, reject) => {
            pendingPointerLockRequests.set(requestId, { target: this, resolve, reject });
            host('pointerLockRequest', requestId, nodeId(this), true);
        });
    };
    Document.prototype.exitPointerLock = function() {
        if (this !== document || !pointerLockElement) return;
        host('pointerLockRequest', nextPointerLockRequest++, 0, false);
    };

    const applyPointerLockResponse = input => {
        const requestId = Number(input.requestId) || 0;
        const pending = pendingPointerLockRequests.get(requestId);
        pendingPointerLockRequests.delete(requestId);
        if (input.disposition === 'entered' && pending?.target) {
            pointerLockElement = pending.target;
            pointerLockPosition = previousPointerPosition || { x: 0, y: 0 };
            pending.resolve();
            document.dispatchEvent(markTrusted(new Event('pointerlockchange')));
            return true;
        }
        if (input.disposition === 'exited') {
            if (!pointerLockElement) return true;
            pointerLockElement = null;
            previousPointerPosition = pointerLockPosition;
            pointerLockPosition = null;
            pending?.resolve();
            document.dispatchEvent(markTrusted(new Event('pointerlockchange')));
            return true;
        }
        pending?.reject(new DOMException('Pointer lock request was denied', 'NotAllowedError'));
        document.dispatchEvent(markTrusted(new Event('pointerlockerror')));
        return false;
    };
