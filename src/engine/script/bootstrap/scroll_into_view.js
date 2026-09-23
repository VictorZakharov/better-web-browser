    // CSSOM View §6.1: scroll each scrolling box from innermost to viewport.
    // https://drafts.csswg.org/cssom-view/#scroll-an-element-into-view
    const scrollTopDescriptor = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollTop');
    const scrollLeftDescriptor = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollLeft');
    const getRect = Element.prototype.getBoundingClientRect;
    const getRects = Element.prototype.getClientRects;
    const clientWidthGetter = Object.getOwnPropertyDescriptor(Element.prototype, 'clientWidth').get;
    const clientHeightGetter = Object.getOwnPropertyDescriptor(Element.prototype, 'clientHeight').get;
    const clientLeftGetter = Object.getOwnPropertyDescriptor(Element.prototype, 'clientLeft').get;
    const clientTopGetter = Object.getOwnPropertyDescriptor(Element.prototype, 'clientTop').get;
    const scrollIntoViewChoice = (value, allowed, label) => {
        value = String(value);
        if (!allowed.includes(value)) throw new TypeError('Invalid ' + label);
        return value;
    };
    const scrollIntoViewOptions = argument => {
        if (argument === undefined || argument === null ||
            (typeof argument !== 'object' && typeof argument !== 'function' && !!argument))
            return { behavior: 'auto', block: 'start', inline: 'nearest', container: 'all' };
        if (typeof argument !== 'object' && typeof argument !== 'function')
            return { behavior: 'auto', block: 'end', inline: 'nearest', container: 'all' };
        return {
            behavior: scrollIntoViewChoice(argument.behavior ?? 'auto', ['auto', 'instant', 'smooth'], 'scroll behavior'),
            block: scrollIntoViewChoice(argument.block ?? 'start', ['start', 'center', 'end', 'nearest'], 'block alignment'),
            inline: scrollIntoViewChoice(argument.inline ?? 'nearest', ['start', 'center', 'end', 'nearest'], 'inline alignment'),
            container: scrollIntoViewChoice(argument.container ?? 'all', ['all', 'nearest'], 'scroll container')
        };
    };
    const nearestScrollDelta = (start, end, boxStart, boxEnd) => {
        const size = end - start, boxSize = boxEnd - boxStart;
        if ((start < boxStart && end > boxEnd) || (start >= boxStart && end <= boxEnd)) return 0;
        if ((start < boxStart && size < boxSize) || (end > boxEnd && size > boxSize))
            return start - boxStart;
        if ((start < boxStart && size > boxSize) || (end > boxEnd && size < boxSize))
            return end - boxEnd;
        return 0;
    };
    const alignedScrollDelta = (start, end, boxStart, boxEnd, alignment) => {
        switch (alignment) {
            case 'start': return start - boxStart;
            case 'end': return end - boxEnd;
            case 'center': return (start + end - boxStart - boxEnd) / 2;
            default: return nearestScrollDelta(start, end, boxStart, boxEnd);
        }
    };
    const scrollSpacingValue = (value, basis) => {
        value = String(value).trim();
        if (value === 'auto') return 0;
        if (value.endsWith('px')) {
            const number = Number(value.slice(0, -2));
            return Number.isFinite(number) ? number : 0;
        }
        if (value.endsWith('%')) {
            const number = Number(value.slice(0, -1));
            return Number.isFinite(number) ? basis * number / 100 : 0;
        }
        const mixed = /^calc\((-?[\d.]+)px\s*([+-])\s*([\d.]+)%\)$/.exec(value);
        if (mixed) return Number(mixed[1]) + (mixed[2] === '-' ? -1 : 1) * basis * Number(mixed[3]) / 100;
        return 0;
    };
    const targetEdges = target => {
        const rect = getRect.call(target);
        const style = computedStyleProxy(target, '');
        return {
            top: rect.top - scrollSpacingValue(style.scrollMarginTop, 0),
            right: rect.right + scrollSpacingValue(style.scrollMarginRight, 0),
            bottom: rect.bottom + scrollSpacingValue(style.scrollMarginBottom, 0),
            left: rect.left - scrollSpacingValue(style.scrollMarginLeft, 0)
        };
    };
    const boxEdges = container => {
        if (!container) {
            const style = computedStyleProxy(document.documentElement, '');
            return {
                top: Math.max(0, scrollSpacingValue(style.scrollPaddingTop, layoutViewportHeight)),
                right: layoutViewportWidth - Math.max(0, scrollSpacingValue(style.scrollPaddingRight, layoutViewportWidth)),
                bottom: layoutViewportHeight - Math.max(0, scrollSpacingValue(style.scrollPaddingBottom, layoutViewportHeight)),
                left: Math.max(0, scrollSpacingValue(style.scrollPaddingLeft, layoutViewportWidth))
            };
        }
        const rect = getRect.call(container);
        const style = computedStyleProxy(container, '');
        const left = rect.left + clientLeftGetter.call(container);
        const top = rect.top + clientTopGetter.call(container);
        return {
            left: left + Math.max(0, scrollSpacingValue(style.scrollPaddingLeft, clientWidthGetter.call(container))),
            top: top + Math.max(0, scrollSpacingValue(style.scrollPaddingTop, clientHeightGetter.call(container))),
            right: left + clientWidthGetter.call(container) - Math.max(0, scrollSpacingValue(style.scrollPaddingRight, clientWidthGetter.call(container))),
            bottom: top + clientHeightGetter.call(container) - Math.max(0, scrollSpacingValue(style.scrollPaddingBottom, clientHeightGetter.call(container)))
        };
    };
    const scrollBoxToTarget = (target, container, options) => {
        const element = targetEdges(target), box = boxEdges(container);
        // Horizontal viewport scrolling is not yet exposed by the host; nested
        // element scrolling is fully two-dimensional.
        const dx = alignedScrollDelta(element.left, element.right, box.left, box.right, options.inline);
        const dy = alignedScrollDelta(element.top, element.bottom, box.top, box.bottom, options.block);
        if (container) {
            if (dx) scrollLeftDescriptor.set.call(container, scrollLeftDescriptor.get.call(container) + dx);
            if (dy) scrollTopDescriptor.set.call(container, scrollTopDescriptor.get.call(container) + dy);
        } else if (dy) {
            scrollViewport(viewportScrollY + dy);
        }
    };
    Object.defineProperty(Element.prototype, 'scrollIntoView', {
        configurable: true, enumerable: true,
        value: function(arg) {
            if (!(this instanceof Element)) throw new TypeError('Illegal Element receiver');
            const options = scrollIntoViewOptions(arg);
            if (!this.isConnected || this.ownerDocument !== document ||
                !getRects.call(this).length) return Promise.resolve();
            for (let ancestor = this.parentElement; ancestor; ancestor = ancestor.parentElement) {
                if (scrollsViewport(ancestor)) continue;
                const scroll = host('elementScroll', nodeId(ancestor));
                if (!Array.isArray(scroll)) continue;
                scrollBoxToTarget(this, ancestor, options);
                if (options.container === 'nearest') return Promise.resolve();
            }
            scrollBoxToTarget(this, null, options);
            return Promise.resolve();
        }
    });
