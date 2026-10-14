    let nextAnimationFrame = 1;
    let animationFrameTimer = null;
    let pendingResizeObserverFrame = false;
    let pendingAnimationFrameCallbacks = new Map();
    let activeAnimationFrameCallbacks = null;
    // A rendering opportunity snapshots every pending callback and invokes that whole batch with
    // one timestamp. A callback requested while the batch is running belongs to the next frame.
    // https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html#animation-frames
    const scheduleAnimationFrame = () => {
        if (animationFrameTimer !== null ||
            (!pendingAnimationFrameCallbacks.size && !pendingResizeObserverFrame)) return;
        animationFrameTimer = queueTimer(() => {
            animationFrameTimer = null;
            if (pendingResizeObserverFrame) {
                pendingResizeObserverFrame = false;
                host('resizeObserverFrame');
            }
            activeAnimationFrameCallbacks = pendingAnimationFrameCallbacks;
            pendingAnimationFrameCallbacks = new Map();
            const timestamp = host('performanceNow');
            for (const [id, callback] of activeAnimationFrameCallbacks) {
                if (!activeAnimationFrameCallbacks.delete(id)) continue;
                try { callback(timestamp); }
                catch (error) { reportGlobalException(error); }
            }
            activeAnimationFrameCallbacks = null;
            scheduleAnimationFrame();
        }, 0, false, [], 'requestAnimationFrame callbacks', 'animationFrameSchedule');
    };
    // Skipped resize observations must not re-enter the loop before the next frame's
    // callbacks. Share the private frame scheduler, even if author APIs are overridden.
    // https://html.spec.whatwg.org/multipage/webappapis.html#update-the-rendering
    const requestResizeObserverFrame = () => {
        pendingResizeObserverFrame = true;
        host('resizeObserverDefer');
        scheduleAnimationFrame();
    };
    windowObject.requestAnimationFrame = callback => {
        if (typeof callback !== 'function')
            throw new TypeError('requestAnimationFrame requires a callback');
        const id = nextAnimationFrame++;
        pendingAnimationFrameCallbacks.set(id, callback);
        scheduleAnimationFrame();
        return id;
    };
    windowObject.cancelAnimationFrame = id => {
        id = Number(id);
        pendingAnimationFrameCallbacks.delete(id);
        activeAnimationFrameCallbacks?.delete(id);
        if (!pendingAnimationFrameCallbacks.size && !pendingResizeObserverFrame && animationFrameTimer !== null) {
            timers.delete(animationFrameTimer);
            host('animationFrameCancel', animationFrameTimer);
            animationFrameTimer = null;
        }
    };
