    // HTML's force-async state is not the reflected async content attribute.
    // https://html.spec.whatwg.org/multipage/scripting.html#dom-script-async
    class HTMLScriptElement extends HTMLElement {
        get async() { return host('scriptForceAsync', this.__id) || this.hasAttribute('async'); }
        set async(value) {
            host('scriptClearForceAsync', this.__id);
            this.toggleAttribute('async', !!value);
        }
        get defer() { return this.hasAttribute('defer'); }
        set defer(value) { this.toggleAttribute('defer', !!value); }
        get text() { return this.textContent; }
        set text(value) { this.textContent = String(value); }
    }
