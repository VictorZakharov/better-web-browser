    class HTMLIFrameElement extends HTMLElement {
        get srcdoc() { return this.getAttribute('srcdoc') || ''; }
        set srcdoc(value) { this.setAttribute('srcdoc', String(value)); }
        get sandbox() { return this.__sandbox ||= new DOMTokenList(this, 'sandbox'); }
        // sandbox is readonly in Web IDL, but [PutForwards=value] makes assignment update the
        // same reflected token list instead of replacing it.
        // https://html.spec.whatwg.org/multipage/iframe-embed-object.html#dom-iframe-sandbox
        set sandbox(value) { this.sandbox.value = String(value); }
        get allow() { return this.getAttribute('allow') || ''; }
        set allow(value) { this.setAttribute('allow', value); }
        get allowFullscreen() { return this.hasAttribute('allowfullscreen'); }
        set allowFullscreen(value) { this.toggleAttribute('allowfullscreen', !!value); }
        get width() { return this.getAttribute('width') || ''; }
        set width(value) { this.setAttribute('width', value); }
        get height() { return this.getAttribute('height') || ''; }
        set height(value) { this.setAttribute('height', value); }
        get referrerPolicy() { return this.getAttribute('referrerpolicy') || ''; }
        set referrerPolicy(value) { this.setAttribute('referrerpolicy', value); }
        get loading() { return this.getAttribute('loading') || ''; }
        set loading(value) { this.setAttribute('loading', value); }
        getSVGDocument() { return null; }
    }
