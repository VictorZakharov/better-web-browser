    // Disclosure elements own their toggle lifecycle independently of base Element.
    class HTMLDetailsElement extends HTMLElement {
        get open() { return this.hasAttribute('open'); }
        set open(value) {
            const wasOpen = this.open;
            const isOpen = !!value;
            if (wasOpen === isOpen) return;
            this.toggleAttribute('open', isOpen);
            setTimeout(() => this.dispatchEvent(new ToggleEvent('toggle', {
                oldState: wasOpen ? 'open' : 'closed',
                newState: isOpen ? 'open' : 'closed'
            })), 0);
        }
    }
    class HTMLDialogElement extends HTMLElement {
        constructor(id, ...metadata) {
            super(id, ...metadata);
            this.returnValue = '';
            this.__isModal = false;
        }
        get open() { return this.hasAttribute('open'); }
        set open(value) { this.toggleAttribute('open', !!value); }
        get closedBy() { return this.getAttribute('closedby') || 'none'; }
        set closedBy(value) { this.setAttribute('closedby', value); }
        show() {
            if (this.open) return;
            const event = new ToggleEvent('beforetoggle', {
                cancelable: true, oldState: 'closed', newState: 'open', source: this
            });
            if (!this.dispatchEvent(event)) return;
            this.open = true;
            this.focus();
            setTimeout(() => this.dispatchEvent(new ToggleEvent('toggle', {
                oldState: 'closed', newState: 'open', source: this
            })), 0);
        }
        showModal() {
            if (!this.isConnected) throw new DOMException('Dialog is not connected to a document', 'InvalidStateError');
            if (this.open) {
                if (!this.__isModal) throw new DOMException('Dialog is already open non-modally', 'InvalidStateError');
                return;
            }
            this.__isModal = true;
            this.show();
        }
        close(returnValue) {
            if (!this.open) return;
            if (returnValue !== undefined) this.returnValue = String(returnValue);
            this.__isModal = false;
            this.open = false;
            setTimeout(() => this.dispatchEvent(new Event('close')), 0);
        }
        requestClose(returnValue) {
            if (!this.open) return;
            if (this.dispatchEvent(new Event('cancel', { cancelable: true }))) this.close(returnValue);
        }
    }
