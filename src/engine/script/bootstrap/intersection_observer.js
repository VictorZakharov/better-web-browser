(() => {
    'use strict';
    const observers = new Set();
    let hasLayoutSnapshot = false;
    const emptyRect = () => ({
        x: 0, y: 0, top: 0, right: 0, bottom: 0, left: 0, width: 0, height: 0,
        toJSON() { return { x: this.x, y: this.y, top: this.top, right: this.right,
            bottom: this.bottom, left: this.left, width: this.width, height: this.height }; }
    });
    const normalizeRect = value => {
        const left = Number(value?.left ?? value?.x) || 0;
        const top = Number(value?.top ?? value?.y) || 0;
        const width = Math.max(0, Number(value?.width) || 0);
        const height = Math.max(0, Number(value?.height) || 0);
        const right = Number.isFinite(Number(value?.right)) ? Number(value.right) : left + width;
        const bottom = Number.isFinite(Number(value?.bottom)) ? Number(value.bottom) : top + height;
        return { x: left, y: top, top, right, bottom, left, width, height,
            toJSON: emptyRect().toJSON };
    };
    const parseMargin = value => {
        const tokens = String(value ?? '0px').trim().split(/\s+/).filter(Boolean);
        if (!tokens.length || tokens.length > 4) throw new SyntaxError('Invalid intersection root margin');
        const parsed = tokens.map(token => {
            const match = token.match(/^(-?(?:\d+|\d*\.\d+))(px|%)$/);
            if (!match) throw new SyntaxError('Invalid intersection root margin');
            return { value: Number(match[1]), unit: match[2] };
        });
        if (parsed.length === 1) parsed.push(parsed[0], parsed[0], parsed[0]);
        else if (parsed.length === 2) parsed.push(parsed[0], parsed[1]);
        else if (parsed.length === 3) parsed.push(parsed[1]);
        return parsed;
    };
    const serializeMargin = margin => margin.map(item => `${item.value}${item.unit}`).join(' ');
    const marginPixels = (item, width) => item.unit === '%' ? item.value * width / 100 : item.value;
    const expandRect = (rect, margin) => {
        const top = marginPixels(margin[0], rect.width);
        const right = marginPixels(margin[1], rect.width);
        const bottom = marginPixels(margin[2], rect.width);
        const left = marginPixels(margin[3], rect.width);
        return normalizeRect({
            left: rect.left - left,
            top: rect.top - top,
            right: rect.right + right,
            bottom: rect.bottom + bottom,
            width: rect.width + left + right,
            height: rect.height + top + bottom
        });
    };
    const viewportRect = () => normalizeRect({
        left: 0, top: 0, right: globalThis.innerWidth, bottom: globalThis.innerHeight,
        width: globalThis.innerWidth, height: globalThis.innerHeight
    });
    const intersect = (target, root) => {
        const left = Math.max(target.left, root.left);
        const top = Math.max(target.top, root.top);
        const right = Math.min(target.right, root.right);
        const bottom = Math.min(target.bottom, root.bottom);
        const isIntersecting = right >= left && bottom >= top;
        return {
            isIntersecting,
            rect: isIntersecting ? normalizeRect({
                left, top, right, bottom,
                width: Math.max(0, right - left), height: Math.max(0, bottom - top)
            }) : emptyRect()
        };
    };

    class IntersectionObserverEntry {
        constructor(init) {
            for (const name of [
                'time', 'rootBounds', 'boundingClientRect', 'intersectionRect', 'isIntersecting',
                'intersectionRatio', 'target', 'isVisible'
            ]) this['_' + name] = init[name];
        }
        get time() { return this._time; }
        get rootBounds() { return this._rootBounds; }
        get boundingClientRect() { return this._boundingClientRect; }
        get intersectionRect() { return this._intersectionRect; }
        get isIntersecting() { return this._isIntersecting; }
        get intersectionRatio() { return this._intersectionRatio; }
        get target() { return this._target; }
        get isVisible() { return this._isVisible; }
    }
    class IntersectionObserver {
        constructor(callback, options = {}) {
            if (typeof callback !== 'function') throw new TypeError('IntersectionObserver callback must be a function');
            options = Object(options);
            this.callback = callback;
            this.root = options.root ?? null;
            if (this.root !== null && !(this.root instanceof Element) && this.root !== document)
                throw new TypeError('IntersectionObserver root must be an Element, Document, or null');
            this._rootMargin = parseMargin(options.rootMargin);
            this._scrollMargin = parseMargin(options.scrollMargin);
            this.rootMargin = serializeMargin(this._rootMargin);
            this.scrollMargin = serializeMargin(this._scrollMargin);
            let thresholds = options.threshold === undefined ? [0]
                : typeof options.threshold === 'number' ? [options.threshold] : Array.from(options.threshold);
            thresholds = thresholds.map(Number);
            if (thresholds.some(value => !Number.isFinite(value) || value < 0 || value > 1))
                throw new RangeError('IntersectionObserver thresholds must be between 0 and 1');
            if (!thresholds.length) thresholds.push(0);
            this.thresholds = Object.freeze(thresholds.sort((left, right) => left - right));
            this.trackVisibility = !!options.trackVisibility;
            this.delay = Math.max(this.trackVisibility ? 100 : 0, Number(options.delay) || 0);
            this._targets = new Set();
            this._previous = new Map();
            this._records = [];
            this._scheduled = false;
            observers.add(this);
        }
        _deliver() {
            this._scheduled = false;
            if (!this._targets.size) return;
            const root = expandRect(this.root && this.root !== document
                ? normalizeRect(this.root.getBoundingClientRect()) : viewportRect(), this._rootMargin);
            for (const target of this._targets) {
                const boundingClientRect = normalizeRect(target.getBoundingClientRect());
                const intersection = intersect(boundingClientRect, root);
                const targetArea = boundingClientRect.width * boundingClientRect.height;
                const intersectionArea = intersection.rect.width * intersection.rect.height;
                const intersectionRatio = targetArea ? intersectionArea / targetArea
                    : intersection.isIntersecting ? 1 : 0;
                const thresholdIndex = this.thresholds.filter(value => value <= intersectionRatio).length;
                const previous = this._previous.get(target);
                this._previous.set(target, {
                    isIntersecting: intersection.isIntersecting,
                    thresholdIndex
                });
                if (previous && previous.isIntersecting === intersection.isIntersecting &&
                    previous.thresholdIndex === thresholdIndex) continue;
                this._records.push(new IntersectionObserverEntry({
                    time: performance.now(), target, rootBounds: root, boundingClientRect,
                    intersectionRect: intersection.rect,
                    isIntersecting: intersection.isIntersecting, intersectionRatio,
                    isVisible: this.trackVisibility ? intersection.isIntersecting : false
                }));
            }
            const records = this.takeRecords();
            if (records.length) this.callback(records, this);
        }
        _queue() {
            if (this._scheduled || !this._targets.size) return;
            this._scheduled = true;
            setTimeout(() => this._deliver(), this.delay);
        }
        observe(target) {
            if (!(target instanceof Element)) throw new TypeError('IntersectionObserver target must be an Element');
            if (this._targets.has(target)) return;
            this._targets.add(target);
            // Intersection observations belong to the rendering update. Initial scripts run
            // before Breeze has a layout snapshot, so delivering here would expose a synthetic
            // zero rectangle and can permanently suppress lazy content. The renderer notifies
            // us after its first layout; observers registered after that point may queue now.
            if (hasLayoutSnapshot) this._queue();
        }
        unobserve(target) { this._targets.delete(target); this._previous.delete(target); }
        disconnect() {
            this._targets.clear();
            this._previous.clear();
            this._records.length = 0;
            observers.delete(this);
        }
        takeRecords() { return this._records.splice(0); }
    }
    const updateObservers = () => {
        hasLayoutSnapshot = true;
        // The embedding invokes this as a dedicated rendering-observer task after publishing a
        // fresh layout snapshot. Do not enqueue through the page timer queue: a timer backlog may
        // not delay rendering-observer notifications.
        for (const observer of observers) observer._deliver();
    };
    globalThis.addEventListener('resize', updateObservers);
    globalThis.addEventListener('scroll', updateObservers);
    Object.assign(globalThis, {
        IntersectionObserver,
        IntersectionObserverEntry,
        __notifyIntersectionObservers: updateObservers
    });
})();
