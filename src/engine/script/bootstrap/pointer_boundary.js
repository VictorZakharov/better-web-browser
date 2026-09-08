    // Boundary events follow target changes, not every move. Enter/leave do not
    // bubble; common ancestors remain hovered when moving between their children.
    const dispatchPointerBoundary = (boundary, init) => {
        if (!boundary || boundary.previous === boundary.next) return;
        const previous = boundary.previous ? nativeTarget(boundary.previous) : null;
        const next = boundary.next ? nativeTarget(boundary.next) : null;
        const emit = (target, type, relatedTarget, bubbles) => {
            const Constructor = type.startsWith('pointer') ? PointerEvent : MouseEvent;
            target.dispatchEvent(markTrusted(new Constructor(type, {
                ...init, relatedTarget, bubbles, cancelable: bubbles, composed: bubbles
            })));
        };
        for (const prefix of ['pointer', 'mouse']) {
            if (previous) emit(previous, prefix + 'out', next, true);
            for (const id of boundary.leave) emit(nativeTarget(id), prefix + 'leave', next, false);
        }
        for (const prefix of ['pointer', 'mouse']) {
            if (next) emit(next, prefix + 'over', previous, true);
            for (const id of boundary.enter) emit(nativeTarget(id), prefix + 'enter', previous, false);
        }
    };
