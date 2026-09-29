    // Invoker attributes are reflected on buttons and button-state inputs. Activation is
    // a click default action, so preventDefault() suppresses it for trusted and synthetic clicks.
    const explicitPopoverTargets = new WeakMap();
    function popoverTargetAttributeChanged(element, namespace, name) {
        if (namespace === null && name === 'popovertarget') explicitPopoverTargets.delete(element);
    }
    const popoverTargetFor = invoker => {
        const explicit = explicitPopoverTargets.get(invoker);
        if (explicit) {
            for (let ancestor = invoker.parentNode; ancestor;
                ancestor = ancestor.parentNode || (ancestor instanceof ShadowRoot ? ancestor.host : null))
                if (ancestor.contains(explicit)) return explicit;
            return null;
        }
        const id = invoker.getAttribute('popovertarget');
        if (id === null) return null;
        const root = invoker.getRootNode();
        return [...root.querySelectorAll('[id]')].find(element => element.id === id) || null;
    };
    const popoverTargetAction = invoker => {
        const value = invoker.getAttribute('popovertargetaction')?.toLowerCase();
        return value === 'show' || value === 'hide' ? value : 'toggle';
    };
    for (const prototype of [HTMLButtonElement.prototype, HTMLInputElement.prototype]) {
        Object.defineProperties(prototype, {
            popoverTargetElement: {
                configurable: true, enumerable: true,
                get() { return popoverTargetFor(this); },
                set(value) {
                    if (value == null) {
                        explicitPopoverTargets.delete(this);
                        this.removeAttribute('popovertarget');
                    } else if (value instanceof Element) {
                        this.setAttribute('popovertarget', '');
                        explicitPopoverTargets.set(this, value);
                    } else throw new TypeError('popoverTargetElement requires an Element or null');
                }
            },
            popoverTargetAction: {
                configurable: true, enumerable: true,
                get() { return popoverTargetAction(this); },
                set(value) { this.setAttribute('popovertargetaction', String(value)); }
            }
        });
    }
    function activatePopoverTarget(target, event) {
        if (!(event instanceof MouseEvent) || event.type !== 'click' ||
            event.defaultPrevented || event.button !== 0) return;
        const element = target instanceof Element ? target : target.parentElement;
        const invoker = element?.closest('button[popovertarget],input[popovertarget]');
        if (!invoker || invoker.matches(':disabled')) return;
        if (invoker instanceof HTMLInputElement &&
            !['button', 'submit', 'reset'].includes(invoker.type)) return;
        if (invoker.form && invoker.type === 'submit') return;
        const popover = popoverTargetFor(invoker);
        if (!(popover instanceof HTMLElement) || popoverMode(popover) === null) return;
        // A click inside a popover nested in its own invoker is not an
        // activation of that invoker (HTML popover target activation step 3).
        if (shadowIncludingContains(popover, target) &&
            popover !== invoker && shadowIncludingContains(invoker, popover)) return;
        const action = popoverTargetAction(invoker);
        try {
            if (action === 'show' && !popoverIsOpen(popover)) popoverShow(popover, invoker);
            else if (action === 'hide' && popoverIsOpen(popover)) popoverHide(popover, { source: invoker });
            else if (action === 'toggle') {
                if (popoverIsOpen(popover)) popoverHide(popover, { source: invoker });
                else popoverShow(popover, invoker);
            }
        } catch (error) {
            // Invoker activation uses the non-throwing forms of the HTML algorithms.
            if (!(error instanceof DOMException &&
                ['InvalidStateError', 'NotSupportedError'].includes(error.name))) throw error;
        }
    }
