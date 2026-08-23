    class UIEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            this.view = init.view === undefined ? null : init.view;
            this.detail = Number(init.detail) || 0;
        }
    }
    class FocusEvent extends UIEvent {
        constructor(type, init = {}) {
            super(type, init);
            this.relatedTarget = init?.relatedTarget === undefined ? null : init.relatedTarget;
        }
    }
    class MouseEvent extends UIEvent {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            this.screenX = Number(init.screenX) || 0;
            this.screenY = Number(init.screenY) || 0;
            this.clientX = Number(init.clientX) || 0;
            this.clientY = Number(init.clientY) || 0;
            this.ctrlKey = !!init.ctrlKey;
            this.shiftKey = !!init.shiftKey;
            this.altKey = !!init.altKey;
            this.metaKey = !!init.metaKey;
            this.button = Number(init.button) || 0;
            this.buttons = Number(init.buttons) || 0;
            this.relatedTarget = init.relatedTarget === undefined ? null : init.relatedTarget;
        }
    }
    class PointerEvent extends MouseEvent {
        constructor(type, init = {}) {
            super(type, init);
            this.pointerId = Number(init?.pointerId) || 1;
            this.width = Number(init?.width) || 1;
            this.height = Number(init?.height) || 1;
            this.pressure = Number(init?.pressure) || 0;
            this.pointerType = init?.pointerType === undefined ? '' : String(init.pointerType);
            this.isPrimary = init?.isPrimary === undefined ? false : !!init.isPrimary;
        }
    }
    class KeyboardEvent extends UIEvent {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            this.key = init.key === undefined ? '' : String(init.key);
            this.code = init.code === undefined ? '' : String(init.code);
            this.location = Number(init.location) || 0;
            this.ctrlKey = !!init.ctrlKey;
            this.shiftKey = !!init.shiftKey;
            this.altKey = !!init.altKey;
            this.metaKey = !!init.metaKey;
            this.repeat = !!init.repeat;
            this.isComposing = !!init.isComposing;
            this.keyCode = Number(init.keyCode) || 0;
            this.charCode = 0;
            this.which = this.keyCode;
        }
        getModifierState(key) {
            return ({ Alt: this.altKey, Control: this.ctrlKey, Meta: this.metaKey, Shift: this.shiftKey })[String(key)] || false;
        }
    }
    class InputEvent extends UIEvent {
        constructor(type, init = {}) {
            super(type, init);
            this.data = init?.data === undefined ? null : init.data;
            this.isComposing = !!init?.isComposing;
            this.inputType = init?.inputType === undefined ? '' : String(init.inputType);
        }
        getTargetRanges() { return []; }
    }
