    // Web Animations composite ordering: CSS animations are before script animations;
    // CSS effects on a target follow animation-name list order, not discovery order.
    // https://drafts.csswg.org/css-animations-2/#animation-composite-order
    const animationSequence = new WeakMap();
    let nextAnimationSequence = 0;
    const compareAnimationOrder = (a, b) => {
        const left = cssAnimationRecords.get(a), right = cssAnimationRecords.get(b);
        if (!!left !== !!right) return left ? -1 : 1;
        if (left && right) {
            if (left.target === right.target) return left.index - right.index;
            // compareDocumentPosition is defined for DOM order; disconnected roots use the
            // stable animation sequence instead of its implementation-specific direction.
            const position = left.target.compareDocumentPosition(right.target);
            if (!(position & Node.DOCUMENT_POSITION_DISCONNECTED)) {
                if (position & Node.DOCUMENT_POSITION_FOLLOWING) return -1;
                if (position & Node.DOCUMENT_POSITION_PRECEDING) return 1;
            }
        }
        return animationSequence.get(a) - animationSequence.get(b);
    };
