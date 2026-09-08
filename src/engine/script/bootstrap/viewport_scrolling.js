    // CSSOM View's viewport offsets are shared by native input and script reads. Ordinary
    // elements without a scrolling box have zero offsets, not missing/expando properties.
    // https://drafts.csswg.org/cssom-view/#dom-element-scrolltop
    // Nested scrolling boxes and horizontal native scrolling are not implemented yet.
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
                return scrollsViewport(this) ? (axis === 'x' ? viewportScrollX : viewportScrollY) : 0;
            },
            set(value) {
                if (!(this instanceof Element)) throw new TypeError('Illegal Element receiver');
                value = finiteScrollValue(value);
                if (scrollsViewport(this) && axis === 'y') scrollViewport(value);
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
