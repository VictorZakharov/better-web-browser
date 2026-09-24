    // HTML's force-async state is not the reflected async content attribute.
    // https://html.spec.whatwg.org/multipage/scripting.html#dom-script-async
    class HTMLScriptElement extends HTMLElement {
        get async() { return host('scriptForceAsync', nodeId(this)) || this.hasAttribute('async'); }
        set async(value) {
            host('scriptClearForceAsync', nodeId(this));
            this.toggleAttribute('async', !!value);
        }
        get defer() { return this.hasAttribute('defer'); }
        set defer(value) { this.toggleAttribute('defer', !!value); }
        get noModule() { return this.hasAttribute('nomodule'); }
        set noModule(value) { this.toggleAttribute('nomodule', !!value); }
        get integrity() { return this.getAttribute('integrity') || ''; }
        set integrity(value) { this.setAttribute('integrity', String(value)); }
        get crossOrigin() { return this.getAttribute('crossorigin'); }
        set crossOrigin(value) {
            if (value == null) this.removeAttribute('crossorigin');
            else this.setAttribute('crossorigin', String(value));
        }
        get referrerPolicy() { return this.getAttribute('referrerpolicy') || ''; }
        set referrerPolicy(value) { this.setAttribute('referrerpolicy', String(value)); }
        get text() { return this.textContent; }
        set text(value) { this.textContent = String(value); }
    }
