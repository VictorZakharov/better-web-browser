    // CSS Transitions Level 1. An attribute-driven style change compares the
    // previous presentation with the new underlying computed style, then writes
    // sampled declarations through the same native animation-origin paint path
    // as Web Animations. No inline style or author stylesheet is rewritten.
    // https://drafts.csswg.org/css-transitions-1/#starting
    const transitionProperties = [
        'opacity', 'color', 'background-color', 'transform', 'width', 'height',
        'top', 'right', 'bottom', 'left', 'font-size', 'letter-spacing', 'word-spacing',
        'border-radius', 'border-top-width', 'border-right-width',
        'border-bottom-width', 'border-left-width', 'border-top-color',
        'border-right-color', 'border-bottom-color', 'border-left-color',
        'margin-top', 'margin-right', 'margin-bottom', 'margin-left',
        'padding-top', 'padding-right', 'padding-bottom', 'padding-left',
        'flex-grow', 'flex-shrink'
    ];
    const transitionSettingNames = ['transition-property', 'transition-duration',
        'transition-delay', 'transition-timing-function'];
    const transitionSnapshotNames = [...transitionSettingNames, ...transitionProperties];
    const maxTransitionTargets = 64, maxTransitionsPerTarget = 16;
    const maxPendingTransitionEvents = maxTransitionTargets * maxTransitionsPerTarget * 2;
    const inlineTransitionDeclaration = /(?:^|;)\s*transition(?:-[a-z-]+)?\s*:/i;
    const hasInlineTransition = value => {
        const style = String(value ?? '');
        // CSS comments are removed before declarations are tokenized. Keep the
        // common no-comment path allocation-free for ordinary style mutations.
        return inlineTransitionDeclaration.test(style.includes('/*') ?
            style.replace(/\/\*[\s\S]*?\*\//g, ' ') : style);
    };
    const runningTransitions = new WeakMap();
    const transitionTargets = new Set();
    const pendingTransitionEvents = [];
    let transitionFrame = null;
    const transitionTime = () => performance.now();
    class TransitionEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            init = init == null ? {} : Object(init);
            const elapsedTime = Number(init.elapsedTime ?? 0);
            if (!Number.isFinite(elapsedTime)) throw new TypeError('elapsedTime must be finite');
            Object.defineProperties(this, {
                propertyName: {value: String(init.propertyName ?? ''), enumerable: true},
                elapsedTime: {value: elapsedTime, enumerable: true},
                pseudoElement: {value: String(init.pseudoElement ?? ''), enumerable: true}
            });
        }
    }
    windowObject.TransitionEvent = TransitionEvent;
    const transitionEvent = (target, type, property, elapsedTime) => {
        // One full run/start phase fits; a script that repeatedly toggles styles
        // without yielding cannot grow the queue without bound.
        if (pendingTransitionEvents.length >= maxPendingTransitionEvents) return;
        pendingTransitionEvents.push([target, type, property, elapsedTime]);
        scheduleTransitionFrame();
    };
    const transitionSnapshot = target => {
        const values = host('computedStyleBatch', nodeId(target), transitionSnapshotNames);
        return values.length === transitionSnapshotNames.length ? values : null;
    };
    const transitionLists = snapshot => ({
        properties: snapshot[0].split(',').map(part => part.trim()),
        durations: snapshot[1].split(',').map(part => parseFloat(part) || 0),
        delays: snapshot[2].split(',').map(part => parseFloat(part) || 0),
        easings: snapshot[3].split(',').map(part => part.trim())
    });
    const transitionShorthands = {
        background: ['background-color'],
        border: ['border-top-color', 'border-right-color', 'border-bottom-color',
            'border-left-color', 'border-top-width', 'border-right-width',
            'border-bottom-width', 'border-left-width'],
        'border-color': ['border-top-color', 'border-right-color',
            'border-bottom-color', 'border-left-color'],
        'border-width': ['border-top-width', 'border-right-width',
            'border-bottom-width', 'border-left-width'],
        'border-top': ['border-top-color', 'border-top-width'],
        'border-right': ['border-right-color', 'border-right-width'],
        'border-bottom': ['border-bottom-color', 'border-bottom-width'],
        'border-left': ['border-left-color', 'border-left-width'],
        margin: ['margin-top', 'margin-right', 'margin-bottom', 'margin-left'],
        padding: ['padding-top', 'padding-right', 'padding-bottom', 'padding-left'],
        inset: ['top', 'right', 'bottom', 'left'],
        flex: ['flex-grow', 'flex-shrink']
    };
    const transitionOptions = (settings, property) => {
        let selected = null;
        for (let index = 0; index < settings.properties.length; index++) {
            const name = settings.properties[index];
            if (name !== 'all' && name !== property &&
                !transitionShorthands[name]?.includes(property)) continue;
            selected = {
                duration: (settings.durations[index % settings.durations.length] ?? 0) * 1000,
                delay: (settings.delays[index % settings.delays.length] ?? 0) * 1000,
                easing: settings.easings[index % settings.easings.length] ?? 'ease'
            };
        }
        return selected;
    };
    const transitionInterpolable = (property, from, to) => {
        if (property === 'transform') return interpolateAnimationTransform(from, to, 0.5) !== null;
        const a = animationNumber.exec(from), b = animationNumber.exec(to);
        if (a && b && a[2] === b[2]) return true;
        return (property.includes('color') || property === 'color') &&
            animationColor(from) !== null && animationColor(to) !== null;
    };
    const transitionProgress = (transition, now) => transition.duration === 0 ? 0 :
        Math.max(0, Math.min(1, (now - transition.started - transition.delay) /
            transition.duration));
    const scheduleTransitionFrame = () => {
        if (transitionFrame !== null ||
            (!transitionTargets.size && !pendingTransitionEvents.length)) return;
        transitionFrame = windowObject.requestAnimationFrame(() => {
            transitionFrame = null;
            const now = transitionTime();
            for (const target of [...transitionTargets]) sampleTransitions(target, now);
            // Deliver after the style change and animation sample, outside the
            // author attribute-mutation stack. Listeners may queue more events.
            const events = pendingTransitionEvents.splice(0);
            for (const [target, type, property, elapsedTime] of events)
                target.dispatchEvent(new TransitionEvent(type,
                    {bubbles: true, propertyName: property, elapsedTime}));
            scheduleTransitionFrame();
        });
    };
    const sampleTransitions = (target, now) => {
        const transitions = runningTransitions.get(target);
        if (!transitions?.size) { transitionTargets.delete(target); return; }
        if (!target.isConnected) {
            // A detached subtree must not be kept alive by the frame scheduler,
            // nor receive a late transitionend after leaving the document.
            for (const [property, transition] of transitions) {
                const elapsed = Math.min(transition.duration,
                    Math.max(0, now - transition.started - transition.delay)) / 1000;
                transitionEvent(target, 'transitioncancel', property, elapsed);
            }
            runningTransitions.delete(target);
            transitionTargets.delete(target);
            setTransitionStyles(target, null);
            return;
        }
        const declarations = new Map(), events = [];
        for (const [property, transition] of transitions) {
            const elapsed = now - transition.started;
            if (transition.pendingRun) {
                transition.pendingRun = false;
                events.push(['transitionrun', property,
                    Math.min(transition.duration, Math.max(0, -transition.delay)) / 1000]);
            }
            if (!transition.startedEvent && elapsed >= transition.delay) {
                transition.startedEvent = true;
                events.push(['transitionstart', property,
                    Math.min(transition.duration, Math.max(0, -transition.delay)) / 1000]);
            }
            if (elapsed >= transition.delay + transition.duration) {
                transitions.delete(property);
                events.push(['transitionend', property, transition.duration / 1000]);
            } else {
                const progress = transitionProgress(transition, now);
                const eased = animationEasingProgress(transition.easing, progress);
                declarations.set(property, interpolateAnimationValue(property,
                    transition.from, transition.to, eased));
            }
        }
        if (!transitions.size) transitionTargets.delete(target);
        setTransitionStyles(target, declarations);
        for (const [type, property, elapsed] of events)
            transitionEvent(target, type, property, elapsed);
    };
    transitionBeforeAttributeChange = (target, name, nextValue) => {
        if (!['class', 'id', 'style'].includes(name) || !target.isConnected) return null;
        if (!runningTransitions.get(target)?.size &&
            !host('mayTransitionOnAttribute', nodeId(target), name,
                String(nextValue ?? '')) &&
            !hasInlineTransition(host('attrGet', nodeId(target), 'style')) &&
            !hasInlineTransition(name === 'style' ? nextValue : ''))
            return null;
        sampleTransitions(target, transitionTime());
        return transitionSnapshot(target);
    };
    transitionAfterAttributeChange = (target, previous) => {
        if (!previous) return;
        const old = runningTransitions.get(target) ?? new Map();
        // Query the after-change style without the presentation overlay. Both
        // host writes occur inside one synchronous mutation, before a paint.
        if (old.size) setTransitionStyles(target, null);
        const next = transitionSnapshot(target);
        if (!next) return;
        const settings = transitionLists(next);
        const now = transitionTime();
        for (let index = 0; index < transitionProperties.length; index++) {
            const property = transitionProperties[index];
            const from = previous[transitionSettingNames.length + index];
            const to = next[transitionSettingNames.length + index];
            const prior = old.get(property);
            const options = transitionOptions(settings, property);
            if (prior && options && prior.to === to) continue;
            if (prior) {
                old.delete(property);
                const elapsed = Math.min(prior.duration,
                    Math.max(0, now - prior.started - prior.delay)) / 1000;
                transitionEvent(target, 'transitioncancel', property, elapsed);
            }
            if (from === to || !options || options.duration + options.delay <= 0 ||
                !transitionInterpolable(property, from, to)) continue;
            if (old.size >= maxTransitionsPerTarget ||
                (!transitionTargets.has(target) && transitionTargets.size >= maxTransitionTargets))
                continue;
            old.set(property, {from, to, ...options, started: now,
                startedEvent: false, pendingRun: true});
        }
        if (old.size) {
            runningTransitions.set(target, old);
            transitionTargets.add(target);
            sampleTransitions(target, now);
            scheduleTransitionFrame();
        } else {
            runningTransitions.delete(target);
            transitionTargets.delete(target);
            setTransitionStyles(target, null);
        }
    };
