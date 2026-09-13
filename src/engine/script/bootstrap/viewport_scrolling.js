    // CSSOM View's viewport offsets are shared by native input and script reads. Ordinary
    // elements without a scrolling box have zero offsets, not missing/expando properties.
    // https://drafts.csswg.org/cssom-view/#dom-element-scrolltop
    const elementScrollEvents = new WeakSet();
    function queueElementScrollEvent(element) {
        if (elementScrollEvents.has(element)) return;
        elementScrollEvents.add(element);
        windowObject.setTimeout(() => {
            elementScrollEvents.delete(element);
            element.dispatchEvent(markTrusted(new Event('scroll')));
        }, 0);
    }
    function potentiallyScrollableBody(body, axis, scrollingElementQuery = false) {
        if (!body || body.localName !== 'body' || !layoutRect(body).hasBox) return false;
        const parent = body.parentElement;
        if (!parent) return false;
        const property = axis === 'x' ? 'overflowX' : 'overflowY';
        let parentOverflow = getComputedStyle(parent)[property] || 'visible';
        if (scrollingElementQuery && parentOverflow === 'clip') parentOverflow = 'hidden';
        const overflow = getComputedStyle(body)[property] || 'visible';
        return !['visible', 'clip'].includes(parentOverflow) &&
            !['visible', 'clip'].includes(overflow);
    }
    function scrollsViewport(element) {
        const owner = element.ownerDocument;
        if (owner !== document || !owner.defaultView || !element.isConnected) return false;
        if (owner.compatMode !== 'BackCompat') return element === owner.documentElement;
        return element === owner.body && element.localName === 'body' &&
            (!potentiallyScrollableBody(element, 'x') || !potentiallyScrollableBody(element, 'y'));
    }
    Object.defineProperty(Document.prototype, 'scrollingElement', {
        configurable: true, enumerable: true,
        get() {
            if (!(this instanceof Document)) throw new TypeError('Illegal Document receiver');
            if (this.compatMode !== 'BackCompat') return this.documentElement;
            const body = this.body;
            return body?.localName === 'body' &&
                !potentiallyScrollableBody(body, 'x', true) &&
                !potentiallyScrollableBody(body, 'y', true) ? body : null;
        }
    });
    const finiteScrollValue = value => {
        const number = +value;
        return Number.isFinite(number) ? number : 0;
    };
    function elementScroll(element, args, relative) {
        if (!(element instanceof Element)) throw new TypeError('Illegal Element receiver');
        let x = relative ? 0 : element.scrollLeft, y = relative ? 0 : element.scrollTop;
        if (args.length < 2) {
            const options = args[0];
            if (options != null && typeof options !== 'object' && typeof options !== 'function')
                throw new TypeError('Scroll options must be a dictionary');
            const behaviorValue = options?.behavior;
            const behavior = behaviorValue === undefined ? 'auto' : String(behaviorValue);
            if (!['auto', 'instant', 'smooth'].includes(behavior)) throw new TypeError('Invalid scroll behavior');
            const left = options?.left, top = options?.top;
            if (left !== undefined) x = finiteScrollValue(left);
            if (top !== undefined) y = finiteScrollValue(top);
        } else { x = finiteScrollValue(args[0]); y = finiteScrollValue(args[1]); }
        element.scrollLeft = x + (relative ? element.scrollLeft : 0);
        element.scrollTop = y + (relative ? element.scrollTop : 0);
    }
    Element.prototype.scroll = Element.prototype.scrollTo = function(...args) { elementScroll(this, args, false); };
    Element.prototype.scrollBy = function(...args) { elementScroll(this, args, true); };
    let viewportScrollEventPending = false;
    function queueViewportScrollEvent() {
        if (viewportScrollEventPending) return;
        viewportScrollEventPending = true;
        windowObject.setTimeout(() => {
            viewportScrollEventPending = false;
            document.dispatchEvent(markTrusted(new Event('scroll', { bubbles: true })));
        }, 0);
    }
    function setViewportScrollOffsets(x, y) {
        if (x === viewportScrollX && y === viewportScrollY) return false;
        viewportScrollX = x;
        viewportScrollY = y;
        return true;
    }
    function scrollViewport(y) {
        y = finiteScrollValue(y);
        if (y === viewportScrollY) return;
        // The host clamps to its current layout and exports one coalesced request to the native
        // viewport. Updating an expando alone would make script geometry disagree with painting.
        const accepted = host('scrollViewport', y);
        if (setViewportScrollOffsets(viewportScrollX, accepted)) queueViewportScrollEvent();
    }
    for (const [property, axis] of [['scrollLeft', 'x'], ['scrollTop', 'y']]) {
        Object.defineProperty(Element.prototype, property, {
            configurable: true, enumerable: true,
            get() {
                if (!(this instanceof Element)) throw new TypeError('Illegal Element receiver');
                if (scrollsViewport(this)) return axis === 'x' ? viewportScrollX : viewportScrollY;
                return host('elementScroll', this.__id)?.[axis === 'x' ? 0 : 1] || 0;
            },
            set(value) {
                if (!(this instanceof Element)) throw new TypeError('Illegal Element receiver');
                value = finiteScrollValue(value);
                if (scrollsViewport(this)) { if (axis === 'y') scrollViewport(value); return; }
                const index = axis === 'x' ? 0 : 1;
                const before = host('elementScroll', this.__id)?.[index] || 0;
                const after = host('elementScroll', this.__id, index, value)?.[index] || 0;
                if (before !== after) queueElementScrollEvent(this);
            }
        });
    }
    for (const [property, axis] of [['scrollWidth', 2], ['scrollHeight', 3]]) {
        Object.defineProperty(Element.prototype, property, {
            configurable: true, enumerable: true,
            get() {
                if (!(this instanceof Element)) throw new TypeError('Illegal Element receiver');
                if (isViewportElement(this)) return Math.round(axis === 3 ? host('documentScrollHeight') : layoutViewportWidth);
                const metrics = host('elementScroll', this.__id);
                return Math.round(metrics?.[axis] ?? (axis === 2 ? clientWidth(this) : clientHeight(this)));
            }
        });
    }
    for (const [property, axis] of [['scrollX', 'x'], ['pageXOffset', 'x'],
        ['scrollY', 'y'], ['pageYOffset', 'y']]) {
        Object.defineProperty(windowObject, property, {
            configurable: true, enumerable: true,
            get() { return axis === 'x' ? viewportScrollX : viewportScrollY; }
        });
    }
    function windowScroll(args, relative) {
        let y;
        if (args.length < 2) {
            const options = args[0];
            if (options != null && typeof options !== 'object' && typeof options !== 'function')
                throw new TypeError('Scroll options must be a dictionary');
            const behaviorValue = options?.behavior;
            const behavior = behaviorValue === undefined ? 'auto' : String(behaviorValue);
            if (!['auto', 'instant', 'smooth'].includes(behavior)) throw new TypeError('Invalid scroll behavior');
            // CSSOM View permits the user agent not to animate smooth scrolling.
            const left = options?.left;
            if (left !== undefined) finiteScrollValue(left);
            const top = options?.top;
            y = top === undefined ? (relative ? 0 : viewportScrollY) : finiteScrollValue(top);
        } else {
            finiteScrollValue(args[0]);
            y = finiteScrollValue(args[1]);
        }
        scrollViewport((relative ? viewportScrollY : 0) + y);
    }
    windowObject.scroll = windowObject.scrollTo = function(...args) { windowScroll(args, false); };
    windowObject.scrollBy = function(...args) { windowScroll(args, true); };
